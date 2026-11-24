// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! What the window is told about a request for a brief to hand a chat off from (SESSION-34,
//! CMD-34).
//!
//! The request is [`bravebot_agent::aside::Question::handoff`], the one the terminal's `/handoff`
//! makes. This module holds the sentences for the refusals made before it is sent, which are
//! decided by [`bravebot_agent::aside::handoff_refusal`] as the terminal's are, and turns its answer
//! into the `handoff.done` event. It reads no word of the brief. Whether the brief is offered is
//! whether the exchange kept it, which the aside's own policy decided from the labels.

use bravebot_agent::aside::{Answered, AsideError, HandoffRefusal, handoff_prompt};
use serde_json::{Value, json};

/// Why a handoff toward `goal` may not be asked for yet, as the sentence the window is told, or
/// `None` where it may. `turns` is how many turns the session has ended, and `recorded` whether it
/// has a record on disk.
pub fn refusal(goal: &str, turns: usize, recorded: bool) -> Option<&'static str> {
    bravebot_agent::aside::handoff_refusal(goal, turns, recorded).map(|refusal| match refusal {
        HandoffRefusal::NoGoal => "Say what the next chat should do.",
        HandoffRefusal::NothingToHandOff => {
            "There is nothing to hand off until a turn of this chat has ended."
        }
    })
}

/// The `handoff.done` event's data for what the request came back with, and what it cost in tokens.
///
/// `status` is `ready` (with `text`, the brief, the sentence saying what did not come with it, and
/// the goal, in that order), `untrusted` (the exchange had met something the planner would not be
/// shown, so nothing is offered), `stopped`, or `failed` (with `message`). `cancelled` is whether
/// the person stopped the request, which is what tells a stop from a failure.
pub fn settle(done: Result<Answered, AsideError>, goal: &str, cancelled: bool) -> (Value, u64) {
    match done {
        Ok(answered) => {
            let tokens = answered.usage.total();
            if answered.kept.is_none() {
                return (json!({"status": "untrusted"}), tokens);
            }
            (
                json!({"status": "ready", "text": handoff_prompt(&answered.shown, goal)}),
                tokens,
            )
        }
        Err(_) if cancelled => (json!({"status": "stopped"}), 0),
        Err(error) => (json!({"status": "failed", "message": error.to_string()}), 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answered(shown: &str, kept: bool) -> Answered {
        Answered {
            shown: shown.to_string(),
            kept: kept.then(|| shown.to_string()),
            usage: bravebot_aichat::protocol::Usage {
                prompt_tokens: 300,
                completion_tokens: 40,
                ..Default::default()
            },
        }
    }

    /// SESSION-34: no goal, no ended turn and no record are each refused, and a goal over a
    /// recorded session that has ended a turn is not.
    #[test]
    fn the_refusals_are_the_terminals() {
        assert!(refusal("", 3, true).is_some());
        assert!(refusal("  \n", 3, true).is_some());
        assert!(refusal("write the tests", 0, true).is_some());
        assert!(refusal("write the tests", 3, false).is_some());
        assert_ne!(refusal("", 3, true), refusal("write the tests", 0, true));
        assert!(refusal("write the tests", 1, true).is_none());
    }

    /// SESSION-34: a brief the exchange kept is offered with the goal on the end, and one it did
    /// not keep is withheld with no text, its cost still reported. A stop is not a failure.
    #[test]
    fn a_brief_is_offered_only_where_the_exchange_kept_it() {
        let (ready, tokens) = settle(
            Ok(answered("We added retries to fetch_page.", true)),
            "write the tests",
            false,
        );
        assert_eq!(ready["status"], "ready");
        assert_eq!(
            ready["text"],
            handoff_prompt("We added retries to fetch_page.", "write the tests")
        );
        assert!(
            ready["text"]
                .as_str()
                .is_some_and(|text| text.starts_with("We added retries")
                    && text.ends_with("write the tests")),
            "{ready}"
        );
        assert_eq!(tokens, 340);

        let (withheld, tokens) = settle(
            Ok(answered("We added retries to fetch_page.", false)),
            "write the tests",
            false,
        );
        assert_eq!(withheld, json!({"status": "untrusted"}));
        assert_eq!(tokens, 340);

        let error = || {
            AsideError::Chat(bravebot_agent::backend::BackendError::from(
                bravebot_aichat::ChatError::Cancelled,
            ))
        };
        assert_eq!(
            settle(Err(error()), "write the tests", true).0,
            json!({"status": "stopped"})
        );
        let (failed, _) = settle(Err(error()), "write the tests", false);
        assert_eq!(failed["status"], "failed");
        assert!(failed.get("text").is_none());
    }
}
