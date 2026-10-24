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
use ratatui::crossterm::event::{self, Event as TermEvent, KeyCode, KeyModifiers, MouseEvent};
use ratatui::layout::Rect;

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

/// Interpret one mouse event, with the theme under the cursor in force as it is for a key.
///
/// A press on a row moves the cursor there, so the row is previewed, and a second press on it takes
/// the theme. Nothing else the mouse does leaves the picker (MOUSE-17).
pub fn click(
    picker: &mut Picker,
    list: Rect,
    mouse: MouseEvent,
    last: &mut Option<select::Click>,
    now: std::time::Instant,
) -> Outcome {
    let outcome = list_picker::handle_mouse(picker, list, mouse, last, now);
    match outcome {
        Outcome::Continue => picker.preview(),
        Outcome::Select => match picker.chosen() {
            Some(theme) => theme::apply(theme),
            None => picker.restore(),
        },
        Outcome::Cancel => picker.restore(),
    }
    outcome
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
    behind: impl FnMut(&mut Frame),
) -> Option<Theme> {
    run(terminal, themes, current, behind, || {
        input::read().map(|event| (event, std::time::Instant::now()))
    })
}

/// [`choose`] with its events taken from `next`, each with the time it arrived, so that a test can
/// press the cells where a row was drawn.
fn run<B: Backend>(
    terminal: &mut Terminal<B>,
    themes: Vec<Theme>,
    current: &str,
    mut behind: impl FnMut(&mut Frame),
    mut next: impl FnMut() -> std::io::Result<(TermEvent, std::time::Instant)>,
) -> Option<Theme> {
    let mut picker = opened(themes, current);
    let panel = panel();
    let mut list = Rect::default();
    let mut last = None;

    loop {
        if terminal
            .draw(|frame| {
                behind(frame);
                list_picker::draw(frame, &picker, &panel);
                list = list_picker::regions(frame.area(), picker.rows().len(), panel.width)[0];
            })
            .is_err()
        {
            picker.restore();
            return None;
        }

        let Ok((event, now)) = next() else {
            picker.restore();
            return None;
        };
        if let TermEvent::Mouse(mouse) = event {
            match click(&mut picker, list, mouse, &mut last, now) {
                Outcome::Select => return picker.chosen().cloned(),
                Outcome::Continue | Outcome::Cancel => continue,
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
    use ratatui::crossterm::event::{MouseButton, MouseEventKind};

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
            list,
            left_press(list.y + 2, list.x),
            &mut last,
            now,
        );
        assert_eq!(outcome, Outcome::Continue);
        let named = offered()[2].name.clone();
        assert_eq!(picker.chosen().expect("a theme").name, named);
        assert_eq!(theme::name(), named, "the row was not previewed");

        let outcome = click(
            &mut picker,
            list,
            left_press(list.y + 2, list.x),
            &mut last,
            now,
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

    /// MOUSE-17. Rejects: a press outside the list or a wheel turn moving the preview or leaving.
    #[test]
    fn a_press_outside_the_list_leaves_the_theme_alone() {
        let _held = theme::exclusive();
        theme::apply_brave();
        let mut picker = opened(offered(), theme::BRAVE);
        let list = Rect::new(10, 5, 30, 8);
        let (mut last, now) = (None, std::time::Instant::now());
        for _ in 0..2 {
            let outcome = click(&mut picker, list, left_press(0, 0), &mut last, now);
            assert_eq!(outcome, Outcome::Continue);
        }
        let wheel = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            ..left_press(list.y + 1, list.x)
        };
        assert_eq!(
            click(&mut picker, list, wheel, &mut last, now),
            Outcome::Continue
        );
        assert_eq!(theme::name(), theme::BRAVE);
    }
}
