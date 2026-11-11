// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! What the window is told about a turn's retrievals
//! ([RETRIEVE-27](../../../docs/specs/retrieval.md#RETRIEVE-27)).

use super::*;
use crate::emit::Emitter;
use bravebot_agent::report::{Place, Retrieved, Spoken};
use bravebot_agent::retrieval::Unavailable;
use serde_json::json;

fn place(ordinal: u32, prompt: u32, said: Spoken) -> Place {
    Place {
        ordinal,
        prompt: Some(prompt),
        said,
    }
}

fn retrieved(used: Vec<Place>, tokens: u64) -> Retrieved {
    Retrieved {
        used,
        tokens,
        fell_back: None,
    }
}

/// Every `turn.retrieved` a reporter for turn 4 sends for these rounds, in order.
fn told(rounds: Vec<Retrieved>) -> Vec<Value> {
    let (sender, heard) = std::sync::mpsc::channel();
    let emitter = Emitter::new(Box::new(move |event| {
        if event.name == "turn.retrieved" {
            let _ = sender.send(event.data);
        }
    }));
    let mut reporter = BridgeReporter::new(emitter, "s1").numbered(4);
    for round in rounds {
        reporter.retrieved(round);
    }
    heard.try_iter().collect()
}

/// RETRIEVE-27: the first round that retrieved is told with its turn, as positions and counts, each
/// with the ordinal a window asks for its excerpts by and no word of them (RETRIEVE-32); a
/// later round drawing on the same messages says nothing, and one that adds to them sends the
/// union. A fallback is told only where nothing was told yet.
#[test]
fn a_turns_retrievals_are_told_once_and_again_only_when_they_add_a_message() {
    let first = vec![
        place(5, 1, Spoken::Reply { nth: 0 }),
        place(2, 0, Spoken::Prompt),
    ];
    let heard = told(vec![
        retrieved(first.clone(), 30),
        retrieved(first, 30),
        retrieved(vec![place(7, 2, Spoken::Tool { nth: Some(1) })], 12),
        Retrieved {
            used: Vec::new(),
            tokens: 0,
            fell_back: Some(Unavailable::OutOfTime),
        },
    ]);

    assert_eq!(
        heard,
        [
            json!({
                "turn": 4,
                "used": [
                    {"ordinal": 2, "prompt": 0, "said": "prompt", "nth": null},
                    {"ordinal": 5, "prompt": 1, "said": "reply", "nth": 0},
                ],
                "tokens": 30,
                "fellBack": null,
            }),
            json!({
                "turn": 4,
                "used": [
                    {"ordinal": 2, "prompt": 0, "said": "prompt", "nth": null},
                    {"ordinal": 5, "prompt": 1, "said": "reply", "nth": 0},
                    {"ordinal": 7, "prompt": 2, "said": "tool", "nth": 1},
                ],
                "tokens": 42,
                "fellBack": null,
            }),
        ]
    );

    let fell_back = told(vec![Retrieved {
        used: Vec::new(),
        tokens: 0,
        fell_back: Some(Unavailable::Key),
    }]);
    assert_eq!(
        fell_back,
        [json!({"turn": 4, "used": [], "tokens": 0, "fellBack": "key"})]
    );
}

/// RETRIEVE-27: a transcript drawn from a record is told what each exchange's retrievals drew on,
/// keyed by the prompt its user row carries.
#[test]
fn a_recounted_transcript_carries_what_each_exchange_recalled() {
    assert_eq!(
        wire::recalled(&[(3, vec![place(0, 0, Spoken::Other)])]),
        json!([{"prompt": 3, "used": [{"ordinal": 0, "prompt": 0, "said": "other", "nth": null}]}])
    );
}
