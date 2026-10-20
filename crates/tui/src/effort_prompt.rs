//! Choosing how hard the model thinks before it answers.
//!
//! Shown by `/effort`. The list is the levels the protocol defines plus the row that asks for
//! none, and the choice is written to `~/.bravebot`: it outlives the session that made it and
//! applies in every directory, the same as the model and the theme.
//!
//! Drawn as a centred panel over the session, so the transcript stays visible. Enter takes the row
//! under the cursor. Escape leaves the level that was in force alone.
//!
//! Nothing labelled is involved. The words never reach a model as a decision: they are drawn for a
//! person, who picks one, and their pick is what endorses the request field it lands in.

use bravebot_aichat::protocol::Effort;
use bravebot_i18n::t;
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::Backend;

use crate::list_picker::{self, ListPicker, Panel, PickerRow};

/// One row of the list.
///
/// The row that asks for no level is offered because it is a choice somebody may want back: a
/// picker of five levels and no way out makes the first pick permanent, and the absent field is
/// what every service applies its own default to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row(pub Option<Effort>);

impl Row {
    /// Every row, the service's own default first and then the levels cheapest first.
    fn all() -> Vec<Row> {
        std::iter::once(Row(None))
            .chain(Effort::ALL.into_iter().map(|level| Row(Some(level))))
            .collect()
    }

    /// The word shown for this row.
    ///
    /// A level is drawn as the word it is sent and stored as, untranslated: it is an identifier
    /// that appears in a settings file and in the request, and a person comparing what they picked
    /// against what a service documents is comparing the same word.
    fn name(self) -> String {
        match self.0 {
            Some(level) => level.as_str().to_string(),
            None => t!(effort_unset).to_string(),
        }
    }

    /// What this row is for, in one line under the list.
    ///
    /// A match rather than a lookup, so every sentence is named in the source: nothing computed
    /// while running can choose which of them a person is shown.
    fn hint(self) -> &'static str {
        match self.0 {
            None => t!(effort_hint_unset),
            Some(Effort::Low) => t!(effort_hint_low),
            Some(Effort::Medium) => t!(effort_hint_medium),
            Some(Effort::High) => t!(effort_hint_high),
            Some(Effort::Xhigh) => t!(effort_hint_xhigh),
            Some(Effort::Max) => t!(effort_hint_max),
        }
    }
}

impl PickerRow for Row {
    fn name(&self) -> String {
        Row::name(*self)
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
    /// Open on the level in use, since that is the row a person is looking for.
    pub fn new(current: Option<Effort>) -> Self {
        let rows = Row::all();
        let previous = Row(current);
        let selected = rows.iter().position(|row| *row == previous).unwrap_or(0);
        Self::open(rows, selected, previous)
    }
}

/// Draw the panel over whatever is already in the frame.
pub fn draw(frame: &mut Frame, picker: &Picker) {
    list_picker::draw(frame, picker, &panel());
}

/// Show the list as a centred panel over whatever `behind` draws, and return the row the person
/// picked, or `None` if they kept what was in force.
///
/// `behind` is the session already on screen. Drawn first each frame so Escape leaves the person
/// looking at the same session they opened the picker from.
pub fn choose<B: Backend>(
    terminal: &mut Terminal<B>,
    current: Option<Effort>,
    behind: impl FnMut(&mut Frame),
) -> Option<Row> {
    list_picker::choose(terminal, Picker::new(current), &panel(), behind)
}

fn panel() -> Panel {
    Panel {
        title: t!(effort_picker_title),
        keys: t!(effort_picker_keys),
        width: 52,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;

    #[test]
    fn the_picker_opens_on_the_level_in_use() {
        let picker = Picker::new(Some(Effort::Xhigh));
        assert_eq!(picker.chosen(), Some(&Row(Some(Effort::Xhigh))));
    }

    /// Somebody who has never chosen opens on the row that asks for no level, which is what their
    /// requests are already doing.
    #[test]
    fn having_chosen_nothing_opens_on_the_row_that_asks_for_none() {
        let picker = Picker::new(None);
        assert_eq!(picker.chosen(), Some(&Row(None)));
    }

    /// A first pick must not be permanent: the row that asks for no level is how somebody gets
    /// back to whatever the service does on its own.
    #[test]
    fn asking_for_no_level_is_one_of_the_rows() {
        let picker = Picker::new(Some(Effort::Max));
        assert!(picker.rows().contains(&Row(None)));
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

    /// The words drawn are the words sent and stored, so somebody comparing what they picked
    /// against what a service documents is reading the same word.
    #[test]
    fn the_list_shows_the_words_a_request_carries() {
        let output = rendered(&Picker::new(None));
        for level in Effort::ALL {
            assert!(output.contains(level.as_str()), "{output}");
        }
    }

    #[test]
    fn the_level_in_use_is_marked() {
        let output = rendered(&Picker::new(Some(Effort::High)));
        assert!(output.contains("high  current"), "{output}");
    }

    /// Every row says what it is for. A list of five ranked words says nothing about which one to
    /// pick, and the whole point of the panel is that somebody can decide.
    #[test]
    fn the_row_under_the_cursor_says_what_it_is_for() {
        let output = rendered(&Picker::new(Some(Effort::Max)));
        assert!(output.contains("the most thinking, cost aside"), "{output}");
    }

    #[test]
    fn the_picker_is_drawn_as_a_centred_panel() {
        let output = rendered(&Picker::new(None));
        assert!(output.contains("effort"), "{output}");
        assert!(
            output.contains('╭') && output.contains('╮'),
            "no rounded border: {output}"
        );
    }

    #[test]
    fn the_panel_stays_inside_a_tiny_terminal() {
        let picker = Picker::new(None);
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
}
