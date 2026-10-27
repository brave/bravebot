// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! CHATGPT-7: a turn says that a renewed ChatGPT sign-in could not be saved, once, and says it where
//! the renewal was made.
//!
//! # Why a binary of its own
//!
//! What the file holds of the renewals is recorded for the process, as the renewer is. A test that
//! sets it would have every other turn in its binary say what it set. An integration test is its
//! own process, and this binary holds one test.

use bravebot_agent::Workspace;
use bravebot_agent::confirm::ApproveWrites;
use bravebot_agent::report::Reporter;
use bravebot_core::cancel::Cancel;
use bravebot_core::event::RecordingSink;

#[path = "../test-support/mock_model.rs"]
mod mock_model;
#[allow(
    dead_code,
    reason = "`turns.rs` is shared by several test binaries, and this one calls only its `run_cancellable`"
)]
#[path = "../test-support/turns.rs"]
mod turns;

use mock_model::{Scratch, config_for, reply_with, serve_sequence};

/// What a turn's reporter was told, in order, and the moment a renewal the file cannot take is made.
#[derive(Default)]
struct Told {
    order: Vec<String>,
    spent: usize,
    /// After which report of what the turn has spent the renewal is made, as the goal check or a
    /// processor makes one when it asks for a token. A one-round turn reports three times: as the
    /// round starts, after the round's request, and as the turn ends.
    renewal_after_spent: Option<usize>,
}

impl Reporter for Told {
    fn todos(&mut self, _rows: Vec<bravebot_core::todo::Row>) {}

    fn notice(&mut self, text: String) {
        self.order.push(format!("notice: {text}"));
    }

    fn spent(&mut self, _spent: bravebot_agent::Spent) {
        self.spent += 1;
        self.order.push(format!("spent {}", self.spent));
        if self.renewal_after_spent == Some(self.spent) {
            bravebot_agent::chatgpt::note_a_renewal_the_file_could_not_take(true);
        }
    }
}

/// One turn on a model of the Brave endpoint, which asks for no account: the notice is about the
/// process's record, not about the model the turn runs on.
fn a_turn(told: &mut Told) {
    let scratch = Scratch::new("agent-chatgpt-renewal-notice");
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    let (endpoint, _received) = serve_sequence(vec![reply_with("done")]);
    let config = config_for(&endpoint);
    turns::run_cancellable(
        &config,
        &bravebot_net::Egress::new(),
        &workspace,
        &bravebot_agent::turn::Task::new("hello"),
        &mut ApproveWrites,
        told,
        &mut RecordingSink::new(),
        bravebot_agent::workspace::trust_store(workspace.root()),
        &Cancel::new(),
    )
    .expect("the turn runs");
}

/// What the person is told is the sentence for the failure, once, after the request that renewed it
/// and before the report that follows the request, and a renewal made after that report is told when
/// the turn ends. The regressions it rejects are the turn never saying it, saying it only at the end
/// (the first expected order), and saying it only after a request (the last).
#[test]
fn a_turn_says_a_renewal_the_file_could_not_take_where_it_was_made_and_once() {
    let said = format!("notice: {}", bravebot_i18n::t!(chatgpt_renewal_unwritten));

    // Nothing failed: nothing is said.
    let mut told = Told::default();
    a_turn(&mut told);
    assert!(
        told.order.iter().all(|line| !line.starts_with("notice")),
        "said with nothing to say: {:?}",
        told.order
    );
    let spent = told.spent;
    assert_eq!(spent, 3, "{:?}", told.order);
    let reports: Vec<String> = (1..=spent)
        .map(|number| format!("spent {number}"))
        .collect();
    let with_notice_at = |at: usize| {
        let mut order = reports.clone();
        order.insert(at, said.clone());
        order
    };

    // A renewal the file could not take before the request: said after that request, before the
    // report that follows it, and not again when the turn ends.
    bravebot_agent::chatgpt::note_a_renewal_the_file_could_not_take(true);
    let mut told = Told::default();
    a_turn(&mut told);
    assert_eq!(told.order, with_notice_at(1));

    // The same failure in the next turn is the same failure: it was said, so it is not said again.
    let mut told = Told::default();
    a_turn(&mut told);
    assert_eq!(told.order, reports, "a failure said once was said again");

    // A write that succeeds clears it, and a renewal made after the report that follows the
    // request, which only the turn's end can say, is said then: after that report and before the
    // turn's.
    bravebot_agent::chatgpt::note_a_renewal_the_file_could_not_take(false);
    let mut told = Told {
        renewal_after_spent: Some(2),
        ..Told::default()
    };
    a_turn(&mut told);
    assert_eq!(told.order, with_notice_at(2));
}
