//! Asking a turn to stop.
//!
//! A turn can spend a long time waiting on a model, and a user who has changed their mind should
//! not have to wait it out. So a turn carries a token it checks between steps, and setting the
//! token asks it to stop at the next check.
//!
//! Cooperative rather than pre-emptive, and deliberately so. Killing a turn mid-effect could
//! leave a file half written, so cancellation happens only at points where nothing is in
//! progress: between rounds, before each tool call, and between the chunks of a streamed reply.
//!
//! A streamed reply is the exception that proves the rule rather than a hole in it. Reading a
//! response body applies nothing and writes nothing, so stopping part way leaves nothing part
//! done, and the reply was going to be discarded whatever happened. Read to the end regardless,
//! stopping a turn took exactly as long as not stopping it, which is the case a person reaches
//! for the key in. A request that is not streamed still runs to completion.
//!
//! The flag only ever goes from unset to set. Reusing a token for a second turn would risk
//! cancelling it before it began, so each turn gets a fresh one.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A one-way flag asking a turn to stop.
///
/// Cheap to clone: every clone refers to the same flag, which is how the interface signals a turn
/// running on another thread.
#[derive(Debug, Clone, Default)]
pub struct Cancel {
    flag: Arc<AtomicBool>,
}

impl Cancel {
    /// A token that has not been cancelled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Ask the turn to stop at its next check.
    ///
    /// Safe to call more than once, and from any thread. `Release` pairs with the `Acquire` in
    /// [`Cancel::is_cancelled`] so a turn that sees the flag also sees everything written before
    /// it was set.
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::Release);
    }

    /// Whether a stop has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::Acquire)
    }
}

/// A one-way flag asking a waited-for command to go on running in the background.
///
/// The same shape as [`Cancel`], for the same reason: the person presses the key on the interface
/// thread and the run reading it is on the turn's. Each run gets a fresh one, so a press meant for
/// one command can never reach the next.
#[derive(Debug, Clone, Default)]
pub struct Handoff {
    flag: Arc<AtomicBool>,
}

impl Handoff {
    /// A token nobody has pressed.
    pub fn new() -> Self {
        Self::default()
    }

    /// Ask the run to stop waiting and keep the command as a job.
    pub fn request(&self) {
        self.flag.store(true, Ordering::Release);
    }

    /// Whether the move has been asked for.
    pub fn is_requested(&self) -> bool {
        self.flag.load(Ordering::Acquire)
    }
}

/// A one-way flag asking one background job to stop.
///
/// The same shape as [`Handoff`]: the person asks on the interface thread, and the turn holding the
/// job reads it on its own and stops the job itself. Each job gets a fresh one, so asking about one
/// job reaches no other.
///
/// Equal only to its own clones, because what two handles have to agree on is which job they reach.
#[derive(Debug, Clone, Default)]
pub struct JobStop {
    flag: Arc<AtomicBool>,
}

impl JobStop {
    /// A token nobody has asked with.
    pub fn new() -> Self {
        Self::default()
    }

    /// Ask the turn to stop the job at its next step.
    pub fn request(&self) {
        self.flag.store(true, Ordering::Release);
    }

    /// Whether the stop has been asked for.
    pub fn is_requested(&self) -> bool {
        self.flag.load(Ordering::Acquire)
    }
}

impl PartialEq for JobStop {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.flag, &other.flag)
    }
}

impl Eq for JobStop {}

/// A one-way flag asking one delegate to stop, leaving the turn and every other delegate going.
///
/// The same shape as [`JobStop`]: the person asks on the interface thread, and the delegate reads
/// it at its own next round. Each delegate gets a fresh one, so asking about one reaches no other.
///
/// It also records whether the delegate acted on the ask. A press that arrives after the delegate
/// has answered changes nothing about how it ended, and the sentence the turn is told about the
/// delegate has to say what happened and not what was asked.
///
/// Equal only to its own clones, because what two handles have to agree on is which delegate they
/// reach.
#[derive(Debug, Clone, Default)]
pub struct DelegateStop {
    asked: Arc<AtomicBool>,
    honoured: Arc<AtomicBool>,
}

impl DelegateStop {
    /// A token nobody has asked with.
    pub fn new() -> Self {
        Self::default()
    }

    /// Ask the delegate to stop at its next round.
    pub fn request(&self) {
        self.asked.store(true, Ordering::Release);
    }

