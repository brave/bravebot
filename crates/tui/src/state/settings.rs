//! Per-session settings, and the builders and adopters that carry them in.

use super::*;

impl Session {
    /// Start under the mode a `/resume` carried over, which is the one the person last chose.
    pub fn with_sandbox(mut self, sandbox: crate::sandbox_command::Sandboxed) -> Self {
        self.live_sandbox.set(sandbox.choice.mode);
        self.sandbox = sandbox;
        self
    }

    /// The value is the inverse of the backend's `governs_sockets_with_a_path`, and false with no
    /// backend.
    pub fn with_sockets_with_a_path_reachable(mut self, reachable: bool) -> Self {
        self.sockets_with_a_path_reachable = reachable;
        self
    }

    /// The sandbox mode the next turn's programs run under.
    pub fn sandbox_mode(&self) -> bravebot_sandbox::SandboxMode {
        self.sandbox.choice.mode
    }

    /// The mode and where it came from, which `/resume` hands to the session it starts so the
    /// command's choice lasts as long as the process does.
    pub fn sandbox(&self) -> &crate::sandbox_command::Sandboxed {
        &self.sandbox
    }

    /// A handle on the mode that a running turn reads, so `/sandbox` typed while it runs applies to
    /// the programs it starts after that (SANDBOX-22). Hand this to a turn rather than the value
    /// [`Session::sandbox_mode`] returns.
    pub fn live_sandbox_mode(&self) -> bravebot_agent::LiveSandbox {
        self.live_sandbox.clone()
    }

    /// Move the mode for the rest of the process. A turn running now follows it from its next
    /// call, and a program already started keeps the profile it started under.
    pub fn set_sandbox_mode(&mut self, mode: bravebot_sandbox::SandboxMode) {
        self.sandbox = crate::sandbox_command::Sandboxed::typed(mode);
        self.live_sandbox.set(mode);
    }

    /// Say what the configuration allows, for the opening screen to draw.
    ///
    /// Takes the configuration rather than the words, so the line drawn at startup and the line
    /// `/status` shows an hour later are one decision and not two. Given a string, this is a place
    /// a caller could hand the opening screen any wording at all and no test would notice.
    pub fn on_tier(mut self, config: &bravebot_config::Config) -> Self {
        self.tier = crate::status::configured_tier(config).to_string();
        self
    }

    /// Open the session in bypass, because `--dangerously-skip-permissions` asked for it.
    ///
    /// The rung is on the ladder either way, so this only chooses where the session starts and
    /// records that it started there, which is what makes the line under the box name asking once
    /// the key leaves bypass (MODE-5).
    pub fn starting_in_bypass(mut self) -> Self {
        self.began_in_bypass = true;
        self.permission_mode
            .set(bravebot_agent::PermissionMode::Bypass);
        self
    }

    /// Take bypass off the ladder, because a settings layer wrote `permissions.bypassUnreachable`.
    ///
    /// A session already in bypass is put back to asking, and the person is told, since the mode
    /// was theirs and the line under the box would otherwise change without a reason. One-way: a
    /// later `/cd` into a checkout that wrote nothing does not put the rung back (PERM-17).
    pub fn make_bypass_unreachable(&mut self) {
        self.bypass_reachable = false;
        if self.permission_mode.get() == bravebot_agent::PermissionMode::Bypass {
            self.permission_mode
                .set(bravebot_agent::PermissionMode::Ask);
            self.note(t!(session_bypass_made_unreachable));
        }
    }

    /// How much this session asks before it acts.
    pub fn permission_mode(&self) -> bravebot_agent::PermissionMode {
        self.permission_mode.get()
    }

    /// A handle on the mode that a running turn reads, so a key pressed while it runs applies to it
    /// (MODE-8). Hand this to a turn rather than the value [`Session::permission_mode`] returns.
    pub fn live_permission_mode(&self) -> bravebot_agent::LiveMode {
        self.permission_mode.clone()
    }

    /// Whether the session opened in bypass because the flag that skips permissions was given.
    pub fn began_in_bypass(&self) -> bool {
        self.began_in_bypass
    }

    /// Move to the next mode, and say nothing: the line under the box is the answer.
    ///
    /// A note in the transcript would be a running commentary on a key somebody is pressing to see
    /// what the modes are, and the one place a mode has to be legible is while it is in force.
    pub fn cycle_permission_mode(&mut self) {
        self.permission_mode
            .set(self.permission_mode.get().cycle(self.bypass_reachable));
    }

    /// Move to the next mode the ladder has short of bypassing, which is what a click on the mode
    /// under the box does (MOUSE-14).
    ///
    /// Bypassing answers every later question without asking, and a click is the one input a stray
    /// press or a terminal that reports a position wrongly can make, so a click never grants it. The
    /// key does, from the line that names the mode, and from bypassing both come back to the
    /// first mode.
    pub fn cycle_permission_mode_short_of_bypass(&mut self) {
        self.permission_mode
            .set(self.permission_mode.get().cycle(false));
    }

