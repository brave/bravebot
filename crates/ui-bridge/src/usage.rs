// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! What a session has spent, as the terminal's `/cost` and `/status` report it.
//!
//! Tokens and time, never money: nothing here knows what a token is charged at, and a figure in
//! money would be composed rather than measured.

use serde_json::{Value, json};

use crate::running::State;

/// The answer to `session.usage`, read from a session between turns.
///
/// `unattributed` is what the total holds that no turn accounts for, the row `/cost` adds so the
/// breakdown adds up: a record written before turns were charged separately keeps all of it there.
pub fn report(state: &State) -> Value {
    let by_turn: Vec<Value> = state
        .spend
        .iter()
        .map(|(turn, tokens)| json!({"turn": turn, "tokens": tokens}))
        .collect();
    let unattributed = state.tokens.saturating_sub(state.spend.values().sum());
    let mut timing = bravebot_agent::timing::Timing::default();
    for turn in state.timing.values() {
        timing.add(*turn);
    }
    let mut report = json!({
        "turns": state.turns,
        "tokens": state.tokens,
        "byTurn": by_turn,
        "unattributed": unattributed,
        "timing": {
            "wallMs": timing.wall_ms,
            "inferenceMs": timing.inference_ms,
            "toolsMs": timing.tools_ms,
            "stalledMs": timing.stalled_ms,
            "overheadMs": timing.overhead_ms(),
        },
    });
    // Only where the service reported something: both zero says a backend that reports no cache as
    // readily as a turn whose cache missed.
    if let Some(cached) = state.cached.filter(bravebot_aichat::protocol::Cached::any) {
        report["cached"] = json!({"read": cached.read_tokens, "written": cached.written_tokens});
    }
    report
}
