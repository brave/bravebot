// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! What the window is told about a request to shorten a conversation (CMD-34).
//!
//! The request is [`bravebot_agent::turn::compact`], the one the terminal's `/compact` makes. This
//! module holds the stop and the end of one that is running, and turns what it came back with into
//! the `compact.done` event. The four outcomes are the four the terminal's `compact_ended` tells
//! apart: summarised, nothing to shorten, stopped, and failed.

use bravebot_agent::compact::{CompactError, Compacted};
use bravebot_core::cancel::Cancel;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A compaction a session has running.
///
/// While one is held the session's conversation is out of its state, on the worker, so the session
/// refuses a turn, a fork, an undo and a note until the worker has put it back and said so.
pub struct Compacting {
    /// The person's stop, which `session.compact.cancel` and `turn.cancel` both reach.
    pub cancel: Cancel,
    /// Set by the worker once the conversation is back and the record saved.
    finished: Arc<AtomicBool>,
}

impl Compacting {
    /// A compaction about to start, and the flag its worker sets on its way out.
    pub fn start() -> (Self, Arc<AtomicBool>) {
        let finished = Arc::new(AtomicBool::new(false));
        (
            Self {
                cancel: Cancel::new(),
                finished: Arc::clone(&finished),
            },
            finished,
        )
    }

    /// Whether the worker has put the conversation back.
    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }
}

/// The `compact.done` event's data for what the request came back with, and what it cost in tokens.
///
/// `status` is `compacted` (with `summarised` and `kept`, the counts of messages that stopped being
/// sent and of those still sent word for word), `nothing` (there was nothing worth shortening, and
/// nothing was asked), `stopped`, or `failed` (with `message`). `cancelled` is whether the person
/// stopped the request, which is what tells a stop from a failure. A summary that came back is kept
/// whether or not a stop arrived after it, as in the terminal.
pub fn settle(done: Result<Option<Compacted>, CompactError>, cancelled: bool) -> (Value, u64) {
    match done {
        Ok(Some(compacted)) => (
            json!({
                "status": "compacted",
                "summarised": compacted.summarised,
                "kept": compacted.kept,
            }),
            compacted.usage.total(),
        ),
        Ok(None) => (json!({"status": "nothing"}), 0),
        Err(error) => {
            let tokens = error.completed_usage().map_or(0, |usage| usage.total());
            if cancelled {
                (json!({"status": "stopped"}), tokens)
            } else {
                (
                    json!({
                        "status": "failed",
                        "message": bravebot_agent::report::why_a_summary_failed(&error),
                    }),
                    tokens,
                )
            }
        }
    }
}

/// The event for a worker that ended without an answer, so the window is not left waiting.
pub fn ended_unexpectedly() -> Value {
    json!({
        "status": "failed",
        "message": "the request to shorten the conversation ended unexpectedly",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_agent::backend::BackendError;

    fn compacted(summarised: usize, kept: usize) -> Compacted {
        Compacted {
            summarised,
            kept,
            model: "stub-model".to_string(),
            usage: Default::default(),
        }
    }

    fn a_failure() -> CompactError {
        CompactError::Chat(BackendError::NoGatewayToken {
            provider: "gateway".into(),
        })
    }

    /// CMD-34: each of the terminal's four endings has its own status. An adopted summary says how
    /// many messages it summarised and kept, and is kept when a stop arrived after it. Nothing to
    /// cut says so. An error after a stop is a stop, and one without is a failure that says why.
    #[test]
    fn each_ending_of_a_compaction_settles_into_its_own_status() {
        let (event, _) = settle(Ok(Some(compacted(6, 4))), false);
        assert_eq!(
            event,
            json!({"status": "compacted", "summarised": 6, "kept": 4})
        );
        assert_eq!(
            settle(Ok(Some(compacted(6, 4))), true).0["status"],
            "compacted"
        );
        assert_eq!(settle(Ok(None), false).0, json!({"status": "nothing"}));
        assert_eq!(
            settle(Err(a_failure()), true).0,
            json!({"status": "stopped"})
        );
        let (failed, _) = settle(Err(a_failure()), false);
        assert_eq!(failed["status"], "failed");
        assert!(
            failed["message"].as_str().is_some_and(|m| !m.is_empty()),
            "{failed}"
        );
    }

    /// The worker's flag is what the session reads to know the conversation is back.
    #[test]
    fn a_compaction_is_finished_once_its_worker_says_so() {
        let (compacting, finished) = Compacting::start();
        assert!(!compacting.is_finished());
        finished.store(true, Ordering::Release);
        assert!(compacting.is_finished());
    }
}