    /// Set plan mode, from whichever mode the session is in, and say nothing for the reason
    /// [`Session::cycle_permission_mode`] gives.
    ///
    /// Plan mode only narrows what is permitted, so this grants nothing the key would not, and no
    /// word reaches bypassing (MODE-5).
    pub fn enter_plan_mode(&mut self) {
        self.permission_mode
            .set(bravebot_agent::PermissionMode::Plan);
    }

    /// Start a turn on a person's task, after [`Session::enter_plan_mode`] (MODE-12).
    pub fn start_planned(
        &mut self,
        task: &str,
        pasted: Vec<AttachedImage>,
        attached: Vec<Attached>,
    ) -> String {
        self.begin_turn(task.to_string(), (attached, pasted), Vec::new())
    }

    /// Load history from disk and keep writing to it.
    ///
    /// Separate from [`Session::new`] so persistence is a deliberate choice at one call site
    /// rather than a side effect of constructing a session.
    ///
    /// What comes back is not trusted: the file can be edited, so a recalled prompt goes into the
    /// input box for the user to read and submit. That keystroke is what makes it trusted, exactly
    /// as typing it would have been.
    ///
    /// Not the model or the effort level, which a settings file may also name and which are
    /// therefore settled by [`Session::adopt_model`] and [`Session::adopt_effort`], the way the
    /// style of editing is: reading the record here as well would put the rule that ranks the two
    /// in two places.
    pub fn with_stored_history(mut self) -> Self {
        self.history =
            crate::history::History::from_entries(bravebot_session::store::load_history());
        self.history.set_project(self.project());
        self.persist = true;
        self
    }

    /// The model to request, or `None` to use the configured default.
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    /// The model the turn in flight was sent with, which is [`Session::model`] when none is running.
    pub fn model_of_the_turn(&self) -> Option<&str> {
        match self.a_turn_is_running() {
            true => self.turn_model.as_deref(),
            false => self.model(),
        }
    }

    /// Draw the models this configuration names by those names, as [`crate::status::model_label`]
    /// gives them.
    pub fn with_model_labels(mut self, config: &bravebot_config::Config) -> Self {
        self.model_labels = crate::status::model_labels(config);
        self
    }

    /// What to call the chosen model on screen: the name the configuration gave it, or the request
    /// name where it gave none.
    pub fn model_label(&self) -> Option<&str> {
        let model = self.model.as_deref()?;
        let named = self.model_labels.iter().find(|(id, _)| id == model);
        Some(named.map_or(model, |(_, label)| label.as_str()))
    }

    /// Record the model the user picked, keeping it for later sessions.
    ///
    /// Written through to disk only for a session that persists, which is the same rule history
    /// follows and for the same reason: a test must not rewrite the developer's own choice. A tier's
    /// model is written as the tier word (BACKEND-47).
    pub fn choose_model(&mut self, model: impl Into<String>, config: &bravebot_config::Config) {
        let model = model.into();
        if self.persist {
            bravebot_session::store::record_model(config, &model);
        }
        self.model = Some(model);
    }

    /// Put `model` in force for this session, over every saved pick and file, and record nothing.
    ///
    /// The command line's own choice (CLI-31): it describes this start and no other, so unlike
    /// [`Session::choose_model`] it does not rewrite the pick the next session opens on.
    pub fn name_model(&mut self, model: String) {
        self.model = Some(model);
    }

    /// The variant `/variant` named, or `None` where none is picked.
    pub fn variant(&self) -> Option<&str> {
        self.variant.as_deref()
    }

    /// Keep the variant a person picked, or drop the pick where `None`. Not recorded anywhere.
    pub fn choose_variant(&mut self, name: Option<String>) {
        self.variant = name;
    }

    /// The model `/advisor` named, resolved, or `None` where it named none.
    pub fn advisor(&self) -> Option<&str> {
        self.advisor.as_deref()
    }

    /// The session's spend limit, which each turn it starts is held to.
    pub fn spend_limit(&self) -> &bravebot_agent::SpendLimit {
        &self.spend_limit
    }

    /// Start the session under the limit the settings name, if they name one.
    pub fn adopt_limit(&mut self, settings: &bravebot_config::Settings) {
        self.spend_limit.set(settings.limit());
    }

    /// The output style `/style` chose, or `None` where it chose none.
    pub fn style(&self) -> Option<&bravebot_agent::styles::Style> {
        self.style.as_ref()
    }

    /// Only a name, which each turn resolves again, so it can name a skill that has since gone.
    pub fn skill_mode(&self) -> Option<&str> {
        self.skill_mode.as_deref()
    }

    /// Keep this skill applied to every later turn, or `None` to leave the mode (SKILL-21).
    pub fn choose_skill_mode(&mut self, name: Option<String>) {
        self.skill_mode = name;
    }

    /// Put back the mode a resumed record held, and say so, since the transcript shown has no line
    /// of its own for it.
    pub fn resume_skill_mode(&mut self, name: &str) {
        self.skill_mode = Some(name.to_string());
        self.note(t!(session_mode_resumed, skill = name));
    }

