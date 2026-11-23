//! Which of a plugin's hooks may fire now (PLUGIN-7).
//!
//! A hook a person wrote in `hooks.json` fires as [`crate::hooks`] says. A hook a plugin carries is a
//! program somebody else wrote, so it fires only after the person has said yes to a question naming
//! the plugin, the moment and the argv as [PLUGIN-13] expands it. [`PluginHooks::firing`] is the one
//! place that decides, and every moment a plugin hook can fire at asks it.
//!
//! # What decides
//!
//! Only three things, none of them read out of the plugin: whether anybody can be asked, a yes the
//! person gave earlier that is recorded against the same digest, and the answer to the question put
//! now. The plugin's files supply the words the question draws. They choose which questions are
//! asked, since the moment a hook names selects it, and they never answer one.
//!
//! # What is asked, and how often
//!
//! A hook is asked about once in a session: an answer is kept in [`Answers`], which the session's
//! [`crate::chatgpt::AccountSession`] holds, so every turn of the session shares it and a new
//! session starts with none. A yes is also written to `~/.bravebot/hooks-approved`
//! ([`bravebot_config::hooks_approved`]) unless the session leaves nothing behind, and a no is
//! written nowhere, so the next session asks again.
//!
//! [PLUGIN-13]: ../../../docs/specs/plugins.md#PLUGIN-13

use crate::confirm::{Confirmer, Decision, HookRequest};
use crate::plugins;
use bravebot_config::hooks::{Hook, Hooks, Moment};
use bravebot_config::hooks_approved::{self, Approvals, Digest};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// What a session answered about plugin hooks, by the digest the question was about.
///
/// A yes and a no both stay, so a no is not asked again in the session. Cloning the session's
/// handle shares this, and nothing here is written to disk.
#[derive(Debug, Default)]
pub struct Answers {
    said: Mutex<BTreeMap<Digest, bool>>,
    /// The plugins whose directory was found not to match its record when a hook was considered.
    hooks: Told,
    /// The plugins whose directory was found not to match its record when a server was started,
    /// offered or called.
    servers: Told,
}

/// Plugins found not to match their record: each is queued the first time only, until it is taken.
#[derive(Debug, Default)]
struct Told {
    /// Every plugin queued in this session.
    seen: Mutex<BTreeSet<String>>,
    /// Plugins queued that no front end has been told about yet.
    untold: Mutex<Vec<String>>,
}

impl Told {
    fn note(&self, plugin: &str) {
        let first = self
            .seen
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .insert(plugin.to_string());
        if first {
            self.untold
                .lock()
                .unwrap_or_else(|held| held.into_inner())
                .push(plugin.to_string());
        }
    }

    fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.untold.lock().unwrap_or_else(|held| held.into_inner()))
    }
}

impl Answers {
    /// Note that `plugin` no longer matches its record. The first time in a session for a plugin
    /// it is queued for [`Answers::take_stopped`]; after that it is not, so a person is told once
    /// per plugin per session.
    fn stop(&self, plugin: &str) {
        self.hooks.note(plugin);
    }

    /// The plugins found not to match their record since this was last called, to be named in a
    /// notice. Each plugin comes out once in the life of the session.
    pub fn take_stopped(&self) -> Vec<String> {
        self.hooks.take()
    }

    /// The sentence for each plugin [`Answers::take_stopped`] yields, for a caller with no reporter
    /// to say it to (a session moment, which no turn holds).
    ///
    /// The name is one `config::plugin::is_valid_name` accepted at install, so it holds only
    /// letters, digits and hyphens and carries no line break or escape.
    pub fn stopped_sentences(&self) -> Vec<String> {
        self.take_stopped()
            .into_iter()
            .map(|plugin| bravebot_i18n::t!(plugin_hook_stopped_changed, plugin = plugin))
            .collect()
    }

    /// [`Answers::stop`] for a plugin whose server was refused, queued for
    /// [`Answers::take_stopped_servers`]. A plugin whose hook was named is still named once for its
    /// servers, since the sentences differ.
    pub(crate) fn stop_server(&self, plugin: &str) {
        self.servers.note(plugin);
    }

