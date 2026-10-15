//! The pickers that `/model`, `/config`, `/theme` and `/effort` open, held as state of the session.
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

use crate::list_picker::{self, Outcome};
use crate::theme::Theme;
use bravebot_aichat::models::Model;

use crate::{config_prompt, effort_prompt, model_prompt, theme_prompt};

/// A picker that is open.
#[derive(Debug)]
pub enum Picking {
    /// `/model`, with the roster the endpoints offered when it was typed.
    Model(model_prompt::Picker),
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
    Model(Model),
    Theme(Theme),
    Effort(effort_prompt::Row),
    Editing(config_prompt::Row),
}

/// What a key press did to an open picker.
#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    Continue,
    /// Left with what was in force alone.
    Cancelled,
    Picked(Picked),
}

impl Picking {
    pub fn press(&mut self, code: KeyCode, modifiers: KeyModifiers) -> Step {
        match self {
            Picking::Model(picker) => {
                let outcome = match model_prompt::handle_key(picker, code, modifiers) {
                    model_prompt::Outcome::Continue => Outcome::Continue,
                    model_prompt::Outcome::Cancel => Outcome::Cancel,
                    model_prompt::Outcome::Select => Outcome::Select,
                };
                step(outcome, picker.chosen().cloned(), Picked::Model)
            }
            Picking::Theme(picker) => {
                let outcome = theme_prompt::press(picker, code, modifiers);
                step(outcome, picker.chosen().cloned(), Picked::Theme)
            }
            Picking::Effort(picker) => {
                let outcome = list_picker::handle_key(picker, code, modifiers);
                step(outcome, picker.chosen().copied(), Picked::Effort)
            }
            Picking::Editing(picker) => {
                let outcome = list_picker::handle_key(picker, code, modifiers);
                step(outcome, picker.chosen().copied(), Picked::Editing)
            }
        }
    }

    /// Draw the panel over whatever is already in the frame.
    pub fn draw(&self, frame: &mut Frame) {
        match self {
            Picking::Model(picker) => model_prompt::draw(frame, picker),
            Picking::Theme(picker) => theme_prompt::draw(frame, picker),
            Picking::Effort(picker) => effort_prompt::draw(frame, picker),
            Picking::Editing(picker) => config_prompt::draw(frame, picker),
        }
    }
}

/// What a key did, given the row under the cursor. A row taken with nothing under the cursor leaves
/// the picker as a cancel does.
fn step<R>(outcome: Outcome, chosen: Option<R>, picked: impl FnOnce(R) -> Picked) -> Step {
    match outcome {
        Outcome::Continue => Step::Continue,
        Outcome::Cancel => Step::Cancelled,
        Outcome::Select => chosen.map_or(Step::Cancelled, |row| Step::Picked(picked(row))),
    }
}