    /// The names of the project's styles offered after the others, as last read.
    pub fn project_styles(&self) -> &[String] {
        &self.project_styles
    }

    /// Hold the names of the project's styles, read once each time a line starts with `/style`.
    ///
    /// Read where the trust map is in hand, which the command that takes the name is not, and not
    /// while a turn runs, which holds it. `resolve` is called once per such line rather than once
    /// per key, and a name held from earlier is what a pick made during a turn is taken on.
    pub fn settle_styles(&mut self, resolve: impl FnOnce() -> Vec<String>) {
        let typing = !self.shell
            && self.status != Status::Working
            && crate::command_table::command_typed(&self.input)
                == Some(crate::command_table::STYLE_COMMAND);
        match (typing, self.project_styles_read) {
            (true, false) => {
                self.project_styles = resolve();
                self.project_styles_read = true;
            }
            (false, true) if self.status != Status::Working => self.project_styles_read = false,
            _ => {}
        }
    }

    /// A session that holds `names` as its project's styles, for a test of what takes them.
    #[cfg(test)]
    pub(crate) fn holding_project_styles(mut self, names: &[&str]) -> Self {
        self.project_styles = names.iter().map(|name| name.to_string()).collect();
        self
    }

    /// Open every later turn's system prompt with this style, or `None` to clear the pick.
    pub fn choose_style(&mut self, style: Option<bravebot_agent::styles::Style>) {
        self.style = style;
    }

    /// Name the model the planner may consult from the next turn on, or `None` to drop the choice.
    ///
    /// Not recorded anywhere: a session that is resumed starts with the setting's advisor, if any.
    pub fn choose_advisor(&mut self, model: Option<String>) {
        self.advisor = model;
    }

    /// How hard to think, or `None` to leave the service its own default.
    ///
    /// What the person chose, which is not always what a request carries. For reporting the choice
    /// and for opening the picker on it.
    pub fn effort(&self) -> Option<Effort> {
        self.effort
    }

    /// The level a turn will actually send, which is nothing where the model does not read one.
    ///
    /// Sending a level to a model whose roster row says it reads none is a field that is dropped at
    /// the far end, so the choice is kept and the request is not given it. A ChatGPT model is sent
    /// the level the session began with and not the one chosen since, so the choice applies to the
    /// next session and to every other model (CHATGPT-10).
    pub fn effort_in_force(&self) -> Option<Effort> {
        let level = match self.model_is_account {
            true => self.account.effort(),
            false => self.effort,
        };
        level.filter(|_| self.model_reads_effort)
    }

    /// Begin the session the ChatGPT account sees, at the level in force now.
    ///
    /// Called once the level is settled, where the session starts, and where `/clear` and a handoff
    /// begin another. A level chosen after that is not this session's for the account.
    pub fn begin_account_session(&mut self) {
        self.account = bravebot_agent::chatgpt::AccountSession::begin_with(self.effort);
    }

    /// Whether the model in force is one of the ChatGPT account's.
    pub fn model_is_account(&self) -> bool {
        self.model_is_account
    }

    /// The level the session began with, where the model in force is the account's, which is the
    /// level that account is sent. `None` for any other model, and `Some(None)` where the session
    /// began with no level.
    pub fn account_level(&self) -> Option<Option<Effort>> {
        self.model_is_account.then(|| self.account.effort())
    }

    /// Record whether the model now in force is one of the ChatGPT account's.
    pub fn note_model_is_account(&mut self, account: bool) {
        self.model_is_account = account;
    }

    /// What this session carries into its requests to the ChatGPT account, which every request it
    /// makes is made in: a turn, a summary, a question beside the work, a goal check, a commit
    /// message and an MCP server's prompt.
    pub fn account_session(&self) -> &bravebot_agent::chatgpt::AccountSession {
        &self.account
    }

    /// Whether the model in force reads a level at all.
    ///
    /// For saying so where it does not: a level reported as in force while the service discards it
    /// is the interface telling somebody they bought something they did not.
    pub fn model_reads_effort(&self) -> bool {
        self.model_reads_effort
    }

    /// Record what the roster said about the model now in force.
    pub fn note_model_reads_effort(&mut self, reads: bool) {
        self.model_reads_effort = reads;
    }

    /// Record the level the user picked, keeping it for later sessions.
    ///
    /// `None` is a choice too: it puts the session back to sending no level at all, which is what
    /// somebody who has never picked one is already doing. Written through to disk only for a
    /// session that persists, the same rule the model follows and for the same reason.
    pub fn choose_effort(&mut self, effort: Option<Effort>) {
        if self.persist {
            bravebot_session::store::save_effort(effort);
        }
        self.effort = effort;
    }

    /// The language `/language` last saved, for saying so.
    ///
    /// Read only for a session that persists, which is the rule every read of a saved pick here
    /// follows: a test must not be handed the developer's own preference.
    pub fn saved_language(&self) -> Option<String> {
        self.persist
            .then(bravebot_session::store::load_language)
            .flatten()
    }

