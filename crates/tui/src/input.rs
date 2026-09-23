//! Reading the terminal, and saying whether an event arrived on its own.
//!
//! A terminal delivers one byte stream and says nothing about who wrote it. A person at a keyboard
//! and a program holding the other end of the pty arrive the same way, byte for byte, so no reader
//! can ask which it has. This module does not try. **Every event is delivered exactly as the
//! terminal reported it**, and what this adds is one fact beside each: whether it was the whole of
//! what was waiting, or arrived together with others.
//!
//! That fact is worth having because it is the only thing a terminal leaves answerable, and three
//! decisions ask for it: the rung that ends a session, the questions a session opens with, and the
//! return that takes the line out of the box. Each starts or ends something on one key, and a
//! gesture asking for a second press is asking for a press: two bytes in one write are no harder to
//! send than one, so `\x03\x03` is two key events and not two presses. Every other reader answers
//! the event it was given, because moving a caret costs nothing if a program does it.
//!
//! **Nothing is withheld, reclassified or delayed here.** A reader that held a person's keystrokes
//! back would be wrong about a fast typist behind a slow redraw, about tmux and about ssh, and being
//! wrong that way costs somebody their own line and their own paste. So the refusing is done by the
//! three above, where it costs one key and says so, rather than by this module, where it would cost
//! text nobody gets back.
//!
//! What that leaves is a narrower version of the same mistake, said plainly rather than denied: a
//! keystroke of a person's own that a terminal delivered alongside something else is refused by
//! those three, and over an intermediary that coalesces, an Enter can be one. The refusal is one key
//! and a line saying which, and the line stays where it was, which is the difference.
//!
//! Every reader in this crate goes through [`read`] and [`poll`] rather than calling crossterm
//! directly, because what arrived together can only be seen where the whole of it is visible: a
//! prompt reading the terminal itself would take the first key of a burst with nothing behind it and
//! believe it arrived alone.

use ratatui::crossterm::event::{self, Event as TermEvent, KeyEvent};
use std::collections::VecDeque;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;

/// The most events taken as having arrived together.
///
/// The gathering loop ends when the terminal has nothing more waiting, which is the ordinary way
/// out, so this bounds only the pathological case: a writer streaming without pause, which would
/// otherwise hold the loop for as long as it kept writing and leave the screen unredrawn. Past the
/// cap the rest is left where it is and is read next time, so nothing is dropped.
const MOST_AT_ONCE: usize = 8192;

/// Events taken from the terminal and not yet handed out, each with whether it arrived alone.
///
/// Everything waiting has to be read before any of it can be said to have arrived alone, so the
/// reader buffers. Shared rather than per thread because the queue must be the same queue whoever
/// reads: the terminal is one stream, and a second queue would hand out events in an order nothing
/// wrote them in.
///
/// Locked here rather than by each caller, so the one thing to say about a poisoned lock is said
/// once: a thread that panicked while holding this left a queue of terminal events, which is a
/// `VecDeque` whose invariants a panic elsewhere cannot have broken. Reading on is right, and the
/// alternative is an interface that stops accepting keys because something unrelated failed.
fn pending() -> MutexGuard<'static, VecDeque<(TermEvent, bool)>> {
    static PENDING: OnceLock<Mutex<VecDeque<(TermEvent, bool)>>> = OnceLock::new();
    PENDING
        .get_or_init(|| Mutex::new(VecDeque::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Whether the event [`read`] last returned was the whole of what was waiting.
fn alone() -> &'static AtomicBool {
    static ALONE: AtomicBool = AtomicBool::new(true);
    &ALONE
}

/// Whether the last event handed out arrived on its own.
///
/// `true` where the terminal had one event waiting, which is what a person pressing a key looks
/// like: nothing fills the buffer between one read and the next. `false` where several were
/// available at the same instant, so no one of them is evidence separate from the rest.
///
/// Read straight after a [`read`], by the one caller that asks. It is a property of the last event
/// handed out rather than of the terminal now, so asking later asks about the wrong event.
pub fn the_last_event_arrived_alone() -> bool {
    alone().load(Ordering::Relaxed)
}

/// The key press an event is, or `None` for an event that is not one.
///
/// Most callers are prompts, which answer a key and discard everything else, and this spares each of
/// them a match on an event they would drop anyway.
pub fn key_of(event: &TermEvent) -> Option<KeyEvent> {
    match event {
        TermEvent::Key(key) => Some(*key),
        _ => None,
    }
}

/// The character a key stands for, or `None` for a key that stands for none.
///
/// A mapping and nothing more: it decides nothing about who pressed the key and withholds nothing.
/// A caller that refused to act on a key can use this to keep what the key spelled, so words another
/// program typed can be shown to somebody rather than disappearing.
///
/// Enter is a newline and Tab a tab, because a command line written into a terminal carries both and
/// dropping them would join two lines into one. Every chord stands for no text: what a program wrote
/// is words, and a chord inside words was not typed by anybody.
pub fn text_of(key: &KeyEvent) -> Option<char> {
    use ratatui::crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
    if key.kind == KeyEventKind::Release {
        return None;
    }
    let bare = (key.modifiers - KeyModifiers::SHIFT).is_empty();
    match key.code {
        KeyCode::Char(c) if bare => Some(c),
        KeyCode::Enter if bare => Some('\n'),
        KeyCode::Tab if bare => Some('\t'),
        _ => None,
    }
}

/// Whether an event is waiting, here or at the terminal.
///
/// Answers for the buffer first, because an event already taken is an event waiting: a caller
/// asking this to decide whether to redraw or to keep reading would otherwise be told the terminal
/// is quiet while gathered events sit unread.
pub fn poll(timeout: Duration) -> io::Result<bool> {
    if !pending().is_empty() {
        return Ok(true);
    }
    event::poll(timeout)
}

/// The next event, blocking until there is one.
///
/// Hands out what is buffered before reading the terminal again, so the order the terminal wrote
/// events in is the order callers see them. Every event the terminal reported is returned: this
/// never answers with nothing, which is what lets a caller call it after [`poll`] has said an event
/// is waiting and be sure of getting one.
pub fn read() -> io::Result<TermEvent> {
    if let Some((event, on_its_own)) = pending().pop_front() {
        alone().store(on_its_own, Ordering::Relaxed);
        return Ok(event);
    }

    gather()?;

    // Non-empty by construction: `gather` queues at least the event it blocked for.
    let (event, on_its_own) = pending().pop_front().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "the terminal was read and nothing was queued",
        )
    })?;
    alone().store(on_its_own, Ordering::Relaxed);
    Ok(event)
}

