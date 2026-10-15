//! What each [`Action`](super::Action) does once `event_loop` holds the session, the conversation
//! and the record: one method of [`Held`] for every arm that is more than a call.

use std::io;

use bravebot_agent::conversation::Conversation;
use bravebot_agent::lsp::LanguageServers;
use bravebot_agent::turn::{self, Task};
use bravebot_agent::{SessionScratch, Workspace};
use bravebot_config::{Config, Settings};
use bravebot_core::credentials::Exposed;
use bravebot_core::programs::{AskedAbout, TrustedPrograms};
use bravebot_core::trust::TrustStore;
use bravebot_i18n::t;
use bravebot_net::Egress;
use bravebot_session::audit::{Stamped, Trail};
use bravebot_session::sessions::Handle;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use super::{
    Answers, BACKGROUND_LOSES, Beginning, Changed, Continued, Ended, RunSettings, Suggesting,
    Switch, Wrote, add_directory, branch_session, change_directory, clear_session, close_directory,
    compact_animated, goal_check_animated, handed_to_the_background, handoff_answered,
    handoff_refusal, left_behind, list_rewind_points, next_after_a_turn, opened_scratch,
    opening_trust, record_to_resume, removal_trail, remove_checkout, rewind, rewind_point,
    run_command, run_turn_animated, save_compacted, status_report, suggest_next, switch_session,
    with_standing,
};
use crate::state::{Attached, AttachedImage, Servers, Session};

type Tty = Terminal<CrosstermBackend<io::Stdout>>;

/// The locals `event_loop` keeps from its startup, borrowed for one pass of its loop.
///
/// Borrowed rather than owned, so the locals stay bound in `Begun`'s field order and drop in the
/// reverse of it. A struct holding them would drop them in its declaration order, which is the
/// order `event_loop` creates them in.
///
/// A method that moves the conversation or an answer into a call that can fail leaves an empty
/// one behind when it does. That is safe because `event_loop` returns the error and the session
/// is never read again.
pub(super) struct Held<'a> {
    pub(super) session: &'a mut Session,
    pub(super) conversation: &'a mut Conversation,
    pub(super) stored: &'a mut Handle,
    pub(super) answers: &'a mut Answers,
    pub(super) workspace: &'a mut Workspace,
    pub(super) scratch: &'a mut Option<SessionScratch>,
    pub(super) settings: &'a Settings,
    pub(super) run_settings: &'a RunSettings,
}

/// What a turn is lent for its length: the conversation and every answer it can change.
pub(super) struct Lent {
    pub(super) conversation: Conversation,
    pub(super) trust: TrustStore,
    pub(super) programs: TrustedPrograms,
    pub(super) servers: Option<LanguageServers>,
    pub(super) asked_about: AskedAbout,
    pub(super) exposed: Exposed,
}

