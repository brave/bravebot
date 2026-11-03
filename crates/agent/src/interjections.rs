//! The lines a person sends while a turn runs, held for the turn to take at a round boundary
//! (INPUT-10, INPUT-46, CLI-36).
//!
//! The terminal, the desktop bridge and the lines protocol each have a front end that adds to the
//! back and a turn that takes from the front. They share this one buffer type and the one rule for a
//! line the turn does not take, so the three cannot drift apart.
//!
//! A line is carried as the string it arrived as. Nothing here reads it, compares it or branches
//! on it: a buffer holds `String`s and hands them back in the order they were added. The turn
//! reaches the buffer through [`crate::confirm::Confirmer::interjection`].
//!
//! # A line the turn does not take
//!
//! A turn that ends, is being stopped, or never asks between rounds leaves lines in the buffer or
//! refuses them. Each such line is not lost and is not delivered late into a turn it was not aimed
//! at: it becomes a turn of its own, after the turns already waiting, in the order it was sent
//! (INPUT-10). [`Interjections::offer`] hands a refused line back to the caller and
//! [`Interjections::drain`] hands back the lines a finished turn left, and making the turn is the
//! caller's, because the window, the terminal and the lines reader each own what a turn needs.
//!
//! # Lines held back
//!
//! The terminal also keeps lines the turn in flight must not take: the ones behind a command, and
//! the ones typed after a press that sent the queue now. They belong to a later turn, so they sit
//! in the same buffer behind the lines the turn may take and are invisible to [`Interjections::take`]
//! and [`Interjections::drain`]. [`Interjections::push_behind`] puts a line there once one is held,
//! [`Interjections::forget_last`] takes back the newest line wherever it is, and
//! [`Interjections::begin_turn`] hands the oldest lines to a turn that starts and moves the rest.
//! The cli and the bridge never hold a line, so for them the buffer is the one queue it always was.
//!
//! # Seeing a line arrive
//!
//! A turn that is waiting on a model reply cannot ask the buffer for its lines, because asking is
//! taking and what comes off is off. [`Interjections::arrival`] is the other way to learn that
//! one is there: a handle that reads whether any line the turn may take is waiting, without the lock
//! and without taking anything. A held line does not count, since the turn may not take it. The
//! handle also counts the lines added, so that a reader can tell one added after a moment it
//! remembers from one that was waiting already.
//!
//! # Poisoning
//!
//! A poisoned lock reads as empty and drops an addition, and the handle reads as nothing waiting
//! from the next change on. The lock is held only while a line is added, taken or moved between
//! the runs, as `begin_turn` does, and no caller code runs under it, so it is only poisoned by a
//! panic elsewhere, and the turn is ending either way. Nothing here is worth failing a turn over.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Lines waiting for the turn in flight, in the order they were sent.
///
/// Cloning shares the buffer rather than copying it. A line that was added can be taken back
/// ([`Interjections::forget_last`]) only because both ends hold the one buffer; a line posted into
/// a channel is gone.
#[derive(Debug, Clone, Default)]
pub struct Interjections(Arc<Mutex<Lines>>, Arrival);

/// Whether a line the turn may take is waiting in the buffer it came from, and how many have been
/// added to it.
///
/// Cheap to clone and to read from any thread. Every change to the buffer moves it, so it never
/// says a line is waiting that the turn could not take. One that comes from no buffer reports
/// nothing, ever, and says so through [`Arrival::is_listening`].
///
/// A model request in flight reads it to decide whether to be cut (INPUT-10), and a `job_output`
/// wait reads it to decide whether to end (RUN-17). Both read whether a line is there and never
/// the line.
#[derive(Debug, Clone)]
pub struct Arrival(Option<Arc<Seen>>);

/// What an [`Arrival`] reads.
#[derive(Debug, Default)]
struct Seen {
    /// Whether any line the turn may take is waiting.
    waiting: AtomicBool,
    /// How many lines have been added for the turn to take, since the buffer was made. Never goes
    /// down, so a reader that remembers the figure can tell a line added after it from one that was
    /// already there.
    typed: AtomicU64,
}

