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
//! The one thing not handed out is a position of the pointer, reported with no button held, that a
//! later one already waiting behind it supersedes. It decides nothing and the next says where the
//! pointer is, so a burst of them is read as the last of it (INPUT-49).
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

use ratatui::crossterm::event::{self, Event as TermEvent, KeyEvent, KeyEventKind};
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

/// Whether the terminal last reported being in front of the person.
fn in_front() -> &'static AtomicBool {
    static IN_FRONT: AtomicBool = AtomicBool::new(true);
    &IN_FRONT
}

/// Whether the terminal is believed to be in front of the person.
///
/// `true` until the terminal reports being left, so one that reports no change of focus is always
/// taken to be in front. Kept here rather than by the readers that act on it, because a prompt
/// discards the events that are not keys and a report it discarded would leave the answer wrong
/// until the next one.
pub fn terminal_is_in_front() -> bool {
    in_front().load(Ordering::Relaxed)
}

/// Take what an event handed out says about the terminal's focus.
fn note_focus(event: &TermEvent) {
    match event {
        TermEvent::FocusGained => in_front().store(true, Ordering::Relaxed),
        TermEvent::FocusLost => in_front().store(false, Ordering::Relaxed),
        _ => {}
    }
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
    use ratatui::crossterm::event::{KeyCode, KeyModifiers};
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
    #[cfg(test)]
    if script::installed() {
        return Ok(true);
    }
    if !pending().is_empty() {
        return Ok(true);
    }
    event::poll(timeout)
}

/// Whether the next event waiting is one a frame may wait out: the pointer's, or a change of size.
///
/// A drag arrives as a stream of positions and a frame for each would trail the pointer, so a burst
/// of them is drawn once, and dragging a window edge is the same stream of sizes. A key, a paste or
/// a focus change is not that: the person is looking at the screen for the result, so it is drawn
/// as soon as it has been handled, however many more are queued behind it. Answers `false` where
/// nothing is waiting.
///
/// Reads what the terminal has waiting into the queue, as a [`read`] would, so a caller asking
/// this and then reading sees the events in the order they were written.
pub fn next_is_a_stream() -> io::Result<bool> {
    if pending().is_empty() {
        if !event::poll(Duration::ZERO)? {
            return Ok(false);
        }
        gather()?;
    }
    Ok(pending()
        .front()
        .is_some_and(|(event, _)| is_a_stream(event)))
}

fn is_a_stream(event: &TermEvent) -> bool {
    matches!(event, TermEvent::Mouse(_) | TermEvent::Resize(..))
}

/// The next event of the queue, with a pointer position that is followed by another one taken to be
/// that one (INPUT-49).
///
/// Only a report of the pointer with no button held is dropped, and only for another of the same kind
/// directly behind it: a drag, a press, a release and the wheel each decide something, and any event
/// between two positions is delivered with both of them left as they were. Reading on past the
/// dropped ones costs nothing and leaves the queue as it would have been had each been handled.
fn take_next(queue: &mut VecDeque<(TermEvent, bool)>) -> Option<(TermEvent, bool)> {
    let mut taken = queue.pop_front()?;
    while is_a_move(&taken.0) && queue.front().is_some_and(|(next, _)| is_a_move(next)) {
        taken = queue.pop_front()?;
    }
    Some(taken)
}

/// Whether an event is the pointer reported at a position with no button held.
///
/// A prompt that reads nothing from the pointer asks this to leave a position out of what it draws
/// again for, since a pointer crossing the window sends one for each cell (MOUSE-44).
pub fn is_a_move(event: &TermEvent) -> bool {
    matches!(
        event,
        TermEvent::Mouse(mouse) if mouse.kind == ratatui::crossterm::event::MouseEventKind::Moved
    )
}

/// The next event, blocking until there is one.
///
/// Hands out what is buffered before reading the terminal again, so the order the terminal wrote
/// events in is the order callers see them. Every event the terminal reported is returned: this
/// never answers with nothing, which is what lets a caller call it after [`poll`] has said an event
/// is waiting and be sure of getting one.
pub fn read() -> io::Result<TermEvent> {
    #[cfg(test)]
    if let Some(scripted) = script::next() {
        alone().store(true, Ordering::Relaxed);
        return scripted;
    }
    if let Some((event, on_its_own)) = take_next(&mut pending()) {
        alone().store(on_its_own, Ordering::Relaxed);
        note_focus(&event);
        return Ok(event);
    }

    gather()?;

    // Non-empty by construction: `gather` queues at least the event it blocked for.
    let (event, on_its_own) = take_next(&mut pending()).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "the terminal was read and nothing was queued",
        )
    })?;
    alone().store(on_its_own, Ordering::Relaxed);
    note_focus(&event);
    Ok(event)
}

