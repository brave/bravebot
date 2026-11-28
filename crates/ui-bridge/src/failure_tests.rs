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
        conceded: Default::default(),
        backend: None,
        retry_setting: bravebot_agent::RetrySetting::None,
        idle_gap: None,
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

/// NAME-13: a turn that ended on the read of a named file is sent to the window as the workspace
/// failure it always was, with the category's name and no file name. The window refuses a send that
/// names an unreadable file before the turn starts (NAME-12), so it has no use for the name here.
///
/// The fault this rejects is the name or the reason's sentence being added to the fields sent, or
/// the kind changing for the named case.
#[test]
fn a_failure_on_a_named_file_is_sent_as_the_workspace_kind_with_no_name() {
    let named = TurnError::of_a_named_read(
        "typed-sentinel.txt",
        bravebot_agent::WorkspaceError::NotUtf8 {
            path: "typed-sentinel.txt".to_string(),
        },
    );
    assert!(named.named_file().is_some(), "{named}");
    let unnamed = TurnError::Workspace(bravebot_agent::WorkspaceError::NotUtf8 {
        path: "typed-sentinel.txt".to_string(),
    });
    let sent = failure_fields(&named, &configured(), "stub-model");
    assert_eq!(sent["kind"], "workspace", "{sent}");
    assert_eq!(sent["category"], "workspace", "{sent}");
    assert!(!sent.to_string().contains("sentinel"), "{sent}");
    assert_eq!(
        sent,
        failure_fields(&unnamed, &configured(), "stub-model"),
        "the fields differ from those of a plain workspace failure"
    );
}

/// BACKEND-37: the window words an unauthorized failure from its category and sends nothing of
/// the terminal's commands, which a window has no place to run, so a refusal of the premium key
/// is sent as every other unauthorized failure is, with its status and no account.
///
/// The faults this rejects: a premium key changing the category the window titles the card by,
/// and the flag the terminal words its commands from being sent as a field of its own.
#[test]
fn a_refused_premium_key_is_sent_as_any_other_unauthorized_failure() {
    let refused = || {
        ChatError::Egress(bravebot_net::EgressError::Status {
            url: "https://premium.example.invalid/v1/chat".to_string(),
            status: 401,
            retry_after: None,
            body: None,
        })
    };
    let marked = failed_with(BackendError::PremiumKey(Box::new(BackendError::from(
        refused(),
    ))));
    let plain = failed_with(BackendError::from(refused()));
    assert_eq!(marked, plain, "{marked}");
    assert_eq!(marked["category"], "unauthorized", "{marked}");
    assert_eq!(marked["status"], 401, "{marked}");
    assert_eq!(marked["account"], Value::Null, "{marked}");
}

/// BACKEND-37, SESSION-23: the reason the desktop stores for a turn whose request failed in
/// transport is the terminal's failure line, through the same `StoredOutcome::ended_by`, so it
/// names the cause, the gap in force, the setting that changes it and that a new message continues,
/// and holds nothing of the transport's own text.
///
/// The faults this rejects: the desktop wording a transport failure from its category alone, and
/// the transport's detail or address reaching the stored reason.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
#[expect(clippy::panic, reason = "a test fails by panicking")]
fn a_transport_failure_is_stored_with_the_terminals_line_naming_its_cause() {
    use bravebot_session::sessions::{StoredOutcome, failure_line};
    let failed = |cause, gap: Duration, setting| {
        TurnError::from(BackendError::Attempted {
            attempts: 2,
            usage: None,
            context_tokens: None,
            conceded: Default::default(),
            backend: None,
            retry_setting: setting,
            idle_gap: Some(gap),
            cause: Box::new(BackendError::from(ChatError::Egress(
                bravebot_net::EgressError::Transport {
                    url: "https://PRIVATE-HOST.example.invalid/v1".to_string(),
                    detail: "PRIVATE-DETAIL".to_string(),
                    transient: true,
                    cause: Some(cause),
                },
            ))),
        })
    };
    for (error, says) in [
        (
            failed(
                bravebot_net::TransportCause::IdleGap,
                Duration::from_secs(300),
                bravebot_agent::RetrySetting::Entry,
            ),
            [
                "went quiet for longer than the 5 minute gap",
                "`idleTimeoutMs`",
            ],
        ),
        (
            failed(
                bravebot_net::TransportCause::Connection,
                Duration::from_secs(300),
                bravebot_agent::RetrySetting::Entry,
            ),
            ["could not connect", "`retryWindowMs`"],
        ),
    ] {
        let stored = match StoredOutcome::ended_by(1, &error, Some("stub-model")) {
            StoredOutcome::Failed { reason } => reason,
            other => panic!("a failure was stored as {other:?}"),
        };
        let diagnosis = error.ending().diagnosis().expect("a failure");
        assert_eq!(
            stored,
            failure_line(&error, diagnosis, Some("stub-model")),
            "the desktop's reason is not the terminal's line"
        );
        for said in says {
            assert!(stored.contains(said), "{stored}");
        }
        assert!(
            stored.ends_with("A new message continues from what was already done"),
            "{stored}"
        );
        assert!(
            !stored.contains("PRIVATE"),
            "the transport's words were stored: {stored}"
        );
    }
}