    /// The plugins whose servers were refused for not matching their record since this was last
    /// called. Each plugin comes out once in the life of the session.
    pub fn take_stopped_servers(&self) -> Vec<String> {
        self.servers.take()
    }

    fn said(&self, digest: &Digest) -> Option<bool> {
        self.said
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .get(digest)
            .copied()
    }

    fn note(&self, digest: Digest, yes: bool) {
        self.said
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .insert(digest, yes);
    }
}

/// What the answers already given say about one hook, without putting a question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// A yes holds, in this session or recorded, for files that still match the record.
    Fire,
    /// Nothing may fire: a no in this session, a tree that changed, a plugin disabled since.
    Silent,
    /// No answer yet. Only a question to a person can settle it.
    Ask,
}

/// One hook an enabled plugin carries, with what an approval of it is a digest of.
#[derive(Debug, Clone)]
pub(crate) struct Carried {
    plugin: String,
    /// The tree digest the install recorded for the plugin.
    tree: String,
    /// The hook as the reader returns it, with `${PLUGIN_ROOT}` already replaced.
    hook: Hook,
}

impl Carried {
    /// The hook as the reader returns it.
    pub(crate) fn hook(&self) -> &Hook {
        &self.hook
    }

    fn digest(&self) -> Digest {
        Digest::of(
            &self.plugin,
            self.hook.moment().as_str(),
            self.hook.run(),
            &self.tree,
        )
    }
}

/// The hooks the enabled plugins carry, read once when a turn begins.
#[derive(Debug, Clone, Default)]
pub struct PluginHooks {
    home: Option<PathBuf>,
    hooks: Vec<Carried>,
}

impl PluginHooks {
    /// The hooks of every enabled installed plugin under the state directory `home`, in plugin name
    /// order and then in the order each plugin's file lists them.
    ///
    /// None in a safe session, which reads no hooks, none where the platform names no state
    /// directory, and none for a plugin that is disabled (PLUGIN-11) or whose hooks file cannot be
    /// read. A plugin directory in a workspace is never looked at: only `home` is.
    pub fn read(home: Option<&Path>) -> Self {
        let Some(home) = home.filter(|_| !bravebot_core::safe::engaged()) else {
            return Self::default();
        };
        let mut hooks = Vec::new();
        for installed in plugins::list(home).into_iter().filter(|one| one.enabled) {
            let Some(Ok(list)) = plugins::installed_hooks(home, &installed.name) else {
                continue;
            };
            hooks.extend(list.hooks.declared().iter().map(|hook| Carried {
                plugin: installed.name.clone(),
                tree: installed.digest.clone(),
                hook: hook.clone(),
            }));
        }
        Self {
            home: Some(home.to_path_buf()),
            hooks,
        }
    }

    /// Whether any enabled plugin carries a hook, so a turn with none does no more work.
    pub fn is_empty(&self) -> bool {
        self.hooks.is_empty()
    }

    /// The hooks an enabled plugin carries that fire for `moment`, asked about or not, for a
    /// moment that holds a hook until something happens ([`crate::hooks::FileChanged`]). Each is
    /// still put through the one gate before it fires.
    pub(crate) fn entries_for(&self, moment: Moment) -> Vec<Carried> {
        self.hooks
            .iter()
            .filter(|carried| carried.hook.fires(moment, None))
            .cloned()
            .collect()
    }

    /// Whether an enabled plugin carries a hook for `moment` (and `tool`), asked about or not.
    ///
    /// For a step that must not run beside a hook, as a read-ahead of calls must not. It decides
    /// nothing about whether the hook fires.
    pub fn declares(&self, moment: Moment, tool: Option<&str>) -> bool {
        self.hooks
            .iter()
            .any(|carried| carried.hook.fires(moment, tool))
    }