/// What a test says the terminal reports, in place of a terminal.
///
/// Held for one thread, so tests that each drive a prompt run side by side without sharing events.
/// While a script is held every [`read`] on the thread answers from it, each event arriving alone
/// as a person's does, and a read past the end is the stream being lost, which every prompt
/// refuses on rather than waiting for.
#[cfg(test)]
pub(crate) mod script {
    use super::{TermEvent, io};
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;

    thread_local! {
        static EVENTS: RefCell<Option<VecDeque<TermEvent>>> = const { RefCell::new(None) };
        static RAN_DRY: Cell<bool> = const { Cell::new(false) };
    }

    /// The script, until it is dropped.
    pub(crate) struct Held;

    impl Drop for Held {
        fn drop(&mut self) {
            EVENTS.with(|events| *events.borrow_mut() = None);
        }
    }

    /// Make `events` what this thread reads.
    pub(crate) fn hold(events: impl IntoIterator<Item = TermEvent>) -> Held {
        EVENTS.with(|held| *held.borrow_mut() = Some(events.into_iter().collect()));
        RAN_DRY.with(|dry| dry.set(false));
        Held
    }

    /// Whether a read found the script empty.
    pub(crate) fn ran_dry() -> bool {
        RAN_DRY.with(Cell::get)
    }

    /// How many events have not been read.
    pub(crate) fn unread() -> usize {
        EVENTS.with(|events| events.borrow().as_ref().map_or(0, VecDeque::len))
    }

    pub(super) fn installed() -> bool {
        EVENTS.with(|events| events.borrow().is_some())
    }

    pub(super) fn next() -> Option<io::Result<TermEvent>> {
        EVENTS.with(|events| {
            let mut events = events.borrow_mut();
            let events = events.as_mut()?;
            Some(events.pop_front().ok_or_else(|| {
                RAN_DRY.with(|dry| dry.set(true));
                io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "the script has no more events",
                )
            }))
        })
    }
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
/// One thing to answer means it was all that was waiting, which is what a person pressing a key looks
/// like. Two or more were available together, whatever they are but a report of the mouse: a key
/// beside a resize is no more a separate press than two keys. A pointer crossing the window sends
/// positions all the time, so the one that lands in the read of a key is no evidence about the key.
///
/// Separated from the reading so both halves of the promise can be tested without a terminal. The
/// halves are that every event is handed back, which is what lets [`read`] be called after [`poll`]
/// has said one is waiting, and that what the tag says matches how many arrived.
fn tagged(taken: Vec<TermEvent>) -> Vec<(TermEvent, bool)> {
    let on_its_own = taken.iter().filter(|event| answerable(event)).count() <= 1;
    taken.into_iter().map(|event| (event, on_its_own)).collect()
}

