//! Choosing which sections the info panel draws.
//!
//! Shown by `/sidebar` ([PANEL-17](../../../docs/specs/info-panel.md#PANEL-17)). One row for each
//! section of the panel, in the panel's order, `[x]` while it is drawn and `[ ]` while it is turned
//! off. Space or Enter turns the row under the cursor on or off, and so does a press on it. The
//! panel behind the list follows each change on the next frame, and Escape closes the list and
//! keeps every change made.
//!
//! A mode of the session only, as `/model` is, so it is the same code at rest and while a turn
//! runs: nothing here waits, reads a file or touches the turn.
//!
//! Nothing labelled is involved. The rows are the fixed headings of the panel from the message
//! catalog, one for each [`Section`], and no text from a session, a tool or a file is drawn.

use bravebot_i18n::t;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyModifiers, MouseEvent};

use crate::list_picker::{self, ListPicker, Panel, PickerRow, Placed, Press};
use crate::panel::{Section, Sections};

/// One row of the list: a section and whether the panel draws it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub section: Section,
    pub shown: bool,
}

impl PickerRow for Row {
    fn name(&self) -> String {
        format!(
            "[{}] {}",
            if self.shown { "x" } else { " " },
            self.section.heading()
        )
    }

    fn hint(&self) -> Option<&'static str> {
        None
    }

    /// No row is marked `current`: the box in front of the name is what says whether it is drawn.
    fn same_as(&self, _other: &Self) -> bool {
        false
    }
}

/// What the picker is showing and where the cursor is.
pub type Picker = ListPicker<Row>;

/// What a key or a press did to the open list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Continue,
    /// Close the list, keeping what was changed.
    Close,
    /// A row was turned on or off, and these are the sections turned off now. The list stays open.
    Changed(Sections),
}

impl Picker {
    /// Open with every section of the panel as a row, drawn as `sections` says, the cursor on the
    /// first.
    pub fn new(sections: Sections) -> Self {
        let rows: Vec<Row> = Section::ALL
            .into_iter()
            .map(|section| Row {
                section,
                shown: sections.shows(section),
            })
            .collect();
        let first = rows[0];
        Self::open(rows, 0, first)
    }

    /// The sections turned off, as the rows stand now.
    pub fn sections(&self) -> Sections {
        self.rows()
            .iter()
            .filter(|row| !row.shown)
            .fold(Sections::default(), |off, row| off.toggled(row.section))
    }

    fn toggle(&mut self) -> Outcome {
        if let Some(row) = self.chosen_mut() {
            row.shown = !row.shown;
        }
        Outcome::Changed(self.sections())
    }
}

/// Interpret one key press. Space and Enter toggle the row, Escape and Ctrl-C close the list, and
/// the arrows and `j` and `k` walk it, as in every list picker.
pub fn press(picker: &mut Picker, code: KeyCode, modifiers: KeyModifiers) -> Outcome {
    if code == KeyCode::Char(' ') && modifiers.is_empty() {
        return picker.toggle();
    }
    match list_picker::handle_key(picker, code, modifiers) {
        list_picker::Outcome::Continue => Outcome::Continue,
        list_picker::Outcome::Cancel => Outcome::Close,
        list_picker::Outcome::Select => picker.toggle(),
    }
}

/// Interpret one mouse event. A press on a row turns it on or off at once, where the other
/// pickers take a row on a second press (MOUSE-15). The wheel and a press outside the panel are
/// theirs unchanged (MOUSE-46, MOUSE-47).
pub fn click(
    picker: &mut Picker,
    placed: Placed,
    mouse: MouseEvent,
    last: &mut Option<crate::select::Click>,
    now: std::time::Instant,
    wheel: u16,
) -> Outcome {
    match list_picker::handle_mouse_taking(picker, placed, mouse, last, now, wheel, Press::First) {
        list_picker::Outcome::Continue => Outcome::Continue,
        list_picker::Outcome::Cancel => Outcome::Close,
        list_picker::Outcome::Select => picker.toggle(),
    }
}

/// Where [`draw`] draws the panel in a frame of `frame`.
pub fn placed(frame: ratatui::layout::Rect, picker: &Picker) -> Placed {
    list_picker::placed(frame, picker, &panel())
}

/// Draw the panel over whatever is already in the frame.
pub fn draw(frame: &mut Frame, picker: &Picker) {
    list_picker::draw(frame, picker, &panel());
}

