// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! What a window is told about a failed turn (CHATGPT-8).

use super::*;
use bravebot_agent::backend::BackendError;
use bravebot_aichat::responses::protocol::Refusal;
use bravebot_aichat::{AccountFailure, ChatError, RenewalFailure, XaiFailure};
use std::time::Duration;

fn configured() -> Config {
    Config::from_lookup(|key| match key {
        "BRAVE_AI_CHAT_ENDPOINT" => Some("https://example.invalid".into()),
        "BRAVE_SERVICES_KEY_ID" => Some("a-key-id".into()),
        "SERVICES_KEY_AICHAT" => Some("a-signing-key".into()),
        _ => None,
    })
    .expect("configured")
}

fn failed_with(error: impl Into<BackendError>) -> Value {
    failure_fields(
        &TurnError::from(error.into()),
        &configured(),
        "chatgpt/gpt-test",
    )
}

/// CHATGPT-8: a failure of the account reaches the window as the fixed name of which one it was,
/// beside its category, so the window can say it in that failure's words.
/// The usage limit, the refusal that says the account may not be used and the model the account does
/// not offer are three names, and the counts of the call are as for any other failure.
///
/// The faults this rejects: sending the category alone, which says every refusal the same way,
/// and sending one name for the two refusals of the account.
#[test]
fn a_failure_of_the_account_is_sent_as_its_own_name_beside_its_category() {
    for (failure, name, category) in [
        (AccountFailure::NotSignedIn, "not-signed-in", "unauthorized"),
        (AccountFailure::Ended, "ended", "unauthorized"),
        (AccountFailure::Refused, "token-refused", "unauthorized"),
        (AccountFailure::Unusable, "unusable", "unauthorized"),
        (
            AccountFailure::Named {
                refusal: Refusal::UsageLimit,
                status: Some(429),
                retry_after: Some(Duration::from_secs(600)),
            },
            "usage-limit",
            "refused",
        ),
        (
            AccountFailure::Named {
                refusal: Refusal::NotPermitted,
                status: None,
                retry_after: None,
            },
            "not-permitted",
            "refused",
        ),
        (
            AccountFailure::ModelNotOffered,
            "model-not-offered",
            "refused",
        ),
        (
            AccountFailure::Renewal(RenewalFailure::Refused),
            "renewal-refused",
            "refused",
        ),
        (
            AccountFailure::Renewal(RenewalFailure::RateLimited),
            "renewal-rate-limited",
            "rate-limited",
        ),
        (
            AccountFailure::Renewal(RenewalFailure::Unavailable),
            "renewal-unavailable",
            "unavailable",
        ),
        (
            AccountFailure::Renewal(RenewalFailure::Incomplete),
            "renewal-incomplete",
            "incomplete",
        ),
        (
            AccountFailure::Renewal(RenewalFailure::Transport),
            "renewal-transport",
            "transport",
        ),
    ] {
        let sent = failed_with(ChatError::Account(failure));
        assert_eq!(sent["account"], name, "{failure:?}: {sent}");
        assert_eq!(sent["category"], category, "{failure:?}: {sent}");
        assert_eq!(sent["kind"], "chat", "{failure:?}: {sent}");
        // The status a refusal came with reaches the window beside the category, and no other
        // failure of the account carries one.
        assert_eq!(
            sent["status"],
            json!(failure.status()),
            "{failure:?}: {sent}"
        );
        // The sentence is the window's own, and the command it names is sent beside it, so the window
        // keeps no table of its own and reads no sentence for the words to draw as code.
        let said = crate::account_words::of_failure(failure);
        assert_eq!(sent["accountMessage"], said.sentence, "{failure:?}: {sent}");
        assert_eq!(
            sent["accountCommand"],
            json!(said.command),
            "{failure:?}: {sent}"
        );
        // Not the terminal's: that one is lower case, two sentences, and has the command in
        // backticks that a paragraph prints as they are.
        let terminal = bravebot_agent::chatgpt::said_of_failure(failure);
        assert_ne!(sent["accountMessage"], terminal, "{failure:?}: {sent}");
        assert!(
            !sent["accountMessage"]
                .as_str()
                .unwrap_or_default()
                .contains('`'),
            "{failure:?}: {sent}"
        );
    }

    let counted = failed_with(BackendError::Attempted {
        attempts: 2,
        usage: None,
        context_tokens: None,
        cause: Box::new(BackendError::from(ChatError::Account(
            AccountFailure::Ended,
        ))),
    });
    assert_eq!(counted["account"], "ended", "{counted}");
    assert_eq!(counted["attempts"], 2, "{counted}");
}

