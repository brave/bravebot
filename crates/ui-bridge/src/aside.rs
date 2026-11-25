// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! What the window is told about a question asked beside the work, and what the session keeps of it
//! (CMD-8, CMD-16).
//!
//! The request itself is [`bravebot_agent::aside::Question`], the one the terminal's `/btw` and
//! `/recap` make. This module reads its answer only as far as the aside's own policy decided: the
//! answer is carried to the window and into the row unread, and whether the record keeps it is the
//! [`Answered::kept`] the exchange's labels settled.

use bravebot_agent::aside::{Answered, AsideError};
use bravebot_session::sessions::{Aside, StoredAside};
use serde_json::{Value, json};

/// The question a recap's row shows, the same one the terminal's `/recap` row shows.
pub const RECAP_LABEL: &str = "/recap";

/// The `aside.done` event's data, the aside the session keeps, and what the request cost in tokens.
///
/// `status` is `answered` (with `answer` and `kept`), `stopped`, or `failed` (with `message`).
/// `stopped` is whether the person stopped the request, which is what tells a stop from a failure.
/// `asked` is the row's question: the person's line, or [`RECAP_LABEL`] for a recap.
pub fn settle(
    done: Result<Answered, AsideError>,
    stopped: bool,
    asked: String,
    recap: bool,
) -> (Value, Option<Aside>, u64) {
    match done {
        Ok(answered) => {
            let tokens = answered.usage.total();
            let kept = answered.kept.is_some();
            let event = json!({
                "status": "answered",
                "question": asked,
                "answer": answered.shown,
                "kept": kept,
                "recap": recap,
            });
            let aside = Aside {
                question: asked,
                answer: Some(answered.shown),
                kept,
            };
            (event, Some(aside), tokens)
        }
        Err(_) if stopped => (
            json!({"status": "stopped", "question": asked, "recap": recap}),
            None,
            0,
        ),
        Err(error) => (
            json!({
                "status": "failed",
                "question": asked,
                "recap": recap,
                "message": error.to_string(),
            }),
            None,
            0,
        ),
    }
}

/// Why `question` may not be asked beside the work, or `None` where it may.
pub fn refusal(question: &str) -> Option<&'static str> {
    question
        .trim()
        .is_empty()
        .then_some("/btw needs a question after it")
}

/// Why a `/btw` carrying a dropped file is refused while a turn runs, in the terminal's words
/// (CMD-5, CMD-8). The read a dropped file needs is one the running turn's policy does not hold.
pub const NO_FILES_BESIDE_A_TURN: &str =
    "/btw does not read dropped files while a turn runs; ask again when it has ended";

/// Why a `/btw` naming a dropped text file by path is refused: on a `/btw` line a text file is its
/// name, put into the question by its marker (DROP-4), and never a file the request reads.
pub const TEXT_FILE_IS_ITS_NAME: &str =
    "a text file dropped on a /btw line goes as its name; `dropped` is not taken";

/// Why a manifest run naming a dropped text file by path is refused: on a `/manifest` line a text
/// file is its name, put into the task by its marker (DROP-4), and never a file the planner reads.
pub const TEXT_FILE_IS_ITS_NAME_ON_A_MANIFEST: &str =
    "a text file dropped on a /manifest line goes as its name; `dropped` is not taken";

/// Why a `/btw` asked while a turn runs is refused before the turn has sent its first request:
/// there is no exchange of the turn's to ask it over yet (CMD-8).
pub const NOTHING_SENT_YET: &str = "the turn has not sent anything to the model yet, so there is nothing to ask about; \
     ask again in a moment";

/// The asides a record kept, as a window draws them on opening the session.
///
/// `answer` is null where the record did not keep it, and the window says so rather than drawing
/// an answer that is not there.
pub fn stored_json(asides: &[StoredAside]) -> Vec<Value> {
    asides
        .iter()
        .map(|aside| {
            json!({
                "question": aside.question,
                "answer": aside.answer,
                "kept": aside.answer.is_some(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answered(shown: &str, kept: bool) -> Answered {
        Answered {
            shown: shown.to_string(),
            kept: kept.then(|| shown.to_string()),
            usage: bravebot_aichat::protocol::Usage {
                prompt_tokens: 40,
                completion_tokens: 6,
                ..Default::default()
            },
        }
    }

    fn error() -> AsideError {
        AsideError::Chat(bravebot_agent::backend::BackendError::from(
            bravebot_aichat::ChatError::Cancelled,
        ))
    }

    /// CMD-16: an answer becomes the event, a row, and a charge.
    #[test]
    fn an_answer_is_shown_kept_and_charged() {
        let (event, aside, tokens) = settle(
            Ok(answered("We are adding retries.", true)),
            false,
            RECAP_LABEL.to_string(),
            true,
        );
        assert_eq!(event["status"], "answered");
        assert_eq!(event["answer"], "We are adding retries.");
        assert_eq!(event["question"], RECAP_LABEL);
        assert_eq!(event["kept"], true);
        assert_eq!(event["recap"], true);
        assert_eq!(
            aside,
            Some(Aside {
                question: RECAP_LABEL.to_string(),
                answer: Some("We are adding retries.".to_string()),
                kept: true,
            })
        );
        assert_eq!(tokens, 46);
    }

    /// WATCH-20: an answer from an exchange that met something untrusted is shown and is a row, and
    /// the row says the record does not keep it.
    #[test]
    fn an_answer_the_exchange_did_not_keep_is_shown_and_not_kept() {
        let (event, aside, _) = settle(
            Ok(answered("It retries twice.", false)),
            false,
            "why".to_string(),
            false,
        );
        assert_eq!(event["kept"], false);
        assert_eq!(event["answer"], "It retries twice.");
        assert!(aside.is_some_and(|aside| !aside.kept));
    }

    /// A stopped question is a stop, any other error carries its sentence, and neither is a row.
    #[test]
    fn a_stopped_or_failed_question_is_not_a_row() {
        let (stopped, aside, tokens) = settle(Err(error()), true, "why".to_string(), false);
        assert_eq!(stopped["status"], "stopped");
        assert!(aside.is_none());
        assert_eq!(tokens, 0);
        let (failed, aside, _) = settle(Err(error()), false, "why".to_string(), false);
        assert_eq!(failed["status"], "failed");
        assert!(!failed["message"].as_str().unwrap_or_default().is_empty());
        assert!(aside.is_none());
    }

    /// CMD-8: an empty question is refused before a request.
    #[test]
    fn an_empty_question_is_refused() {
        assert!(refusal("  \n").is_some());
        assert!(refusal("why is it shaped so").is_none());
    }
}
