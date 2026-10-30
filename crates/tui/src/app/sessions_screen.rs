//! Opening the Sessions screen and carrying out what it asks for (SCREEN-1, SCREEN-8).
//!
//! The Sessions screen holds the list and the keys. What needs the roster or the state directory is done by
//! the functions here that take the session alone, so it is done by whichever loop holds the keys,
//! a turn's included. What ends the session, or needs the record being run or the terminal, is
//! `Held`'s, because only the loop at the plain prompt holds them.

use std::path::Path;
use std::time::{Duration, Instant};

use bravebot_i18n::t;
use bravebot_session::jobs::{Roster, Stopped};
use bravebot_session::listing;
use bravebot_session::sessions::{self, Deletion};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use super::actions::Held;
use super::keys::Action;
use super::opening::switch_session;
use super::{Elsewhere, Ended, How, Switch, left_behind};
use crate::sessions_screen::{Effect, SessionsScreen};
use crate::state::{Servers, Session, Status};

type Tty = Terminal<CrosstermBackend<std::io::Stdout>>;

/// How long a stop waits for a process to end before it is ended by force.
const GRACE: Duration = Duration::from_secs(5);

/// Open the Sessions screen over the session whose record is `current`, working in `directory`, with the
/// preferences the person last left, and read the machine's sessions into it.
///
/// With `from_pr`, only the sessions linked to that pull request are listed (`--from-pr`). With
/// `typing`, the search box has the focus, as it does for `--resume` and `/resume`.
pub(super) fn open_sessions_screen(
    session: &mut Session,
    current: &str,
    directory: &Path,
    from_pr: Option<&str>,
    typing: bool,
) {
    let mut screen = SessionsScreen::new(
        listing::load_grouping(),
        listing::load_pinned(),
        listing::load_archived(),
        Some(current.to_string()),
    )
    .in_directory(directory.to_path_buf())
    .reading();
    if let Some(wanted) = from_pr {
        screen = screen.from_pull_request(wanted);
    }
    if typing {
        screen = screen.typing_first();
    }
    session.open_sessions_screen(screen);
    read_sessions_screen(session, Instant::now());
}

/// Read the machine's sessions into the open sessions_screen. The session the terminal is in is the one the
/// sessions_screen was opened for, and is working while a turn or a command runs.
pub(super) fn read_sessions_screen(session: &mut Session, now: Instant) {
    let working = matches!(session.status, Status::Working | Status::Running);
    let Some(sessions_screen) = session.sessions_screen_mut() else {
        return;
    };
    let root = sessions::sessions_root().unwrap_or_default();
    let roster = Roster::readable();
    sessions_screen.refresh(
        &root,
        roster.as_ref(),
        Some(working),
        sessions::is_held_in,
        now,
    );
}

/// Take what the thread that reads has found, and ask it for the list again if the Sessions screen is
/// open and it is time to. Whether the list changed, which asks for a redraw.
pub(super) fn read_sessions_screen_if_due(session: &mut Session, now: Instant) -> bool {
    if !session
        .sessions_screen()
        .is_some_and(|sessions_screen| sessions_screen.due(now))
    {
        return false;
    }
    let working = matches!(session.status, Status::Working | Status::Running);
    let Some(sessions_screen) = session.sessions_screen_mut() else {
        return false;
    };
    let mut changed = sessions_screen.poll();
    if sessions_screen.read_due(now) {
        let root = sessions::sessions_root().unwrap_or_default();
        // The roster is read on the thread that reads, when there is one.
        let roster = (!sessions_screen.reads_apart())
            .then(Roster::readable)
            .flatten();
        changed |= sessions_screen.read_later(
            &root,
            roster.as_ref(),
            Some(working),
            sessions::is_held_in,
            now,
        );
    }
    changed
}

/// What a task is called in a sentence: its first line, with what a terminal would act on pictured.
fn shown(task: &listing::Task) -> String {
    crate::render::printable(task.title.lines().next().unwrap_or("").trim())
}

fn say(session: &mut Session, notice: impl Into<String>) {
    if let Some(sessions_screen) = session.sessions_screen_mut() {
        sessions_screen.say(notice);
    }
}