impl Default for Arrival {
    /// The handle of a buffer that has just been made, with nothing in it.
    fn default() -> Self {
        Self(Some(Arc::default()))
    }
}

impl Arrival {
    /// A handle that never reports a line, for a turn nobody types into.
    pub fn never() -> Self {
        Self(None)
    }

    /// Whether the handle comes from a buffer anybody can add to, so that watching it can ever
    /// see a line.
    pub fn is_listening(&self) -> bool {
        self.0.is_some()
    }

    /// Whether a line the turn may take is waiting.
    pub fn is_waiting(&self) -> bool {
        self.0
            .as_ref()
            .is_some_and(|seen| seen.waiting.load(Ordering::Acquire))
    }

    /// How many lines have been added for the turn to take, in all. A request that remembers the
    /// figure as it goes out is cut only by a line added after, which is the one typed while it was
    /// out.
    pub fn typed(&self) -> u64 {
        self.0
            .as_ref()
            .map_or(0, |seen| seen.typed.load(Ordering::Acquire))
    }

    fn set(&self, waiting: bool) {
        if let Some(seen) = &self.0 {
            seen.waiting.store(waiting, Ordering::Release);
        }
    }

    /// Count a line added for the turn to take.
    fn count(&self) {
        if let Some(seen) = &self.0 {
            seen.typed.fetch_add(1, Ordering::AcqRel);
        }
    }
}

/// The two runs of lines one buffer keeps, oldest first. `held` is always the tail of the queue.
#[derive(Debug, Default)]
struct Lines {
    /// Lines the turn in flight may take.
    waiting: VecDeque<String>,
    /// Lines behind those that the turn in flight may not take.
    held: VecDeque<String>,
}

impl Interjections {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a line for the turn to take at its next boundary.
    ///
    /// `false` where the lock is poisoned and the line was dropped.
    pub fn push(&self, said: String) -> bool {
        let Ok(mut lines) = self.0.lock() else {
            self.1.set(false);
            return false;
        };
        lines.waiting.push_back(said);
        self.publish(&lines);
        self.1.count();
        true
    }

    /// The handle that says whether a line the turn may take is waiting, without taking it.
    pub fn arrival(&self) -> Arrival {
        self.1.clone()
    }

    /// Move the handle to what the buffer holds now. Called under the lock by everything that
    /// changes it, so the handle and the buffer cannot be read in disagreement by a thread that
    /// also holds the lock.
    fn publish(&self, lines: &Lines) {
        self.1.set(!lines.waiting.is_empty());
    }

    /// Take the oldest line the turn may take, or `None`. Never waits. A held line is not taken.
    pub fn take(&self) -> Option<String> {
        let Ok(mut lines) = self.0.lock() else {
            self.1.set(false);
            return None;
        };
        let taken = lines.waiting.pop_front();
        self.publish(&lines);
        taken
    }

    /// Take every line the turn may take, oldest first, leaving none of them.
    ///
    /// These are the lines a turn left untaken when it ended. Held lines stay held.
    pub fn drain(&self) -> Vec<String> {
        let Ok(mut lines) = self.0.lock() else {
            self.1.set(false);
            return Vec::new();
        };
        let drained = lines.waiting.drain(..).collect();
        self.publish(&lines);
        drained
    }

    /// Drop the newest line, held or not, for one the person is taking back.
    ///
    /// `false` where there was nothing to drop, which is how a caller learns that the turn has
    /// already taken every line there was. The two ends move independently, so a line can be taken
    /// between the key press and the moment anything looks at the buffer.
    pub fn forget_last(&self) -> bool {
        let Ok(mut lines) = self.0.lock() else {
            self.1.set(false);
            return false;
        };
        let forgot = lines.held.pop_back().is_some() || lines.waiting.pop_back().is_some();
        self.publish(&lines);
        forgot
    }

