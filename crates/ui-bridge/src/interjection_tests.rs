//! The queue a turn reads at its round boundary (INPUT-46).

use crate::running::Running;
use std::sync::atomic::Ordering;

/// A run plans before anything is read and never asks between rounds, so a line handed to one
/// is taken by nobody and the window has to be told it was not taken.
#[test]
fn work_that_asks_nothing_between_rounds_takes_no_line() {
    let mut started = Running::start();
    started.running.interjections = None;
    assert!(!started.running.interject("anything".into()));
}

/// Lines come out in the order they went in, which is the order they were typed.
#[test]
fn lines_are_read_in_the_order_they_were_typed() {
    let started = Running::start();
    assert!(started.running.interject("first".into()));
    assert!(started.running.interject("second".into()));
    assert_eq!(started.interjections.take().as_deref(), Some("first"));
    assert_eq!(started.interjections.take().as_deref(), Some("second"));
    assert_eq!(started.interjections.take(), None);
}

/// The confirmer a turn is given reports a line the moment `turn.interject` adds it, which is how a
/// turn waiting on a reply that is still thinking learns to end the wait (INPUT-10). Rejects a
/// confirmer that leaves the handle at its default, which never reports a line.
#[test]
fn the_turns_confirmer_sees_a_line_as_it_is_added() {
    use crate::turn::{BridgeConfirmer, Pending};
    use bravebot_agent::confirm::Confirmer;
    use std::sync::{Arc, Mutex};

    let started = Running::start();
    let pending: Pending = Arc::new(Mutex::new(None));
    let (_answers, answering) = std::sync::mpsc::channel();
    let confirmer = BridgeConfirmer::new(
        crate::emit::Emitter::new(Box::new(|_| {})),
        "s1",
        pending,
        answering,
        Arc::new(std::sync::atomic::AtomicU64::new(0)),
        started.cancel.clone(),
    )
    .taking_interjections(started.interjections.clone());

    let arrival = confirmer.arrival();
    assert!(arrival.is_listening());
    assert!(!arrival.is_waiting());
    assert!(started.running.interject("no, the other one".into()));
    assert!(
        arrival.is_waiting(),
        "the turn's confirmer did not see the line `turn.interject` added"
    );
}

/// Work that has ended cannot read a line, so the window makes a turn of it.
#[test]
fn finished_work_takes_no_line() {
    let started = Running::start();
    started.finished.store(true, Ordering::Release);
    assert!(!started.running.interject("late".into()));
}
