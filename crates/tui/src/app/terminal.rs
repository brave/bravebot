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
/// selection, so it is given back on the way out.
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
pub(super) fn take_over_terminal<W: Write>(out: &mut W) -> io::Result<()> {
    enable_raw_mode()?;
    // After raw mode, so the reply arrives as bytes rather than a line, and before the
    // alternate screen, so the query is not painted into the session.
    crate::theme::sense(out);
    crate::theme::restore_saved();
    ask_for_modes(out, enhanced_keys())
}

/// Ask the terminal for every mode the interface draws and reads in.
///
/// Separate from raw mode, and told rather than asked whether the terminal understands
/// disambiguated keys, because both of those are properties of a real tty: what is left here is
/// bytes, so a test can read back what was asked for and what was given up.
pub(super) fn ask_for_modes<W: Write>(out: &mut W, enhanced: bool) -> io::Result<()> {
    execute!(
        out,
        EnterAlternateScreen,
        EnableMouseCapture,
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
    write!(out, "{TRACK_MOTION_ONLY_WHILE_DRAGGING}")?;
    out.flush()
}

/// Put the terminal back the way it was found.
pub(super) fn hand_back_terminal<W: Write>(out: &mut W) -> io::Result<()> {
    disable_raw_mode()?;
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

    take_over_terminal(terminal.backend_mut())?;
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

    take_over_terminal(terminal.backend_mut())?;
    terminal.clear()?;

    // Said rather than swallowed, for the reason the prompt's editor says it: an editor that
    // would not start looks exactly like a key that does nothing.
    if let Err(failure) = shown {
        session.note(failure.to_string());
    }
    Ok(())
}