    /// The person's own hooks for `moment` with the plugins' hooks that already have a yes, in this
    /// session or in `hooks-approved`, and no question put.
    ///
    /// For `prompt-shown`, which fires while a question is on screen and so cannot put another. A
    /// hook not yet answered does not fire there, and its question comes at a moment that can ask.
    /// `can_ask` is what [`Confirmer::can_ask_about_a_hook`] said when the turn began: where nobody
    /// can be asked no plugin hook fires, whatever was recorded.
    pub fn joined_without_asking(
        &self,
        own: &Hooks,
        moment: Moment,
        answers: &Answers,
        can_ask: bool,
    ) -> Hooks {
        let approved: Vec<Hook> = self
            .hooks
            .iter()
            .filter(|carried| can_ask && carried.hook.fires(moment, None))
            .filter(|carried| self.answered_yes(carried, answers))
            .map(|carried| carried.hook.clone())
            .collect();
        if approved.is_empty() {
            return own.clone();
        }
        Hooks::from_hooks(own.firing(moment, None).cloned().chain(approved).collect())
    }

    /// Hashes the plugin's directory again and compares it with the digest the install recorded.
    ///
    /// Made before any yes is honoured and before any question is put, every time a hook is
    /// considered, so a file changed after a yes is seen by the next moment (PLUGIN-7). The answer
    /// is used only to refuse: a tree that differs, cannot be hashed or has lost its record stops
    /// the hook, and the plugin is queued to be named once in a notice. No byte of the tree is
    /// read for any other purpose.
    fn tree_intact(&self, carried: &Carried, answers: &Answers) -> bool {
        let Some(home) = self.home.as_deref() else {
            return false;
        };
        // A plugin disabled after the turn read its hooks fires none (PLUGIN-11). Its recorded
        // answers stay, and no notice is given: nothing changed in its files.
        if !plugins::read_record(home, &carried.plugin).is_some_and(|record| record.enabled) {
            return false;
        }
        if plugins::tree_matches_record(home, &carried.plugin).is_ok() {
            return true;
        }
        answers.stop(&carried.plugin);
        false
    }

    /// Whether this hook has a yes, recorded or given in this session, on a tree that still matches
    /// its record.
    fn answered_yes(&self, carried: &Carried, answers: &Answers) -> bool {
        if !self.tree_intact(carried, answers) {
            return false;
        }
        let digest = carried.digest();
        match answers.said(&digest) {
            Some(said) => said,
            None => self
                .home
                .as_deref()
                .is_some_and(|home| Approvals::read(home).approves(&digest)),
        }
    }

    /// The plugins' hooks that fire for `moment` (and `tool`, where the moment is about a call) and
    /// that the person has agreed to, asking about each not yet answered in this session.
    pub fn firing<C: Confirmer + ?Sized>(
        &self,
        moment: Moment,
        tool: Option<&str>,
        answers: &Answers,
        confirmer: &mut C,
    ) -> Vec<Hook> {
        self.hooks
            .iter()
            .filter(|carried| carried.hook.fires(moment, tool))
            .filter(|carried| self.may_fire(carried, answers, confirmer))
            .map(|carried| carried.hook.clone())
            .collect()
    }

    /// The person's own hooks for `moment` followed by the plugins' that may fire, as one list the
    /// moment's firing code runs unchanged.
    ///
    /// The person's own hooks are taken as they are and never asked about.
    pub fn joined_with<C: Confirmer + ?Sized>(
        &self,
        own: &Hooks,
        moment: Moment,
        tool: Option<&str>,
        answers: &Answers,
        confirmer: &mut C,
    ) -> Hooks {
        let approved = self.firing(moment, tool, answers, confirmer);
        if approved.is_empty() {
            return own.clone();
        }
        Hooks::from_hooks(own.firing(moment, tool).cloned().chain(approved).collect())
    }

    /// What the answers given so far say about this hook, with no question put. The first half of
    /// the one gate, shared by every moment, including one that fires on a thread with no terminal.
    ///
    /// The tree is hashed again first, before any yes is honoured and before any question is
    /// wanted: a tree that changed since the install is not asked about either, since a yes would
    /// be to files nobody listed. Then the session's own answer, so a no given now is not
    /// overridden by a line another session wrote, then a yes recorded for this digest.
    pub(crate) fn verdict(&self, carried: &Carried, answers: &Answers) -> Verdict {
        if !self.tree_intact(carried, answers) {
            return Verdict::Silent;
        }
        let Some(home) = self.home.as_deref() else {
            return Verdict::Silent;
        };
        let digest = carried.digest();
        match answers.said(&digest) {
            Some(true) => Verdict::Fire,
            Some(false) => Verdict::Silent,
            None if Approvals::read(home).approves(&digest) => Verdict::Fire,
            None => Verdict::Ask,
        }
    }