impl Held<'_> {
    /// Move the conversation and the answers a turn changes out of the session. Empty ones stand
    /// in until [`Held::take_back`] puts back what the turn returns.
    pub(super) fn lend(&mut self) -> Lent {
        Lent {
            conversation: std::mem::take(self.conversation),
            trust: std::mem::replace(
                &mut self.answers.trust,
                TrustStore::new(self.workspace.root()),
            ),
            programs: std::mem::take(&mut self.answers.programs),
            servers: self.answers.servers.take(),
            asked_about: std::mem::take(&mut self.answers.asked_about),
            exposed: std::mem::take(&mut self.answers.exposed),
        }
    }

    /// Put back what a turn returned, and hand on the events it recorded.
    pub(super) fn take_back(&mut self, continued: Continued) -> Vec<Stamped> {
        // Taken apart with no `..`, so an answer a turn learns to remember does not build until it
        // has a place in `answers`, where `/cd`, `/clear`, `/branch`, `/undo` and `/rewind` decide
        // it.
        let Continued {
            conversation,
            trust,
            programs,
            servers,
            asked_about,
            exposed,
            events,
        } = continued;
        *self.conversation = conversation;
        self.answers.trust = trust;
        self.answers.programs = programs;
        self.answers.servers = servers;
        self.answers.asked_about = asked_about;
        self.answers.exposed = exposed;
        events
    }

    /// Write the session's record under `title`, with everything the record holds.
    pub(super) fn save(&mut self, title: &str) {
        with_standing(
            self.session,
            self.conversation,
            &self.answers.trust,
            &self.answers.programs,
            self.workspace,
            |standing| self.stored.save(title, standing),
        );
    }

    pub(super) fn export(&mut self, path: Option<&str>) {
        let markdown = crate::render::as_markdown(self.session, self.stored.title());
        let exported_path = bravebot_session::sessions::export(
            self.workspace.root(),
            self.stored.id(),
            path,
            &markdown,
        );
        match exported_path {
            Ok(p) => self
                .session
                .note(t!(session_exported, path = p.display().to_string())),
            Err(e) => self
                .session
                .note(t!(session_export_failed, problem = e.to_string())),
        }
    }

    /// Put the session back to where it stood `steps` turns ago, for `/undo` and `/rewind`.
    pub(super) fn rewind_steps(&mut self, steps: usize) {
        let (trust, programs, language_servers) = self.answers.what_a_rewind_takes_back();
        rewind(
            self.session,
            self.conversation,
            trust,
            programs,
            self.stored,
            self.workspace,
            language_servers,
            steps,
        );
    }

    pub(super) fn rewind_command(&mut self, turns: &str) {
        if turns.is_empty() {
            list_rewind_points(self.session);
        } else {
            match turns.parse::<usize>() {
                Ok(steps) if steps > 0 => self.rewind_steps(steps),
                _ => self.session.note(t!(session_rewind_needs_a_number)),
            }
        }
    }

    pub(super) fn add_directory_command(&mut self, directory: &str) {
        // The snapshot holds a trust map without this directory's rule in it, while the
        // directory itself would stay open.
        self.session.rewind.close();
        add_directory(
            self.session,
            self.workspace,
            &mut self.answers.trust,
            directory,
        );
    }

    pub(super) fn close_directory_command(&mut self, directory: &str) {
        if close_directory(
            self.session,
            self.workspace,
            &mut self.answers.trust,
            directory,
        ) {
            // The snapshot holds a trust map with this directory's rule in it, while the
            // directory itself would stay closed.
            self.session.rewind.close();
            // Written now rather than at the end of the next turn: a session that closed
            // a directory and then quit would otherwise resume with it open and trusted.
            // Wherever a record is on disk, which a rewind to before the first turn can
            // leave, and not only once a turn has been had.
            if self.session.turns > 0 || self.stored.resumable().is_some() {
                let title = self.stored.title().to_string();
                self.save(&title);
            }
        }
    }

    /// The session left behind, where the person asked to leave at the new directory's rules question.
    pub(super) fn change_directory_command(
        &mut self,
        terminal: &mut Tty,
        directory: &str,
    ) -> Option<Ended> {
        // The record moves with the working directory, so the snapshot describes a
        // session that is no longer written where it was.
        self.session.rewind.close();
        let changed = change_directory(
            self.session,
            self.workspace,
            self.answers,
            self.stored.id(),
            directory,
            |asking, carried| crate::trust_prompt::ask_granted(terminal, asking, carried),
        );
        if changed != Changed::Stayed {
            with_standing(
                self.session,
                self.conversation,
                &self.answers.trust,
                &self.answers.programs,
                self.workspace,
                |standing| self.stored.move_to(self.workspace.root(), standing),
            );
        }
        // After the move, so the session picked up again is picked up where it went.
        (changed == Changed::Left).then(|| left_behind(self.stored))
    }

    /// How the session ends, where `/bg` handed it to a background process.
    pub(super) fn background(&mut self) -> Option<Ended> {
        match handed_to_the_background(
            self.session,
            self.stored.to_resume(),
            BACKGROUND_LOSES.get().map(String::as_str),
            bravebot_config::sandbox::in_force().mode,
            bravebot_core::incognito::engaged(),
        ) {
            Ok(ended) => Some(ended),
            Err(refused) => {
                self.session.note(refused);
                None
            }
        }
    }

    pub(super) fn branch(&mut self, name: &str) {
        // And a directory of its own, as a session carrying on from another is given one
        // (TRUST-15): the old one goes with the session that wrote in it.
        if branch_session(
            self.session,
            self.stored,
            self.conversation,
            self.answers,
            self.workspace,
            &self.workspace.session_checkouts(),
            name,
        ) {
            *self.scratch = opened_scratch(self.session, self.workspace);
        }
    }

    /// The session it leaves behind, where `/resume` chose a record to switch to.
    pub(super) fn resume(
        &mut self,
        terminal: &mut Tty,
        servers: &mut Servers,
        switch: &mut Option<Switch>,
        typed: &str,
    ) -> Option<Ended> {
        let record = record_to_resume(
            terminal,
            self.session,
            self.workspace.root(),
            self.stored.id(),
            typed,
        )?;
        *switch = Some(switch_session(
            self.session,
            self.workspace,
            servers,
            record,
        ));
        Some(left_behind(self.stored))
    }

    pub(super) fn status(&mut self, config: &Config) {
        let report = status_report(
            self.session,
            self.stored,
            self.workspace,
            self.scratch.as_ref().map(SessionScratch::path),
            config,
            Some((&self.answers.trust, &self.answers.programs)),
        );
        self.session.report(report);
    }

    pub(super) fn forget_trust_command(&mut self) {
        // This session's map is left as it is: the answer it began with was given, and
        // what is withdrawn is the next session's. /clear begins one now, and it asks.
        self.session.note(super::forget_trust(
            bravebot_agent::home::directory().as_deref(),
            self.workspace.root(),
        ));
    }

    pub(super) fn reach(&mut self, argument: &str) {
        let home = bravebot_agent::home::directory();
        let profile = bravebot_agent::home::profile();
        let paths = bravebot_agent::reach::requested_paths_command(self.workspace, argument);
        self.session.note(match home.as_deref() {
            _ if paths.is_some() => paths.unwrap_or_default(),
            Some(home) => bravebot_agent::reach::command(
                &bravebot_agent::reach::Typed {
                    home,
                    profile: profile.as_deref(),
                    session: self.stored.id(),
                    directory: self.workspace.root(),
                    today: &bravebot_agent::reach::today(),
                },
                argument,
            ),
            None => t!(reach_refused_no_home).to_string(),
        });
    }

    pub(super) fn list_checkouts(&mut self) {
        let mut branches = std::collections::BTreeMap::new();
        self.session
            .report_checkouts(&self.workspace.session_checkouts(), |checkout| {
                branches
                    .entry(checkout.repository.clone())
                    .or_insert_with(|| {
                        crate::checkouts_command::Branches::read(&checkout.repository)
                    })
                    .pushed(&checkout.id)
            });
        self.session
            .report_unlisted_checkouts(&bravebot_session::sessions::unlisted_checkouts(
                self.workspace,
            ));
    }

    pub(super) fn list_memories(&mut self) {
        let memories = bravebot_agent::agents::memories(
            self.workspace,
            bravebot_agent::home::directory().as_deref(),
            &self.answers.trust,
            self.answers.rules.permissions.clone(),
            &mut Trail::new(),
        );
        self.session.report_memories(&memories);
    }

    pub(super) fn remove_checkout_command(&mut self, terminal: &mut Tty, id: &str) {
        let kept = self.workspace.session_checkouts();
        let named = kept.iter().find(|listed| listed.id == id).cloned();
        remove_checkout(
            self.session,
            kept,
            id,
            |listed| crate::confirm::ask_remove_checkout(terminal, listed),
            |id| {
                self.workspace
                    .remove_session_checkout(id, &mut self.answers.trust)
            },
        );
        let removal = removal_trail(named.as_ref(), &self.workspace.session_checkouts());
        self.stored
            .append_audit(self.session.turns, removal.events());
    }

    pub(super) fn apply_checkout(
        &mut self,
        terminal: &mut Tty,
        config: &Config,
        id: &str,
        paths: &[String],
    ) {
        // The person's own typing is the request, so no turn is run: each file goes
        // through the write gate and is put to them, whatever the map would have said
        // (CHECKOUT-14).
        let permission_mode = self.session.live_permission_mode();
        let task = Task::new(format!("/checkouts apply {id}"))
            .with_home(bravebot_agent::home::directory())
            .with_profile(bravebot_agent::home::profile())
            .with_cache(bravebot_agent::home::cache())
            .remembering(Some(self.stored.id().to_string()))
            .with_permissions(self.answers.rules.permissions.clone())
            .with_permission_mode(permission_mode.clone())
            .with_auto_vetting(self.session.auto_vetting())
            .with_deadlines(bravebot_agent::exec::Deadlines::resolve(
                self.settings.run_deadlines(),
            ));
        let mut trail = Trail::new();
        let mut asking = crate::confirm::TerminalConfirmer::new(terminal);
        let mut confirmer =
            bravebot_agent::Confining::new(&mut asking, permission_mode, task.auto_vetting);
        match turn::apply_checkout_asked_for(
            config,
            &Egress::new(),
            self.workspace,
            &task,
            id,
            paths,
            &mut confirmer,
            &mut trail,
            self.answers.trust.clone(),
        ) {
            Ok(done) => {
                self.answers.trust = done.trust;
                let heading = match done.applied {
                    true => t!(checkouts_applied, id = id),
                    false => t!(checkouts_not_applied, id = id),
                };
                self.session.note(format!("{heading}\n{}", done.text));
            }
            Err(_)
                if !self
                    .workspace
                    .session_checkouts()
                    .iter()
                    .any(|kept| kept.id == id) =>
            {
                self.session.note(t!(checkouts_no_such, id = id))
            }
            Err(refusal) => self.session.note(refusal),
        }
        self.stored.append_audit(self.session.turns, trail.events());
    }

    pub(super) fn compact(
        &mut self,
        terminal: &mut Tty,
        config: &Config,
        focus: &str,
    ) -> io::Result<()> {
        // The snapshot holds the conversation as it was before it was shortened.
        self.session.rewind.close();
        let (shortened, events) = compact_animated(
            terminal,
            self.session,
            config,
            std::mem::take(self.conversation),
            &self.answers.trust,
            focus,
        )?;
        *self.conversation = shortened;
        save_compacted(
            self.session,
            self.stored,
            self.conversation,
            &self.answers.trust,
            &self.answers.programs,
            self.workspace,
            &events,
        );
        Ok(())
    }

    pub(super) fn aside(
        &mut self,
        terminal: &mut Tty,
        config: &Config,
        question: &str,
        pasted: &[AttachedImage],
        attached: &[Attached],
    ) -> io::Result<()> {
        if question.is_empty() {
            self.session.note(t!(btw_needs_a_question));
            return Ok(());
        }
        self.ask_aside(terminal, config, question, false, pasted, attached)
    }

    pub(super) fn recap(&mut self, terminal: &mut Tty, config: &Config) -> io::Result<()> {
        self.session.away.recapped();
        self.ask_aside(terminal, config, "", true, &[], &[])
    }

    fn ask_aside(
        &mut self,
        terminal: &mut Tty,
        config: &Config,
        question: &str,
        recap: bool,
        pasted: &[AttachedImage],
        attached: &[Attached],
    ) -> io::Result<()> {
        // The snapshot holds the spend and the timing as they were before the turn,
        // and an aside is charged to that turn: rewinding to it would un-charge a
        // request that really went out. The same reason `/compact` closes it.
        self.session.rewind.close();
        let events = super::aside_animated(
            terminal,
            self.session,
            config,
            self.conversation,
            &mut self.answers.trust,
            question,
            recap,
            pasted,
            attached,
            self.workspace,
        )?;

        // Written now rather than at the end of the next turn, for the reason a
        // compaction is: the aside is the change, and a session that asked something
        // and then slept should resume with the answer still there.
        //
        // Nothing yet where no turn has been had, as everywhere else that writes
        // outside a turn: a session opened and abandoned should leave no record, and
        // a question asked before the first prompt was asked over an empty exchange.
        if self.session.turns > 0 {
            let title = self.stored.title().to_string();
            self.save(&title);
            self.stored.append_audit(self.session.turns, &events);
        }
        Ok(())
    }

    pub(super) fn manifest(
        &mut self,
        terminal: &mut Tty,
        config: &Config,
        task: &str,
        pasted: &[AttachedImage],
        attached: &[Attached],
    ) -> io::Result<()> {
        if task.is_empty() {
            self.session.note(t!(manifest_needs_a_task));
        } else {
            // The snapshot holds the spend and the timing as they were before the turn,
            // and this run is charged to that turn, exactly as an aside is: rewinding to
            // it would un-charge requests that really went out, and steps that really
            // wrote to the tree are not something the rewind window covers.
            self.session.rewind.close();
            let events = super::manifest_animated(
                terminal,
                self.session,
                config,
                self.workspace,
                task,
                pasted,
                attached,
                &mut self.answers.trust,
                &self.answers.rules.permissions,
                self.run_settings,
            )?;

            // Taken off the workspace rather than kept for anything: a run is not a turn of
            // this conversation, and `rewind.close` above already shut the window
            // that could have rewound to one, so nothing will put these back. What the
            // drain is for is the *next* turn, which takes whatever the workspace is
            // holding as its own. Left here, the run's writes would be attributed to that
            // turn and `/undo` on it would revert them.
            let _ = self.workspace.take_backups();

            // The session's own record, written for the reason an aside's is: the run is
            // the change, and a session that started one and then slept should resume
            // with the note saying where it went still in the transcript. The run's own
            // record is a separate file that `manifest_animated` already wrote.
            with_standing(
                self.session,
                self.conversation,
                &self.answers.trust,
                &self.answers.programs,
                self.workspace,
                |standing| crate::manifest_run::save_session(self.stored, standing, &events),
            );
        }
        Ok(())
    }

    /// `/clear`, or with a `brief` a handoff, which begins the new session with that first prompt.
    ///
    /// The session left behind, where one of the questions the new session opens with ended it.
    pub(super) fn clear(&mut self, terminal: &mut Tty, brief: Option<String>) -> Option<Ended> {
        // What a handoff adds to a clear is the link back to the session it came from and a
        // first prompt, and what it takes away is nothing: the file decisions, the
        // permissions and the quarantine are asked about again below, as after `/clear`.
        let source = match brief {
            Some(_) => self.stored.to_resume().map(|r| r.id),
            None => None,
        };
        (*self.conversation, *self.stored) = clear_session(
            self.session,
            self.workspace,
            brief.as_ref().and(source.as_deref()),
        );
        // Where this map came from decides nothing further: a directory a settings file
        // named was opened by an answer the cleared session's user gave, and it closed
        // with that session rather than carrying into this one.
        let Some((fresh, whence)) = opening_trust(
            terminal,
            self.session,
            self.workspace.root(),
            Beginning::New,
            self.stored.id(),
        ) else {
            return Some(left_behind(self.stored));
        };
        self.answers.trust = fresh;
        // And the rules a new session opens with, for the root it is in: the grants the
        // cleared session installed were answered by its user, and the files may have
        // changed since. Where the person already granted a rule the record says so, and
        // nothing is asked twice.
        if !self.answers.rules.read_for(
            self.session,
            self.workspace.root(),
            whence,
            self.stored.id(),
            |asking, carried| crate::trust_prompt::ask_granted(terminal, asking, carried),
        ) {
            return Some(left_behind(self.stored));
        }
        self.answers.forget_what_was_agreed();
        // And a new directory, since nothing in the old one outlives the session that
        // wrote it. The old one is removed either way: what the cleared context wrote is
        // not something the session after it should find lying there.
        *self.scratch = opened_scratch(self.session, self.workspace);
        // Last, once everything the new session is asked has been answered: the next pass
        // of the loop sends it as the person's own line.
        if let Some(brief) = brief {
            self.session.hand_over(brief);
        }
        None
    }

    pub(super) fn handoff(
        &mut self,
        terminal: &mut Tty,
        config: &Config,
        goal: &str,
    ) -> io::Result<()> {
        if let Some(refusal) =
            handoff_refusal(goal, self.session.turns, self.stored.to_resume().is_some())
        {
            self.session.note(refusal);
        } else {
            // Closed for the reason an aside closes it: the request is charged to a turn
            // the snapshot predates.
            self.session.rewind.close();
            let (done, events) = super::ask_animated(
                terminal,
                self.session,
                config,
                self.conversation,
                &mut self.answers.trust,
                "",
                false,
                Some(goal),
                &[],
                &[],
                self.workspace,
            )?;
            self.stored.append_audit(self.session.turns, &events);
            handoff_answered(self.session, goal, done);
        }
        Ok(())
    }

    /// Send `prompt` and every turn that follows it before the person is next asked anything.
    ///
    /// Returns the request for a suggestion of what to send next, where one went out.
    pub(super) fn submit(
        &mut self,
        terminal: &mut Tty,
        config: &mut Config,
        prompt: String,
    ) -> io::Result<Option<Suggesting>> {
        // A prompt sent while this one was running goes when it ends, in the order it was
        // typed, and so does anything typed during that one. Looping here rather than
        // going back round the outer loop keeps a queued prompt from waiting on a key
        // press that nobody is there to make.
        // Whose line each one is, which decides whether a `@path` in it vouches for a
        // file. Everything the person typed or queued is theirs; the sentence a goal
        // carries the work on with is this program's, and so is the one a watch fires
        // with. A fire also must not leave a loop behind repeating itself, which is the
        // other thing this answers.
        let whose = if self.session.watch_is_firing() {
            Wrote::TheDriver
        } else {
            Wrote::ThePerson
        };
        // Whatever was offered is not the next prompt any more. An answer still on its way
        // is for a state that has passed, and is dropped when the chain's own request
        // replaces it below.
        self.session.dismiss_suggestion();
        let mut sending = Some((prompt, whose));
        while let Some((prompt, wrote)) = sending {
            // Here as well as at the top of the outer loop, which no turn in this chain
            // goes back round until the last one ends.
            self.session.keep_awake();
            let history_start = self.conversation.recounted().len();
            // With the record's rules, which the turn about to run will read: without them a
            // yes this turn gives to a recorded memory would outlive rewinding past it.
            let recorded = bravebot_agent::memory::with_recorded(
                &self.answers.trust,
                self.workspace,
                bravebot_agent::home::directory().as_deref(),
            );
            let point = rewind_point(
                self.session,
                self.conversation,
                &recorded,
                &self.answers.programs,
                self.stored,
            );
            let _ = self.workspace.take_backups();
            self.session.rewind.open(point, prompt.clone());
            self.session.rewind.bind_coverage(self.workspace);

            // Everything the session holds is lent for the turn and taken back: a turn that
            // writes untrusted data into a trusted path records that, and the next turn must
            // honour it; a turn that has been had is a turn the next one can be asked about;
            // a server somebody approved answers the next question without asking again;
            // and a run prompt already read is what tells the next one to name a settings
            // file instead of a key.
            let Lent {
                conversation,
                trust,
                programs,
                servers,
                asked_about,
                exposed,
            } = self.lend();
            let continued = run_turn_animated(
                terminal,
                self.session,
                config,
                self.workspace,
                self.scratch.as_ref().map(SessionScratch::path),
                &prompt,
                wrote,
                conversation,
                trust,
                programs,
                servers,
                asked_about,
                exposed,
                &self.answers.rules.permissions,
                self.run_settings,
                self.stored,
            )?;
            let events = self.take_back(continued);

            self.session.record_turn(history_start, self.conversation);
            self.session
                .rewind
                .keep_backups(self.workspace.take_backups());

            // Written after each turn rather than at the end, because the end may never
            // come: the session worth resuming is the one whose machine slept and never
            // woke. Best-effort, like everything else under ~/.bravebot.
            self.save(&prompt);
            self.stored.append_audit(self.session.turns, &events);

            sending = next_after_a_turn(self.session, |session| {
                let (carrying_on, checked) = goal_check_animated(
                    terminal,
                    session,
                    config,
                    self.conversation,
                    &self.answers.trust,
                )?;
                // The check is a request that really went out, and a refusal in one is
                // exactly what somebody reading the trail afterwards wants to find.
                self.stored.append_audit(session.turns, &checked);
                Ok(carrying_on)
            })?;
        }
        // After the last turn of the chain, which is the only one the person is about to
        // answer.
        Ok(suggest_next(
            self.session,
            config,
            self.conversation,
            &self.answers.trust,
        ))
    }

    /// A `!` line, run as the person's own command. `private` keeps it out of the conversation the
    /// planner reads.
    pub(super) fn run_line(
        &mut self,
        terminal: &mut Tty,
        line: &str,
        private: bool,
    ) -> io::Result<()> {
        // Shell effects have no file-tool journal, but older edits can still be undone.
        self.session.rewind.bind_coverage(self.workspace);
        self.workspace
            .mark_rewind_gap(bravebot_agent::rewind::CoverageGap::Command);
        let events = run_command(
            terminal,
            self.session,
            self.workspace,
            line,
            self.conversation,
            private,
        )?;
        // Saved like a turn, and for the same reason: the command is in the conversation
        // now, so a session resumed without it would have the planner referring to output
        // it can no longer see.
        self.save(line);
        self.stored.append_audit(self.session.turns, &events);
        Ok(())
    }
}
