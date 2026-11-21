//! BOT-17.

#[cfg_attr(
    not(test),
    expect(
        clippy::wildcard_imports,
        reason = "a child of state.rs, which holds Session and the types every sibling shares"
    )
)]
use super::*;
use crate::bots_pane::{Focus, State, drawn};
use std::cell::RefCell;
use std::rc::Rc;

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

    pub fn as_bot(&mut self, slug: String) {
        self.bot = Some(slug);
    }

    pub fn bot_slug(&self) -> Option<&str> {
        self.bot.as_deref()
    }

    pub fn request_bot(&mut self, slug: String, conversation: Option<String>) {
        self.bot_open = Some(BotOpen { slug, conversation });
    }

    pub fn take_bot_open(&mut self) -> Option<BotOpen> {
        self.bot_open.take()
    }

    pub fn return_to_the_list(&mut self) {
        self.return_to_the_list = true;
    }

    pub fn take_return_to_the_list(&mut self) -> bool {
        std::mem::take(&mut self.return_to_the_list)
    }

    pub fn share_confirmed_directories(&self) -> Rc<RefCell<Vec<std::path::PathBuf>>> {
        Rc::clone(&self.confirmed_cd)
    }

    pub fn use_confirmed_directories(&mut self, confirmed: Rc<RefCell<Vec<std::path::PathBuf>>>) {
        self.confirmed_cd = confirmed;
    }

    pub fn note_confirmed_directory(&self, path: std::path::PathBuf) {
        let path = bravebot_sandbox::rules::without_verbatim_prefix(path);
        let mut confirmed = self.confirmed_cd.borrow_mut();
        if !confirmed.iter().any(|have| have == &path) {
            confirmed.push(path);
        }
    }

    pub fn confirmed_directories(&self) -> Vec<std::path::PathBuf> {
        self.confirmed_cd.borrow().clone()
    }

    pub fn remember_workspace_before_the_bot(&mut self, workspace: bravebot_agent::Workspace) {
        self.before_the_bot = Some(workspace);
    }

    /// The workspace a bot session should leave behind. A typed `/cd` keeps the directory it
    /// confirmed. The move into the bot's folder does not.
    pub fn workspace_to_leave_with(
        &self,
        current: bravebot_agent::Workspace,
    ) -> bravebot_agent::Workspace {
        if self.root_was_confirmed(current.root()) {
            return current;
        }
        self.before_the_bot.clone().unwrap_or(current)
    }

    /// Whether a typed `/cd` confirmed `root`. An ordinary Windows drive path is compared with
    /// the `\\?\` prefix removed from both sides. A network path and a path too long for that
    /// spelling keep the prefix.
    pub fn root_was_confirmed(&self, root: &std::path::Path) -> bool {
        let root = bravebot_sandbox::rules::without_verbatim_prefix(root.to_path_buf());
        self.confirmed_directories()
            .iter()
            .any(|path| bravebot_sandbox::rules::without_verbatim_prefix(path.clone()) == root)
    }

    pub fn remember_model_beside_a_bot(&mut self, model: Option<String>) {
        self.model_beside_a_bot = Some(model);
    }

    pub fn model_beside_a_bot(&self) -> Option<Option<String>> {
        self.model_beside_a_bot.clone()
    }

    pub fn model_from_the_definition(&self) -> Option<String> {
        self.model_from_the_definition.clone()
    }

    pub fn note_model_from_the_definition(&mut self, model: String) {
        self.model_from_the_definition = Some(model);
    }

    pub fn clear_model_from_the_definition(&mut self) {
        self.model_from_the_definition = None;
    }

    pub fn use_model(&mut self, model: Option<String>) {
        self.model = model;
    }
}

/// A bot the panes asked the phase to open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotOpen {
    pub slug: String,
    pub conversation: Option<String>,
}
