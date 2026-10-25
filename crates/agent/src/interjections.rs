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
//! # Poisoning
//!
//! A poisoned lock reads as empty and drops an addition. The lock is only held for a push or a pop,
//! so it is only poisoned by a panic elsewhere, and the turn is ending either way. Nothing here is
//! worth failing a turn over.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Lines waiting for the turn in flight, in the order they were sent.
///
/// Cloning shares the buffer rather than copying it. A line that was added can be taken back
/// ([`Interjections::forget_last`]) only because both ends hold the one buffer; a line posted into
/// a channel is gone.
#[derive(Debug, Clone, Default)]
pub struct Interjections(Arc<Mutex<VecDeque<String>>>);

impl Interjections {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a line for the turn to take at its next boundary.
    ///
    /// `false` where the lock is poisoned and the line was dropped.
    pub fn push(&self, said: String) -> bool {
        let Ok(mut waiting) = self.0.lock() else {
            return false;
        };
        waiting.push_back(said);
        true
    }

    /// Take the oldest line, or `None`. Never waits.
    pub fn take(&self) -> Option<String> {
        self.0.lock().ok()?.pop_front()
    }

    /// Take every line, oldest first, leaving the buffer empty.
    ///
    /// These are the lines a turn left untaken when it ended.
    pub fn drain(&self) -> Vec<String> {
        self.0
            .lock()
            .map(|mut waiting| waiting.drain(..).collect())
            .unwrap_or_default()
    }

    /// Drop the newest line, for one the person is taking back.
    ///
    /// `false` where there was nothing to drop, which is how a caller learns that the turn has
    /// already taken every line there was. The two ends move independently, so a line can be taken
    /// between the key press and the moment anything looks at the buffer.
    pub fn forget_last(&self) -> bool {
        self.0
            .lock()
            .map(|mut waiting| waiting.pop_back().is_some())
            .unwrap_or(false)
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
            Ok(mut waiting) => {
                waiting.push_back(said);
                Ok(())
            }
            Err(_) => Err(said),
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

    /// A poisoned lock reads as empty and drops an addition, and none of the five operations
    /// panics. Rejects `lock().unwrap()`, which would fail the turn over a panic elsewhere, and
    /// `into_inner` recovery, which would keep serving lines from a turn that is ending.
    #[test]
    fn a_poisoned_lock_reads_as_empty_and_drops_additions() {
        let buffer = poisoned();
        assert!(!buffer.push("late".into()));
        assert_eq!(buffer.take(), None);
        assert!(buffer.drain().is_empty());
        assert!(!buffer.forget_last());
        assert_eq!(buffer.offer(true, "back".into()), Err("back".to_string()));
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