/// Carry out an effect that changes nothing the session is running: ending a process, archiving,
/// deleting, renaming a session that is not this one, saving what the person chose, reading more.
///
/// What is left is given back for the loop that holds the record and the terminal: opening a task,
/// copying one, beginning a session, and naming this one.
pub(super) fn carry_out(session: &mut Session, effect: Effect) -> Option<Effect> {
    match effect {
        Effect::Stop(task) => {
            if stop_task(session, &task) {
                let name = shown(&task);
                say(session, t!(sessions_screen_note_stopped, name = name));
            }
            read_sessions_screen(session, Instant::now());
        }
        Effect::Rename { task, name } if !task.current => rename_other(session, &task, &name),
        Effect::Archive(task) => archive_task(session, &task),
        Effect::Delete(task) => delete_task(session, &task),
        Effect::ShowMore => {
            let now = Instant::now();
            if let Some(sessions_screen) = session.sessions_screen_mut() {
                sessions_screen.want_more(now);
            }
            read_sessions_screen(session, now);
        }
        Effect::Grouping(grouping) => listing::save_grouping(grouping),
        Effect::Pins(pinned) => listing::save_pinned(&pinned),
        left @ (Effect::Open(_)
        | Effect::Fork(_)
        | Effect::New(_)
        | Effect::Rename { .. }
        | Effect::StopTurnThen(_)) => {
            return Some(left);
        }
    }
    None
}

/// Put the question before stopping the turn that is running so that an opening, a copy or a new
/// session can be carried out, or say why it cannot be, before the turn is touched (SCREEN-15).
///
/// Nothing is asked where nothing would end: the session this terminal is in only leaves the
/// screen. The refusals a session has at rest that rest on what the list already knows, a manifest
/// run, a record another terminal holds and a directory that is gone, are said here, so that a turn
/// is never stopped for a session that was going to be refused.
pub(super) fn ask_to_stop_the_turn(session: &mut Session, root: &Path, effect: Effect) {
    if let Effect::Open(task) = &effect
        && task.current
    {
        session.close_sessions_screen();
        return;
    }
    if let Some(refusal) = refused_before_stopping(&effect, root) {
        say(session, refusal);
        return;
    }
    let asked = session
        .sessions_screen_mut()
        .is_some_and(|sessions_screen| sessions_screen.ask_to_stop_the_turn(effect));
    if !asked {
        say(session, t!(sessions_screen_note_wait_for_turn));
    }
}

/// What a working loop other than a turn's is busy with, as the sentence about it names it.
#[derive(Clone, Copy)]
pub(super) enum Busy {
    /// A compaction, an aside, a commit message, a handoff, an enhanced prompt or a goal check.
    Request,
    /// A plan run.
    Plan,
    /// A command for the shell.
    Command,
}

/// Say why opening, copying or beginning a session does nothing while a loop that is not a turn's
/// has the keys, where a turn asks about stopping it (SCREEN-15). `true` where `action` was one of
/// those and has been answered.
///
/// None of these loops can be stopped by a question the way a turn can: a compaction cannot be
/// half-done, and the others end on their own in moments. The session this terminal is in only
/// leaves the screen, so that is done as it is during a turn.
pub(super) fn say_why_it_waits(session: &mut Session, action: &Action, busy: Busy) -> bool {
    let Action::SessionsScreen(effect) = action else {
        return false;
    };
    match effect {
        Effect::Open(task) if task.current => {
            session.close_sessions_screen();
        }
        Effect::Open(_) | Effect::Fork(_) | Effect::New(_) => {
            let sentence = match busy {
                Busy::Request => t!(sessions_screen_note_busy_request),
                Busy::Plan => t!(sessions_screen_note_busy_plan),
                Busy::Command => t!(sessions_screen_note_busy_command),
            };
            say(session, sentence);
        }
        _ => return false,
    }
    true
}

/// The sentence a session is refused with at rest that the list can already tell, or `None` where
/// the turn may be asked about stopping (SCREEN-15).
fn refused_before_stopping(effect: &Effect, root: &Path) -> Option<String> {
    use crate::resume::Refusal;
    let (task, joins_a_process) = match effect {
        Effect::Open(task) => (task, task.live),
        Effect::Fork(task) => (task, false),
        Effect::New(Some(directory)) if Path::new(directory) != root => {
            return directory_gone(directory);
        }
        _ => return None,
    };
    if task.manifest {
        return Some(Refusal::Manifest.note());
    }
    if matches!(effect, Effect::Open(_))
        && !task.live
        && task.status == listing::Status::OpenElsewhere
    {
        return Some(Refusal::HeldByAnother.note());
    }
    let here = Path::new(&task.directory) == root;
    if joins_a_process || !here {
        return directory_gone(&task.directory);
    }
    None
}

/// The sentence for a directory that is gone, or `None` where it is there.
fn directory_gone(directory: &str) -> Option<String> {
    (!Path::new(directory).is_dir()).then(|| {
        t!(
            sessions_screen_note_directory_gone,
            directory = crate::render::printable(directory)
        )
        .to_string()
    })
}