    /// Keep the language the person chose for the next start, or forget the choice where `None`,
    /// and say whether that was done.
    ///
    /// Nothing in this session changes with it, since the interface is in one language for the
    /// whole process: a session that persists nothing has nowhere to keep the choice, and answers
    /// that it kept none.
    pub fn choose_language(&mut self, tag: Option<&str>) -> bool {
        self.persist && bravebot_session::store::save_language(tag)
    }

    /// Put `effort` in force for this session over every saved pick and file, and record nothing,
    /// for the reason [`Session::name_model`] records no model.
    pub fn name_effort(&mut self, effort: Effort) {
        self.effort = Some(effort);
    }

    /// Settle the style of editing for this session, given what a settings file said.
    ///
    /// A choice the person made outranks the file, the same rule the model follows: a recorded choice
    /// outlives the session that made it, and a file read afterwards would undo what somebody had just
    /// asked for. With no choice recorded the file answers, and with neither it is the box everybody
    /// has.
    ///
    /// A word that names no style is no choice at all, whichever of the two spelled it: both are read
    /// from a file somebody may have edited by hand, so both are resolved here by the same rule rather
    /// than each being trusted where it came from. A corrupt recorded word therefore leaves the file
    /// answering, and a mistyped file leaves the ordinary box. Nothing is said about either: a settings
    /// file is reported by `doctor`, and a session that refused to start over a mistyped editing
    /// preference would be worse than one that ignores it.
    ///
    /// The recorded choice is read only for a session that persists, which is the rule every write
    /// here follows: a test must not be handed the developer's own preference, or what the box does
    /// under it would depend on the machine it ran on.
    pub fn adopt_editing(&mut self, configured: Option<&str>) {
        let stored = self
            .persist
            .then(bravebot_session::store::load_editing)
            .flatten();
        self.editing = stored
            .as_deref()
            .and_then(crate::vim::Editing::named)
            .or_else(|| configured.and_then(crate::vim::Editing::named))
            .unwrap_or_default();
    }

    /// Settle the model this session opens on, given the settings in force.
    ///
    /// The saved pick where [`bravebot_session::store::model`] leaves it in force, and otherwise
    /// `None`, which is the configured model: a checkout's settings that outrank the pick named
    /// the model the configuration already resolved to (BACKEND-11). The rule is in the store for
    /// the reason [`Session::adopt_effort`]'s is.
    ///
    /// The pick is read only for a session that persists, for the reason given there. A pick
    /// [`bravebot_agent::backend::pick`] sets aside is named in the transcript (BACKEND-47).
    pub fn adopt_model(
        &mut self,
        settings: &bravebot_config::Settings,
        config: &bravebot_config::Config,
    ) {
        let recorded = self
            .persist
            .then(bravebot_session::store::load_model)
            .flatten();
        let pick = bravebot_agent::backend::pick(
            config,
            bravebot_session::store::model(recorded, settings),
        );
        match &pick {
            bravebot_agent::backend::Pick::SetAside(model) => {
                self.note(t!(session_model_pick_set_aside, model = model.as_str()));
            }
            bravebot_agent::backend::Pick::Refused { recorded, reason } => {
                self.note(t!(
                    session_model_pick_refused,
                    model = recorded.as_str(),
                    reason = reason.as_str()
                ));
            }
            bravebot_agent::backend::Pick::Absent | bravebot_agent::backend::Pick::InForce(_) => {}
        }
        self.model = pick.into_model();
    }

    /// Settle how hard this session asks the model to think, given the settings in force.
    ///
    /// The rule is [`bravebot_session::store::effort`] and the whole of it is there, so the terminal,
    /// a session in lines and a one-shot run cannot come to disagree about which of the two answers
    /// (BACKEND-43). What that decides is the level the picker opens on and the level a turn sends;
    /// a level the model in force reads none of is still not sent, by
    /// [`Session::effort_in_force`], and is still not forgotten.
    ///
    /// Nothing is written: a level a file named is not a pick, and recording one would make reading
    /// a settings file once enough to outlive it.
    ///
    /// The recorded pick is read only for a session that persists, the rule
    /// [`Session::adopt_editing`] follows: a test must not be handed the developer's own choice, or
    /// what it asks for would depend on the machine it ran on.
    pub fn adopt_effort(&mut self, settings: &bravebot_config::Settings) {
        let recorded = self
            .persist
            .then(bravebot_session::store::load_effort)
            .flatten();
        self.effort = bravebot_session::store::effort(recorded, settings);
    }

