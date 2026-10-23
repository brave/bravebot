//! Handing the terminal to the interface and back: the modes it draws in, and the editor and pager
//! that take the screen for a while.

use super::*;

/// Asks for motion reported only while a button is held.
///
/// Sent after [`EnableMouseCapture`], which asks for all three tracking modes at once, including
/// the one that reports a pointer merely crossing the window. Turning that one off is the whole
/// intent, but terminals disagree about what the three modes are: some keep a flag per mode and
/// use the highest one set, others keep a single state that the last request wins. Sending the
/// two that are wanted again, after the one that is not, lands both kinds in the same place.
/// Without that, a terminal of the second kind reads it as "no tracking at all" and the wheel
/// goes back to scrolling the window behind the session.
pub(super) const TRACK_MOTION_ONLY_WHILE_DRAGGING: &str = "\x1b[?1003l\x1b[?1000h\x1b[?1002h";

/// Put the terminal into the state the interface draws in.
///
/// Mouse capture is what makes the wheel scroll the transcript. It costs the terminal's own text
/// selection, so it is given back on the way out. `mouse` is `tui.mouse`: where it is off, capture
/// is never asked for and the terminal keeps its own selection and wheel.
///
/// Bracketed paste is what keeps a pasted prompt from sending itself. Without it the terminal
/// delivers a paste as ordinary keystrokes, and the newline most clipboards carry at the end
/// arrives as Enter.
///
/// Disambiguated keys are what makes Shift-Enter reach the interface at all. A terminal sends the
/// same byte for Enter however it was pressed, so without this the modifier is not merely ignored,
/// it never arrives, and a newline in the prompt would be unreachable.
///
/// Asked for only where the terminal says it understands the request. Sending it blind to one that
/// does not leaves the escape sequence on the screen.
///
/// One definition rather than one per caller, because the interface gives the terminal away and
/// takes it back again whenever the prompt is edited elsewhere, and a difference between the two
/// setups would show as a mode that only survives until the first edit.
pub(super) fn take_over_terminal<W: Write>(out: &mut W, mouse: bool) -> io::Result<()> {
    enable_raw_mode()?;
    // After raw mode, so the reply arrives as bytes rather than a line, and before the
    // alternate screen, so the query is not painted into the session.
    crate::theme::sense(out);
    crate::theme::restore_saved();
    ask_for_modes(out, enhanced_keys(), mouse, crate::pointer::is_asked())
}

/// Ask the terminal for every mode the interface draws and reads in.
///
/// Separate from raw mode, and told rather than asked whether the terminal understands
/// disambiguated keys, because both of those are properties of a real tty: what is left here is
/// bytes, so a test can read back what was asked for and what was given up. `hover` is whether the
/// terminal draws the pointer shape it is asked for ([`crate::pointer::is_asked`]).
pub(super) fn ask_for_modes<W: Write>(
    out: &mut W,
    enhanced: bool,
    mouse: bool,
    hover: bool,
) -> io::Result<()> {
    execute!(out, EnterAlternateScreen)?;
    if mouse {
        execute!(out, EnableMouseCapture)?;
    }
    execute!(
        out,
        EnableBracketedPaste,
        // Asked for so the clipboard can be looked at when the user comes back from copying
        // something, which is the moment a picture appears on it and the only cheap moment to
        // notice. Polling instead would spawn a clipboard tool every few frames forever.
        EnableFocusChange
    )?;

    if enhanced {
        execute!(
            out,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )?;
    }

    // Mouse capture asks for motion reported whether or not a button is down, which is a stream
    // of events for a pointer merely crossing the window and a redraw for each one. Only the
    // drag matters here, so all-motion reporting goes back off: what stays on reports the
    // buttons, the wheel, and motion while a button is held, which is the gesture being read.
    // Except where the terminal draws a pointer shape on request: finding the border to draw it
    // over takes the pointer's position without a button, so all motion stays on there. Capture
    // asks for it last of the three tracking modes, which is the one a terminal that keeps a single
    // tracking state ends in.
    if mouse && !hover {
        write!(out, "{TRACK_MOTION_ONLY_WHILE_DRAGGING}")?;
    }
    out.flush()
}

