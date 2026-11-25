// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! BACKEND-27: the terminal client notes whether the model in force is recorded as refusing the cache
//! breakpoints after each turn, after each manifest run, on a pick and at start, so the status report
//! and the info panel can say caching is off for it. One test for each place, each stating which
//! models the backend crate's record holds, and each failing where that place stops asking.

use super::*;
use models::scripted_breakpoints::refuse;

/// A turn that has ended notes the answer for the model in force, so a refusal the turn learned is
/// said without waiting for the next pick.
///
/// Rejects an end of a turn that never asks the record.
#[test]
fn the_caching_off_note_is_made_after_a_turn_has_ended() {
    let config = a_config_needing_no_sign_in();
    let _record = refuse(&[config.default_model.as_str()]);
    let mut session = a_turn_running_on("first");
    assert!(!session.breakpoints_given_up(), "noted before any turn");

    stop_through_finish_turn(&mut session, "first");

    assert!(
        session.breakpoints_given_up(),
        "the end of a turn did not ask whether caching is off"
    );
}

/// A manifest run that has ended notes the answer too, since a run sends requests the way a turn
/// does and the model it was sent to may have refused the breakpoints in them.
///
/// Rejects an end of a run that never asks the record.
#[test]
fn the_caching_off_note_is_made_after_a_manifest_run_has_ended() {
    let config = a_config_needing_no_sign_in();
    let _record = refuse(&[config.default_model.as_str()]);
    let mut session = Session::new("none");
    session.begin_aside();
    assert!(!session.breakpoints_given_up(), "noted before any run");
    let worker = std::thread::spawn(|| {
        (
            Err(bravebot_agent::TurnError::Cancelled { attempts: None }),
            Trail::new(),
            TrustStore::new("/work"),
        )
    });
    // Stopped, so the run is charged and writes no record of its own.
    let cancel = Cancel::new();
    cancel.cancel();

    finish_manifest_run(
        &mut session,
        &config,
        &workspace_for_test(),
        worker,
        &mut TrustStore::new("/work"),
        &cancel,
        &crate::manifest_run::Run::default(),
        "summarise",
    );

    assert!(
        session.breakpoints_given_up(),
        "the end of a run did not ask whether caching is off"
    );
}

/// Choosing a model notes the answer for that model, and for the one chosen and no other: a model
/// the record does not hold clears what the model before it set.
///
/// Rejects a pick that never asks the record, one that keeps the answer of the model before it, and
/// one that asks about the model in force and not the one chosen.
#[test]
fn the_caching_off_note_follows_the_model_a_pick_puts_in_force() {
    let mut config = a_config_needing_no_sign_in();
    let _record = refuse(&["a-model-the-record-holds"]);
    let mut session = Session::new("none");

    for (key, given_up) in [
        ("a-model-the-record-holds", true),
        ("a-model-the-record-does-not-hold", false),
    ] {
        model_picked(
            &mut session,
            &mut config,
            listed(key, Some(200_000)),
            crate::model_prompt::Scope::EverySession,
        );
        assert_eq!(
            session.breakpoints_given_up(),
            given_up,
            "after choosing {key}"
        );
    }
}

/// The listing a session takes when it starts notes the answer for the model in force, so a model
/// recorded as refusing the breakpoints says so from the first frame of a session opened on it.
///
/// Rejects a start that never asks the record.
#[test]
fn the_caching_off_note_is_made_when_the_session_starts() {
    let mut config = a_config_needing_no_sign_in();
    let _record = refuse(&[config.default_model.as_str()]);
    let mut session = Session::new("none");
    assert!(!session.breakpoints_given_up(), "noted before the start");

    adopt_listing(&mut session, &mut config, &[]);

    assert!(
        session.breakpoints_given_up(),
        "the start did not ask whether caching is off"
    );
}