    /// Whether the stop has been asked for.
    pub fn is_requested(&self) -> bool {
        self.asked.load(Ordering::Acquire)
    }

    /// Record that the delegate took its tools away because it was asked to.
    pub fn honour(&self) {
        self.honoured.store(true, Ordering::Release);
    }

    /// Whether the delegate stopped because the person asked.
    pub fn was_honoured(&self) -> bool {
        self.honoured.load(Ordering::Acquire)
    }
}

impl PartialEq for DelegateStop {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.asked, &other.asked)
    }
}

impl Eq for DelegateStop {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn a_fresh_token_is_not_cancelled() {
        assert!(!Cancel::new().is_cancelled());
    }

    #[test]
    fn cancelling_is_observable() {
        let cancel = Cancel::new();
        cancel.cancel();
        assert!(cancel.is_cancelled());
    }

    /// The point of the type: one side sets the flag and the other sees it.
    #[test]
    fn a_clone_shares_the_flag() {
        let cancel = Cancel::new();
        let remote = cancel.clone();
        assert!(!remote.is_cancelled());
        cancel.cancel();
        assert!(remote.is_cancelled(), "the clone did not see the request");
    }

    /// Cancellation crosses a thread boundary, which is the only way it is ever used.
    #[test]
    fn cancelling_crosses_threads() {
        let cancel = Cancel::new();
        let worker = cancel.clone();

        let handle = thread::spawn(move || {
            while !worker.is_cancelled() {
                std::hint::spin_loop();
            }
            "stopped"
        });

        cancel.cancel();
        assert_eq!(handle.join().expect("worker finished"), "stopped");
    }

    /// Asking twice is not an error: a user may press the key more than once.
    #[test]
    fn cancelling_twice_is_harmless() {
        let cancel = Cancel::new();
        cancel.cancel();
        cancel.cancel();
        assert!(cancel.is_cancelled());
    }

    /// The flag never clears, so a turn cannot be told to stop and then quietly continue.
    #[test]
    fn cancellation_does_not_reset() {
        let cancel = Cancel::new();
        cancel.cancel();
        for _ in 0..10 {
            assert!(cancel.is_cancelled());
        }
    }

    #[test]
    fn a_fresh_handoff_is_not_requested() {
        assert!(!Handoff::new().is_requested());
    }

    /// The interface holds one clone and the run reads another, so the press has to cross. The
    /// wait is bounded so a press that never arrives fails here instead of spinning forever.
    #[test]
    fn a_handoff_requested_on_one_thread_is_seen_on_another() {
        let handoff = Handoff::new();
        let run = handoff.clone();

        let handle = thread::spawn(move || {
            let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while !run.is_requested() {
                if std::time::Instant::now() >= until {
                    return false;
                }
                std::hint::spin_loop();
            }
            true
        });

        handoff.request();
        assert!(
            handle.join().expect("run finished"),
            "the press never reached the other thread"
        );
    }

    /// The interface holds one clone and the turn reads another, so the request has to cross, and
    /// it reaches only the job whose token it is.
    #[test]
    fn a_job_stop_reaches_the_turn_and_no_other_job() {
        let asked = JobStop::new();
        let other = JobStop::new();
        let turn = asked.clone();
        let other_in_the_turn = other.clone();

        let handle = thread::spawn(move || {
            let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while !turn.is_requested() {
                if std::time::Instant::now() >= until {
                    return (false, other_in_the_turn.is_requested());
                }
                std::hint::spin_loop();
            }
            (true, other_in_the_turn.is_requested())
        });

        asked.request();
        let (seen, other_seen) = handle.join().expect("turn finished");
        assert!(seen, "the request never reached the other thread");
        assert!(!other_seen, "asking to stop one job asked to stop another");
        assert_eq!(asked, asked.clone());
        assert_ne!(asked, other, "two jobs' tokens compare equal");
    }

    #[test]
    fn asking_one_delegate_to_stop_reaches_no_other() {
        let one = DelegateStop::new();
        let other = DelegateStop::new();
        one.request();
        assert!(one.is_requested());
        assert!(!other.is_requested());
        assert_ne!(one, other);
    }

    #[test]
    fn a_clone_of_a_delegates_stop_reaches_the_same_delegate() {
        let stop = DelegateStop::new();
        let remote = stop.clone();
        remote.request();
        assert!(stop.is_requested());
        assert!(!stop.was_honoured());
        stop.honour();
        assert!(remote.was_honoured());
        assert_eq!(stop, remote);
    }
}
