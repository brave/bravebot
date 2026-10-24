// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! What a window is told about a failed turn (CHATGPT-8).

use super::*;
use bravebot_agent::backend::BackendError;
use bravebot_aichat::responses::protocol::Refusal;
use bravebot_aichat::{AccountFailure, ChatError, RenewalFailure};

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
/// The usage limit and the refusal that says the account may not be used are two names, and the
/// counts of the call are as for any other failure.
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
            AccountFailure::Named(Refusal::UsageLimit),
            "usage-limit",
            "refused",
        ),
        (
            AccountFailure::Named(Refusal::NotPermitted),
            "not-permitted",
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
        assert_eq!(sent["status"], Value::Null, "{failure:?}: {sent}");
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
        assert_eq!(sent["category"], category, "{sent}");
    }
    let workspace = failure_fields(
        &TurnError::Precommit("nothing".to_string()),
        &configured(),
        "stub-model",
    );
    assert_eq!(workspace["account"], Value::Null, "{workspace}");
}

/// CHATGPT-8: what is sent about a failure is the fixed names and the counts, and nothing a
/// reply, a request or a settings file held. The keys are `kind`, `message`, `category`, `attempts` and
/// `status`, and the name of the account's failure.
#[test]
fn the_failure_sent_holds_only_fixed_names_and_counts() {
    let sent = failed_with(ChatError::Account(AccountFailure::Named(
        Refusal::UsageLimit,
    )));
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
            "account", "attempts", "category", "kind", "message", "status"
        ],
        "{sent}"
    );
    // `message` is the category's name and not a sentence of the account's, which the window words.
    assert_eq!(sent["message"], "refused", "{sent}");
}
