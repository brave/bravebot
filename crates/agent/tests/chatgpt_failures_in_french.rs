// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! CHATGPT-8: the sentences for a failure of the ChatGPT account are the catalog's, so a person who
//! chose French reads them in French, in the error a turn, a goal check and a compaction print.
//!
//! In a process of its own because the locale is chosen once per process (LOCALE-7), and this binary
//! holds one test.

use bravebot_agent::backend::BackendError;
use bravebot_agent::chatgpt::{said_of_failure, said_of_refusal};
use bravebot_aichat::responses::protocol::Refusal;
use bravebot_aichat::{AccountFailure, ChatError, RenewalFailure};
use std::time::Duration;

/// Every condition the account can fail in, and a usage limit that asked for a wait.
fn every_failure() -> Vec<AccountFailure> {
    let refused = |refusal| AccountFailure::Named {
        refusal,
        status: None,
        retry_after: None,
    };
    vec![
        AccountFailure::NotSignedIn,
        AccountFailure::Ended,
        AccountFailure::Refused,
        AccountFailure::Unusable,
        refused(Refusal::UsageLimit),
        refused(Refusal::NotPermitted),
        AccountFailure::ModelNotOffered,
        AccountFailure::Renewal(RenewalFailure::Refused),
        AccountFailure::Renewal(RenewalFailure::RateLimited),
        AccountFailure::Renewal(RenewalFailure::Unavailable),
        AccountFailure::Renewal(RenewalFailure::Incomplete),
        AccountFailure::Renewal(RenewalFailure::Transport),
        AccountFailure::Named {
            refusal: Refusal::UsageLimit,
            status: Some(429),
            retry_after: Some(Duration::from_secs(601)),
        },
    ]
}

/// A sentence in the reference language is not the sentence in French, the commands that are the
/// remedy are the same in both since a person types them, and the error a turn prints is the French
/// sentence once the catalog is chosen. A wait is said in French too.
#[test]
fn each_failure_of_the_account_is_said_in_the_chosen_catalog_and_names_the_same_command() {
    let in_english: Vec<String> = every_failure().into_iter().map(said_of_failure).collect();

    bravebot_i18n::init(bravebot_i18n::resolve("fr"));
    let in_french: Vec<String> = every_failure().into_iter().map(said_of_failure).collect();

    for ((failure, english), french) in every_failure().iter().zip(&in_english).zip(&in_french) {
        assert_ne!(english, french, "{failure:?} is not translated");
        for command in [
            "bravebot auth login chatgpt",
            "bravebot auth status chatgpt",
        ] {
            assert_eq!(
                english.contains(command),
                french.contains(command),
                "{failure:?}: {french}"
            );
        }
        // The status of a refusal that came with one follows the sentence.
        let expected = match failure.status() {
            Some(status) => format!("{french} (HTTP {status})"),
            None => french.clone(),
        };
        let printed = BackendError::from(ChatError::Account(*failure)).to_string();
        assert_eq!(
            printed, expected,
            "{failure:?} was printed in another language"
        );
    }
    assert!(in_french[1].contains("a pris fin"), "{}", in_french[1]);
    assert_eq!(
        said_of_refusal(Refusal::UsageLimit),
        in_french[4],
        "a refusal is worded one way in the listing and in a turn"
    );
    assert!(
        in_french[6].contains("ne propose pas ce modèle"),
        "{}",
        in_french[6]
    );
    assert!(
        in_french[12].contains("une attente d'environ 11 minutes"),
        "{}",
        in_french[12]
    );
}
