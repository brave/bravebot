// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! What a window's session keeps of the links a turn recorded with `set_link` (SETLINK-2).

use super::*;
use bravebot_agent::links::{LinkKind, LinkSet};
use bravebot_agent::turn::{CompletedTurn, Decisions};
use bravebot_session::sessions::Link;

const ISSUE: &str = "https://github.com/brave/bravebot/issues/1267";
const PULL: &str = "https://github.com/brave/bravebot/pull/1270";

fn a_turn_that_failed_after_setting(links: Vec<LinkSet>) -> CompletedTurn {
    CompletedTurn {
        outcome: Err(TurnError::Precommit("the request failed".to_string())),
        decisions: Decisions {
            trust: TrustStore::new("/work"),
            programs: bravebot_core::programs::TrustedPrograms::new(),
            asked_about: Default::default(),
            exposed: Default::default(),
            injected_facts: Default::default(),
            links,
        },
        work: Default::default(),
    }
}

fn a_point_before(turns: usize) -> bravebot_session::sessions::TurnSnapshot {
    bravebot_session::sessions::TurnSnapshot {
        conversation: bravebot_agent::Conversation::new().snapshot(),
        turns,
        tokens: 0,
        split: Default::default(),
        spend: Default::default(),
        timing: Default::default(),
        cached: None,
        cached_prompt_tokens: None,
        trust: TrustStore::new("/work"),
        programs: bravebot_core::programs::TrustedPrograms::new(),
        transcript_len: turns,
        title: "a session".to_string(),
        was_wrote: true,
    }
}

/// SETLINK-2, PANEL-12: the links a turn recorded reach the session's record when the turn ended in
/// an error, and the turn's rewind point is neither made nor given up by it. `run_turn` makes one
/// call to `keep_the_links_of` after the turn returns, so the function is tested directly.
///
/// The faults this rejects: keeping the links only from a turn that answered, so a turn that
/// failed after `set_link` loses the address, and a hand-off that opens or drops a rewind point.
#[test]
fn a_link_set_by_a_turn_that_failed_reaches_the_record_and_leaves_the_rewind_points() {
    if !crate::bridge::test_profile::in_isolated_profile() {
        return;
    }
    let directory = tempfile::tempdir().expect("a project directory");
    let project = directory.path().to_path_buf();
    let mut state = State::fresh(TrustStore::new(&project));
    state
        .rewind
        .open(a_point_before(0), "the failed turn".to_string());
    let points = state.rewind.points().len();
    let mut completed = a_turn_that_failed_after_setting(vec![
        LinkSet {
            kind: LinkKind::Issue,
            url: ISSUE.to_string(),
        },
        LinkSet {
            kind: LinkKind::PullRequest,
            url: PULL.to_string(),
        },
    ]);

    keep_the_links_of(&mut state, &project, &mut completed);

    let handle = state.handle.as_ref().expect("a record was opened for it");
    assert_eq!(handle.link(Link::Issue), Some(ISSUE), "the record lost it");
    assert_eq!(handle.link(Link::PullRequest), Some(PULL));
    assert!(
        completed.decisions.links.is_empty(),
        "the links were kept twice"
    );
    assert_eq!(
        state.rewind.points().len(),
        points,
        "setting a link made or gave up a rewind point"
    );
}

/// A turn that set no link opens no record for one.
#[test]
fn a_turn_that_set_no_link_opens_no_record() {
    if !crate::bridge::test_profile::in_isolated_profile() {
        return;
    }
    let directory = tempfile::tempdir().expect("a project directory");
    let project = directory.path().to_path_buf();
    let mut state = State::fresh(TrustStore::new(&project));
    let mut completed = a_turn_that_failed_after_setting(Vec::new());

    keep_the_links_of(&mut state, &project, &mut completed);

    assert!(state.handle.is_none());
}