/// Put the terminal back the way it was found.
pub(super) fn hand_back_terminal<W: Write>(out: &mut W) -> io::Result<()> {
    disable_raw_mode()?;
    // Before the screen is left: a terminal that keeps a pointer shape for each screen applies the
    // reset to the one it is on, and the shape was asked for on this one.
    crate::pointer::give_back(out)?;
    give_back_modes(out, enhanced_keys())?;
    crate::title::give_back(out)
}

/// Give back every mode [`ask_for_modes`] asked for.
pub(super) fn give_back_modes<W: Write>(out: &mut W, enhanced: bool) -> io::Result<()> {
    // Popped before the modes below, so the stack is unwound in the order it was built.
    if enhanced {
        execute!(out, PopKeyboardEnhancementFlags)?;
    }
    execute!(
        out,
        DisableBracketedPaste,
        DisableFocusChange,
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    out.flush()
}

/// Whether the terminal understands the request for disambiguated keys.
///
/// Asked once and remembered, because answering means writing a query and waiting for a reply, and
/// doing that on every handover to an editor would cost a round trip each time.
pub(super) fn enhanced_keys() -> bool {
    use std::sync::OnceLock;
    static SUPPORTED: OnceLock<bool> = OnceLock::new();
    *SUPPORTED.get_or_init(|| supports_keyboard_enhancement().unwrap_or(false))
}

/// Hand the terminal to the user's editor, and take back whatever they saved.
///
/// The editor gets the terminal properly rather than sharing it: raw mode off, the alternate
/// screen left, the mouse and paste modes given back. A full-screen editor drawing over an
/// interface that still believes it owns the screen is the alternative, and neither of them
/// would be legible.
///
/// The screen is cleared on the way back because the alternate screen came back empty while the
/// interface still holds the frame it drew before leaving. Without it the first redraw sends only
/// what changed, over a screen that has nothing under it.
pub(super) fn edit_prompt(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
) -> io::Result<()> {
    hand_back_terminal(terminal.backend_mut())?;
    terminal.show_cursor()?;

    let edited = crate::editor::edit(session.input());

    take_over_terminal(terminal.backend_mut(), session.mouse_captured())?;
    terminal.clear()?;

    match edited {
        Ok(line) => session.take_edited(line),
        // Said rather than swallowed: an editor that would not start looks exactly like a key
        // that does nothing, and the user has no other way to tell the two apart.
        Err(failure) => session.note(failure.to_string()),
    }
    Ok(())
}

/// Hand the transcript to the user's editor, and take nothing back from it.
///
/// The terminal is given up and taken back the way it is for a prompt, since an editor needs the
/// screen. Nothing is read afterwards: the file was a look at the record, not a draft of it.
pub(super) fn show_transcript(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
) -> io::Result<()> {
    let text = render::as_text(session);

    hand_back_terminal(terminal.backend_mut())?;
    terminal.show_cursor()?;

    let shown = crate::editor::show(&text);

    take_over_terminal(terminal.backend_mut(), session.mouse_captured())?;
    terminal.clear()?;

    // Said rather than swallowed, for the reason the prompt's editor says it: an editor that
    // would not start looks exactly like a key that does nothing.
    if let Err(failure) = shown {
        session.note(failure.to_string());
    }
    Ok(())
}

/// Hand the terminal to the user's editor on the body of a write that is waiting for an answer, and
/// take back what they saved.
///
/// The terminal is given up and taken back the way it is for a prompt, and for the same reason: an
/// editor needs the screen. `mouse` is whether capture was on, read by the caller because the
/// session cannot be borrowed while the prompt that called this is open. What comes back is text
/// for the write prompt to put to the person again, so a failure is returned as the sentence the
/// prompt says, rather than noted on a transcript the prompt is drawn over.
pub(super) fn edit_body(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    mouse: bool,
    body: &str,
    path: &str,
) -> Result<String, String> {
    hand_back_terminal(terminal.backend_mut()).map_err(|error| error.to_string())?;
    // A cursor that cannot be shown does not stop the editor from running.
    let _ = terminal.show_cursor();

    let edited = crate::editor::edit_body(body, &crate::editor::scratch_extension(path));

    take_over_terminal(terminal.backend_mut(), mouse).map_err(|error| error.to_string())?;
    terminal.clear().map_err(|error| error.to_string())?;

    edited.map_err(|failure| failure.to_string())
}

/// Stop this process the way the terminal would have on Ctrl-Z, and return once it is continued.
///
/// The session holds the terminal in raw mode, so the driver never turns the key into `SIGTSTP`
/// and this sends it instead (INPUT-47). It goes to the whole process group, as the driver sends
/// it: a session started in a pipeline or by a wrapper has other members, and a shell reports a job
/// stopped only when every member of its group has, so one left running would leave the shell
/// waiting on a job it cannot tell is not done. A command a turn started leads a group of its own
/// (RUN-11) and is not reached.
///
/// Sent from the thread that reads the terminal, which leads the process, so the signal is
/// delivered before the call returns and what follows it runs only after `SIGCONT`.
#[cfg(unix)]
pub(super) fn stop_until_continued() -> io::Result<()> {
    rustix::process::kill_current_process_group(rustix::process::Signal::TSTP)
        .map_err(io::Error::from)
}

/// Run `stop` with the terminal given away: given back before it, taken again after it.
///
/// The order is the whole of what a stop owes the shell (INPUT-47): a stop sent while the screen of
/// its own and raw mode are still on leaves the shell drawing under a mouse it does not know is
/// reported, and a session that does not take the terminal again is a screen nobody draws on. So
/// the terminal is not stopped over when it could not be given back, and it is taken again whatever
/// the stop did. What the stop reports is the caller's to say, which is why it is not a result.
///
/// Generic in what it holds so the order can be watched without a terminal, which a test does not
/// have.
#[cfg(unix)]
pub(super) fn around_a_stop<T>(
    held: &mut T,
    give_back: impl FnOnce(&mut T) -> io::Result<()>,
    stop: impl FnOnce(),
    take_again: impl FnOnce(&mut T) -> io::Result<()>,
) -> io::Result<()> {
    give_back(held)?;
    stop();
    take_again(held)
}

/// Make `attempt` again for as long as it reports that a signal interrupted it.
///
/// A session the shell continues with `bg` is outside the terminal's foreground group, and a
/// process there is stopped by the terminal at the first mode it sets (INPUT-47). The call that was
/// stopped reports itself interrupted when `fg` continues the session, and the same call made again
/// is the one that works. Every pass either succeeds or stops the process again, so this does not
/// spin, and any other failure is returned as it came.
#[cfg(unix)]
pub(super) fn until_not_interrupted<T>(
    mut attempt: impl FnMut() -> io::Result<T>,
) -> io::Result<T> {
    loop {
        match attempt() {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            settled => return settled,
        }
    }
}

/// Give the terminal to the shell, stop until the shell continues the session, and take it back.
///
/// The terminal is given up and taken again as it is for an editor, through the same two functions,
/// so the modes the session asks for are the ones it gives back (INPUT-33). The session is not
/// touched: the line, the caret and what is attached are where they were when the key was pressed.
///
/// The screen is cleared on the way back for the reason an editor's is. The alternate screen is
/// empty, and the interface holds the frame it drew before leaving, so a redraw that sent only what
/// changed would paint over nothing.
#[cfg(unix)]
pub(super) fn suspend_session(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
) -> io::Result<()> {
    let mouse = session.mouse_captured();
    let mut failure = None;

    let taken_again = around_a_stop(
        terminal,
        |terminal| {
            hand_back_terminal(terminal.backend_mut())?;
            terminal.show_cursor()
        },
        || failure = stop_until_continued().err(),
        |terminal| {
            until_not_interrupted(|| take_over_terminal(terminal.backend_mut(), mouse))?;
            terminal.clear()
        },
    );

    // Said rather than swallowed, for the reason an editor that will not start is: a key that
    // does nothing and explains nothing reads as an interface that has stopped answering. Before
    // the terminal's own failure is returned, which ends the session and takes the note with it.
    if let Some(failure) = failure {
        session.note(t!(suspend_failed, reason = failure.to_string()));
    }
    taken_again
}