fn panel() -> Panel {
    Panel {
        title: t!(sidebar_picker_title),
        keys: t!(sidebar_picker_keys),
        scope_line: None,
        width: 42,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{MouseButton, MouseEventKind};
    use ratatui::layout::Rect;

    fn off(sections: &[Section]) -> Sections {
        sections
            .iter()
            .fold(Sections::default(), |off, section| off.toggled(*section))
    }

    fn names(picker: &Picker) -> Vec<String> {
        picker.rows().iter().map(PickerRow::name).collect()
    }

    fn at_row(picker: &Picker) -> Option<Section> {
        picker.chosen().map(|row| row.section)
    }

    fn pressed(picker: &mut Picker, code: KeyCode) -> Outcome {
        press(picker, code, KeyModifiers::NONE)
    }

    /// VIEW-46. The sidebar checklist has no scope: no scope line is drawn for it, Tab leaves it with
    /// none and changes no section, and the cursor stays where it was.
    ///
    /// Rejects: a panel given a scope line, which draws an Enter and Tab row that does nothing here,
    /// and a Tab that gives the picker a scope, moves its cursor or toggles a section.
    #[test]
    fn the_sidebar_checklist_has_no_scope_and_ignores_tab() {
        let mut picker = Picker::new(Sections::default());
        pressed(&mut picker, KeyCode::Down);
        assert_eq!(panel().scope_line, None);
        assert_eq!(picker.scope(), None);
        for (code, modifiers) in [
            (KeyCode::Tab, KeyModifiers::NONE),
            (KeyCode::BackTab, KeyModifiers::SHIFT),
        ] {
            assert_eq!(press(&mut picker, code, modifiers), Outcome::Continue);
            assert_eq!(picker.scope(), None, "{code:?}");
            assert_eq!(at_row(&picker), Some(Section::Model), "{code:?}");
            assert_eq!(picker.sections(), Sections::default(), "{code:?}");
        }
        let mut terminal = ratatui::Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        terminal
            .draw(|frame| draw(frame, &picker))
            .expect("draw succeeds");
        let drawn: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(!drawn.contains("Tab:"), "{drawn}");
    }

    /// The list has a row for each section of the panel in the panel's order, `[x]` for the ones the
    /// panel draws and `[ ]` for the ones it does not, and says nothing but the panel's headings.
    ///
    /// Rejects: a missing section, an order other than the panel's, marks the wrong way round, and a
    /// row whose words are not the catalog's heading of its section.
    #[test]
    fn a_row_for_each_section_in_the_panels_order_checked_while_it_is_drawn() {
        let all_on = Picker::new(Sections::default());
        let expected: Vec<String> = Section::ALL
            .iter()
            .map(|section| format!("[x] {}", section.heading()))
            .collect();
        assert_eq!(names(&all_on), expected);
        assert_eq!(
            expected,
            [
                "[x] Session",
                "[x] Model",
                "[x] Goal",
                "[x] Pull requests",
                "[x] Issues",
                "[x] Context",
                "[x] Language servers",
                "[x] MCP servers",
                "[x] Files",
                "[x] Plan",
            ]
        );

        let some_off = Picker::new(off(&[Section::Model, Section::Plan]));
        let marks: Vec<String> = names(&some_off)
            .iter()
            .map(|name| name.chars().take(3).collect())
            .collect();
        for (section, mark) in Section::ALL.iter().zip(&marks) {
            let wanted = if matches!(section, Section::Model | Section::Plan) {
                "[ ]"
            } else {
                "[x]"
            };
            assert_eq!(mark, wanted, "{}", section.heading());
        }
        assert_eq!(at_row(&some_off), Some(Section::Session));
    }

    /// Space and Enter each turn the row under the cursor off and on again, the cursor stays on it,
    /// and the list stays open, so the person sees the box change.
    ///
    /// Rejects: one of the two keys doing nothing, Enter closing the list as it does in the others,
    /// a toggle that moves the cursor on, a toggle of another row, and a change that reports the
    /// sections as they were.
    #[test]
    fn space_and_enter_turn_the_row_under_the_cursor_off_and_on_and_the_cursor_stays() {
        for toggle in [KeyCode::Char(' '), KeyCode::Enter] {
            let mut picker = Picker::new(Sections::default());
            pressed(&mut picker, KeyCode::Down);
            pressed(&mut picker, KeyCode::Down);
            assert_eq!(at_row(&picker), Some(Section::Goal));

            assert_eq!(
                pressed(&mut picker, toggle),
                Outcome::Changed(off(&[Section::Goal])),
                "{toggle:?}"
            );
            assert_eq!(at_row(&picker), Some(Section::Goal));
            assert_eq!(names(&picker)[2], "[ ] Goal");
            assert_eq!(names(&picker)[1], "[x] Model");
            assert_eq!(names(&picker)[3], "[x] Pull requests");

            assert_eq!(
                pressed(&mut picker, toggle),
                Outcome::Changed(Sections::default()),
                "{toggle:?} again"
            );
            assert_eq!(names(&picker)[2], "[x] Goal");
        }
    }

    /// Two sections turned off at different rows are both reported, so a toggle adds to what was
    /// already off.
    ///
    /// Rejects: a change that reports only the row just toggled.
    #[test]
    fn a_change_reports_every_section_that_is_off_and_not_only_the_last() {
        let mut picker = Picker::new(off(&[Section::Plan]));
        assert_eq!(
            pressed(&mut picker, KeyCode::Char(' ')),
            Outcome::Changed(off(&[Section::Plan, Section::Session]))
        );
    }

    /// The arrows and `j` and `k` walk the list and stop at both ends, as in every list picker, and
    /// walking changes no section.
    ///
    /// Rejects: a list that wraps, and a key of the walk that toggles.
    #[test]
    fn the_arrows_and_j_and_k_walk_the_list_and_stop_at_both_ends() {
        let mut picker = Picker::new(Sections::default());
        for up in [KeyCode::Up, KeyCode::Char('k')] {
            assert_eq!(pressed(&mut picker, up), Outcome::Continue);
            assert_eq!(at_row(&picker), Some(Section::Session), "{up:?} at the top");
        }
        for down in [KeyCode::Down, KeyCode::Char('j')] {
            let before = at_row(&picker);
            assert_eq!(pressed(&mut picker, down), Outcome::Continue);
            assert_ne!(at_row(&picker), before, "{down:?}");
        }
        for _ in 0..40 {
            pressed(&mut picker, KeyCode::Down);
        }
        assert_eq!(at_row(&picker), Some(Section::Plan));
        assert_eq!(picker.sections(), Sections::default());
    }

    /// Escape and Ctrl-C close the list and nothing they do undoes a toggle: the rows keep what was
    /// changed and no row is toggled by the key that closes.
    ///
    /// Rejects: a close that is a cancel back to how the list opened, and a close that toggles.
    #[test]
    fn escape_and_ctrl_c_close_the_list_and_keep_what_was_changed() {
        for (code, modifiers) in [
            (KeyCode::Esc, KeyModifiers::NONE),
            (KeyCode::Char('c'), KeyModifiers::CONTROL),
        ] {
            let mut picker = Picker::new(Sections::default());
            pressed(&mut picker, KeyCode::Char(' '));
            assert_eq!(press(&mut picker, code, modifiers), Outcome::Close);
            assert_eq!(picker.sections(), off(&[Section::Session]), "{code:?}");
        }
    }

    /// Space with Control, Alt or Shift held toggles nothing, so a chord somebody meant for another
    /// part of the terminal does not change the panel.
    ///
    /// Rejects: a toggle that reads the character and not the chord.
    #[test]
    fn a_space_with_a_modifier_toggles_nothing() {
        for modifiers in [
            KeyModifiers::CONTROL,
            KeyModifiers::ALT,
            KeyModifiers::SHIFT,
        ] {
            let mut picker = Picker::new(Sections::default());
            assert_eq!(
                press(&mut picker, KeyCode::Char(' '), modifiers),
                Outcome::Continue,
                "{modifiers:?}"
            );
            assert_eq!(picker.sections(), Sections::default());
        }
    }

    fn placed_in_140_by_40(picker: &Picker) -> Placed {
        placed(Rect::new(0, 0, 140, 40), picker)
    }

    fn mouse(kind: MouseEventKind, row: u16, column: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn left(row: u16, column: u16) -> MouseEvent {
        mouse(MouseEventKind::Down(MouseButton::Left), row, column)
    }

    fn clicked(picker: &mut Picker, event: MouseEvent) -> Outcome {
        let shown = placed_in_140_by_40(picker);
        click(
            picker,
            shown,
            event,
            &mut None,
            std::time::Instant::now(),
            3,
        )
    }

    /// MOUSE-15 for this list. One press on a row turns it off, and the next press on it, however
    /// soon, turns it on again, with the cursor on the row and the list open.
    ///
    /// Rejects: the two presses the other pickers need, which would leave the first press doing
    /// nothing, and a second press that is counted as a double click and so does nothing.
    #[test]
    fn one_press_on_a_row_turns_it_off_and_the_next_turns_it_on_again() {
        let mut picker = Picker::new(Sections::default());
        let shown = placed_in_140_by_40(&picker);
        let event = left(shown.list.y + 3, shown.list.x + 4);
        let mut last = None;
        let now = std::time::Instant::now();

        let first = click(&mut picker, shown, event, &mut last, now, 3);
        assert_eq!(first, Outcome::Changed(off(&[Section::PullRequests])));
        assert_eq!(at_row(&picker), Some(Section::PullRequests));

        let second = click(&mut picker, shown, event, &mut last, now, 3);
        assert_eq!(second, Outcome::Changed(Sections::default()));
    }

    /// The press that turned a row off found the row it was drawn on, and not the row the cursor was
    /// on before it.
    ///
    /// Rejects: a press that toggles the row under the cursor and not the row it landed on.
    #[test]
    fn a_press_turns_off_the_row_it_landed_on_and_not_the_one_under_the_cursor() {
        let mut picker = Picker::new(Sections::default());
        let shown = placed_in_140_by_40(&picker);
        let last_row = Section::ALL.len() as u16 - 1;
        let event = left(shown.list.y + last_row, shown.list.x + 2);
        assert_eq!(
            clicked(&mut picker, event),
            Outcome::Changed(off(&[Section::Plan]))
        );
        assert_eq!(names(&picker)[0], "[x] Session");
    }

    /// MOUSE-46, MOUSE-47 for this list. The wheel moves the cursor and toggles nothing, a press
    /// outside the panel closes it, and a press on the border, below the last row or under the list
    /// changes nothing.
    ///
    /// Rejects: a press anywhere in the panel that toggles the row under the cursor, and a wheel
    /// that toggles.
    #[test]
    fn the_wheel_walks_a_press_outside_closes_and_a_press_off_the_rows_changes_nothing() {
        let mut picker = Picker::new(Sections::default());
        let shown = placed_in_140_by_40(&picker);

        let wheel = mouse(MouseEventKind::ScrollDown, shown.list.y, shown.list.x);
        assert_eq!(clicked(&mut picker, wheel), Outcome::Continue);
        assert_eq!(
            at_row(&picker),
            Some(Section::PullRequests),
            "three rows of the wheel"
        );
        assert_eq!(picker.sections(), Sections::default());

        let rows = Section::ALL.len() as u16;
        for (row, column) in [
            (shown.area.y, shown.area.x + 4),
            (shown.area.y + shown.area.height - 1, shown.area.x + 4),
            (shown.list.y, shown.area.x),
            (shown.list.y + rows + 1, shown.list.x + 2),
        ] {
            assert_eq!(
                clicked(&mut picker, left(row, column)),
                Outcome::Continue,
                "a press at {row},{column}"
            );
        }
        assert_eq!(picker.sections(), Sections::default());
        assert_eq!(at_row(&picker), Some(Section::PullRequests));

        assert_eq!(clicked(&mut picker, left(0, 0)), Outcome::Close);
        assert_eq!(picker.sections(), Sections::default());
    }

    /// The panel is drawn with its title and its key line, and each row as `[x] Name`, so the person
    /// can read what is on offer and how to leave.
    ///
    /// Rejects: a list that is drawn without the checkboxes, and a panel with no word on its keys.
    #[test]
    fn the_panel_draws_the_boxes_the_title_and_the_keys() {
        let picker = Picker::new(off(&[Section::Files]));
        let mut terminal = ratatui::Terminal::new(TestBackend::new(140, 40)).expect("terminal");
        terminal
            .draw(|frame| draw(frame, &picker))
            .expect("draw succeeds");
        let shown = placed_in_140_by_40(&picker);
        let buffer = terminal.backend().buffer();
        let rows: Vec<String> = (shown.list.y..shown.list.y + Section::ALL.len() as u16)
            .map(|y| {
                (shown.list.x..shown.list.right())
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect();
        assert_eq!(rows[0], "❯ [x] Session");
        assert_eq!(rows[8], "  [ ] Files");
        assert_eq!(rows[9], "  [x] Plan");
        let all: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(all.contains(t!(sidebar_picker_title)), "{all}");
        assert!(all.contains(t!(sidebar_picker_keys)), "{all}");
    }
}