    /// Drop the line that has `after` lines behind it, held or not, for one the person is removing
    /// from the middle of what is waiting. `0` is the newest line, as in [`Interjections::forget_last`].
    ///
    /// Counted from the newest because the turn takes from the oldest end: a line the session still
    /// shows as waiting has the same number of lines after it in the buffer and in the session's queue
    /// whether or not the turn has taken the ones before it. `false` where the buffer holds no line
    /// that far from the end, which is how a caller learns that the turn has already taken this one.
    pub fn forget_after(&self, after: usize) -> bool {
        let Ok(mut lines) = self.0.lock() else {
            self.1.set(false);
            return false;
        };
        let total = lines.waiting.len() + lines.held.len();
        let forgot = match total.checked_sub(after + 1) {
            Some(at) if at < lines.waiting.len() => lines.waiting.remove(at).is_some(),
            Some(at) => {
                let held = at - lines.waiting.len();
                lines.held.remove(held).is_some()
            }
            None => false,
        };
        self.publish(&lines);
        forgot
    }

    /// Add a line behind everything already in the buffer.
    ///
    /// Held where any line is already held, because the turn taking this one ahead of an older
    /// held line would hand over what was typed last first. Otherwise the turn may take it.
    ///
    /// `false` where the lock is poisoned and the line was dropped.
    pub fn push_behind(&self, said: String) -> bool {
        let Ok(mut lines) = self.0.lock() else {
            self.1.set(false);
            return false;
        };
        let readable = lines.held.is_empty();
        if readable {
            lines.waiting.push_back(said);
        } else {
            lines.held.push_back(said);
        }
        self.publish(&lines);
        if readable {
            self.1.count();
        }
        true
    }

    /// How many lines are held back from the turn in flight.
    pub fn held(&self) -> usize {
        self.0.lock().map(|lines| lines.held.len()).unwrap_or(0)
    }

    /// Hand the oldest `begun` lines, held or not, to a turn that is starting, and keep the rest.
    ///
    /// The rest keep their order. They are held when `hold` is set, and otherwise left for the new
    /// turn to take.
    pub fn begin_turn(&self, begun: usize, hold: bool) {
        let Ok(mut lines) = self.0.lock() else {
            self.1.set(false);
            return;
        };
        let mut rest = std::mem::take(&mut lines.waiting);
        rest.append(&mut lines.held);
        rest.drain(..begun.min(rest.len()));
        if hold {
            lines.held = rest;
        } else {
            lines.waiting = rest;
        }
        self.publish(&lines);
    }

