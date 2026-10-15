//! The pickers that `/config`, `/theme` and `/effort` open, held as state of the session.
//!
//! At rest each of them is a loop of its own: it draws, reads a key and returns what was picked.
//! A turn cannot wait inside such a loop, because the same thread draws the turn and puts the
//! worker's questions on the screen. So while a turn runs the picker is a mode of the session
//! instead: the key handler gives it every key, the frame draws it over everything else, and the
//! loop that runs the turn goes on as before. A question the worker asks takes the screen for as
//! long as it is open, and the picker is there again when it is answered.
//!
//! The rows, the cursor and the keys are the ones the picker has at rest, read from the same
//! modules. What this adds is only where the state lives.
//!
//! Nothing labelled is involved. The rows are fixed strings or names a person picks from, and the
//! pick is the person's own keystroke.

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use crate::list_picker::Outcome;
use crate::theme::Theme;
use crate::{config_prompt, effort_prompt, theme_prompt};

/// A picker that is open.
#[derive(Debug)]
pub enum Picking {
    /// `/theme` alone. The theme under the cursor is in force while the picker is open.
    Theme(theme_prompt::Picker),
    /// `/effort` alone.
    Effort(effort_prompt::Picker),
    /// `/config`.
    Editing(config_prompt::Picker),
}

/// What a person picked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Picked {
    Theme(Theme),
    Effort(effort_prompt::Row),
    Editing(config_prompt::Row),
}

/// What a key press did to an open picker.
#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    /// Still choosing.
    Continue,
    /// Left with what was in force alone.
    Cancelled,
    /// Took a row.
    Picked(Picked),
}

impl Picking {
    /// Interpret one key press.
    pub fn press(&mut self, code: KeyCode, modifiers: KeyModifiers) -> Step {
        match self {
            Picking::Theme(picker) => match theme_prompt::press(picker, code, modifiers) {
                Outcome::Continue => Step::Continue,
                Outcome::Cancel => Step::Cancelled,
                Outcome::Select => picker
                    .chosen()
                    .cloned()
                    .map_or(Step::Cancelled, |theme| Step::Picked(Picked::Theme(theme))),
            },
            Picking::Effort(picker) => {
                match crate::list_picker::handle_key(picker, code, modifiers) {
                    Outcome::Continue => Step::Continue,
                    Outcome::Cancel => Step::Cancelled,
                    Outcome::Select => picker
                        .chosen()
                        .copied()
                        .map_or(Step::Cancelled, |row| Step::Picked(Picked::Effort(row))),
                }
            }
            Picking::Editing(picker) => {
                match crate::list_picker::handle_key(picker, code, modifiers) {
                    Outcome::Continue => Step::Continue,
                    Outcome::Cancel => Step::Cancelled,
                    Outcome::Select => picker
                        .chosen()
                        .copied()
                        .map_or(Step::Cancelled, |row| Step::Picked(Picked::Editing(row))),
                }
            }
        }
    }

    /// Draw the panel over whatever is already in the frame.
    pub fn draw(&self, frame: &mut Frame) {
        match self {
            Picking::Theme(picker) => theme_prompt::draw(frame, picker),
            Picking::Effort(picker) => effort_prompt::draw(frame, picker),
            Picking::Editing(picker) => config_prompt::draw(frame, picker),
        }
    }
}