    /// Settle whether a check that finds nothing may promote a slot without anybody being asked.
    ///
    /// The three routes resolved into one answer, by
    /// [`bravebot_core::vetting::auto`], which is the whole of the rule. `asked` is the
    /// command line's, taken as an argument rather than read here so a test decides it; `configured`
    /// is the `vetting.auto` key from the home settings layer.
    ///
    /// The recorded choice is read only for a session that persists, which is the rule
    /// [`Session::adopt_editing`] follows and matters more here: a test, and a session asked to
    /// leave nothing behind, must not pick up a developer's standing answer to whether somebody is
    /// asked before content nobody vouched for reaches the planner.
    pub fn adopt_vetting(&mut self, asked: bool, configured: Option<bool>) {
        let chosen = self
            .persist
            .then(bravebot_session::store::load_vetting)
            .flatten();
        self.vetting = bravebot_core::vetting::auto(asked, chosen, configured);
    }

    /// Whether a check that finds nothing may promote a slot without the person being asked.
    pub fn auto_vetting(&self) -> bool {
        self.vetting
    }

    /// Record the answer about auto-vetting the person gave at a prompt.
    ///
    /// Written through to disk only for a session that persists, the same rule the editing style
    /// follows and for the same reason: a test, and a session asked to leave nothing behind, must
    /// not rewrite the developer's own answer.
    ///
    /// Takes effect from the next turn. The turn in flight keeps the setting it began with, so a
    /// question already on the screen is not withdrawn from under the person answering it.
    pub fn choose_vetting(&mut self, auto: bool) {
        if self.persist {
            bravebot_session::store::save_vetting(auto);
        }
        self.vetting = auto;
    }

    /// Adopt configured keybindings from settings.
    pub fn adopt_keybindings(&mut self, configured: &std::collections::BTreeMap<String, String>) {
        self.bindings = crate::keybindings::Keybindings::from_map(configured);
    }

    /// Adopt the keys of the Sessions screen from settings (SCREEN-14). What a settings file asked
    /// for and could not have is kept, to be said when the screen opens.
    pub fn adopt_sessions_keys(&mut self, configured: &std::collections::BTreeMap<String, String>) {
        let (keys, refused) = crate::sessions_keys::SessionsKeys::from_map(configured);
        self.sessions_keys = keys;
        self.sessions_keys_refused = refused;
    }

    /// The keys the Sessions screen answers in this session, and what was refused.
    pub fn sessions_keys(
        &self,
    ) -> (
        &crate::sessions_keys::SessionsKeys,
        &[crate::sessions_keys::Refused],
    ) {
        (&self.sessions_keys, &self.sessions_keys_refused)
    }

    /// The active keybindings for this session.
    pub fn bindings(&self) -> &crate::keybindings::Keybindings {
        &self.bindings
    }

    /// Record how many rows one wheel event moves the view by, from `tui.wheelRows`.
    ///
    /// Clamped rather than refused, which is the decision [`bravebot_agent::exec`] takes about a
    /// deadline and for the same reason: the movement a figure asks for is visible the moment the
    /// wheel is turned, so a count held to the range says so on the screen. A count nobody named
    /// leaves [`WHEEL_ROWS`] in force.
    pub fn adopt_wheel_rows(&mut self, configured: Option<usize>) {
        if configured.is_some() {
            self.wheel_rows = wheel_rows_named(configured);
        }
    }

    /// Record how long a planner's question waits for a key before it is closed, from
    /// `askIdleSeconds` (ASK-9). Absence waits for as long as it takes; what counts as usable is
    /// `Settings::parse`'s rule alone.
    pub fn adopt_ask_idle(&mut self, seconds: Option<u64>) {
        self.ask_idle = seconds.map(Duration::from_secs);
    }

    /// How long `asking` waits with no key pressed before it is closed, where it may be at all.
    ///
    /// Only what the driver marked as the planner's own question is closed on a clock, and the
    /// time is the person's setting: a plan, a spend limit and a repeated call wait for the person
    /// whatever the settings say, and nothing a question says is read here.
    pub fn ask_idle_for(&self, asking: &bravebot_core::ask::Asking) -> Option<Duration> {
        self.ask_idle.filter(|_| asking.closes_when_idle)
    }

    /// Record whether the interface captures the mouse, from `tui.mouse`. Only a `false` turns it
    /// off, so an absent key leaves the capture on.
    pub fn adopt_mouse(&mut self, configured: Option<bool>) {
        self.mouse = configured != Some(false);
    }

    /// Record whether a right click opens the small menu, from `tui.contextMenu`. Only a `false`
    /// turns it off, so an absent key leaves it on (MOUSE-52).
    pub fn adopt_context_menu(&mut self, configured: Option<bool>) {
        self.context_menu = configured != Some(false);
    }

    /// Whether a right click opens the small menu where there is one to open.
    pub fn context_menu_on(&self) -> bool {
        self.context_menu
    }

    /// Whether the interface asks the terminal to report the mouse. Where it does not, the
    /// terminal's own selection and wheel work as they do outside a session.
    pub fn mouse_captured(&self) -> bool {
        self.mouse
    }

    /// Name the directory a line's first word is looked up in. Left unset, no line is one (CMD-17).
    pub fn adopt_prompt_files(&mut self, directory: Option<std::path::PathBuf>) {
        self.prompt_files = directory;
    }

