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

/// Work that has ended cannot read a line, so the window makes a turn of it.
#[test]
fn finished_work_takes_no_line() {
    let started = Running::start();
    started.finished.store(true, Ordering::Release);
    assert!(!started.running.interject("late".into()));
}