/// BACKEND-37: a window is sent the agent's own line for a request that did not get through, the
/// line the terminal shows, made by the same function, so the desktop card says the cause, the gap
/// in force, the setting that changes it and that a new message continues, as the terminal does. It
/// is one sentence in sentence case with no backticks, as the card says a failure, and the setting
/// names arrive beside it, in order, for the card to draw as code. Nothing the transport wrote is
/// in either.
///
/// The faults this rejects: the bridge sending the category alone, which leaves the window's own
/// table to word it; the terminal's line sent as it is, with `error:`, backticks and a sentence for
/// each step; the setting names left for the window to find in the sentence; and the transport's
/// detail or address reaching the window.
#[test]
fn the_window_is_sent_the_agents_line_for_a_transport_failure() {
    let failed = |cause, setting| {
        failed_with(BackendError::Attempted {
            attempts: 3,
            usage: None,
            context_tokens: None,
            conceded: Default::default(),
            backend: None,
            retry_setting: setting,
            idle_gap: Some(Duration::from_secs(300)),
            cause: Box::new(BackendError::from(ChatError::Egress(
                bravebot_net::EgressError::Transport {
                    url: "https://PRIVATE-HOST.example.invalid/v1".to_string(),
                    detail: "PRIVATE-DETAIL".to_string(),
                    transient: true,
                    cause: Some(cause),
                },
            ))),
        })
    };
    let quiet = failed(
        bravebot_net::TransportCause::IdleGap,
        bravebot_agent::RetrySetting::Entry,
    );
    assert_eq!(quiet["category"], "transport", "{quiet}");
    assert_eq!(quiet["failureCause"], "idle-gap", "{quiet}");
    assert_eq!(
        quiet["failureMessage"],
        "The reply went quiet for longer than the 5 minute gap allowed, after 3 attempts; raise \
         idleTimeoutMs in the options of this provider entry to wait longer on a quiet reply, or \
         maxRetries to send the request again more times; a new message continues from what was \
         already done.",
        "{quiet}"
    );
    assert_eq!(
        quiet["failureCodes"],
        json!(["idleTimeoutMs", "maxRetries"]),
        "{quiet}"
    );

    let refused = failed(
        bravebot_net::TransportCause::Connection,
        bravebot_agent::RetrySetting::Entry,
    );
    let said = refused["failureMessage"].as_str().unwrap_or_default();
    assert!(
        said.starts_with("The request could not connect"),
        "{refused}"
    );
    assert!(!said.contains("gap"), "{refused}");
    assert_eq!(refused["failureCause"], "connection", "{refused}");
    assert_eq!(
        refused["failureCodes"],
        json!(["retryWindowMs", "maxRetries"]),
        "{refused}"
    );

    // A reply that did not begin has no setting that lengthens it, so none is named or sent.
    let silent = failed(
        bravebot_net::TransportCause::Begin,
        bravebot_agent::RetrySetting::Entry,
    );
    assert_eq!(silent["failureCodes"], json!([]), "{silent}");
    assert_eq!(silent["failureCause"], "begin", "{silent}");

    for sent in [&quiet, &refused, &silent] {
        let said = sent["failureMessage"].as_str().unwrap_or_default();
        assert!(
            said.ends_with("; a new message continues from what was already done."),
            "{sent}"
        );
        assert_eq!(said.matches('.').count(), 1, "not one sentence: {sent}");
        assert!(!said.contains('`') && !said.contains("error:"), "{sent}");
        assert!(!sent.to_string().contains("PRIVATE"), "{sent}");
    }

    // Any other failure is worded by the window from its category.
    let other = failed_with(ChatError::NoContent);
    assert_eq!(other["failureMessage"], Value::Null, "{other}");
    assert_eq!(other["failureCodes"], Value::Null, "{other}");
}

/// BACKEND-37: a reply that began and then went quiet past the gap is incomplete, not a transport
/// failure, and the window is sent the agent's line for it all the same, naming the gap in force
/// and the setting that changes it, with `idle-gap` as the cause the card is titled by. An
/// incomplete reply that stopped for any other reason has no gap or setting to name, so it is sent
/// neither and the window words it from its category.
///
/// The faults this rejects: the line and the cause sent only for the transport category, which
/// leaves a reply that went quiet with the category's words and no setting; and the line or the
/// cause sent for every incomplete reply, which names a gap that did not end it.
#[test]
fn the_window_is_sent_the_agents_line_for_a_reply_that_went_quiet_past_the_gap() {
    let stopped = |cause: ChatError| {
        failed_with(BackendError::Attempted {
            attempts: 3,
            usage: None,
            context_tokens: None,
            conceded: Default::default(),
            backend: None,
            retry_setting: bravebot_agent::RetrySetting::Entry,
            idle_gap: Some(Duration::from_secs(300)),
            cause: Box::new(BackendError::from(cause)),
        })
    };
    let quiet = stopped(ChatError::WentQuiet);
    assert_eq!(quiet["category"], "incomplete", "{quiet}");
    assert_eq!(quiet["failureCause"], "idle-gap", "{quiet}");
    let said = quiet["failureMessage"].as_str().unwrap_or_default();
    assert!(
        said.starts_with("The reply went quiet for longer than the 5 minute gap allowed"),
        "{quiet}"
    );
    assert!(said.contains("idleTimeoutMs"), "{quiet}");
    assert_eq!(
        quiet["failureCodes"],
        json!(["idleTimeoutMs", "maxRetries"]),
        "{quiet}"
    );

    let dropped = stopped(ChatError::Incomplete);
    assert_eq!(dropped["category"], "incomplete", "{dropped}");
    assert_eq!(dropped["failureMessage"], Value::Null, "{dropped}");
    assert_eq!(dropped["failureCause"], Value::Null, "{dropped}");
}

