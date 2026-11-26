//! Choosing the sandbox mode programs `run` starts are held to.
//!
//! Shown by a bare `/sandbox` (SANDBOX-22). The list is every mode in [`SandboxMode::ALL`], `off`
//! included, in the order they are listed everywhere else, with the mode in force marked as current
//! and the cursor on it.
//!
//! Drawn as a centred panel over the session, so the transcript stays visible. Enter takes the row
//! under the cursor and Escape leaves the mode alone.
//!
//! Every row is drawn the same way. A mode the managed file does not allow is refused when it is
//! chosen, with the sentence `/sandbox <word>` gives and the file named, because the choice goes
//! through the same function (`sandbox_command::run`) and not through a second check here.
//!
//! Nothing labelled is involved. The rows are the fixed names of the modes and sentences from the
//! catalog, and the held mode is the only state read.

use bravebot_i18n::t;
use bravebot_sandbox::SandboxMode;
use ratatui::Frame;

use crate::list_picker::{self, ListPicker, Panel, PickerRow};

/// One row of the list: a mode a person may ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row(pub SandboxMode);

impl Row {
    /// What this row is for, in one line under the list.
    ///
    /// A match rather than a lookup, so every sentence is named in the source: nothing computed
    /// while running can choose which of them a person is shown.
    fn hint(self) -> &'static str {
        match self.0 {
            SandboxMode::Strict => t!(sandbox_hint_strict),
            SandboxMode::ReadOnly => t!(sandbox_hint_read_only),
            SandboxMode::WorkspaceWrite => t!(sandbox_hint_workspace_write),
            SandboxMode::Off => t!(sandbox_hint_off),
        }
    }
}

impl PickerRow for Row {
    /// The word `/sandbox` takes and every report spells the mode with, untranslated.
    fn name(&self) -> String {
        self.0.name().to_string()
    }

    fn hint(&self) -> Option<&'static str> {
        Some(Row::hint(*self))
    }

    fn same_as(&self, other: &Self) -> bool {
        self == other
    }
}

/// What the picker is showing and where the cursor is.
pub type Picker = ListPicker<Row>;

impl Picker {
    /// Open on the mode in force, since that is the row a person is looking for.
    pub fn new(current: SandboxMode) -> Self {
        let rows: Vec<Row> = SandboxMode::ALL.into_iter().map(Row).collect();
        let previous = Row(current);
        let selected = rows.iter().position(|row| *row == previous).unwrap_or(0);
        Self::open(rows, selected, previous)
    }
}

/// Where [`draw`] draws the panel in a frame of `frame`.
pub fn placed(frame: ratatui::layout::Rect, picker: &Picker) -> list_picker::Placed {
    list_picker::placed(frame, picker, &panel())
}

/// Draw the panel over whatever is already in the frame.
pub fn draw(frame: &mut Frame, picker: &Picker) {
    list_picker::draw(frame, picker, &panel());
}

fn panel() -> Panel {
    Panel {
        title: t!(sandbox_picker_title),
        keys: t!(sandbox_picker_keys),
        scope_line: None,
        width: 72,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    /// The text of each row of an 80x24 frame the picker is drawn in.
    fn screen(picker: &Picker) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        terminal
            .draw(|frame| draw(frame, picker))
            .expect("draw succeeds");
        terminal
            .backend()
            .buffer()
            .content()
            .chunks(80)
            .map(|line| line.iter().map(|cell| cell.symbol()).collect())
            .collect()
    }

    /// The rows the list is drawn on, top to bottom, trimmed.
    fn listed(picker: &Picker) -> Vec<String> {
        let list = placed(ratatui::layout::Rect::new(0, 0, 80, 24), picker).list;
        let screen = screen(picker);
        (list.y..list.bottom())
            .map(|y| screen[usize::from(y)].trim().to_string())
            .collect()
    }

    /// The list offers every mode in `SandboxMode::ALL`, `off` included, in that order, and each
    /// is drawn as the word `/sandbox` takes.
    ///
    /// Rejects: a list that leaves `off` out because it is the dangerous one, one that adds a
    /// fifth row such as the old word `standard`, and one that is ordered differently from the
    /// list `/sandbox` names in its refusals.
    #[test]
    fn the_list_is_every_mode_in_the_order_they_are_listed() {
        let picker = Picker::new(SandboxMode::WorkspaceWrite);
        let modes: Vec<SandboxMode> = picker.rows().iter().map(|row| row.0).collect();
        assert_eq!(modes, SandboxMode::ALL.to_vec());
        let drawn = listed(&picker);
        for (mode, line) in SandboxMode::ALL.iter().zip(&drawn) {
            assert!(
                line.contains(mode.name()),
                "{mode:?} is not drawn: {drawn:?}"
            );
        }
        assert!(drawn[0].contains("strict"), "{drawn:?}");
    }

    /// The cursor opens on the mode in force, whichever it is.
    ///
    /// Rejects: a picker that always opens on the first row, which makes Enter on opening move the
    /// session to `strict`.
    #[test]
    fn the_picker_opens_on_the_mode_in_force() {
        for mode in SandboxMode::ALL {
            let picker = Picker::new(mode);
            assert_eq!(picker.chosen(), Some(&Row(mode)), "{mode:?}");
        }
    }

    /// The mode in force is marked as current on the screen, and only that row.
    ///
    /// Rejects: a picker that marks no row, and one that marks the row the cursor opened on rather
    /// than the mode held (the two are the same on opening, so the cursor is moved first).
    #[test]
    fn the_mode_in_force_is_drawn_as_current_and_no_other() {
        let mut picker = Picker::new(SandboxMode::ReadOnly);
        list_picker::handle_key(
            &mut picker,
            ratatui::crossterm::event::KeyCode::Down,
            ratatui::crossterm::event::KeyModifiers::NONE,
        );
        let drawn = listed(&picker);
        let marked: Vec<&String> = drawn
            .iter()
            .filter(|line| line.contains("current"))
            .collect();
        assert_eq!(marked.len(), 1, "{drawn:?}");
        assert!(marked[0].contains("read-only"), "{drawn:?}");
    }

    /// Every row says what it is for, in the catalog's words, so somebody choosing between four
    /// names is told what each one leaves open.
    ///
    /// Rejects: a row whose hint is empty or is the hint of another mode.
    #[test]
    fn the_row_under_the_cursor_says_what_it_is_for() {
        let hints = [
            (SandboxMode::Strict, t!(sandbox_hint_strict)),
            (SandboxMode::ReadOnly, t!(sandbox_hint_read_only)),
            (
                SandboxMode::WorkspaceWrite,
                t!(sandbox_hint_workspace_write),
            ),
            (SandboxMode::Off, t!(sandbox_hint_off)),
        ];
        for (mode, hint) in hints {
            assert!(!hint.is_empty(), "{mode:?}");
            let picker = Picker::new(mode);
            let drawn = screen(&picker).concat();
            assert!(drawn.contains(hint), "{mode:?}: {drawn}");
        }
    }

    /// The panel is a bordered panel with the catalog's title and keys.
    #[test]
    fn the_picker_is_drawn_as_a_titled_panel_with_its_keys() {
        let drawn = screen(&Picker::new(SandboxMode::Off)).concat();
        assert!(drawn.contains(t!(sandbox_picker_title)), "{drawn}");
        assert!(drawn.contains(t!(sandbox_picker_keys)), "{drawn}");
        assert!(drawn.contains('╭') && drawn.contains('╮'), "{drawn}");
    }
}
