// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! The message id every `turn.send` carries, for tests that send one.

use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};

static SENT: AtomicU64 = AtomicU64::new(0);

/// `params` with a `messageId` no other send in this test process has used.
///
/// Each call is a new send, as each press of Send is, so two sends of the same prompt are two
/// turns and not one repeated.
pub fn with_fresh(mut params: Value) -> Value {
    let id = SENT.fetch_add(1, Ordering::Relaxed) + 1;
    if let Some(fields) = params.as_object_mut() {
        fields.insert("messageId".into(), Value::String(format!("test-{id}")));
    }
    params
}
