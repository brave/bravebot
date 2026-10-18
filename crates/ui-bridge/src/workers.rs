// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! Whether a session's worker threads have ended.
//!
//! A turn or a manifest run works on a thread of its own and saves the record before it ends. A
//! close stops the work and refuses what it waits on, but the thread ends later, and until it has
//! the session can still write. A session is `closed` once every worker it started has returned,
//! and not before. That says nothing about whether the last save succeeded.

use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// The workers one session has started and not yet seen end.
#[derive(Default)]
pub struct Workers {
    live: Mutex<usize>,
    ended: Condvar,
}

/// Held by a worker for as long as it runs. Dropping it, on any exit including a panic, counts the
/// worker as ended.
struct Running(Arc<Workers>);

impl Workers {
    /// Count a worker as running, from now until the returned guard is dropped. Called before the
    /// thread starts, so a close that follows cannot find the session already ended.
    fn start(self: &Arc<Self>) -> Running {
        *self.counted() += 1;
        Running(Arc::clone(self))
    }

    /// Run `work` on a new thread that counts as a worker until `work` has returned and dropped
    /// everything it held.
    pub fn spawn(self: &Arc<Self>, work: impl FnOnce() + Send + 'static) {
        let running = self.start();
        std::thread::spawn(move || {
            // Declared first so that it drops last, after `work` and what it captured.
            let _running = running;
            work();
        });
    }

    /// The count. A thread that panicked while holding the lock cannot have left a half-made
    /// count, since the lock is held only to add or subtract one, so the count is used as it is.
    fn counted(&self) -> std::sync::MutexGuard<'_, usize> {
        self.live
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// A worker that panicked has already been counted out by its guard, so a panic does not leave
    /// a session looking busy for good.
    pub fn is_idle(&self) -> bool {
        *self.counted() == 0
    }

    /// Wait for every worker to end, however long that takes.
    pub fn wait_until_idle(&self) {
        let mut live = self.counted();
        while *live > 0 {
            live = self
                .ended
                .wait(live)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    /// Wait up to `limit` for every worker to end, and say whether they had.
    pub fn wait_idle(&self, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        let mut live = self.counted();
        while *live > 0 {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                return false;
            };
            live = self
                .ended
                .wait_timeout(live, left)
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
        true
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        {
            let mut live = self.0.counted();
            *live = live.saturating_sub(1);
        }
        self.0.ended.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    const PATIENCE: Duration = Duration::from_secs(10);

    /// A session whose worker is still running is not closed, whatever the close has already
    /// stopped. Reporting it closed at once would let a replacement writer start beside one that
    /// can still save.
    #[test]
    fn a_session_is_not_idle_while_a_worker_runs() {
        let workers = Arc::new(Workers::default());
        let (release, held) = mpsc::channel::<()>();
        workers.spawn(move || {
            let _ = held.recv_timeout(PATIENCE);
        });

        assert!(!workers.is_idle(), "idle with a worker still running");
        assert!(
            !workers.wait_idle(Duration::from_millis(50)),
            "waiting finished with a worker still running"
        );

        release.send(()).unwrap();
        assert!(
            workers.wait_idle(PATIENCE),
            "the worker never counted as ended"
        );
        assert!(workers.is_idle());
    }

    /// A worker that panics has still ended. Counting only a clean return would leave the session
    /// closing for good.
    #[test]
    fn a_worker_that_panics_counts_as_ended() {
        let workers = Arc::new(Workers::default());
        workers.spawn(|| panic!("a worker failing on purpose"));
        assert!(workers.wait_idle(PATIENCE));
    }

    /// The count includes what the work still holds when it returns. A worker that has returned
    /// but not yet dropped its state can still be saving it.
    #[test]
    fn a_worker_counts_as_ended_only_after_what_it_held_is_dropped() {
        struct Slow(mpsc::Receiver<()>);
        impl Drop for Slow {
            fn drop(&mut self) {
                let _ = self.0.recv_timeout(PATIENCE);
            }
        }
        let workers = Arc::new(Workers::default());
        let (release, held) = mpsc::channel::<()>();
        let slow = Slow(held);
        workers.spawn(move || {
            let _slow = &slow;
        });

        assert!(
            !workers.wait_idle(Duration::from_millis(100)),
            "counted as ended while the worker's state was still being dropped"
        );
        release.send(()).unwrap();
        assert!(workers.wait_idle(PATIENCE));
    }

    /// Two workers, as a turn followed by a run would be: idle only when both have ended.
    #[test]
    fn idle_means_every_worker_has_ended() {
        let workers = Arc::new(Workers::default());
        let (first_release, first) = mpsc::channel::<()>();
        let (second_release, second) = mpsc::channel::<()>();
        workers.spawn(move || {
            let _ = first.recv_timeout(PATIENCE);
        });
        workers.spawn(move || {
            let _ = second.recv_timeout(PATIENCE);
        });

        first_release.send(()).unwrap();
        assert!(!workers.wait_idle(Duration::from_millis(100)));
        second_release.send(()).unwrap();
        assert!(workers.wait_idle(PATIENCE));
    }
}
