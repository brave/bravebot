//! Ending a model request early for a line the person typed while it was out (INPUT-10).
//!
//! A turn reads what the person typed between rounds, and a round lasts as long as the model takes
//! to write its reply and the calls in it take to run. While the reply is still being thought out,
//! or is a call being written, nothing has happened that the line could be too late for, so the
//! request is ended and sent again with the line in it instead of being waited out. A reply that
//! is prose is left to finish, because a person is reading it.
//!
//! What this looks at is whether a line has been typed and which kind of event the stream has
//! delivered so far. It reads nothing the model wrote, and nothing the model wrote goes on: the
//! reply that was cut is dropped whole, so no part of it reaches the conversation, a later request
//! or the record.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{Scope, ScopedJoinHandle};
use std::time::Duration;

use bravebot_core::cancel::Cancel;

use crate::interjections::Arrival;

/// How often a request that may be cut short looks for a line. The stream itself looks every 50 ms,
/// so a faster look would not end the request sooner.
const LOOK_EVERY: Duration = Duration::from_millis(25);

/// Whether a request that is out may still be cut short, shared between the stream's progress
/// reports and the watch over the request.
///
/// Set while the reply is thinking or writing a call, and taken away while it is prose, because
/// prose is what a person is reading as it comes.
#[derive(Debug, Clone)]
pub(super) struct Cuttable(Arc<AtomicBool>);

impl Cuttable {
    /// A request that has just gone out has said nothing yet.
    pub(super) fn new() -> Self {
        Self(Arc::new(AtomicBool::new(true)))
    }

    /// Record what the stream has delivered: `reading` is whether the reply is prose that a
    /// person is reading, which is so from the first words until a call begins.
    pub(super) fn note(&self, reading: bool) {
        self.0.store(!reading, Ordering::Release);
    }

    fn is_set(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// A look over one request that cancels `cut` when a line is typed while the request is out and
/// cuttable.
pub(super) struct Watch<'scope> {
    over: Arc<AtomicBool>,
    thread: Option<ScopedJoinHandle<'scope, ()>>,
}

impl<'scope> Watch<'scope> {
    /// Start watching, or `None` where `arrival` comes from no buffer anyone types into, so that
    /// a turn with nobody at the keyboard starts no thread.
    pub(super) fn start(
        scope: &'scope Scope<'scope, '_>,
        arrival: Arrival,
        cuttable: Cuttable,
        cut: Cancel,
    ) -> Option<Self> {
        if !arrival.is_listening() {
            return None;
        }
        let over = Arc::new(AtomicBool::new(false));
        let ended = Arc::clone(&over);
        // Remembered as the request goes out. A line already waiting then was typed before it and
        // is read at the next boundary, as it was before this existed, so only one counted after
        // this cuts the request.
        let before = arrival.typed();
        let thread = scope.spawn(move || {
            while !ended.load(Ordering::Acquire) {
                if arrival.typed() > before && arrival.is_waiting() && cuttable.is_set() {
                    cut.cancel();
                    return;
                }
                std::thread::sleep(LOOK_EVERY);
            }
        });
        Some(Self {
            over,
            thread: Some(thread),
        })
    }

    /// Stop watching, once the request has come back.
    pub(super) fn end(mut self) {
        self.over.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            // The thread holds nothing that can panic, and a panic in it would only have meant the
            // request was not cut, so the result says nothing the turn acts on.
            drop(thread.join());
        }
    }
}

