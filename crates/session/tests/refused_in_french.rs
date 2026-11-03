// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! BACKEND-37: the line for a refused request is the catalog's, so a person who chose French reads
//! the backend, the fields the retries left out and the next step in French, and the commands and
//! settings they would type are the same words as in English.
//!
//! In a process of its own because the locale is chosen once per process (LOCALE-7), and this binary
//! holds one test.

use bravebot_agent::{BackendKind, Category, Diagnosis};
use bravebot_aichat::call::{Conceded, Concession};
use bravebot_session::sessions::{failure_reason, failure_reason_with_model};

fn refused() -> Diagnosis {
    let mut given = Conceded::default();
    given.push(Concession::Effort);
    given.push(Concession::Ceiling);
    Diagnosis::of(Category::Refused)
        .with_status(400)
        .after(3)
        .with_conceded(given)
        .on_backend(BackendKind::Aichat)
}

/// Rejects a refused line that is still the reference language once French is chosen, one that
/// leaves a closed word of the backend or of a field untranslated, and one that drops a command or
/// a setting a person types.
#[test]
fn a_refused_request_is_said_in_the_chosen_catalog_and_names_the_same_commands() {
    let english = failure_reason_with_model(refused(), None, Some("model-x"));
    let plain_english = failure_reason(Diagnosis::of(Category::Refused), None);

    bravebot_i18n::init(bravebot_i18n::resolve("fr"));
    let french = failure_reason_with_model(refused(), None, Some("model-x"));
    let plain_french = failure_reason(Diagnosis::of(Category::Refused), None);

    assert_ne!(english, french, "the line is not translated");
    assert_ne!(
        plain_english, plain_french,
        "the plain line is not translated"
    );
    for typed in [
        "/model",
        "bravebot doctor",
        "/effort",
        "BRAVEBOT_OUTPUT_BUDGET",
        "HTTP 400",
    ] {
        assert!(french.contains(typed), "{typed} is missing: {french}");
    }
    assert!(
        french.contains("envoyée vers le point d'accès Brave"),
        "{french}"
    );
    assert!(french.contains("le niveau d'effort"), "{french}");
    assert!(french.contains("la limite de réponse supposée"), "{french}");
    assert!(
        french.contains("les nouvelles tentatives ayant retiré"),
        "{french}"
    );
    assert!(french.contains("Choisissez un autre modèle"), "{french}");
    assert!(french.contains("avec le modèle model-x"), "{french}");
    assert!(!french.contains("using the model"), "{french}");
    assert!(!french.contains("sent to"), "{french}");
    assert!(!french.contains("leaving out"), "{french}");
    assert!(
        plain_french.contains("Choisissez un autre modèle"),
        "{plain_french}"
    );
}