    /// Add a line if the turn will read it, and hand it back if not.
    ///
    /// `taking` is the caller's answer to whether the turn is still reading: it has not ended, is
    /// not being stopped, and asks between rounds. `Err` carries the line unread to the caller,
    /// which makes a turn of it. An `Ok` is not a promise that the line is read: a turn that
    /// answers without another round ends with it still waiting, and [`Interjections::drain`]
    /// returns it then.
    pub fn offer(&self, taking: bool, said: String) -> Result<(), String> {
        if !taking {
            return Err(said);
        }
        // A poisoned lock drops the line in `push`, so it is handed back here and not lost.
        match self.0.lock() {
            Ok(mut lines) => {
                lines.waiting.push_back(said);
                self.publish(&lines);
                self.1.count();
                Ok(())
            }
            Err(_) => {
                self.1.set(false);
                Err(said)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn poisoned() -> Interjections {
        let buffer = Interjections::new();
        buffer.push("kept".into());
        let held = buffer.clone();
        let _ = std::thread::spawn(move || {
            let _lock = held.0.lock().unwrap();
            panic!("poison the lock");
        })
        .join();
        assert!(buffer.0.is_poisoned());
        buffer
    }

    /// Lines come out in the order they went in. Rejects a stack (`pop_back` in `take`) and a
    /// buffer that reorders, which would hand the planner the newest line first.
    #[test]
    fn lines_are_taken_in_the_order_they_were_pushed() {
        let buffer = Interjections::new();
        assert!(buffer.push("one".into()));
        assert!(buffer.push("two".into()));
        assert!(buffer.push("three".into()));
        assert_eq!(buffer.take().as_deref(), Some("one"));
        assert_eq!(buffer.take().as_deref(), Some("two"));
        assert_eq!(buffer.take().as_deref(), Some("three"));
        assert_eq!(buffer.take(), None);
    }

    /// A clone is the same buffer. Rejects a `Clone` that copies the queue, where the front end
    /// would add to one buffer and the turn would poll another.
    #[test]
    fn a_clone_shares_the_buffer() {
        let front_end = Interjections::new();
        let turn = front_end.clone();
        front_end.push("said".into());
        assert_eq!(turn.take().as_deref(), Some("said"));
        assert_eq!(front_end.take(), None);
    }

    /// Drain returns everything, oldest first, and leaves the buffer empty. Rejects a drain that
    /// takes one line, one that reverses, and one that clones and leaves the lines to be taken
    /// twice.
    #[test]
    fn drain_returns_every_line_oldest_first_and_empties_the_buffer() {
        let buffer = Interjections::new();
        buffer.push("a".into());
        buffer.push("b".into());
        buffer.push("c".into());
        assert_eq!(buffer.drain(), vec!["a", "b", "c"]);
        assert_eq!(buffer.take(), None);
        assert!(buffer.drain().is_empty());
    }

    /// Forgetting drops the newest and says whether there was one. Rejects dropping the oldest
    /// (the end Up does not take from) and always answering `true`, which would offer a line the
    /// turn already took back to the box.
    #[test]
    fn forget_last_drops_the_newest_and_says_whether_it_did() {
        let buffer = Interjections::new();
        buffer.push("old".into());
        buffer.push("new".into());
        assert!(buffer.forget_last());
        assert_eq!(buffer.take().as_deref(), Some("old"));
        assert!(!buffer.forget_last(), "nothing was left to forget");
    }

    /// Forgetting a line from the middle drops that line and keeps the order of the others, whether
    /// it is one the turn may take or one held back. Rejects counting from the oldest, which drops
    /// the wrong line, and a held line that stays after being removed.
    #[test]
    fn forget_after_drops_the_line_that_many_from_the_newest_and_keeps_the_rest_in_order() {
        let buffer = Interjections::new();
        for line in ["a", "b", "c", "d"] {
            buffer.push(line.into());
        }
        assert!(buffer.forget_after(1), "`c` was not dropped");
        assert_eq!(buffer.drain(), vec!["a", "b", "d"]);

        for line in ["a", "b", "c", "d", "e"] {
            buffer.push(line.into());
        }
        buffer.begin_turn(1, true);
        buffer.push_behind("f".into());
        assert_eq!(
            buffer.held(),
            5,
            "the lines behind the turn's were not held"
        );
        assert!(buffer.forget_after(1), "`e` was not dropped");
        assert!(buffer.forget_after(3), "`b` was not dropped");
        assert_eq!(buffer.held(), 3);
        assert!(
            !buffer.forget_after(3),
            "a place past the oldest dropped a line"
        );
        buffer.begin_turn(0, false);
        assert_eq!(buffer.drain(), vec!["c", "d", "f"]);
    }

    /// The turn taking the oldest lines does not change which line is `after` lines from the newest,
    /// so a line the session still shows is found, and one the turn took is not.
    #[test]
    fn forget_after_says_false_for_a_line_the_turn_has_taken() {
        let buffer = Interjections::new();
        for line in ["a", "b", "c"] {
            buffer.push(line.into());
        }
        assert_eq!(buffer.take().as_deref(), Some("a"));
        assert!(buffer.forget_after(1), "`b` is one from the newest");
        assert!(
            !buffer.forget_after(1),
            "`a` was taken, and nothing is one from the newest"
        );
        assert_eq!(buffer.take().as_deref(), Some("c"));
    }

    /// A poisoned lock reads as empty and drops an addition, and none of the eight operations
    /// panics. Rejects `lock().unwrap()`, which would fail the turn over a panic elsewhere, and
    /// `into_inner` recovery, which would keep serving lines from a turn that is ending.
    #[test]
    fn a_poisoned_lock_reads_as_empty_and_drops_additions() {
        let buffer = poisoned();
        assert!(!buffer.push("late".into()));
        assert!(
            !buffer.arrival().is_waiting(),
            "a poisoned buffer reported a line"
        );
        assert_eq!(buffer.take(), None);
        assert!(buffer.drain().is_empty());
        assert!(!buffer.forget_last());
        assert_eq!(buffer.offer(true, "back".into()), Err("back".to_string()));
        assert!(!buffer.push_behind("behind".into()));
        assert_eq!(buffer.held(), 0);
        buffer.begin_turn(1, true);
        buffer.begin_turn(1, false);
        assert_eq!(buffer.held(), 0);
        assert_eq!(
            buffer.take(),
            None,
            "a poisoned begin_turn handed a line out"
        );
        assert!(buffer.drain().is_empty());
    }

    /// A held line is invisible to the turn and is kept in order behind the lines it may take.
    /// Rejects a hold that lands in the readable run, which would deliver a later turn's line into
    /// the turn in flight, and a hold that lets a newer line pass an older held one.
    #[test]
    fn a_held_line_is_not_taken_or_drained_and_keeps_its_place() {
        let buffer = Interjections::new();
        assert!(buffer.push_behind("now".into()));
        assert!(buffer.push_behind("later".into()));
        buffer.begin_turn(0, true);
        assert!(buffer.push_behind("after later".into()));
        assert_eq!(buffer.held(), 3);
        assert_eq!(buffer.take(), None);
        buffer.begin_turn(1, false);
        assert_eq!(buffer.held(), 0);
        assert_eq!(buffer.take().as_deref(), Some("later"));
        assert_eq!(buffer.take().as_deref(), Some("after later"));
    }

    /// A line pushed behind a held one is held, and one pushed with none held is readable. Rejects
    /// a push that always lands in the readable run, which would let the turn in flight take what
    /// was typed after a held line ahead of it.
    #[test]
    fn a_line_pushed_behind_a_held_line_is_held() {
        let buffer = Interjections::new();
        assert!(buffer.push_behind("a".into()));
        assert_eq!(buffer.held(), 0);
        buffer.begin_turn(0, true);
        assert_eq!(buffer.take(), None);
        assert!(buffer.push_behind("b".into()));
        assert_eq!(buffer.held(), 2);
        assert!(buffer.drain().is_empty());
    }

    /// Forgetting takes the newest line wherever it is, held lines included. Rejects a forget that
    /// looks only at the readable run, which would report nothing to take back and leave the held
    /// line the person pressed Up on to be sent, and one that drops the oldest line instead.
    #[test]
    fn forget_last_drops_a_held_line_before_a_readable_one() {
        let buffer = Interjections::new();
        buffer.push_behind("now".into());
        buffer.begin_turn(0, true);
        buffer.push_behind("later".into());
        assert!(buffer.forget_last(), "the held line was not found");
        assert_eq!(buffer.held(), 1);
        buffer.begin_turn(0, false);
        assert_eq!(buffer.take().as_deref(), Some("now"));
        assert!(!buffer.forget_last());
    }

    /// Beginning a turn removes the oldest lines whichever run they are in and keeps the rest in
    /// order, held or readable as asked. Rejects a begin that drops only readable lines, which would
    /// leave a copy to reach the planner twice, one that drops the newest lines, and one that
    /// reorders the rest.
    #[test]
    fn beginning_a_turn_removes_the_oldest_lines_and_keeps_the_rest_in_order() {
        let buffer = Interjections::new();
        for line in ["a", "b", "c", "d"] {
            buffer.push_behind(line.into());
        }
        buffer.begin_turn(0, true);
        assert_eq!(buffer.held(), 4);
        buffer.begin_turn(1, true);
        assert_eq!(buffer.held(), 3, "the oldest held line was not removed");
        assert_eq!(buffer.take(), None);
        buffer.begin_turn(0, false);
        assert_eq!(buffer.held(), 0);
        assert_eq!(buffer.drain(), vec!["b", "c", "d"]);
    }

    /// The handle follows what the turn may take, through every way a line enters or leaves.
    /// Rejects a handle set by `push` alone, which would still say a line is waiting once the turn
    /// took it, and a turn that cut its reply short for a line that was no longer there.
    #[test]
    fn the_arrival_handle_follows_every_way_a_line_enters_or_leaves() {
        let buffer = Interjections::new();
        let arrival = buffer.arrival();
        assert!(!arrival.is_waiting());

        buffer.push("a".into());
        assert!(arrival.is_waiting());
        buffer.take();
        assert!(!arrival.is_waiting(), "a taken line still read as waiting");

        buffer.push("b".into());
        buffer.push("c".into());
        buffer.drain();
        assert!(
            !arrival.is_waiting(),
            "a drained line still read as waiting"
        );

        buffer.push("d".into());
        assert!(buffer.forget_last());
        assert!(
            !arrival.is_waiting(),
            "a line taken back still read as waiting"
        );

        assert_eq!(buffer.offer(true, "e".into()), Ok(()));
        assert!(arrival.is_waiting(), "an offered line was not seen");
        buffer.drain();

        assert_eq!(buffer.offer(false, "f".into()), Err("f".to_string()));
        assert!(!arrival.is_waiting(), "a refused line was seen");
    }

    /// The count rises for each line added for the turn to take and for nothing else. Rejects a
    /// count that moves when a line is taken, forgotten, held or released from hold, which would
    /// read a line that was already there as one just typed.
    #[test]
    fn the_count_rises_for_each_line_added_for_the_turn_and_for_nothing_else() {
        let buffer = Interjections::new();
        let arrival = buffer.arrival();
        assert_eq!(arrival.typed(), 0);
        buffer.push("a".into());
        assert_eq!(arrival.typed(), 1);
        buffer.push_behind("b".into());
        assert_eq!(arrival.typed(), 2);
        assert_eq!(buffer.offer(true, "c".into()), Ok(()));
        assert_eq!(arrival.typed(), 3);
        assert_eq!(
            buffer.offer(false, "refused".into()),
            Err("refused".to_string())
        );
        assert_eq!(arrival.typed(), 3, "a refused line was counted");
        buffer.take();
        buffer.forget_last();
        buffer.drain();
        assert_eq!(arrival.typed(), 3, "taking a line moved the count");

        buffer.push_behind("d".into());
        buffer.begin_turn(0, true);
        let before = arrival.typed();
        buffer.push_behind("held".into());
        assert_eq!(arrival.typed(), before, "a held line was counted");
        buffer.begin_turn(1, false);
        assert_eq!(arrival.typed(), before, "releasing a held line was counted");
        assert_eq!(Arrival::never().typed(), 0);
    }

    /// A held line is for a later turn, so the turn in flight is not told of it. Rejects a handle
    /// that reads the whole queue, which would cut a reply short for a line the turn may not take
    /// and then find nothing to take.
    #[test]
    fn a_held_line_does_not_read_as_waiting() {
        let buffer = Interjections::new();
        let arrival = buffer.arrival();
        buffer.push_behind("now".into());
        assert!(arrival.is_waiting());
        buffer.begin_turn(0, true);
        assert!(
            !arrival.is_waiting(),
            "a line moved into hold still read as waiting"
        );
        buffer.push_behind("later".into());
        assert_eq!(buffer.held(), 2);
        assert!(!arrival.is_waiting(), "a held line read as waiting");
        buffer.begin_turn(1, false);
        assert!(
            arrival.is_waiting(),
            "a line released from hold was not seen"
        );
    }

    /// A clone of the buffer reports through the same handle, as the front end and the turn hold
    /// different clones.
    #[test]
    fn the_handle_is_shared_by_every_clone() {
        let front_end = Interjections::new();
        let turn = front_end.clone();
        let arrival = turn.arrival();
        front_end.push("said".into());
        assert!(arrival.is_waiting());
        assert!(!Arrival::never().is_waiting());
        assert!(!Arrival::never().is_listening());
        assert!(arrival.is_listening());
    }

    /// A line for a turn that is not reading comes back unread and does not enter the buffer.
    /// Rejects an `offer` that pushes whatever `taking` says, which would deliver a late line into
    /// a turn it was not aimed at.
    #[test]
    fn a_line_for_a_turn_that_is_not_reading_is_handed_back() {
        let buffer = Interjections::new();
        assert_eq!(buffer.offer(false, "late".into()), Err("late".to_string()));
        assert_eq!(buffer.take(), None);
        assert_eq!(buffer.offer(true, "now".into()), Ok(()));
        assert_eq!(buffer.take().as_deref(), Some("now"));
    }
}