/// What the question over the list asked for once the turn it names has been stopped, or has
/// ended by itself while the question was up: the opening, the copy or the new session (SCREEN-15).
pub(super) fn after_the_turn(effect: Effect) -> Effect {
    match effect {
        Effect::StopTurnThen(then) => *then,
        other => other,
    }
}

/// End the process running a task. Whether it ended, with the reason said where it did not.
fn stop_task(session: &mut Session, task: &listing::Task) -> bool {
    let Some(roster) = Roster::writable() else {
        say(session, t!(sessions_screen_note_no_state));
        return false;
    };
    let name = shown(task);
    match roster.stop(&task.id, GRACE) {
        Ok(Stopped::Stopped) => true,
        Ok(Stopped::AlreadyNotRunning | Stopped::Missing) => {
            say(session, t!(sessions_screen_note_not_running, name = name));
            false
        }
        Err(_) => {
            say(session, t!(sessions_screen_note_stop_failed, name = name));
            false
        }
    }
}

/// Call a task that is not this session something else (SCREEN-9).
pub(super) fn rename_other(session: &mut Session, task: &listing::Task, name: &str) {
    let shown_name = shown(task);
    let roster = Roster::writable();
    let said = match listing::rename(Path::new(&task.directory), &task.id, name, roster.as_ref()) {
        Ok(title) => t!(
            sessions_screen_note_renamed,
            name = crate::render::printable(&title)
        ),
        Err(listing::Renamed::Held) => t!(sessions_screen_note_rename_held, name = shown_name),
        Err(listing::Renamed::Missing) => t!(sessions_screen_note_gone, name = shown_name),
        Err(listing::Renamed::Refused | listing::Renamed::Failed) => {
            t!(sessions_screen_note_rename_failed, name = shown_name)
        }
    };
    read_sessions_screen(session, Instant::now());
    say(session, said);
}

/// Take a task off the list until it is used again (SCREEN-10). A process running it is ended first.
fn archive_task(session: &mut Session, task: &listing::Task) {
    if task.current {
        say(session, t!(sessions_screen_note_current));
        return;
    }
    if task.live && !stop_task(session, task) {
        return;
    }
    let name = shown(task);
    // After the record's own time, so a record written in this second is still hidden by it.
    let at = sessions::now();
    if let Some(sessions_screen) = session.sessions_screen_mut() {
        let was_pinned = sessions_screen.pinned().contains(&task.id);
        sessions_screen.archived_at(&task.id, at);
        listing::save_archived(sessions_screen.archived());
        if was_pinned {
            listing::save_pinned(sessions_screen.pinned());
        }
        sessions_screen.say(t!(sessions_screen_note_archived, name = name));
    }
}

/// Remove a task and everything its record kept (SCREEN-10). A process running it is ended first,
/// and a record another terminal has open is left.
fn delete_task(session: &mut Session, task: &listing::Task) {
    if task.current {
        say(session, t!(sessions_screen_note_current));
        return;
    }
    let project = Path::new(&task.directory);
    if !task.live && sessions::is_open_elsewhere(project, &task.id) {
        say(session, t!(sessions_screen_note_open_elsewhere));
        return;
    }
    if task.live && !stop_task(session, task) {
        return;
    }
    let name = shown(task);
    match sessions::delete(project, &task.id) {
        Ok(()) | Err(Deletion::NotFound) => {
            if let Some(sessions_screen) = session.sessions_screen_mut() {
                let was_pinned = sessions_screen.pinned().contains(&task.id);
                sessions_screen.remove(&task.id);
                if was_pinned {
                    listing::save_pinned(sessions_screen.pinned());
                }
                sessions_screen.say(t!(sessions_screen_note_deleted, name = name));
            }
        }
        Err(Deletion::Invalid | Deletion::Failed(_)) => {
            say(session, t!(sessions_screen_note_delete_failed, name = name));
        }
    }
}

