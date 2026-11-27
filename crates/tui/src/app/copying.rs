//! What `/copy`, a selection release and a click on a reply's marker put on the clipboard, and
//! handing a clicked web address to the browser.

use super::*;

/// Take what the selection covers and put it on the clipboard.
///
/// What the user swept over is what they saw: wrapped, scrolled and trimmed exactly as it was
/// drawn. So it is read back off a frame rather than out of the transcript, which would have to
/// be laid out a second time to say what any of it looked like.
///
/// Drawn again to get one. A finished draw resets the buffer the next one will be built in, so
/// the frame that drew the screen is the only place the screen can be read from.
///
/// A click that swept over nothing just puts the selection away.
///
/// `copy` puts the text where it is going and says whether anything took it, as the one `/copy`
/// is given does, so a test can see what a release hands over.
pub(super) fn copy_selection<B>(
    terminal: &mut Terminal<B>,
    session: &mut Session,
    copy: impl FnOnce(&str) -> bool,
) -> io::Result<()>
where
    B: ratatui::backend::Backend,
    B::Error: Send + Sync + 'static,
{
    let Some(selection) = session.selection else {
        return Ok(());
    };
    if selection.is_empty() {
        session.clear_selection();
        return Ok(());
    }

    let text = {
        // Drawn to be read back rather than to be looked at. The view may have scrolled since the
        // last frame was drawn, so the layout this one reports is recorded as any frame's is, and the
        // selection is read against the rows it drew.
        let mut laid = crate::state::Laid::default();
        let completed = terminal
            .draw(|frame| laid = render::draw(frame, session))
            .map_err(io::Error::other)?;
        session.note_layout(laid);
        let Some(selection) = session.selection else {
            return Ok(());
        };
        selected_text(session, completed.buffer, &selection)
    };

    // Nothing but the padding between widgets, which is not something to put on a clipboard and
    // not something to claim to have copied either.
    if text.is_empty() {
        return Ok(());
    }

    if copy(&text) {
        session.note_copied_from(text.chars().count(), selection.pane());
    }
    Ok(())
}

/// Carry out the copy a click on a reply's marker asked for, in a loop that runs while work does.
///
/// [`answer_mouse`] carries out a copy of a selection itself, so a reply copied by a click is carried
/// out here, where `/copy` carries it out mid-turn. A command answered
/// while work runs takes the view to the tail so its answer is read, and a click is made from
/// wherever the person is reading, so the view is left there.
pub(super) fn copy_a_reply_clicked(
    session: &mut Session,
    action: Action,
    copy: impl FnOnce(&str) -> bool,
) {
    let Action::CopyReply(text) = action else {
        return;
    };
    let held = session.scroll;
    session.answer_while_working(|session| copy_reply(session, &text, copy));
    session.scroll = held;
}

/// What `selection` covers on the drawn `buffer`, as it is put on the clipboard.
///
/// The rows of the info panel that continue on the row below, as the two halves of the session id
/// do, are joined without a line break, and so are the transcript's rows that one wrapped line
/// was drawn over.
pub(super) fn selected_text(
    session: &Session,
    buffer: &ratatui::buffer::Buffer,
    selection: &select::Selection,
) -> String {
    // A selection of the transcript is read from the transcript's own rows, since it can reach rows
    // that are off the screen (MOUSE-35).
    if selection.scrolls() {
        let Some(window) = session.laid.window else {
            return String::new();
        };
        let rows = render::Rows::new(session, window);
        return select::text_of_transcript(selection, |first, last| rows.window(first, last));
    }
    let joined = session
        .laid
        .panes
        .iter()
        .find(|(pane, _)| *pane == select::Pane::Panel)
        .map(|(_, rect)| crate::panel::continued_rows(session, *rect))
        .unwrap_or_default();
    select::text_without_chrome(
        buffer,
        selection,
        &joined,
        &session.laid.continued,
        &session.laid.chrome,
    )
}

/// What `/copy` was asked for: a reply, or with the word `block` a fenced block of one (MOUSE-27), or
/// with the word `text` a reply as the screen shows it (MOUSE-52).
pub(super) fn text_to_copy(session: &Session, argument: &str) -> Result<String, String> {
    if let Some(back) = after_the_word(argument, COPY_BLOCK) {
        return block_to_copy(session, back);
    }
    if let Some(back) = after_the_word(argument, COPY_TEXT) {
        return reply_back(session, back).map(copyable_as_text);
    }
    reply_to_copy(session, argument)
}