impl Drop for Watch<'_> {
    /// A watch dropped without being ended, by an early return, still stops looking, so the scope
    /// the turn runs in is not held open by a thread polling for a request that is over.
    fn drop(&mut self) {
        self.over.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interjections::Interjections;
    use std::time::Instant;

    /// Waits for `condition`, which the watch makes true on a thread of its own.
    fn within_a_second(condition: impl Fn() -> bool) -> bool {
        let until = Instant::now() + Duration::from_secs(1);
        while Instant::now() < until {
            if condition() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        condition()
    }

    /// A line typed while the reply is no more than thinking cuts the request, and the turn's own
    /// token is left alone. Rejects a watch that cancels the turn's token, which would end the turn
    /// over a line.
    #[test]
    fn a_line_cuts_a_request_that_has_said_nothing_and_leaves_the_turn_going() {
        let buffer = Interjections::new();
        let turn = Cancel::new();
        let request = turn.within();
        std::thread::scope(|scope| {
            let watch = Watch::start(scope, buffer.arrival(), Cuttable::new(), request.clone())
                .expect("a buffer is listening");
            assert!(!request.is_cancelled(), "cut before any line was typed");
            buffer.push("no, the other one".into());
            assert!(
                within_a_second(|| request.is_cancelled()),
                "the line did not cut it"
            );
            watch.end();
        });
        assert!(request.was_cancelled_itself());
        assert!(!turn.is_cancelled(), "a line stopped the turn");
    }

    /// Once prose has arrived the request is not cut, and it is cut again if a retry starts the
    /// reply afresh. Rejects a watch that ignores the stream and cuts a reply a person is reading.
    #[test]
    fn a_reply_that_has_begun_to_say_something_is_not_cut() {
        let buffer = Interjections::new();
        let request = Cancel::new();
        let cuttable = Cuttable::new();
        std::thread::scope(|scope| {
            cuttable.note(true);
            let watch = Watch::start(scope, buffer.arrival(), cuttable.clone(), request.clone())
                .expect("a buffer is listening");
            buffer.push("wait".into());
            std::thread::sleep(LOOK_EVERY * 6);
            assert!(!request.is_cancelled(), "a reply with prose in it was cut");
            cuttable.note(false);
            assert!(
                within_a_second(|| request.is_cancelled()),
                "a reply with no prose was not cut once it had none again"
            );
            watch.end();
        });
    }

    /// A line that was waiting before the request went out does not cut it: it is read at the next
    /// boundary, as a line queued before a request always was. Rejects a watch that reads only
    /// whether a line is waiting, which would take a prompt queued behind a stop into the request
    /// that the stop was meant to end. A line typed after it does cut the request.
    #[test]
    fn a_line_waiting_before_the_request_does_not_cut_it_and_one_typed_after_does() {
        let buffer = Interjections::new();
        buffer.push("queued before".into());
        let request = Cancel::new();
        std::thread::scope(|scope| {
            let watch = Watch::start(scope, buffer.arrival(), Cuttable::new(), request.clone())
                .expect("a buffer is listening");
            std::thread::sleep(LOOK_EVERY * 6);
            assert!(
                !request.is_cancelled(),
                "a line that was waiting before the request cut it"
            );
            buffer.push("typed during".into());
            assert!(
                within_a_second(|| request.is_cancelled()),
                "a line typed while the request was out did not cut it"
            );
            watch.end();
        });
    }

    /// A turn nobody types into starts no thread and so cannot be cut.
    #[test]
    fn nothing_is_watched_where_nobody_types() {
        std::thread::scope(|scope| {
            assert!(
                Watch::start(scope, Arrival::never(), Cuttable::new(), Cancel::new()).is_none()
            );
        });
    }

    /// A line held back from the turn is not for it, so it cuts nothing.
    #[test]
    fn a_held_line_cuts_nothing() {
        let buffer = Interjections::new();
        buffer.push_behind("first".into());
        buffer.begin_turn(0, true);
        buffer.push_behind("later".into());
        assert_eq!(buffer.held(), 2);
        let request = Cancel::new();
        std::thread::scope(|scope| {
            let watch = Watch::start(scope, buffer.arrival(), Cuttable::new(), request.clone())
                .expect("a buffer is listening");
            std::thread::sleep(LOOK_EVERY * 6);
            assert!(!request.is_cancelled(), "a held line cut the request");
            watch.end();
        });
    }
}