impl Held<'_> {
    /// Open the Sessions screen over the plain prompt, on the session this terminal is in.
    pub(super) fn open_sessions_screen(&mut self) {
        let id = self.stored.id().to_string();
        open_sessions_screen(self.session, &id, self.workspace.root(), None, false);
    }

    /// Whether `task` ran in the directory this session works in, so that the session it is can be
    /// carried on in this process.
    fn here(&self, task: &listing::Task) -> bool {
        Path::new(&task.directory) == self.workspace.root()
    }

    /// Whether the directory a task ran in is still there to start a session in.
    fn reachable(&mut self, directory: &str) -> bool {
        let there = Path::new(directory).is_dir();
        if !there {
            say(
                self.session,
                t!(
                    sessions_screen_note_directory_gone,
                    directory = crate::render::printable(directory)
                ),
            );
        }
        there
    }

    /// Carry out what the Sessions screen asked for that changes the session being run. The session left
    /// behind, where the effect ended this one (SCREEN-8).
    pub(super) fn sessions_screen_effect(
        &mut self,
        terminal: &mut Tty,
        servers: &mut Servers,
        switch: &mut Option<Switch>,
        effect: Effect,
    ) -> Option<Ended> {
        match after_the_turn(effect) {
            Effect::Open(task) => self.open_task(servers, switch, &task),
            Effect::Fork(task) => self.fork_task(servers, switch, &task),
            Effect::New(directory) => match self.fresh_elsewhere(directory) {
                Some(elsewhere) => elsewhere,
                None => {
                    self.session.close_sessions_screen();
                    self.clear(terminal, None)
                }
            },
            Effect::Rename { task, name } if task.current => {
                self.rename_this_session(&name);
                None
            }
            other => {
                carry_out(self.session, other);
                None
            }
        }
    }

    /// Name the session being run, as `/rename` names it, and show the name in the list.
    pub(super) fn rename_this_session(&mut self, name: &str) {
        super::session_commands::rename_session(self.session, self.stored, name);
        read_sessions_screen(self.session, Instant::now());
    }

    /// Carry on in a task: this session, a background process to join, or a record to resume
    /// (SCREEN-8).
    pub(super) fn open_task(
        &mut self,
        servers: &mut Servers,
        switch: &mut Option<Switch>,
        task: &listing::Task,
    ) -> Option<Ended> {
        if task.current {
            self.session.close_sessions_screen();
            return None;
        }
        // Said before anything is loaded: a manifest run has no conversation to carry on from.
        if task.manifest {
            say(self.session, crate::resume::Refusal::Manifest.note());
            return None;
        }
        // A process runs it, so it is joined and not resumed: a second writer would fork the
        // record (BG-9).
        if task.live {
            if !self.reachable(&task.directory) {
                return None;
            }
            return Some(Ended::Open(Elsewhere {
                directory: task.directory.clone(),
                how: How::Attach(task.id.clone()),
            }));
        }
        let record =
            match crate::resume::named(Path::new(&task.directory), self.stored.id(), &task.id) {
                Ok(record) => record,
                Err(refused) => {
                    say(self.session, refused.note());
                    return None;
                }
            };
        if !self.here(task) {
            if !self.reachable(&task.directory) {
                return None;
            }
            return Some(Ended::Open(Elsewhere {
                directory: task.directory.clone(),
                how: How::Resume(task.id.clone()),
            }));
        }
        self.session.close_sessions_screen();
        *switch = Some(switch_session(
            self.session,
            self.workspace,
            servers,
            record,
        ));
        Some(left_behind(self.stored))
    }

    /// Copy a task's record and carry on in the copy (SCREEN-8).
    pub(super) fn fork_task(
        &mut self,
        servers: &mut Servers,
        switch: &mut Option<Switch>,
        task: &listing::Task,
    ) -> Option<Ended> {
        if task.manifest {
            say(self.session, crate::resume::Refusal::Manifest.note());
            return None;
        }
        if !self.here(task) {
            if !self.reachable(&task.directory) {
                return None;
            }
            return Some(Ended::Open(Elsewhere {
                directory: task.directory.clone(),
                how: How::Fork(task.id.clone()),
            }));
        }
        match sessions::fork(Path::new(&task.directory), &task.id) {
            Some(record) => {
                self.session.close_sessions_screen();
                *switch = Some(switch_session(
                    self.session,
                    self.workspace,
                    servers,
                    Box::new(record),
                ));
                Some(left_behind(self.stored))
            }
            None => {
                let name = shown(task);
                say(
                    self.session,
                    t!(sessions_screen_note_fork_failed, name = name),
                );
                None
            }
        }
    }

    /// Begin a session where the task under the cursor ran, when that is not here. `None` where it
    /// is here, or nowhere in particular, and the session is begun in this process; otherwise what
    /// to do in its place, which is nothing but a note when the directory is gone.
    pub(super) fn fresh_elsewhere(&mut self, directory: Option<String>) -> Option<Option<Ended>> {
        let directory =
            directory.filter(|directory| Path::new(directory) != self.workspace.root())?;
        if !self.reachable(&directory) {
            return Some(None);
        }
        Some(Some(Ended::Open(Elsewhere {
            directory,
            how: How::Fresh,
        })))
    }
}
