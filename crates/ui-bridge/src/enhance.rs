// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! What the window is told about a request to rewrite the message in its box (INPUT-50).
//!
//! The request itself is [`bravebot_agent::aside::Question::enhance`], the one the terminal's
//! Ctrl-P makes. This module is the half that reads its answer: whether the rewrite may be offered
//! at all, and what the window says where it may not. It reads no word of the rewrite. The only
//! things it looks at are whether the exchange kept the answer (the aside's own policy decided that
//! from the labels), how many characters the rewrite has, and how many of them draw as nothing.

use bravebot_agent::aside::{Answered, AsideError, ENHANCE_LIMIT, enhancement};
use serde_json::{Value, json};

/// The rewrite with every character that draws as nothing taken out, and how many that was.
///
/// What the person cannot see they cannot have meant, and the rewrite is about to be read and sent
/// as their own message. The window has no paste path that removes them, so they are taken out
/// here, before the rewrite is counted: one made of nothing but those characters is empty and is
/// not offered.
fn without_invisible(shown: &str) -> (String, usize) {
    let removed = shown
        .chars()
        .filter(|c| bravebot_core::invisible::draws_as_nothing(*c))
        .count();
    let kept = shown
        .chars()
        .filter(|c| !bravebot_core::invisible::draws_as_nothing(*c))
        .collect();
    (kept, removed)
}

/// The `enhance.done` event's data for what the request came back with, and what it cost in tokens.
///
/// `status` is `ready` (with `text` and `removed`), `untrusted` (the exchange had met something the
/// planner would not be shown, so nothing is offered), `unusable` (empty, or past `limit`
/// characters), `stopped`, or `failed` (with `message`). `cancelled` is whether the person stopped
/// the request, which is what tells a stop from a failure.
pub fn settle(done: Result<Answered, AsideError>, cancelled: bool) -> (Value, u64) {
    match done {
        Ok(answered) => {
            let tokens = answered.usage.total();
            if answered.kept.is_none() {
                return (json!({"status": "untrusted"}), tokens);
            }
            let (cleaned, removed) = without_invisible(&answered.shown);
            match enhancement(&cleaned) {
                Some(text) => (
                    json!({"status": "ready", "text": text, "removed": removed}),
                    tokens,
                ),
                None => (
                    json!({"status": "unusable", "limit": ENHANCE_LIMIT, "removed": removed}),
                    tokens,
                ),
            }
        }
        Err(_) if cancelled => (json!({"status": "stopped"}), 0),
        Err(error) => (json!({"status": "failed", "message": error.to_string()}), 0),
    }
}

/// Whether `draft` may be sent to be rewritten: it holds something, and no more than the rewrite
/// could be offered at.
pub fn refusal(draft: &str) -> Option<&'static str> {
    if draft.trim().is_empty() {
        Some("there is nothing in the message to make more specific")
    } else if draft.trim().chars().count() > ENHANCE_LIMIT {
        Some("the message is too long to be made more specific")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answered(shown: &str, kept: bool) -> Answered {
        Answered {
            shown: shown.to_string(),
            kept: kept.then(|| shown.to_string()),
            usage: Default::default(),
        }
    }

    /// INPUT-50: a rewrite the exchange kept is offered as the text, trimmed.
    #[test]
    fn a_rewrite_the_exchange_kept_is_offered() {
        let (event, _) = settle(
            Ok(answered("  Add a retry to `fetch_page`.\n", true)),
            false,
        );
        assert_eq!(event["status"], "ready");
        assert_eq!(event["text"], "Add a retry to `fetch_page`.");
        assert_eq!(event["removed"], 0);
    }

    /// INPUT-50: a rewrite from an exchange that met something untrusted is withheld whole, and
    /// the window is given no text to draw.
    #[test]
    fn a_rewrite_the_exchange_did_not_keep_is_withheld() {
        let (event, _) = settle(Ok(answered("Add a retry.", false)), false);
        assert_eq!(event["status"], "untrusted");
        assert!(event.get("text").is_none());
    }

    /// INPUT-50: an empty rewrite and one past the limit are not offered, and the limit counts
    /// characters.
    #[test]
    fn a_rewrite_that_is_empty_or_too_long_is_not_offered() {
        let (empty, _) = settle(Ok(answered("  \n", true)), false);
        assert_eq!(empty["status"], "unusable");
        let fits = "\u{e9}".repeat(ENHANCE_LIMIT);
        assert!(fits.len() > ENHANCE_LIMIT);
        assert_eq!(
            settle(Ok(answered(&fits, true)), false).0["status"],
            "ready"
        );
        let past = format!("{fits}\u{e9}");
        let (long, _) = settle(Ok(answered(&past, true)), false);
        assert_eq!(long["status"], "unusable");
        assert_eq!(long["limit"], ENHANCE_LIMIT);
    }

    /// INPUT-50: characters that draw as nothing are taken out and counted, and a rewrite made of
    /// nothing else is empty.
    #[test]
    fn characters_that_draw_as_nothing_are_taken_out_and_counted() {
        let (event, _) = settle(Ok(answered("Add\u{200b} a retry\u{e0041}.", true)), false);
        assert_eq!(event["text"], "Add a retry.");
        assert_eq!(event["removed"], 2);
        let (hidden, _) = settle(Ok(answered("\u{200b}\u{e0041}", true)), false);
        assert_eq!(hidden["status"], "unusable");
        assert_eq!(hidden["removed"], 2);
    }

    /// INPUT-50: a request the person stopped is a stop, and any other error carries its sentence.
    #[test]
    fn a_stopped_request_is_not_a_failure() {
        let error = || {
            AsideError::Chat(bravebot_agent::backend::BackendError::from(
                bravebot_aichat::ChatError::Cancelled,
            ))
        };
        assert_eq!(settle(Err(error()), true).0["status"], "stopped");
        let (event, tokens) = settle(Err(error()), false);
        assert_eq!(event["status"], "failed");
        assert!(!event["message"].as_str().unwrap_or_default().is_empty());
        assert_eq!(tokens, 0);
    }

    /// INPUT-50: the cost of an answer that was not offered is still reported, so it is charged.
    #[test]
    fn what_a_withheld_answer_cost_is_reported() {
        let mut spent = answered("Add a retry.", false);
        spent.usage = bravebot_aichat::protocol::Usage {
            prompt_tokens: 400,
            completion_tokens: 60,
            ..Default::default()
        };
        assert_eq!(settle(Ok(spent), false).1, 460);
    }

    /// INPUT-50: nothing is asked of the model for an empty draft or one past the limit.
    #[test]
    fn a_draft_with_nothing_in_it_or_too_much_is_refused_before_a_request() {
        assert!(refusal(" \n ").is_some());
        assert!(refusal(&"x".repeat(ENHANCE_LIMIT)).is_none());
        assert!(refusal(&"x".repeat(ENHANCE_LIMIT + 1)).is_some());
    }
}