    /// How many rows one wheel event moves the view by.
    pub fn wheel_rows(&self) -> u16 {
        self.wheel_rows
    }

    /// Record the style of editing the person chose, keeping it for later sessions.
    ///
    /// Written through to disk only for a session that persists, the same rule the model and the
    /// effort level follow and for the same reason: a test must not rewrite the developer's own
    /// choice.
    ///
    /// The mode comes back to INSERT whichever style was chosen, because the choice is made away from
    /// the box: coming back to one that takes the next letter as an instruction is not what somebody
    /// who has just turned vi editing on expects. It is also the only sound answer for the other
    /// direction, NORMAL not being a state the ordinary box has.
    pub fn choose_editing(&mut self, editing: crate::vim::Editing) {
        if self.persist {
            bravebot_session::store::save_editing(editing.as_str());
        }
        let changed = self.editing != editing;
        self.editing = editing;
        // The selection goes with the mode that showed it, the way Escape out of VISUAL mode
        // abandons it. INSERT mode has no stretch to act on, and the ordinary box has nowhere to
        // draw one: what is left otherwise is a reversed run of characters in a box whose keys
        // cannot account for it. Let go of while the mode still says which kind it was.
        self.let_go_of_the_selection();
        self.mode = crate::vim::Mode::Insert;
        // An instruction waiting for its next key goes with the mode it was typed in, for the same
        // reason: INSERT mode would draw it beside a box that is typing its letters. A count is the
        // front of such an instruction and goes with it.
        self.half_typed = None;
        self.count = None;
        self.held_count = None;
        // A step taken under one style answers to keys the other does not press, so a style change
        // leaves nothing to undo, the way INPUT-28 already says for choosing the style already chosen.
        if changed {
            self.inserting = None;
            self.undo.clear();
            self.ordinary_run_open = false;
        }
    }

    /// Open the info panel where it was left open last time.
    ///
    /// Read only for a session that persists, as the editing style is, so a test is not handed the
    /// developer's own choice. Closed where nothing was recorded.
    pub fn adopt_panel(&mut self) {
        self.panel = self
            .persist
            .then(bravebot_session::store::load_panel)
            .flatten()
            .unwrap_or(false);
        self.panel_width = self
            .persist
            .then(bravebot_session::store::load_panel_width)
            .flatten()
            .unwrap_or(crate::panel::WIDTH);
    }

    /// The columns the info panel takes on a frame `columns` wide, border included: the width the
    /// person dragged it to, held between its minimum and what leaves the transcript its own.
    pub fn panel_columns(&self, columns: u16) -> u16 {
        if self.files.is_some() {
            return crate::files_mode::columns(columns);
        }
        crate::panel::clamped(self.panel_width, columns)
    }

    /// Whether a press here is on the panel's border, the column a drag resizes it by.
    fn on_panel_border(&self, row: u16, column: u16) -> bool {
        // The files state is half the terminal, which dragging does not change (FILES-4).
        self.files.is_none() && self.on_the_panel_border_column(row, column)
    }

    /// Whether a cell is in the column the panel draws its border in, in every state of the panel.
    /// The border is no text, so a press on it begins no selection (MOUSE-2).
    pub(super) fn on_the_panel_border_column(&self, row: u16, column: u16) -> bool {
        self.laid.panes.iter().any(|(pane, rect)| {
            *pane == crate::select::Pane::Panel
                && rect.x == column
                && rect.y <= row
                && row < rect.bottom()
        })
    }

    /// What a click at this cell acts on in the info panel, if the panel drew a row there, as the
    /// last frame recorded it: the question the tests of the panel put to the registry a click is
    /// answered from.
    #[cfg(test)]
    pub fn panel_target_at(&self, row: u16, column: u16) -> Option<crate::panel::Target> {
        match self.target_at(row, column)?.hit {
            crate::state::Hit::Panel(target) => Some(target),
            _ => None,
        }
    }

    /// The address the row `target` of the info panel opens, if it is the pull request or issue row.
    ///
    /// The one the person gave with `/pr` or `/issue` (PANEL-12), as it was set and not the text the
    /// row draws for it, which may be the shorthand or cut. A row is a target only for an address the
    /// opener takes, which the panel decides where it lays the rows out.
    pub fn panel_link(&self, target: crate::panel::Target) -> Option<&str> {
        use crate::panel::Target;
        match target {
            Target::PullRequest => self.identity.pull_request.as_deref(),
            Target::Issue => self.identity.issue.as_deref(),
            Target::Name
            | Target::Model
            | Target::Effort
            | Target::Context
            | Target::Servers
            | Target::Languages
            | Target::File(_)
            | Target::CloseFiles
            | Target::PreviousEdit
            | Target::NextEdit
            | Target::Files
            | Target::Hide
            | Target::Task(_)
            | Target::Detail(_) => None,
        }
    }