/// Whether an event is one anything downstream could answer.
///
/// Everything except a key release and a report of the mouse. A release is the tail of a press that
/// has already been answered rather than a second thing arriving, and counting it as one would make a
/// keystroke look like two. That matters because a terminal is free to report releases and some do:
/// Windows sends a key-up for every keystroke, so there a press and its own release land in one read
/// whenever the interface was busy longer than somebody held the key down, and every guard that asks
/// whether a key arrived alone would refuse a press nobody shared with anything.
///
/// A report of the mouse is not counted either, whatever button it names. The interface asks for the
/// position of the pointer with no button held (INPUT-33), so a person moving the mouse while they
/// press a key shares the read with a position, and counting it would refuse that key. The guards ask whether keys arrived as a run, as a paste or a program's write
/// does, and a position, a click or a turn of the wheel is not a key that a run of keys could
/// consist of.
///
/// Nothing is withheld by this: both are still handed out, exactly as the terminal reported them. They
/// are only not counted, which they can afford to be because no guard that asks is answered by one.
fn answerable(event: &TermEvent) -> bool {
    match event {
        TermEvent::Key(key) => key.kind != KeyEventKind::Release,
        TermEvent::Mouse(_) => false,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{KeyCode, KeyModifiers};

    fn released(code: KeyCode) -> TermEvent {
        TermEvent::Key(KeyEvent::new_with_kind(
            code,
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ))
    }

    fn key(code: KeyCode) -> TermEvent {
        TermEvent::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn pointer(kind: ratatui::crossterm::event::MouseEventKind, column: u16) -> TermEvent {
        TermEvent::Mouse(ratatui::crossterm::event::MouseEvent {
            kind,
            column,
            row: 1,
            modifiers: KeyModifiers::NONE,
        })
    }

    fn moved_to(column: u16) -> TermEvent {
        pointer(ratatui::crossterm::event::MouseEventKind::Moved, column)
    }

    /// What reading the whole of `waiting` hands out, one event at a time.
    fn handed_out(waiting: Vec<TermEvent>) -> Vec<TermEvent> {
        let mut queue: VecDeque<(TermEvent, bool)> =
            waiting.into_iter().map(|event| (event, true)).collect();
        let mut handed = Vec::new();
        while let Some((event, _)) = take_next(&mut queue) {
            handed.push(event);
        }
        handed
    }

    /// INPUT-49. Rejects: every position of a burst being handed out, which is a lookup of what is
    /// under the pointer and a decision about a redraw for each of a thousand reports of a pointer
    /// crossing the window, where the last of them says where the pointer is.
    #[test]
    fn a_burst_of_pointer_positions_is_read_as_the_last_of_it() {
        let burst: Vec<TermEvent> = (0..1000).map(moved_to).collect();
        assert_eq!(handed_out(burst), vec![moved_to(999)]);
    }

    /// INPUT-49. Rejects: a position dropped across an event that decides something, so that a press
    /// is read at a place the pointer had left, and a drag, a press, a release or a turn of the wheel
    /// taken for a position and dropped with the rest: only a position directly behind another goes.
    #[test]
    fn only_a_position_directly_behind_another_is_not_handed_out() {
        use ratatui::crossterm::event::{MouseButton, MouseEventKind};
        let press = pointer(MouseEventKind::Down(MouseButton::Left), 2);
        assert_eq!(
            handed_out(vec![moved_to(1), press.clone(), moved_to(3)]),
            vec![moved_to(1), press, moved_to(3)]
        );
        assert_eq!(
            handed_out(vec![
                moved_to(1),
                moved_to(2),
                key(KeyCode::Char('a')),
                moved_to(3),
                moved_to(4)
            ]),
            vec![moved_to(2), key(KeyCode::Char('a')), moved_to(4)]
        );
        for kind in [
            MouseEventKind::Drag(MouseButton::Left),
            MouseEventKind::Down(MouseButton::Right),
            MouseEventKind::Up(MouseButton::Left),
            MouseEventKind::ScrollDown,
        ] {
            let held = vec![pointer(kind, 1), pointer(kind, 2), pointer(kind, 3)];
            assert_eq!(handed_out(held.clone()), held, "{kind:?} was coalesced");
        }
    }

    /// The events a terminal reports for `ESC[<0;300;120M` and the three that go with it: the press,
    /// the drag (button 32), the release (the final `m`) and the two turns of the wheel (64 and 65).
    /// The parser counts from 0, so each is at column 299 and row 119.
    fn reports_past_223() -> Vec<TermEvent> {
        use ratatui::crossterm::event::{MouseButton, MouseEventKind};
        [
            MouseEventKind::Down(MouseButton::Left),
            MouseEventKind::Drag(MouseButton::Left),
            MouseEventKind::Up(MouseButton::Left),
            MouseEventKind::ScrollUp,
            MouseEventKind::ScrollDown,
        ]
        .into_iter()
        .map(|kind| {
            TermEvent::Mouse(ratatui::crossterm::event::MouseEvent {
                kind,
                column: 299,
                row: 119,
                modifiers: KeyModifiers::NONE,
            })
        })
        .collect()
    }

    /// MOUSE-53. The bytes are read by crossterm, which a test cannot give a terminal to read from, so
    /// what is held here is what this crate's reader does with the events it gets back: it hands every
    /// one out whole, at the column and row it was reported at, and a position is superseded by the
    /// one behind it at a column past 223 as at any other. Rejects: a reader that keeps a position
    /// in a byte, or holds it to the range the older reports have, so that the last of a burst past
    /// the edge of that range, or a press, drag, release or turn of the wheel there, is another cell.
    #[test]
    fn a_report_past_column_223_is_handed_out_as_the_cell_it_names() {
        let reports = reports_past_223();
        assert_eq!(handed_out(reports.clone()), reports);

        let burst: Vec<TermEvent> = (200..320).map(moved_to).collect();
        assert_eq!(handed_out(burst), vec![moved_to(319)]);
        assert_eq!(
            handed_out(vec![moved_to(299), moved_to(43)]),
            vec![moved_to(43)],
            "the last of two positions is the one the second names"
        );
        assert_eq!(
            handed_out(vec![moved_to(43), moved_to(299)]),
            vec![moved_to(299)],
            "a position past 223 was taken for one before it"
        );
    }

    /// A frame may wait out a drag or a resize and nothing else. A key waiting behind another key is a person
    /// typing, and holding its frame back is the stutter the coalescing was never meant to cause.
    #[test]
    fn only_a_stream_of_positions_or_sizes_is_an_event_a_frame_waits_out() {
        use ratatui::crossterm::event::{MouseEvent, MouseEventKind};
        let drag = TermEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Moved,
            column: 1,
            row: 1,
            modifiers: KeyModifiers::NONE,
        });
        assert!(is_a_stream(&drag));
        assert!(is_a_stream(&TermEvent::Resize(80, 24)));
        for other in [
            key(KeyCode::Char('a')),
            released(KeyCode::Char('a')),
            TermEvent::Paste("pasted".into()),
            TermEvent::FocusGained,
        ] {
            assert!(!is_a_stream(&other), "{other:?} was coalesced like a drag");
        }
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

    /// One keystroke is one arrival even where the terminal reports it as two events. Windows sends a
    /// key-up for every keystroke, and a press that shared its read with its own release used to be
    /// read as crowded: the line would not send and the rung that leaves could not be reached, on a
    /// platform where nobody had typed anything twice.
    #[test]
    fn a_press_and_its_own_release_are_one_keystroke() {
        let tags: Vec<bool> = tagged(vec![key(KeyCode::Enter), released(KeyCode::Enter)])
            .into_iter()
            .map(|(_, alone)| alone)
            .collect();
        assert_eq!(
            tags,
            vec![true, true],
            "a keystroke reported as a press and a release was counted as two"
        );
    }

    /// And two whole keystrokes in one read are still two, which is the case the fact exists for.
    #[test]
    fn two_keystrokes_in_one_read_arrived_together() {
        let taken = vec![
            key(KeyCode::Char('c')),
            released(KeyCode::Char('c')),
            key(KeyCode::Char('c')),
            released(KeyCode::Char('c')),
        ];
        assert!(
            tagged(taken).into_iter().all(|(_, alone)| !alone),
            "two presses in one write were read as having arrived on their own"
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

    /// The tags of the events a read held, in order.
    fn arrived_alone(read: Vec<TermEvent>) -> Vec<bool> {
        tagged(read).into_iter().map(|(_, alone)| alone).collect()
    }

    /// INPUT-34. The interface asks for the pointer's position with no button held, so a person
    /// moving the mouse as they press Return shares the read with a position. Rejects: counting a
    /// report of the mouse as another arrival, which refuses the Return, the answer to the question a
    /// session opens with and the Ctrl-C that leaves, for a key nobody shared with a program.
    #[test]
    fn a_report_of_the_mouse_in_the_read_of_a_key_is_not_another_arrival() {
        use ratatui::crossterm::event::{MouseButton, MouseEventKind};
        let enter = key(KeyCode::Enter);
        assert_eq!(
            arrived_alone(vec![moved_to(3), enter.clone()]),
            vec![true, true],
            "a position before the key"
        );
        assert_eq!(
            arrived_alone(vec![enter.clone(), moved_to(3)]),
            vec![true, true],
            "a position after the key"
        );
        assert_eq!(
            arrived_alone(vec![moved_to(1), enter.clone(), moved_to(2)]),
            vec![true, true, true],
            "a position on each side of the key"
        );
        for kind in [
            MouseEventKind::Down(MouseButton::Left),
            MouseEventKind::Up(MouseButton::Left),
            MouseEventKind::Drag(MouseButton::Left),
            MouseEventKind::ScrollDown,
        ] {
            assert_eq!(
                arrived_alone(vec![pointer(kind, 1), enter.clone()]),
                vec![true, true],
                "{kind:?} before the key"
            );
        }
    }

    /// INPUT-34. Reports of the mouse among keys take nothing from what makes a run of keys one: two
    /// presses are still two, and a position between them does not make either of them alone.
    /// Rejects: taking a read that holds a report of the mouse as alone whatever else it holds, so
    /// that `\x03\x03` with a position written between the two bytes arms the offer and takes it.
    #[test]
    fn keys_with_reports_of_the_mouse_among_them_are_still_a_run() {
        let interrupt = TermEvent::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert_eq!(
            arrived_alone(vec![interrupt.clone(), moved_to(3), interrupt.clone()]),
            vec![false, false, false]
        );
        assert_eq!(
            arrived_alone(vec![
                moved_to(1),
                key(KeyCode::Char('y')),
                moved_to(2),
                key(KeyCode::Char('y')),
            ]),
            vec![false, false, false, false]
        );
        assert_eq!(
            arrived_alone(vec![moved_to(1), moved_to(2)]),
            vec![true, true],
            "positions with no key in the read"
        );
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

    /// The seam between the queue and the terminal: [`poll`] answers for what is buffered before
    /// asking the terminal, and [`read`] hands that out before touching it. This is what lets a
    /// caller poll and then read and be sure of getting something, and a reader that swallowed an
    /// event would send `read` back to block in the terminal, stalling every loop shaped
    /// `while poll(ZERO) { read() }` until the next key arrived.
    ///
    /// One test rather than several, and it opens with what the flag says before anything is read,
    /// because the queue and the flag are process globals: split up, these assertions would race
    /// each other under parallel execution rather than testing anything. Nothing else in this crate
    /// touches either, and `read` is called exactly as many times as there are events queued, so a
    /// fault fails here instead of reaching the terminal a test does not have. It ends with the
    /// focus flag, which `read` sets from the events it hands out and which is a global for the
    /// same reason.
    #[test]
    fn the_queue_is_answered_before_the_terminal_and_in_arrival_order() {
        assert!(
            the_last_event_arrived_alone(),
            "a session that has read nothing yet must not treat a first press as crowded"
        );

        let arrived = vec![key(KeyCode::Char('s')), key(KeyCode::Enter)];
        {
            let mut queue = pending();
            for pair in tagged(arrived.clone()) {
                queue.push_back(pair);
            }
        }

        assert!(
            poll(Duration::ZERO).expect("a queue with events in it cannot reach the terminal"),
            "an event already taken was reported as nothing waiting"
        );

        for expected in arrived {
            let taken = read().expect("an event was queued, so reading cannot reach the terminal");
            assert_eq!(
                taken, expected,
                "the queue handed its events out in another order"
            );
            assert!(
                !the_last_event_arrived_alone(),
                "one of two events that arrived together was reported as having arrived alone"
            );
        }

        assert!(pending().is_empty(), "the queue held something back");

        // `next_is_a_stream` looks at the front of a non-empty queue without taking it, so a key
        // waiting behind a drag is not reported until the drag has been read.
        let drag = TermEvent::Mouse(ratatui::crossterm::event::MouseEvent {
            kind: ratatui::crossterm::event::MouseEventKind::Moved,
            column: 1,
            row: 1,
            modifiers: KeyModifiers::NONE,
        });
        for (front, behind, stream) in [
            (key(KeyCode::Char('a')), drag.clone(), false),
            (drag, key(KeyCode::Char('a')), true),
        ] {
            {
                let mut queue = pending();
                queue.push_back((front, true));
                queue.push_back((behind, false));
            }
            assert_eq!(
                next_is_a_stream().expect("a queue with events in it cannot reach the terminal"),
                stream,
                "the answer was not about the event at the front of the queue"
            );
            assert_eq!(pending().len(), 2, "asking took an event out of the queue");
            pending().clear();
        }

        // A prompt reads through `read` and throws away every event that is not a key, so a report
        // of the terminal being left or returned to is noted where it is read, and a person who
        // comes back while a question is open is in front when the turn ends.
        assert!(
            terminal_is_in_front(),
            "nothing had reported the terminal left"
        );
        for (reported, in_front) in [
            (TermEvent::FocusLost, false),
            (key(KeyCode::Char('a')), false),
            (TermEvent::FocusGained, true),
        ] {
            pending().push_back((reported.clone(), true));
            assert_eq!(read().expect("an event was queued"), reported);
            assert_eq!(
                terminal_is_in_front(),
                in_front,
                "{reported:?} left the focus wrong"
            );
        }

        // A burst of pointer positions waiting in the queue comes out of `read` as the last of them,
        // with the key behind it left where it was.
        for column in [3, 4, 5] {
            pending().push_back((moved_to(column), false));
        }
        pending().push_back((key(KeyCode::Enter), false));
        assert_eq!(read().expect("positions were queued"), moved_to(5));
        assert_eq!(pending().len(), 1, "the burst was not taken whole");
        assert_eq!(read().expect("a key was queued"), key(KeyCode::Enter));
        assert!(pending().is_empty());

        // The reports at column 300 and row 120 come out of `read` as the cell they name (MOUSE-53).
        for report in reports_past_223() {
            pending().push_back((report, false));
        }
        for expected in reports_past_223() {
            assert_eq!(read().expect("a report was queued"), expected);
        }
        assert!(pending().is_empty());
    }
}