/// Read everything the terminal has waiting and queue it, noting whether it was one event.
fn gather() -> io::Result<()> {
    let mut taken = vec![event::read()?];
    while taken.len() < MOST_AT_ONCE && event::poll(Duration::ZERO)? {
        taken.push(event::read()?);
    }

    let mut queue = pending();
    for pair in tagged(taken) {
        queue.push_back(pair);
    }
    Ok(())
}

/// What a read of the terminal becomes: each event, and whether it was the whole of the read.
///
/// One event means it was all that was waiting, which is what a person pressing a key looks like.
/// Two or more were available together, whatever they are: a key beside a resize is no more a
/// separate press than two keys.
///
/// Separated from the reading so both halves of the promise can be tested without a terminal. The
/// halves are that every event is handed back, which is what lets [`read`] be called after [`poll`]
/// has said one is waiting, and that what the tag says matches how many arrived.
fn tagged(taken: Vec<TermEvent>) -> Vec<(TermEvent, bool)> {
    let on_its_own = taken.len() == 1;
    taken.into_iter().map(|event| (event, on_its_own)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{KeyCode, KeyModifiers};

    fn key(code: KeyCode) -> TermEvent {
        TermEvent::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    /// One event waiting is what a person pressing a key looks like, since nothing fills the buffer
    /// between one read and the next. This is the fact the three decisions rest on.
    #[test]
    fn one_event_in_a_read_arrived_on_its_own() {
        assert_eq!(
            tagged(vec![key(KeyCode::Char('y'))]),
            vec![(key(KeyCode::Char('y')), true)]
        );
    }

    /// Two or more were available at the same instant, so neither is evidence separate from the
    /// other. `\x03\x03` in one write is this, and it is what stops the offer being armed and taken
    /// by one write.
    #[test]
    fn several_events_in_a_read_arrived_together() {
        let interrupt = TermEvent::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert_eq!(
            tagged(vec![interrupt.clone(), interrupt.clone()]),
            vec![(interrupt.clone(), false), (interrupt, false)]
        );
    }

    /// Whatever they are. A key beside a resize is no more a separate press than two keys, so the
    /// tag is about how many arrived rather than about what they were.
    #[test]
    fn a_key_beside_something_that_is_not_one_arrived_together() {
        let tags: Vec<bool> = tagged(vec![key(KeyCode::Enter), TermEvent::Resize(80, 24)])
            .into_iter()
            .map(|(_, alone)| alone)
            .collect();
        assert_eq!(tags, vec![false, false]);
    }

    /// Every event is handed back, which is what lets [`read`] be called after [`poll`] has said one
    /// is waiting and be sure of getting one. A read that swallowed an event would send `read` back
    /// to block in the terminal, stalling every loop shaped `while poll(ZERO) { read() }`.
    #[test]
    fn nothing_read_from_the_terminal_is_swallowed() {
        for count in 1..=5 {
            let read: Vec<TermEvent> = (0..count).map(|_| key(KeyCode::Down)).collect();
            assert_eq!(
                tagged(read).len(),
                count,
                "a read of {count} event(s) handed back a different number"
            );
        }
    }

    /// The reader adds a fact and takes nothing away, so there is nothing here that decides what an
    /// event is. What the fact is used for is tested where it is used, at the rung that leaves.
    #[test]
    fn the_flag_starts_out_saying_a_key_arrived_alone() {
        assert!(
            the_last_event_arrived_alone(),
            "a session that has read nothing yet must not treat a first press as crowded"
        );
    }
}
