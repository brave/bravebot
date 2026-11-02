//! BOT-17.

use super::{Action, TermEvent, copy_selection, idle_action, redraw, took_input};
use crate::bots_pane::State;
use crate::input;
use crate::layout::Layout;
use crate::state::Session;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io;

#[derive(Debug)]
pub(super) enum Exit {
    Quit,
    Layout(Layout),
    Failed(io::Error),
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Step {
    Stay(Action),
    Quit,
    Layout(Layout),
}

pub(super) fn step(session: &mut Session, taken: TermEvent) -> Step {
    let noted = what_the_screen_says(session);
    took_input(session, &taken);
    match idle_action(session, taken) {
        Action::Quit => Step::Quit,
        Action::Layout(layout) => Step::Layout(layout),
        Action::None if what_the_screen_says(session) != noted => Step::Stay(Action::Redraw),
        other => Step::Stay(other),
    }
}

fn what_the_screen_says(session: &Session) -> (Option<String>, Option<String>) {
    (
        crate::render::note_at_the_right(session),
        session.bots().message().map(str::to_string),
    )
}

pub(super) fn opened(
    confinement: String,
    sockets_with_a_path_reachable: bool,
    sandbox: crate::sandbox_command::Sandboxed,
    bots: State,
    columns: u16,
) -> Session {
    let settings = bravebot_config::Settings::load();
    let mut session = Session::new(confinement)
        .with_sockets_with_a_path_reachable(sockets_with_a_path_reachable)
        .with_sandbox(sandbox)
        .in_layout(Layout::Bots)
        .with_stored_history()
        .with_bots(bots)
        .with_no_session_begun();
    session.adopt_editing(settings.editor_mode());
    session.adopt_keybindings(settings.keybindings());
    session.adopt_wheel_rows(settings.wheel_rows());
    session.adopt_mouse(settings.tui_mouse());
    session.ctrl_enter_arrives = cfg!(windows) || super::enhanced_keys();
    session.start_in_the_list_where_it_is_drawn(columns);
    session
}

pub(super) fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    confinement: String,
    sockets_with_a_path_reachable: bool,
    sandbox: crate::sandbox_command::Sandboxed,
) -> Exit {
    let columns = terminal.size().map_or(0, |size| size.width);
    let mut session = opened(
        confinement,
        sockets_with_a_path_reachable,
        sandbox,
        State::read(),
        columns,
    );
    match drive(terminal, &mut session) {
        Ok(exit) => exit,
        Err(failure) => Exit::Failed(failure),
    }
}

pub(super) fn drop_the_ended_sessions_title<W: std::io::Write>(out: &mut W) -> io::Result<()> {
    crate::title::show(out, "")
}

pub(super) fn owes_a_frame(taken: Option<&Step>) -> bool {
    !matches!(taken, None | Some(Step::Stay(Action::None)))
}

fn drive(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
) -> io::Result<Exit> {
    drop_the_ended_sessions_title(terminal.backend_mut())?;
    let mut owed = true;
    loop {
        if owed {
            redraw(terminal, session)?;
        }
        crate::pointer::show(terminal.backend_mut(), session.pointer_shape())?;
        let taken = if input::poll(super::POLL)? {
            Some(step(session, input::read()?))
        } else {
            None
        };
        owed = owes_a_frame(taken.as_ref());
        match taken {
            None => {}
            Some(Step::Quit) => return Ok(Exit::Quit),
            Some(Step::Layout(layout)) => return Ok(Exit::Layout(layout)),
            Some(Step::Stay(Action::Copy)) => copy_selection(terminal, session)?,
            Some(Step::Stay(Action::Paste)) => {
                super::take_from_clipboard(session, crate::clipboard::paste());
            }
            #[cfg(unix)]
            Some(Step::Stay(Action::Suspend)) => super::suspend_session(terminal, session)?,
            Some(Step::Stay(_)) => {}
        }
        if session.is_quitting() {
            return Ok(Exit::Quit);
        }
    }
}
