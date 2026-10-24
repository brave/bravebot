// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! CHATGPT-8: a turn whose summary could not be made on the ChatGPT account says that failure's own
//! sentence in the line it narrates, which names the command that signs in again, and carries on with
//! the history it has.
//!
//! # Why a binary of its own
//!
//! The state directory the sign-in is read from is named in the process's environment, and the test
//! points it at a scratch directory with nobody signed in, so the turn fails before any request is
//! sent and no service is asked. An integration test is its own process, and this binary holds one
//! test.

use bravebot_agent::Workspace;
use bravebot_agent::turn::{self, Task};
use bravebot_core::cancel::Cancel;
use bravebot_core::event::RecordingSink;

#[path = "../test-support/mock_model.rs"]
mod mock_model;

use mock_model::Scratch;

/// A session of four exchanges that the server has already said is large.
fn a_long_conversation() -> bravebot_agent::Conversation {
    let mut conversation = bravebot_agent::Conversation::new();
    for (prompt, answer) in [
        ("port the parser to the new lexer", "started on it"),
        ("what about the error type", "widened it"),
        ("now the tests", "updated them"),
        ("carry on", "carrying on"),
    ] {
        conversation.push(bravebot_aichat::protocol::Message::user(prompt));
        conversation.push(bravebot_aichat::protocol::Message::assistant(answer));
    }
    conversation.measured(50_000);
    conversation
}

/// A summary that fails on the account is narrated as the account's sentence, in the one line that
/// says the turn goes on without it, and not as the name of a category, which cannot say which
/// command to type. The regressions it rejects are the category's name shown for a failure of the
/// account, and a line that left out that the turn carries on.
#[test]
fn a_summary_that_failed_on_the_account_is_narrated_in_the_accounts_sentence() {
    let scratch = Scratch::new("agent-chatgpt-compaction");
    let state = scratch.path.join("state");
    std::fs::create_dir_all(&state).expect("a state directory");
    // SAFETY: this binary holds one test, and nothing here reads the environment concurrently.
    unsafe { std::env::set_var(bravebot_agent::home::STATE_VARIABLE, &state) };
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    let config = bravebot_config::Config::from_lookup(|key| match key {
        "SERVICES_KEY_AICHAT" => Some("test-key".into()),
        "BRAVE_SERVICES_KEY_ID" => Some("test-id".into()),
        "BRAVE_AI_CHAT_ENDPOINT" => Some("http://127.0.0.1:9".into()),
        "BRAVEBOT_CONTEXT_BUDGET" => Some("1000".into()),
        _ => None,
    })
    .expect("config");
    let mut conversation = a_long_conversation();
    let mut reporter = bravebot_agent::report::RecordingReporter::default();
    let task = Task::new("finish").with_model(Some("chatgpt/gpt-6-sol".to_string()));

    let ended = turn::resume(
        &config,
        &bravebot_net::Egress::new(),
        &workspace,
        &task,
        &mut conversation,
        &mut bravebot_agent::confirm::ApproveWrites,
        &mut reporter,
        &mut RecordingSink::new(),
        bravebot_agent::workspace::trust_store(workspace.root()),
        bravebot_core::programs::TrustedPrograms::new(),
        None,
        &Cancel::new(),
    )
    .outcome;

    // Nobody is signed in, so the turn's own request fails as well. What is under test is the line
    // that the summary's failure narrated before it.
    assert!(ended.is_err(), "a turn with no sign-in went through");
    let narration = reporter.narration.join("\n");
    assert!(
        narration.contains("not signed in to a ChatGPT account"),
        "{narration}"
    );
    assert!(
        narration.contains("bravebot auth login chatgpt"),
        "{narration}"
    );
    assert!(
        narration.contains("continuing with the existing context"),
        "{narration}"
    );
    assert!(!narration.contains("unauthorized"), "{narration}");
}