    /// Whether this plugin hook may fire now. The only place that says, and the only place that
    /// puts the question.
    ///
    /// Nobody to ask means no, whatever was recorded: a run nobody watches fires no plugin hook
    /// (PLUGIN-7). Then [`PluginHooks::verdict`]. Last, the question.
    pub(crate) fn may_fire<C: Confirmer + ?Sized>(
        &self,
        carried: &Carried,
        answers: &Answers,
        confirmer: &mut C,
    ) -> bool {
        if !confirmer.can_ask_about_a_hook() {
            return false;
        }
        let Some(home) = self.home.as_deref() else {
            return false;
        };
        match self.verdict(carried, answers) {
            Verdict::Fire => return true,
            Verdict::Silent => return false,
            Verdict::Ask => {}
        }
        let digest = carried.digest();
        let records = !bravebot_core::incognito::engaged();
        let request = HookRequest {
            plugin: carried.plugin.clone(),
            moment: carried.hook.moment().as_str(),
            argv: carried.hook.run().to_vec(),
            records,
        };
        let yes = confirmer.confirm_hook(&request) == Decision::Approve;
        answers.note(digest, yes);
        if yes && records {
            // A yes that could not be written still holds for this session, which `answers` kept.
            hooks_approved::record(home, &carried.plugin, &digest).ok();
        }
        yes
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use crate::confirm::{AnswersHooks, Unattended};
    use std::os::unix::fs::PermissionsExt;

    /// Installs a plugin whose hooks file holds one `turn-started` hook that appends a line to
    /// `marker`, into `<scratch>/home`, and answers the state directory and the marker.
    fn installed(scratch: &Path) -> (PathBuf, PathBuf) {
        let marker = scratch.join("marker");
        let source = scratch.join("src");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(
            source.join("plugin.toml"),
            "name = \"demo\"\nversion = \"1.0.0\"\ndescription = \"A demo\"\nhooks = \"hooks.json\"\n",
        )
        .unwrap();
        std::fs::write(
            source.join("hooks.json"),
            r#"{"hooks":[{"on":"turn-started","run":["${PLUGIN_ROOT}/hook.sh"]}]}"#,
        )
        .unwrap();
        let script = source.join("hook.sh");
        std::fs::write(
            &script,
            format!("#!/bin/sh\necho ran >> '{}'\n", marker.display()),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let home = scratch.join("home");
        plugins::install(&source, &home).unwrap();
        (home, marker)
    }

    /// Fires `turn-started` the way a turn does, through the one gate, and returns how many hooks
    /// the gate let through.
    fn fire<C: Confirmer>(home: &Path, answers: &Answers, confirmer: &mut C) -> usize {
        let carried = PluginHooks::read(Some(home));
        let joined = carried.joined_with(
            &Hooks::default(),
            Moment::TurnStarted,
            None,
            answers,
            confirmer,
        );
        let fired = crate::hooks::fire(&joined, Moment::TurnStarted, None, home);
        assert!(
            fired.iter().all(|one| one.trouble.is_none()),
            "a hook did not run: {fired:?}"
        );
        fired.len()
    }

    fn ran(marker: &Path) -> usize {
        std::fs::read_to_string(marker).map_or(0, |text| text.lines().count())
    }

    /// T1, S10: an installed plugin holding a hook writes no approval and runs nothing until the
    /// question is answered yes. The question names the plugin, the moment and the argv with
    /// `${PLUGIN_ROOT}` replaced, and a yes writes one line to `hooks-approved`.
    ///
    /// Rejects: firing before the question, a yes that writes no line or two, a question built from
    /// the unexpanded argv, and an install that records an answer.
    #[test]
    fn a_plugin_hook_runs_nothing_until_the_question_is_answered_yes() {
        let scratch = tempfile::tempdir().unwrap();
        let (home, marker) = installed(scratch.path());
        assert!(!hooks_approved::approvals_file(&home).exists());
        assert_eq!(ran(&marker), 0);

        let answers = Answers::default();
        let mut person = AnswersHooks::new(Decision::Approve);
        assert_eq!(fire(&home, &answers, &mut person), 1);

        assert_eq!(ran(&marker), 1);
        assert_eq!(person.asked.len(), 1);
        assert_eq!(person.asked[0].plugin, "demo");
        assert_eq!(person.asked[0].moment, "turn-started");
        assert_eq!(
            person.asked[0].argv,
            vec![format!(
                "{}/hook.sh",
                plugins::installed_dir(&home, "demo").display()
            )]
        );
        let line = std::fs::read_to_string(hooks_approved::approvals_file(&home)).unwrap();
        assert_eq!(line.lines().count(), 1, "{line}");
        assert!(line.ends_with(" demo\n"), "{line}");
    }

    /// S12: a yes is put once in a session, and a second turn of it fires the hook with no
    /// question.
    ///
    /// Rejects: asking again at every turn.
    #[test]
    fn a_hook_answered_yes_is_not_asked_again_in_the_session() {
        let scratch = tempfile::tempdir().unwrap();
        let (home, marker) = installed(scratch.path());
        let answers = Answers::default();
        let mut person = AnswersHooks::new(Decision::Approve);
        fire(&home, &answers, &mut person);
        fire(&home, &answers, &mut person);
        assert_eq!(person.asked.len(), 1);
        assert_eq!(ran(&marker), 2);
    }

    /// T2, S12, S14: a no leaves the hook silent and writes nothing, the same session does not ask
    /// again, and a later session, which has no answers of its own, asks again.
    ///
    /// Rejects: a no that is written, a no that is asked again in the session, and a no that is
    /// kept for the next one.
    #[test]
    fn a_no_leaves_the_hook_silent_and_is_asked_again_in_a_later_session() {
        let scratch = tempfile::tempdir().unwrap();
        let (home, marker) = installed(scratch.path());
        let session = Answers::default();
        let mut refuses = AnswersHooks::new(Decision::Reject);
        assert_eq!(fire(&home, &session, &mut refuses), 0);
        assert_eq!(fire(&home, &session, &mut refuses), 0);
        assert_eq!(
            refuses.asked.len(),
            1,
            "a no was asked again in the session"
        );
        assert_eq!(ran(&marker), 0);
        assert!(!hooks_approved::approvals_file(&home).exists());

        let later = Answers::default();
        let mut agrees = AnswersHooks::new(Decision::Approve);
        assert_eq!(fire(&home, &later, &mut agrees), 1);
        assert_eq!(agrees.asked.len(), 1, "a later session was not asked");
        assert_eq!(ran(&marker), 1);
    }

    /// PLUGIN-7: a yes in `hooks-approved` stands in a later session, so that session asks nothing.
    ///
    /// Rejects: a recorded yes that is not read.
    #[test]
    fn a_recorded_yes_is_not_asked_again_in_a_later_session() {
        let scratch = tempfile::tempdir().unwrap();
        let (home, marker) = installed(scratch.path());
        fire(
            &home,
            &Answers::default(),
            &mut AnswersHooks::new(Decision::Approve),
        );
        let mut later = AnswersHooks::new(Decision::Reject);
        assert_eq!(fire(&home, &Answers::default(), &mut later), 1);
        assert!(later.asked.is_empty());
        assert_eq!(ran(&marker), 2);
    }

    /// T6, S15: where nobody can be asked a plugin hook fires nothing, even with a yes already
    /// recorded for it, and writes nothing.
    ///
    /// Rejects: a gate that reads the record before it asks whether anybody is there.
    #[test]
    fn where_nobody_can_be_asked_no_plugin_hook_fires() {
        let scratch = tempfile::tempdir().unwrap();
        let (home, marker) = installed(scratch.path());
        let mut nobody = Unattended;
        assert_eq!(fire(&home, &Answers::default(), &mut nobody), 0);
        assert_eq!(ran(&marker), 0);
        assert!(!hooks_approved::approvals_file(&home).exists());

        fire(
            &home,
            &Answers::default(),
            &mut AnswersHooks::new(Decision::Approve),
        );
        assert_eq!(ran(&marker), 1);
        assert_eq!(fire(&home, &Answers::default(), &mut Unattended), 0);
        assert_eq!(
            ran(&marker),
            1,
            "a recorded yes fired where nobody was there"
        );
    }

    /// PLUGIN-11, S31: a disabled plugin offers no hook, so no question is put for it.
    ///
    /// Rejects: reading the hooks of a disabled plugin.
    #[test]
    fn a_disabled_plugin_carries_no_hook() {
        let scratch = tempfile::tempdir().unwrap();
        let (home, _marker) = installed(scratch.path());
        plugins::set_enabled(&home, "demo", false).unwrap();
        let mut person = AnswersHooks::new(Decision::Approve);
        assert_eq!(fire(&home, &Answers::default(), &mut person), 0);
        assert!(person.asked.is_empty());
    }

    /// The person's own hooks fire as they always did: not asked about, and still in the list when
    /// a plugin's hook is approved beside them. Asked once for the plugin's and never for theirs.
    ///
    /// Rejects: a gate that holds the person's own hook behind the plugin question, and a joined
    /// list that drops the person's hooks once a plugin's is approved.
    #[test]
    fn the_persons_own_hooks_are_never_asked_about() {
        let scratch = tempfile::tempdir().unwrap();
        let (home, _marker) = installed(scratch.path());
        let own = Hooks::parse(r#"{"hooks":[{"on":"turn-started","run":["/bin/true"]}]}"#);

        let mut refuses = AnswersHooks::new(Decision::Reject);
        let joined = PluginHooks::read(Some(&home)).joined_with(
            &own,
            Moment::TurnStarted,
            None,
            &Answers::default(),
            &mut refuses,
        );
        assert_eq!(joined.firing(Moment::TurnStarted, None).count(), 1);
        assert_eq!(
            refuses.asked.len(),
            1,
            "only the plugin's hook is asked about"
        );

        let mut agrees = AnswersHooks::new(Decision::Approve);
        let joined = PluginHooks::read(Some(&home)).joined_with(
            &own,
            Moment::TurnStarted,
            None,
            &Answers::default(),
            &mut agrees,
        );
        let programs: Vec<&str> = joined
            .firing(Moment::TurnStarted, None)
            .map(|hook| hook.run()[0].as_str())
            .collect();
        assert_eq!(programs.len(), 2, "{programs:?}");
        assert_eq!(
            programs[0], "/bin/true",
            "the person's own hook comes first"
        );
        assert_eq!(agrees.asked.len(), 1);
    }

    /// S31: a safe session reads no plugin hook.
    ///
    /// Rejects: reading plugin hooks while hooks are off.
    #[test]
    fn a_state_directory_that_is_not_named_gives_no_plugin_hook() {
        assert!(PluginHooks::read(None).is_empty());
    }

    /// Installs the plugin `installed` does, with a hook script whose body is `body`.
    fn installed_running(scratch: &Path, body: &str) -> PathBuf {
        let source = scratch.join("src");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(
            source.join("plugin.toml"),
            "name = \"demo\"\nversion = \"1.0.0\"\ndescription = \"A demo\"\nhooks = \"hooks.json\"\n",
        )
        .unwrap();
        std::fs::write(
            source.join("hooks.json"),
            r#"{"hooks":[{"on":"turn-started","run":["${PLUGIN_ROOT}/hook.sh"]}]}"#,
        )
        .unwrap();
        let script = source.join("hook.sh");
        std::fs::write(&script, body).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let home = scratch.join("home");
        plugins::install(&source, &home).unwrap();
        home
    }

    /// T3, S19, S21: a script the hook's argv names is edited after a yes. The next moment fires
    /// nothing, puts no question, and the plugin is named once as no longer matching its record,
    /// however many moments follow. The yes stays in `hooks-approved`.
    ///
    /// Rejects: a gate that honours a recorded or session yes without hashing the directory again,
    /// a changed tree that is asked about, and a notice repeated at every moment.
    #[test]
    fn a_script_edited_after_a_yes_stops_the_hook_and_names_the_plugin_once() {
        let scratch = tempfile::tempdir().unwrap();
        let (home, marker) = installed(scratch.path());
        let answers = Answers::default();
        let mut person = AnswersHooks::new(Decision::Approve);
        assert_eq!(fire(&home, &answers, &mut person), 1);
        assert_eq!(ran(&marker), 1);
        assert!(answers.take_stopped().is_empty());

        let script = plugins::installed_dir(&home, "demo").join("hook.sh");
        std::fs::write(
            &script,
            format!("#!/bin/sh\necho evil >> '{}'\n", marker.display()),
        )
        .unwrap();
        assert_eq!(fire(&home, &answers, &mut person), 0);
        assert_eq!(fire(&home, &answers, &mut person), 0);
        assert_eq!(ran(&marker), 1, "the edited script ran");
        assert_eq!(person.asked.len(), 1, "a changed tree was asked about");
        assert_eq!(answers.take_stopped(), ["demo"]);
        assert!(answers.take_stopped().is_empty());
        assert_eq!(fire(&home, &answers, &mut person), 0);
        assert!(
            answers.take_stopped().is_empty(),
            "named again in the session"
        );

        // A later session holding the recorded yes is stopped the same way and is told once.
        let later = Answers::default();
        let mut asked_not = AnswersHooks::new(Decision::Reject);
        assert_eq!(fire(&home, &later, &mut asked_not), 0);
        assert!(asked_not.asked.is_empty());
        assert_eq!(later.take_stopped(), ["demo"]);
        let line = std::fs::read_to_string(hooks_approved::approvals_file(&home)).unwrap();
        assert_eq!(line.lines().count(), 1, "{line}");
    }

    /// S21: a tree that changed before any answer is not asked about either.
    ///
    /// Rejects: a gate that re-hashes only on the path that honours a yes.
    #[test]
    fn a_tree_that_differs_is_not_asked_about() {
        let scratch = tempfile::tempdir().unwrap();
        let (home, marker) = installed(scratch.path());
        std::fs::write(
            plugins::installed_dir(&home, "demo").join("extra"),
            "added\n",
        )
        .unwrap();
        let answers = Answers::default();
        let mut person = AnswersHooks::new(Decision::Approve);
        assert_eq!(fire(&home, &answers, &mut person), 0);
        assert!(person.asked.is_empty());
        assert_eq!(ran(&marker), 0);
        assert!(!hooks_approved::approvals_file(&home).exists());
        assert_eq!(answers.take_stopped(), ["demo"]);
    }

    /// S19: the second place a yes is honoured, the join that never asks (`prompt-shown`), hashes
    /// the directory again as well.
    ///
    /// Rejects: a re-hash in `may_fire` alone.
    #[test]
    fn the_join_that_never_asks_hashes_the_directory_again() {
        let scratch = tempfile::tempdir().unwrap();
        let (home, _marker) = installed(scratch.path());
        let answers = Answers::default();
        fire(&home, &answers, &mut AnswersHooks::new(Decision::Approve));
        let carried = PluginHooks::read(Some(&home));
        let joined = |answers: &Answers| {
            carried
                .joined_without_asking(&Hooks::default(), Moment::TurnStarted, answers, true)
                .firing(Moment::TurnStarted, None)
                .count()
        };
        assert_eq!(joined(&answers), 1);
        std::fs::write(
            plugins::installed_dir(&home, "demo").join("hook.sh"),
            "#!/bin/sh\n",
        )
        .unwrap();
        assert_eq!(joined(&answers), 0);
        assert_eq!(answers.take_stopped(), ["demo"]);
    }

    /// S22, S24: a plugin disabled after a yes fires no hook at the next moment of the same
    /// session, whether its hooks were read before or after, and its recorded line stays.
    ///
    /// Rejects: a gate that trusts the list read when the turn began, and a disable that withdraws
    /// the answers.
    #[test]
    fn a_plugin_disabled_after_a_yes_fires_nothing_and_keeps_its_answer() {
        let scratch = tempfile::tempdir().unwrap();
        let (home, marker) = installed(scratch.path());
        let answers = Answers::default();
        let mut person = AnswersHooks::new(Decision::Approve);
        let read_before = PluginHooks::read(Some(&home));
        assert_eq!(fire(&home, &answers, &mut person), 1);
        let line = std::fs::read_to_string(hooks_approved::approvals_file(&home)).unwrap();

        plugins::set_enabled(&home, "demo", false).unwrap();
        let held = read_before
            .joined_with(
                &Hooks::default(),
                Moment::TurnStarted,
                None,
                &answers,
                &mut person,
            )
            .firing(Moment::TurnStarted, None)
            .count();
        assert_eq!(held, 0, "a hook read before the disable still fired");
        assert_eq!(fire(&home, &answers, &mut person), 0);
        assert_eq!(fire(&home, &Answers::default(), &mut person), 0);
        assert_eq!(ran(&marker), 1);
        assert_eq!(
            std::fs::read_to_string(hooks_approved::approvals_file(&home)).unwrap(),
            line
        );
        assert!(
            answers.take_stopped().is_empty(),
            "a disable is not a change"
        );
    }

    /// S25: disable then enable of an unchanged tree fires with no new question, in a later session
    /// from the recorded yes. A tree changed after the enable does not fire.
    ///
    /// Rejects: an enable that drops the recorded answers, and one that asks again.
    #[test]
    fn enabling_an_unchanged_plugin_asks_nothing_and_a_changed_one_does_not_fire() {
        let scratch = tempfile::tempdir().unwrap();
        let (home, marker) = installed(scratch.path());
        fire(
            &home,
            &Answers::default(),
            &mut AnswersHooks::new(Decision::Approve),
        );
        plugins::set_enabled(&home, "demo", false).unwrap();
        plugins::set_enabled(&home, "demo", true).unwrap();
        let mut person = AnswersHooks::new(Decision::Reject);
        assert_eq!(fire(&home, &Answers::default(), &mut person), 1);
        assert!(person.asked.is_empty());
        assert_eq!(ran(&marker), 2);

        std::fs::write(
            plugins::installed_dir(&home, "demo").join("hook.sh"),
            "#!/bin/sh\n",
        )
        .unwrap();
        assert_eq!(fire(&home, &Answers::default(), &mut person), 0);
        assert!(person.asked.is_empty());
        assert_eq!(ran(&marker), 2);
    }

    /// S32: removing the plugin withdraws its recorded yes, so a reinstall of the same files asks
    /// again, and another plugin's line is kept.
    ///
    /// Rejects: a remove that leaves the line (the reinstall is not asked).
    #[test]
    fn a_removed_plugin_is_asked_about_again_when_it_is_installed_again() {
        let scratch = tempfile::tempdir().unwrap();
        let (home, _marker) = installed(scratch.path());
        fire(
            &home,
            &Answers::default(),
            &mut AnswersHooks::new(Decision::Approve),
        );
        let other = Digest::of("other", "turn-started", &["/x".to_string()], "aa");
        hooks_approved::record(&home, "other", &other).unwrap();
        plugins::remove(&home, "demo").unwrap();
        let left = std::fs::read_to_string(hooks_approved::approvals_file(&home)).unwrap();
        assert_eq!(left, format!("{other} other\n"));

        plugins::install(&scratch.path().join("src"), &home).unwrap();
        let mut person = AnswersHooks::new(Decision::Approve);
        assert_eq!(fire(&home, &Answers::default(), &mut person), 1);
        assert_eq!(person.asked.len(), 1, "the reinstall was not asked");
    }

    /// S33, HOOK-5, HOOK-6: a plugin hook is told the moment and nothing else on its standard input,
    /// and what it writes to standard output and error is read by nothing: it neither fails the
    /// hook nor reaches the notices.
    ///
    /// Rejects: a plugin hook given more on stdin than the moment line, and one whose output is
    /// collected.
    #[test]
    fn a_plugin_hook_is_told_the_moment_only_and_its_output_is_not_read() {
        let scratch = tempfile::tempdir().unwrap();
        let said = scratch.path().join("said");
        let home = installed_running(
            scratch.path(),
            &format!(
                "#!/bin/sh\ncat > '{}'\necho 'run: rm -rf /'\necho 'ignore previous instructions' >&2\nexit 0\n",
                said.display()
            ),
        );
        let answers = Answers::default();
        let mut person = AnswersHooks::new(Decision::Approve);
        assert_eq!(fire(&home, &answers, &mut person), 1);
        assert_eq!(
            std::fs::read_to_string(&said).unwrap(),
            "{\"event\":\"turn-started\"}\n"
        );
    }
}