    /// What the hint drawn at this cell of the row under the box stands for, as the last frame drew
    /// it, where the key it names does something now (MOUSE-14, MOUSE-48): the question the tests of
    /// the hints put to the registry a click is answered from.
    #[cfg(test)]
    pub fn footer_at(&self, row: u16, column: u16) -> Option<crate::state::Footer> {
        match self.target_at(row, column)?.hit {
            crate::state::Hit::Footer(footer) => Some(footer),
            _ => None,
        }
    }

    /// Begin dragging the panel's border where the button went down, and say whether it was
    /// there. A press that is not leaves the session untouched.
    pub fn begin_panel_drag(&mut self, row: u16, column: u16) -> bool {
        if !self.on_panel_border(row, column) {
            return false;
        }
        self.panel_drag = true;
        self.clear_selection();
        true
    }

    /// Whether a drag that began on the panel's border is still held.
    pub fn dragging_panel(&self) -> bool {
        self.panel_drag
    }

    /// Record where a mouse report put the pointer. Every kind of report says, so the position is
    /// as current as the last thing the terminal sent.
    pub(crate) fn note_pointer(&mut self, row: u16, column: u16) {
        self.pointer = Some((row, column));
    }

    /// The shape the pointer should have: the one for a border that can be dragged while it is over
    /// the panel's border or such a drag is held, and the terminal's own otherwise. Read against the
    /// layout as it is now, so a border that moved or a panel that closed under a pointer at rest
    /// is not where the pointer still is.
    pub(crate) fn pointer_shape(&self) -> crate::pointer::Shape {
        let over = self
            .pointer
            .is_some_and(|(row, column)| self.on_panel_border(row, column));
        if over || self.panel_drag {
            crate::pointer::Shape::ResizeColumns
        } else {
            crate::pointer::Shape::Default
        }
    }

    /// Move the border to `column`: the panel is whatever is to its right.
    pub fn drag_panel_to(&mut self, column: u16) {
        let columns = self.laid.columns;
        self.panel_width = crate::panel::clamped(columns.saturating_sub(column), columns);
    }

    /// End the drag and keep the width it reached for the next session.
    pub fn end_panel_drag(&mut self) {
        self.panel_drag = false;
        if self.persist {
            bravebot_session::store::save_panel_width(self.panel_width);
        }
    }

    /// Take the answer an earlier session gave to `/caffeinate`'s explanation.
    ///
    /// Read only for a session that persists, as the panel is, so a test is not handed the
    /// developer's own answer.
    pub fn adopt_caffeinate(&mut self) {
        if self.persist && bravebot_session::store::load_caffeinate_confirmed() {
            self.caffeinate.confirm();
        }
    }

    /// Carry out `/caffeinate`, and take or release the hold it changes at once.
    pub fn toggle_caffeinate(&mut self) {
        use crate::caffeinate::Toggled;
        match self.caffeinate.toggle() {
            Toggled::Explained => self.note(t!(caffeinate_explained)),
            Toggled::Confirmed => {
                if self.persist {
                    bravebot_session::store::save_caffeinate_confirmed();
                }
                self.note(t!(caffeinate_on));
            }
            Toggled::On => self.note(t!(caffeinate_on)),
            Toggled::Off => self.note(t!(caffeinate_off)),
        }
        self.keep_awake();
    }

    /// Whether the session has work that sleeping would stop: a turn in flight, or a loop with a
    /// tick to come, which a `schedule_next` wait is.
    ///
    /// A background job ends with its turn, so the turn covers it. A watch is not counted: it waits
    /// on a file with no end in view, and holding a laptop awake for that is holding it awake for
    /// as long as the session is open.
    pub fn work_pending(&self) -> bool {
        self.status == Status::Working || !self.looping.is_empty()
    }

    /// Hold the machine awake where `/caffeinate` is on and work is pending, and release it
    /// otherwise. A hold that could not be kept is said, and `/caffeinate` is then off.
    pub fn keep_awake(&mut self) {
        use crate::caffeinate::Unavailable;
        let busy = self.work_pending();
        match self.caffeinate.follow(busy) {
            Ok(()) => {}
            Err(Unavailable::Missing(error)) => self.note(t!(
                caffeinate_unavailable,
                program = crate::caffeinate::PROGRAM,
                reason = error.to_string()
            )),
            Err(Unavailable::Ended) => {
                self.note(t!(caffeinate_ended, program = crate::caffeinate::PROGRAM))
            }
        }
    }

    /// Whether the person has the info panel open, whether or not the last frame had room for it.
    pub fn panel_open(&self) -> bool {
        self.panel || self.files.is_some()
    }

    /// Open or close the info panel.
    ///
    /// On a terminal too narrow to draw it, a press to open changes nothing and a note says why.
    /// Opening a panel the screen cannot show would leave the press looking as if it did nothing,
    /// and the next widening would bring back a panel nobody remembered opening. A press to close
    /// always closes, so a panel left open before the terminal was narrowed can be put away.
    pub fn toggle_panel(&mut self) {
        // The key hides the panel from either state, and the state goes with it (FILES-3).
        if self.files.take().is_some() {
            self.panel = false;
            if self.persist {
                bravebot_session::store::save_panel(false);
            }
            return;
        }
        if !self.panel && !crate::panel::fits(self.laid.columns) {
            self.note(t!(panel_too_narrow, columns = crate::panel::NARROWEST));
            return;
        }
        self.panel = !self.panel;
        if self.persist {
            bravebot_session::store::save_panel(self.panel);
        }
    }