/// BACKEND-37, BACKEND-42: a window is sent the agent's own line for a reply that reached its output
/// limit, so the desktop card names the limit, the call the reply was part way through,
/// `BRAVEBOT_OUTPUT_BUDGET` as what raises it, that the limit is on one reply and not on the
/// context window, so compaction does not change it, and what to do next, as the terminal does. The
/// setting name arrives in `failureCodes`, so the card draws it as code. The tool's name is the
/// request's own spelling of a tool it offered.
///
/// The faults this rejects: the bridge sending the category alone, which leaves the window to word
/// the limit from a table that says less; the setting named only where the ceiling is known, or
/// written without backticks, so it is not sent as a code; a variant of the line that leaves out
/// the context window; and a line with no next step.
#[test]
fn the_window_is_sent_the_agents_line_for_a_reply_that_reached_its_output_limit() {
    use bravebot_aichat::{CutOff, OpenCall};
    let stopped = |call: Option<&str>, thought| {
        failed_with(ChatError::TooLong(CutOff {
            ceiling: 32_000,
            call: call.map(|tool| OpenCall {
                tool: Some(tool.to_string()),
                arguments: 0,
            }),
            thought,
        }))
    };
    let call = stopped(Some("write_file"), true);
    assert_eq!(call["category"], "too-long", "{call}");
    assert_eq!(
        call["failureMessage"],
        "The model reached its output limit of 32000 tokens, which BRAVEBOT_OUTPUT_BUDGET raises, \
         part way through a call to write_file, so the call was not made; the limit is on one reply \
         and not on the context window, so compaction does not change it; ask for the work in \
         smaller parts, or raise the limit in the env block of ~/.bravebot/settings.json.",
        "{call}"
    );
    assert_eq!(
        call["failureCodes"],
        json!(["BRAVEBOT_OUTPUT_BUDGET"]),
        "{call}"
    );

    let thinking = stopped(None, true);
    // A reply that stopped in no call and with no thinking says the same.
    let neither = failure_fields(
        &TurnError::from(BackendError::from(ChatError::TooLong(CutOff {
            ceiling: 8_192,
            call: None,
            thought: false,
        }))),
        &configured(),
        "stub-model",
    );
    for sent in [&call, &thinking, &neither] {
        let said = sent["failureMessage"].as_str().unwrap_or_default();
        assert!(said.contains("BRAVEBOT_OUTPUT_BUDGET"), "{sent}");
        assert!(!said.contains('`'), "{sent}");
        assert_eq!(
            sent["failureCodes"],
            json!(["BRAVEBOT_OUTPUT_BUDGET"]),
            "{sent}"
        );
        assert!(
            said.contains(
                "the limit is on one reply and not on the context window, so compaction does not \
                 change it"
            ),
            "{sent}"
        );
        assert!(
            said.ends_with(
                "; ask for the work in smaller parts, or raise the limit in the env block of \
                 ~/.bravebot/settings.json."
            ),
            "{sent}"
        );
        assert!(
            said.starts_with("The model reached its output limit"),
            "{sent}"
        );
    }
    assert!(
        thinking["failureMessage"]
            .as_str()
            .unwrap_or_default()
            .contains("while it was still thinking"),
        "{thinking}"
    );
}

/// CHATGPT-8: what is sent about a failure is the fixed names and the counts, and nothing a
/// reply, a request or a settings file held. The keys are `kind`, `message`, `category`, `attempts` and
/// `status`, the name, the sentence and the command of the account's failure, and the agent's line
/// and the setting names in it, which are `null` for a failure of the account.
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
            "failureCause",
            "failureCodes",
            "failureMessage",
            "kind",
            "message",
            "status"
        ],
        "{sent}"
    );
    // The account's own sentence is the wording, so the agent's line is not sent beside it.
    assert_eq!(sent["failureMessage"], Value::Null, "{sent}");
    assert_eq!(sent["failureCause"], Value::Null, "{sent}");
    assert_eq!(sent["failureCodes"], Value::Null, "{sent}");
    // A failure of no account and no transport has no cause to title a card by either.
    let other = failed_with(ChatError::NoContent);
    assert_eq!(other["failureCause"], Value::Null, "{other}");
    // `message` is the category's name and not a sentence of the account's, which the window draws as `accountMessage`.
    assert_eq!(sent["message"], "refused", "{sent}");
}
