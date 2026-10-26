// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! What the window is told when a turn moves to the person's fallback model (CHATGPT-8,
//! BACKEND-53).

use super::*;
use crate::emit::Emitter;
use bravebot_agent::Category;
use bravebot_aichat::{AccountFailure, RenewalFailure};
use serde_json::json;

/// What a reporter says when asked to narrate a move to the fallback model, as events.
fn said_for(why: Why) -> Vec<Value> {
    let (sender, heard) = std::sync::mpsc::channel();
    let emitter = Emitter::new(Box::new(move |event| {
        if event.name == "narration" {
            let _ = sender.send(event.data);
        }
    }));
    BridgeReporter::new(emitter, "s1").fell_back("chatgpt/gpt-a", "chatgpt/gpt-b", why);
    heard.try_iter().collect()
}

/// CHATGPT-8: where the ChatGPT account's renewal is what failed, the window is told that failure in
/// its own sentence, one for each cause, with no command beside it and none of the terminal's line.
/// The window is handed the closed value, so what it gets differs with the value and never with the
/// category alone. A failure of any other service is narrated as the terminal narrates it.
///
/// The regression it rejects is the bridge keeping the default, which narrates the terminal's lower
/// case line, `chatgpt/gpt-a failed (the ChatGPT sign-in could not be renewed: ...)`, and so tells
/// the window nothing in its own words and sends no `command` field.
#[test]
fn a_move_to_the_fallback_after_the_account_failed_reaches_the_window_as_its_own_sentence() {
    let limited = said_for(Why::Account(AccountFailure::Renewal(
        RenewalFailure::RateLimited,
    )));
    assert_eq!(
        limited,
        [json!({
            "text": "This turn moves from chatgpt/gpt-a to chatgpt/gpt-b because the ChatGPT sign-in could not be renewed while OpenAI is limiting requests.",
            "command": null,
        })]
    );

    let unavailable = said_for(Why::Account(AccountFailure::Renewal(
        RenewalFailure::Unavailable,
    )));
    assert_eq!(
        unavailable,
        [json!({
            "text": "This turn moves from chatgpt/gpt-a to chatgpt/gpt-b because the ChatGPT sign-in could not be renewed while OpenAI's sign-in service is unavailable.",
            "command": null,
        })]
    );

    // Any other service is narrated as the terminal narrates it, with no `command` field.
    assert_eq!(
        said_for(Why::Category(Category::Unavailable)),
        [json!({
            "text": "chatgpt/gpt-a failed as unavailable; this turn moves to chatgpt/gpt-b",
        })]
    );
}
