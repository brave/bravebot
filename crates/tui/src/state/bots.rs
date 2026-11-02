//! BOT-17.

use super::*;
use crate::bots_pane::{Focus, State, drawn};

impl Session {
    pub fn without_a_session(&self) -> bool {
        self.without_a_session
    }

    pub fn with_no_session_begun(mut self) -> Self {
        self.without_a_session = true;
        self
    }

    pub fn with_bots(mut self, bots: State) -> Self {
        self.bots = bots;
        self
    }

    pub fn bots(&self) -> &State {
        &self.bots
    }

    pub fn bots_mut(&mut self) -> &mut State {
        &mut self.bots
    }

    pub fn frame_columns(&self) -> u16 {
        self.laid.columns
    }

    pub fn panes_are_drawn(&self) -> bool {
        self.interface_layout == crate::layout::Layout::Bots && drawn(self.laid.columns)
    }

    pub fn panes_have_the_keys(&self) -> bool {
        self.panes_are_drawn() && self.bots.focus() == Focus::Panes
    }

    pub fn start_in_the_list_where_it_is_drawn(&mut self, columns: u16) {
        let focus = if drawn(columns) {
            Focus::Panes
        } else {
            Focus::Prompt
        };
        self.bots.set_focus(focus);
    }
}
