//! Choosing how the input box edits text.
//!
//! Shown by `/config`. One question so far, which is whether the box edits the way vi does, and the
//! choice is written to `~/.bravebot`: it outlives the session that made it and applies in every
//! directory, the same as the model, the theme and the effort level.
//!
//! Drawn as a centred panel over the session, so the transcript stays visible. Enter takes the row
//! under the cursor. Escape leaves the style that was in force alone.
//!
//! Nothing labelled is involved. The words are drawn for a person, who picks one, and what their pick
//! changes is which keys move a caret in a box they are typing into.
//!
//! # Why the panel exists for one question
//!
//! Because the alternative is a command per preference, and this is the second of them: `/theme` and
//! `/effort` each took a word of the command space for one choice. A panel is where the third and the
//! fourth go without another verb being invented, and it is the surface somebody coming from Claude
//! Code already looks for.

use bravebot_i18n::t;
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::Backend;

use crate::list_picker::{self, ListPicker, Panel, PickerRow};
use crate::vim::Editing;

/// One row of the list: a style of editing to choose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row(pub Editing);

impl Row {
    /// Every row, the ordinary box first, which is what most people are already using.
    fn all() -> Vec<Row> {
        Editing::ALL.into_iter().map(Row).collect()
    }

    /// The word shown for this row.
    ///
    /// The word the setting is spelled with, untranslated: it appears in a settings file and in the
    /// documentation of the tools this borrows the name from, so somebody comparing what they picked
    /// against what they wrote in a file is comparing the same word.
    fn name(self) -> &'static str {
        self.0.as_str()
    }

    /// What this row means, in one line under the list.
    ///
    /// A match rather than a lookup, so every sentence is named in the source: nothing computed while
    /// running can choose which of them a person is shown.
    fn hint(self) -> &'static str {
        match self.0 {
            Editing::Ordinary => t!(config_editing_hint_ordinary),
            Editing::Vi => t!(config_editing_hint_vi),
        }
    }
}

impl PickerRow for Row {
    fn name(&self) -> String {
        Row::name(*self).to_string()
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
    /// Open on the style in use, since that is the row a person is looking for.
    pub fn new(current: Editing) -> Self {
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
    current: Editing,
    behind: impl FnMut(&mut Frame),
) -> Option<Row> {
    list_picker::choose(terminal, Picker::new(current), &panel(), behind)
}

fn panel() -> Panel {
    Panel {
        title: t!(config_picker_title),
        keys: t!(config_picker_keys),
        width: 52,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The picker opens on the style in force, since that is the row somebody is looking for: opening
    /// on the first row would make the list say the ordinary box was chosen when it was not.
    #[test]
    fn the_picker_opens_on_the_style_in_force() {
        let picker = Picker::new(Editing::Vi);
        assert_eq!(picker.chosen(), Some(&Row(Editing::Vi)));
    }

    /// Every row says what it means, so the panel is legible to somebody who does not already know
    /// what `vim` names here. A row with nothing under it would be a word and a guess.
    #[test]
    fn every_row_says_what_it_means() {
        for row in Row::all() {
            assert!(!row.hint().is_empty(), "{} has no hint", row.name());
        }
    }

    fn rendered(picker: &Picker) -> String {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).expect("terminal");
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

    /// The words drawn are the words the setting is spelled with, so somebody comparing what they
    /// picked against what they wrote in a file is reading the same word.
    #[test]
    fn the_list_shows_the_words_the_setting_is_spelled_with() {
        let output = rendered(&Picker::new(Editing::Ordinary));
        for style in Editing::ALL {
            assert!(output.contains(style.as_str()), "{output}");
        }
    }

    /// The style in force is marked on the screen and not only in the state, since the panel is where
    /// somebody goes to find out which one it is.
    #[test]
    fn the_style_in_force_is_drawn_as_current() {
        let output = rendered(&Picker::new(Editing::Vi));
        assert!(output.contains("vim  current"), "{output}");
    }

    /// The panel says what its keys are, so a person who opened it has a way out they can read rather
    /// than one they have to guess at.
    #[test]
    fn the_panel_says_which_keys_it_answers() {
        let output = rendered(&Picker::new(Editing::Ordinary));
        assert!(output.contains(t!(config_picker_keys)), "{output}");
        assert!(output.contains(t!(config_picker_title)), "{output}");
    }
}