/// What follows `word` in `argument`, with the blanks around it left out, where `word` is the whole
/// of the first word: `blocks` is not `block`.
fn after_the_word<'a>(argument: &'a str, word: &str) -> Option<&'a str> {
    let back = argument.strip_prefix(word)?;
    (back.is_empty() || back.starts_with(char::is_whitespace)).then(|| back.trim())
}

/// The block `/copy block` was asked for: the latest when nothing follows the word, otherwise the
/// one that many blocks back, counting the blocks of every reply from the latest as the replies
/// are counted. It is refused as a reply is: a number that is not one, no block at all, and a
/// number past the oldest block each come back as the note that says so.
fn block_to_copy(session: &Session, back: &str) -> Result<String, String> {
    let steps = steps_back(back);
    if steps == 0 {
        return Err(t!(session_copy_block_needs_a_number).to_string());
    }
    if let Some(block) = session.blocks_newest_first().nth(steps - 1) {
        return Ok(copyable(&block));
    }
    match session.blocks_newest_first().count() {
        0 => Err(t!(session_copy_no_block).to_string()),
        blocks => Err(t!(session_copy_block_goes_no_further, blocks = blocks)),
    }
}

/// The words `/copy` puts on the clipboard for the reply it was asked for (CMD-11).
pub(super) fn reply_to_copy(session: &Session, back: &str) -> Result<String, String> {
    reply_back(session, back).map(copyable)
}

/// The reply `/copy` was asked for: the latest when nothing follows the word, otherwise the one
/// that many replies back. What it refuses comes back as the note that says so (CMD-11).
fn reply_back<'a>(session: &'a Session, back: &str) -> Result<&'a str, String> {
    let steps = steps_back(back);
    if steps == 0 {
        return Err(t!(session_copy_needs_a_number).to_string());
    }
    if let Some(reply) = session.replies_newest_first().nth(steps - 1) {
        return Ok(reply);
    }
    // Saying how far back it does go, since the next thing the person types is that number.
    match session.replies_newest_first().count() {
        0 => Err(t!(session_copy_no_reply).to_string()),
        replies => Err(t!(session_copy_goes_no_further, replies = replies)),
    }
}

/// How many back `/copy` and `/copy block` were asked for: 1 when nothing follows the word, and 0
/// for a word that is not a count, which each caller refuses with its own note.
fn steps_back(back: &str) -> usize {
    // Digits alone: `+2` is a word, and a number too long to hold is still past the oldest reply.
    if back.is_empty() {
        1
    } else if back.bytes().all(|byte| byte.is_ascii_digit()) {
        back.parse::<usize>().unwrap_or(usize::MAX)
    } else {
        0
    }
}

/// A reply as the screen shows it, with every control character left out but its line breaks and
/// tabs: none of the marks of the markdown it was written in, none of the marker and margin the screen
/// draws, and no wrapping (MOUSE-52). What `/copy text` and the item of the menu that says so copy.
pub(super) fn copyable_as_text(words: &str) -> String {
    copyable(&render::reply_as_text(words))
}

/// A reply with every control character left out but its line breaks and tabs.
///
/// The screen draws none of them in a reply, and what is pasted into a shell is read as typed, so
/// a reply holding the sequence that ends a bracketed paste would run whatever followed it.
pub(super) fn copyable(reply: &str) -> String {
    reply
        .chars()
        .filter(|&c| matches!(c, '\n' | '\t') || !c.is_control())
        .collect()
}

/// Put a reply on the clipboard and say how much went, or say that nothing took it.
///
/// Takes the copy as a closure because the real one runs the platform's clipboard tool.
pub(super) fn copy_reply(session: &mut Session, text: &str, copy: impl FnOnce(&str) -> bool) {
    // A sweep's highlight or count left up would say that is what the clipboard holds.
    session.clear_selection();
    if copy(text) {
        session.note_copied(text.chars().count());
    } else {
        session.note(t!(session_copy_failed));
    }
}

/// Hand a web address a person clicked to their browser, and say so where no browser was started
/// (MOUSE-40). The address is the one the cells showed, which the opener tests again.
///
/// `working` is whether a turn or a command is running, where a note is kept off the transcript
/// until it ends as every command's answer is.
pub(super) fn open_link(session: &mut Session, address: &str, working: bool) {
    open_link_with(session, address, working, session.opens_an_address);
}

/// [`open_link`], with what starts the browser stated so a test starts none.
pub(crate) fn open_link_with(
    session: &mut Session,
    address: &str,
    working: bool,
    open: impl FnOnce(&str) -> bool,
) {
    if open(address) {
        return;
    }
    let said = t!(session_link_not_opened).to_string();
    if working {
        session.answer_while_working(|session| session.note(said));
    } else {
        session.note(said);
    }
}