    /// Record what the info panel names the session by. True where an open panel now has
    /// something to show that the last frame did not, so the caller draws another.
    pub fn identify(&mut self, name: &str, directory: String, branch: Option<&str>) -> bool {
        let identity = Identity {
            name: name.to_string(),
            directory,
            branch: branch.map(str::to_string),
            ..self.identity.clone()
        };
        self.reidentify(identity)
    }

    /// Record the links the info panel shows, answering as [`Session::identify`] does.
    pub fn link(&mut self, issue: Option<&str>, pull_request: Option<&str>) -> bool {
        let identity = Identity {
            issue: issue.map(str::to_string),
            pull_request: pull_request.map(str::to_string),
            ..self.identity.clone()
        };
        self.reidentify(identity)
    }

    /// Record the repository a `#<number>` in a reply links into, from the checkout the session is
    /// in, or none where its `origin` is not on GitHub. Read where the session opens and where it
    /// moves to another directory, since reading it opens a file.
    pub fn adopt_repository(&mut self, repository: Option<crate::issue_link::Repository>) {
        self.repository = repository;
    }

    /// The repository a `#<number>` in a reply links into, where there is one.
    pub fn repository(&self) -> Option<&crate::issue_link::Repository> {
        self.repository.as_ref()
    }

    /// Record the id the info panel shows, answering as [`Session::identify`] does.
    pub fn name_id(&mut self, id: Option<&str>) -> bool {
        let identity = Identity {
            id: id.map(str::to_string),
            ..self.identity.clone()
        };
        self.reidentify(identity)
    }

    fn reidentify(&mut self, identity: Identity) -> bool {
        let changed = self.identity != identity;
        self.identity = identity;
        changed && self.panel
    }

    /// Turn follow-up suggestions on or off for the session, from the `suggestions` setting
    /// (INPUT-43). Read once at startup, so what is paid for does not change under a turn.
    pub fn adopt_suggestions(&mut self, on: bool) {
        self.suggesting = on;
        if !on {
            self.suggestion = None;
        }
    }

    /// Whether the session asks for a suggestion when a turn ends.
    pub fn suggests(&self) -> bool {
        self.suggesting
    }

    /// The suggestion on offer in the empty box, if there is one.
    pub fn suggestion(&self) -> Option<&str> {
        self.suggestion.as_deref()
    }

    /// Put `text` in the empty box as a suggestion, where the box is still empty and idle.
    ///
    /// A suggestion that arrives after the person has begun typing, or sent something, is not
    /// offered: it was a guess about a state that has passed.
    pub fn offer_suggestion(&mut self, text: String) {
        if self.suggesting
            && self.status == Status::Idle
            && self.input.is_empty()
            && !self.shell
            && !self.searching_history()
        {
            self.suggestion = Some(text);
        }
    }

    /// Take the suggestion away, for a key that is not the one that accepts it.
    pub fn dismiss_suggestion(&mut self) {
        self.suggestion = None;
    }

    /// Drop a suggestion the box has stopped being empty for, or the session stopped being idle for.
    pub fn settle_suggestion(&mut self) {
        if self.suggestion.is_some() && (!self.input.is_empty() || self.status != Status::Idle) {
            self.suggestion = None;
        }
    }

    /// Move the suggestion into the box as text to edit. Nothing sends it, and one that begins
    /// with `/` is words in the box like any other until Enter is pressed on it (CMD-1).
    ///
    /// Whether there was one to take.
    pub fn accept_suggestion(&mut self) -> bool {
        match self.suggestion.take() {
            Some(text) if self.input.is_empty() => {
                self.set_input(text);
                true
            }
            _ => false,
        }
    }

    /// Charge what a suggestion's request cost to the turn it followed, outside any turn's own
    /// figures and still in the session's total, as an aside is.
    pub fn charge_suggestion(&mut self, usage: bravebot_aichat::protocol::Usage) {
        self.charge(usage.total());
        self.charged_split(usage.total(), usage.completion_tokens);
    }
}

/// How many rows one wheel event moves a list by, given what `tui.wheelRows` said: the count held
/// to the range, and [`WHEEL_ROWS`] where nobody named one. For the lists that run before a session
/// exists, which read the setting themselves.
pub fn wheel_rows_named(configured: Option<usize>) -> u16 {
    let Some(rows) = configured else {
        return WHEEL_ROWS;
    };
    let rows = u16::try_from(rows).unwrap_or(WHEEL_ROWS_CEILING);
    rows.clamp(WHEEL_ROWS_FLOOR, WHEEL_ROWS_CEILING)
}
