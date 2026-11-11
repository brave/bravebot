// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! What each `turn.send` of an open session was answered, by its `messageId` (RPCVIEW-11).

use crate::protocol::Failure;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// The most sends a session records. A send past it is refused rather than a record dropped, since
/// a dropped id could be sent again and start a second turn.
pub const SENDS_KEPT: usize = 4096;

#[derive(Default)]
pub struct Sends {
    sent: HashMap<String, Sent>,
}

struct Sent {
    digest: [u8; 32],
    answer: Result<Value, Failure>,
}

fn digest(params: &Value) -> [u8; 32] {
    Sha256::digest(serde_json::to_vec(params).unwrap_or_default()).into()
}

impl Sends {
    /// The answer a send of `id` was already given, or `None` for an id not yet sent. A send that
    /// would need a record past [`SENDS_KEPT`], or that names an id sent with other parameters, is
    /// refused.
    pub fn earlier(&self, id: &str, params: &Value) -> Option<Result<Value, Failure>> {
        match self.sent.get(id) {
            Some(sent) if sent.digest == digest(params) => Some(sent.answer.clone()),
            Some(_) => Some(Err(Failure::bad_request(
                "this messageId was sent before with other parameters; a new send needs a new messageId",
            ))),
            None if self.sent.len() >= SENDS_KEPT => Some(Err(Failure::bad_request(format!(
                "this session has recorded {SENDS_KEPT} sends, the most it keeps; \
                 another send needs a new session"
            )))),
            None => None,
        }
    }

    pub fn record(&mut self, id: String, params: &Value, answer: Result<Value, Failure>) {
        let digest = digest(params);
        self.sent.insert(id, Sent { digest, answer });
    }

    /// The answer to `turn.lookup` for `id`.
    pub fn lookup(&self, id: &str) -> Value {
        match self.sent.get(id).map(|sent| &sent.answer) {
            Some(Ok(started)) => json!({
                "state": "accepted",
                "turn": started["turn"],
                "target": started["target"],
            }),
            Some(Err(failure)) => json!({
                "state": "refused",
                "error": { "code": failure.code.as_str(), "message": failure.message },
            }),
            None => json!({ "state": "not_received" }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ErrorCode;

    fn params(prompt: &str) -> Value {
        json!({"session": "s1", "prompt": prompt, "messageId": "m1"})
    }

    /// A refusal is recorded as an acceptance is, so the same send is refused the same way again,
    /// even once the turn that made the session busy has ended.
    #[test]
    fn a_refused_send_is_refused_again_and_looked_up_as_refused() {
        let mut sends = Sends::default();
        let busy = Failure::new(ErrorCode::TurnInFlight, "a turn is already running");
        sends.record("m1".into(), &params("hi"), Err(busy));
        let again = sends.earlier("m1", &params("hi")).expect("recorded");
        assert_eq!(again.expect_err("refused").code, ErrorCode::TurnInFlight);
        assert_eq!(
            sends.lookup("m1"),
            json!({"state": "refused", "error": {"code": "turn_in_flight", "message": "a turn is already running"}})
        );
    }

    /// An accepted send answers again with its turn, and the same id with another prompt is refused.
    #[test]
    fn an_accepted_send_answers_again_and_other_parameters_are_refused() {
        let mut sends = Sends::default();
        assert!(sends.earlier("m1", &params("hi")).is_none());
        let started = json!({"turn": 3, "target": 9});
        sends.record("m1".into(), &params("hi"), Ok(started.clone()));
        assert_eq!(
            sends.earlier("m1", &params("hi")).unwrap().unwrap(),
            started
        );
        let other = sends.earlier("m1", &params("bye")).unwrap();
        assert_eq!(other.expect_err("refused").code, ErrorCode::BadRequest);
        assert_eq!(
            sends.lookup("m1"),
            json!({"state": "accepted", "turn": 3, "target": 9})
        );
        assert_eq!(sends.lookup("m2"), json!({"state": "not_received"}));
    }

    /// The digest is of the parameters and not of how a client ordered their keys, so a repeat
    /// built in another order is the same send.
    #[test]
    fn a_repeat_with_its_keys_in_another_order_is_the_same_send() {
        let mut sends = Sends::default();
        let first: Value =
            serde_json::from_str(r#"{"session":"s1","prompt":"hi","messageId":"m1"}"#).unwrap();
        let reordered: Value =
            serde_json::from_str(r#"{"messageId":"m1","prompt":"hi","session":"s1"}"#).unwrap();
        sends.record("m1".into(), &first, Ok(json!({"turn": 1, "target": 1})));
        assert!(sends.earlier("m1", &reordered).unwrap().is_ok());
    }

    /// The last new id that fits is taken. Past it a new id is refused and every recorded one still
    /// answers, so no id that may be retried is forgotten.
    #[test]
    fn a_new_send_past_the_limit_is_refused_and_no_record_is_dropped() {
        let mut sends = Sends::default();
        for at in 0..SENDS_KEPT {
            let id = format!("m{at}");
            assert!(sends.earlier(&id, &params("hi")).is_none(), "{id} refused");
            sends.record(id, &params("hi"), Ok(json!({"turn": at, "target": at})));
        }
        let refused = sends.earlier("new", &params("hi")).expect("refused");
        assert_eq!(refused.expect_err("refused").code, ErrorCode::BadRequest);
        assert_eq!(sends.sent.len(), SENDS_KEPT);
        assert!(sends.earlier("m0", &params("hi")).unwrap().is_ok());
        assert_eq!(sends.lookup("m0")["state"], "accepted");
    }
}
