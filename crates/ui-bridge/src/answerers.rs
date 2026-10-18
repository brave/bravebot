// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! Which questions a session's controller can answer right now.
//!
//! A turn that wants to write, run a command or fetch a page asks a person and waits. Where nobody
//! is in a position to answer, waiting is wrong: the question would hang until something cancels
//! the turn. [`Answerers`] is what the asking side consults, under the lock that registers the
//! question, so a question is either refused before it is shown or registered where the loss of
//! the controller will refuse it.

use crate::protocol::Failure;
use crate::turn::Kind;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
enum Who {
    /// Every kind: the stdio transports, whose one client answers whatever it is asked.
    #[default]
    Anyone,
    /// Only these kinds. Empty when nobody is ready to answer.
    Only(Vec<Kind>),
}

/// Shared between the host, which says who is ready, and the confirmer of each turn, which asks.
#[derive(Clone, Default)]
pub struct Answerers(Arc<Mutex<Who>>);

impl Answerers {
    /// A set that answers nothing until it is told otherwise.
    pub fn none() -> Self {
        Self(Arc::new(Mutex::new(Who::Only(Vec::new()))))
    }

    /// Whether a question of `kind` can be put to somebody. A lock another thread panicked
    /// holding answers no: an unreadable state is not a person who is there.
    pub fn can_answer(&self, kind: Kind) -> bool {
        match self.0.lock() {
            Ok(who) => match &*who {
                Who::Anyone => true,
                Who::Only(kinds) => kinds.contains(&kind),
            },
            Err(_) => false,
        }
    }

    /// From now on only these kinds can be answered. Questions already waiting are not touched.
    /// Fails where the set cannot be written, so a caller does not report a client ready to
    /// answer when it is not.
    pub fn only(&self, kinds: &[Kind]) -> Result<(), Failure> {
        let mut who = self
            .0
            .lock()
            .map_err(|_| Failure::bad_request("who can answer cannot be changed"))?;
        *who = Who::Only(kinds.to_vec());
        Ok(())
    }

    /// From now on no question can be answered.
    pub fn nobody(&self) -> Result<(), Failure> {
        self.only(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_set_answers_every_kind_so_a_client_that_never_chose_is_unchanged() {
        let answerers = Answerers::default();
        assert!(answerers.can_answer(Kind::Write));
        assert!(answerers.can_answer(Kind::Output));
    }

    #[test]
    fn only_the_kinds_named_can_be_answered_and_nobody_answers_none() {
        let answerers = Answerers::default();
        answerers.only(&[Kind::Write, Kind::Ask]).unwrap();
        assert!(answerers.can_answer(Kind::Write));
        assert!(answerers.can_answer(Kind::Ask));
        assert!(!answerers.can_answer(Kind::Run));
        assert!(!answerers.can_answer(Kind::Output));
        answerers.nobody().unwrap();
        assert!(!answerers.can_answer(Kind::Write));
        assert!(!answerers.can_answer(Kind::Ask));
    }

    #[test]
    fn a_clone_is_the_same_set() {
        let answerers = Answerers::default();
        let held = answerers.clone();
        answerers.nobody().unwrap();
        assert!(!held.can_answer(Kind::Write));
    }

    /// A set that cannot be written does not say it was: a caller that told a client it could
    /// answer would be wrong, and a question is refused either way.
    #[test]
    fn a_set_that_cannot_be_written_fails_and_answers_nothing() {
        let answerers = Answerers::default();
        let held = answerers.clone();
        let _ = std::thread::spawn(move || {
            let _guard = held.0.lock().unwrap();
            panic!("a thread panicked holding the set");
        })
        .join();

        assert!(answerers.only(&[Kind::Write]).is_err());
        assert!(answerers.nobody().is_err());
        assert!(!answerers.can_answer(Kind::Write));
    }
}