/// XAI-15: a failure of the xAI account reaches the window as its own fixed name beside its category,
/// with the window's sentence and the command it names, so the window says it in xAI's words and sends
/// a person to xAI's command. The name of one service's failure is never the other's, so a window
/// that drew ChatGPT's sentence for an xAI failure would name `bravebot auth login chatgpt`.
#[test]
fn a_failure_of_the_xai_account_is_sent_as_its_own_name_with_xais_sentence_and_command() {
    for (failure, name, category, command) in [
        (
            XaiFailure::NotSignedIn,
            "xai-not-signed-in",
            "unauthorized",
            Some("bravebot auth login xai"),
        ),
        (
            XaiFailure::Ended,
            "xai-ended",
            "unauthorized",
            Some("bravebot auth login xai"),
        ),
        (
            XaiFailure::Rejected,
            "xai-token-refused",
            "unauthorized",
            Some("bravebot auth login xai"),
        ),
        (
            XaiFailure::NotPermitted,
            "xai-not-permitted",
            "refused",
            None,
        ),
        (
            XaiFailure::Unusable,
            "xai-unusable",
            "unauthorized",
            Some("bravebot auth status xai"),
        ),
        (
            XaiFailure::Renewal(RenewalFailure::RateLimited),
            "xai-renewal-rate-limited",
            "rate-limited",
            None,
        ),
        (
            XaiFailure::Renewal(RenewalFailure::Transport),
            "xai-renewal-transport",
            "transport",
            None,
        ),
    ] {
        let sent = failed_with(ChatError::Account(AccountFailure::Xai(failure)));

        assert_eq!(sent["account"], name, "{failure:?}: {sent}");
        assert_eq!(sent["category"], category, "{failure:?}: {sent}");
        assert_eq!(
            sent["accountCommand"],
            json!(command),
            "{failure:?}: {sent}"
        );
        let message = sent["accountMessage"].as_str().unwrap_or_default();
        assert!(message.contains("xAI"), "{failure:?}: {sent}");
        assert!(
            !message.contains("ChatGPT") && !message.contains("chatgpt") && !message.contains('`'),
            "{failure:?}: {sent}"
        );
        assert_eq!(sent["status"], json!(null), "{failure:?}: {sent}");
    }
}

/// CHATGPT-8: the account is named only where the account failed. Any other failure of a chat
/// turn carries `null`, so a window that read a name where there was none would say a refused
/// gateway key is the ChatGPT sign-in failing.
#[test]
fn a_failure_with_no_account_in_it_is_sent_with_no_account_name() {
    let status = |status: u16| {
        failed_with(ChatError::Egress(bravebot_net::EgressError::Status {
            url: "https://api.example.invalid/v1/responses".to_string(),
            status,
            retry_after: None,
            body: None,
        }))
    };
    for (sent, category) in [
        (status(401), "unauthorized"),
        (status(429), "rate-limited"),
        (status(503), "unavailable"),
        (failed_with(ChatError::NoContent), "undecodable"),
    ] {
        assert_eq!(sent["account"], Value::Null, "{sent}");
        assert_eq!(sent["accountMessage"], Value::Null, "{sent}");
        assert_eq!(sent["accountCommand"], Value::Null, "{sent}");
        assert_eq!(sent["category"], category, "{sent}");
    }
    let workspace = failure_fields(
        &TurnError::Precommit("nothing".to_string()),
        &configured(),
        "stub-model",
    );
    assert_eq!(workspace["account"], Value::Null, "{workspace}");
    assert_eq!(workspace["accountMessage"], Value::Null, "{workspace}");
    assert_eq!(workspace["accountCommand"], Value::Null, "{workspace}");
}

/// CHATGPT-8: what is sent about a failure is the fixed names and the counts, and nothing a
/// reply, a request or a settings file held. The keys are `kind`, `message`, `category`, `attempts` and
/// `status`, and the name, the sentence and the command of the account's failure.
#[test]
fn the_failure_sent_holds_only_fixed_names_and_counts() {
    let sent = failed_with(ChatError::Account(AccountFailure::Named {
        refusal: Refusal::UsageLimit,
        status: None,
        retry_after: None,
    }));
    let mut keys: Vec<&str> = sent
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "account",
            "accountCommand",
            "accountMessage",
            "attempts",
            "category",
            "kind",
            "message",
            "status"
        ],
        "{sent}"
    );
    // `message` is the category's name and not a sentence of the account's, which the window draws as `accountMessage`.
    assert_eq!(sent["message"], "refused", "{sent}");
}
