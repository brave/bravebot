// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! What `consult.brief` says of each way a draft can come back, and what it holds (CONSULT-5).

use super::*;
use bravebot_agent::aside::{Answered, AsideError};

fn a_brief() -> Brief {
    Brief {
        bot: "harbour".to_string(),
        line: "ask @harbour about it".to_string(),
        private: false,
        then: Vec::new(),
    }
}

fn answered(kept: Option<&str>, private: bool) -> Answered {
    Answered {
        shown: "the draft".to_string(),
        kept: kept.map(str::to_string),
        private,
        usage: Default::default(),
    }
}

/// What `drafted` said of `done`, and the brief it left held.
fn drafting(done: Result<Answered, AsideError>, cancelled: bool) -> (Value, Option<Brief>) {
    let held = Mutex::new(None);
    let state = Mutex::new(State::fresh(TrustStore::new("/work")));
    let data = drafted(
        done,
        cancelled,
        a_brief(),
        "a row".to_string(),
        &held,
        &state,
    );
    (data, held.into_inner().ok().flatten())
}

/// CONSULT-5: a brief that holds nothing private is marked to go at once and held for that send,
/// whether the host could have held it or a person's yes promoted it, as both arrive kept.
#[test]
fn a_brief_holding_nothing_private_is_marked_to_go_at_once() {
    let (data, held) = drafting(Ok(answered(Some("the draft"), false)), false);
    assert_eq!(data["send"], true, "{data}");
    assert_eq!(
        data["brief"], "ask @harbour about it\n\nthe draft",
        "{data}"
    );
    assert_eq!(held.map(|brief| brief.private), Some(false));
}

/// CONSULT-5, CONSULT-6: a private brief is offered marked to wait, and held as private, so the
/// consult it starts holds private content.
#[test]
fn a_private_brief_is_marked_to_wait_and_held_as_private() {
    let (data, held) = drafting(Ok(answered(Some("the draft"), true)), false);
    assert_eq!(data["send"], false, "{data}");
    assert_eq!(data["private"], true, "{data}");
    assert_eq!(held.map(|brief| brief.private), Some(true));
}

/// CONSULT-6: a draft nobody kept, because the person said no on its card, offers no brief and
/// holds none for `turn.send` to send.
#[test]
fn a_declined_draft_holds_no_brief() {
    let (data, held) = drafting(Ok(answered(None, false)), false);
    assert!(data["brief"].is_null(), "{data}");
    assert_eq!(held, None);
    assert_eq!(
        data["message"],
        t!(consult_brief_declined, bot = "harbour").to_string()
    );
}

/// CONSULT-5: a stopped draft offers no brief, holds none, and says it was stopped.
#[test]
fn a_stopped_draft_holds_no_brief() {
    let stopped = AsideError::from(bravebot_aichat::ChatError::Cancelled);
    let (data, held) = drafting(Err(stopped), true);
    assert!(data["brief"].is_null(), "{data}");
    assert_eq!(held, None);
    assert_eq!(
        data["message"],
        t!(consult_brief_stopped, bot = "harbour").to_string()
    );
}
