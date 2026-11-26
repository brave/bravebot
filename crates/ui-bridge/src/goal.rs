// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! `/goal` typed in the window: the goal an open chat works towards, and what `session.goal`
//! answers.
//!
//! The state machine is [`bravebot_agent::goal`]'s, the one the terminal runs, and only the
//! sentences are the bridge's. Only a line a person typed into the composer reaches
//! [`command`] (CMD-1, GOAL-1): the window sends it as `session.goal`, which the host's allowlist
//! does not offer, and nothing a turn writes is a request.
//!
//! The slot sits on the open chat beside its watches and never in the session's state, so
//! nothing that writes the session record can reach it (GOAL-12). Closing the chat drops it.

use bravebot_agent::goal::{Asked, Running};
use bravebot_i18n::t;
use serde_json::{Value, json};
use std::time::Instant;

/// The goal an open chat works towards, if it has one.
#[derive(Debug, Default)]
pub(crate) struct GoalSlot {
    pub(crate) running: Option<Running>,
}

/// What `session.goal` did: the lines the window shows, and whether the goal changed, which is
/// when `goal.changed` is sent.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Answer {
    pub(crate) lines: Vec<String>,
    pub(crate) changed: bool,
}

/// Carry out `/goal` with what [`bravebot_agent::goal::parse`] read from its argument.
///
/// `tokens` is the chat's token total now, which a goal set here counts its spend from.
/// `mid_turn` says a turn is running, whose work the goal's record leaves out (GROUND-4).
/// `watches_ended` is how many watches the caller stopped before a goal is set (GOAL-11). Setting a
/// goal sends nothing (GOAL-2).
pub(crate) fn command(
    slot: &mut GoalSlot,
    asked: Asked,
    tokens: u64,
    mid_turn: bool,
    watches_ended: usize,
) -> Answer {
    let unchanged = |line: String| Answer {
        lines: vec![line],
        changed: false,
    };
    match asked {
        Asked::Report => match slot.running.as_ref() {
            None => unchanged(t!(desktop_goal_none).to_string()),
            Some(running) => Answer {
                lines: vec![
                    t!(desktop_goal_active, condition = running.condition()).to_string(),
                    match running.last_reason() {
                        Some(reason) => t!(desktop_goal_last_check, reason = reason).to_string(),
                        None => t!(desktop_goal_never_checked).to_string(),
                    },
                ],
                changed: false,
            },
        },
        Asked::Set(condition) => {
            let mut lines = Vec::new();
            if slot.running.is_some() {
                lines.push(t!(desktop_goal_replaced).to_string());
            }
            if watches_ended > 0 {
                lines.push(t!(desktop_goal_replaces_watches).to_string());
            }
            lines.push(t!(desktop_goal_set, condition = &condition).to_string());
            slot.running = Some(Running::begin(condition, tokens, mid_turn));
            Answer {
                lines,
                changed: true,
            }
        }
        Asked::Clear => match slot.running.take() {
            None => unchanged(t!(desktop_goal_none).to_string()),
            Some(_) => Answer {
                lines: vec![t!(desktop_goal_cleared).to_string()],
                changed: true,
            },
        },
        Asked::Pause => match slot.running.as_mut().map(Running::pause) {
            None => unchanged(t!(desktop_goal_none).to_string()),
            Some(false) => unchanged(t!(desktop_goal_already_paused).to_string()),
            Some(true) => Answer {
                lines: vec![t!(desktop_goal_paused).to_string()],
                changed: true,
            },
        },
        Asked::Resume => match slot.running.as_mut().map(Running::resume) {
            None => unchanged(t!(desktop_goal_none).to_string()),
            Some(false) => unchanged(t!(desktop_goal_not_paused).to_string()),
            Some(true) => Answer {
                lines: vec![t!(desktop_goal_resumed).to_string()],
                changed: true,
            },
        },
    }
}

/// The goal as the window draws it, or `null` where none is set.
///
/// `tokens` is the chat's token total now, and the spend is counted from the goal's start, so the
/// figures are counts the driver holds (GOAL-13).
pub(crate) fn snapshot(slot: &GoalSlot, tokens: u64, now: Instant) -> Value {
    match slot.running.as_ref() {
        None => Value::Null,
        Some(running) => json!({
            "condition": bravebot_agent::confirm::printable(running.condition()),
            "rounds": running.rounds(),
            "left": running.left(),
            "paused": running.is_paused(),
            "stalled": running.is_stalled(),
            "lastReason": running.last_reason().map(bravebot_agent::confirm::printable),
            "elapsedSeconds": running.elapsed(now).as_secs(),
            "spentTokens": running.spent(tokens),
        }),
    }
}

/// What `session.goal` answers: the goal as it now stands, whether it changed, and the lines.
pub(crate) fn answered(answer: &Answer, goal: Value) -> Value {
    let lines: Vec<String> = answer
        .lines
        .iter()
        .map(|line| bravebot_agent::confirm::printable(line))
        .collect();
    json!({ "goal": goal, "changed": answer.changed, "lines": lines })
}
