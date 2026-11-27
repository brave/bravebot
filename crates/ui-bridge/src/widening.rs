// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! The moments a session can come to reach more than it did, which the bridge tells whoever holds a
//! connection to a ceiling (decision 18 in `docs/design/mobile/decisions-and-tradeoffs.md`).
//!
//! The bridge says what happened and decides nothing about a connection. The host compares each
//! ceiling it holds and answers with the kinds of question the connections it still serves can
//! answer, where that changed.

use crate::turn::Kind;
use bravebot_core::scope::Scope;
use std::sync::Arc;

/// What may have widened a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Widening {
    /// A turn is starting, before anything of it is sent: the scope the session holds now, or
    /// `None` where it could not be read. A settings edit between turns is found here.
    TurnStarting(Option<Scope>),
    /// The trust question was answered: the scope the session holds now, or `None` where it could
    /// not be read.
    TrustAnswered(Option<Scope>),
    /// A person approved, for this turn, a grant that no ceiling holds.
    Granted(Grant),
}

/// A grant for one turn that a ceiling has no field for, so no ceiling covers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grant {
    /// A fetch of a web host.
    Fetch,
    /// A line run with no sandbox (SANDBOX-29).
    UnconfinedRun,
}

/// Told of each [`Widening`] of the session named by the first argument, before the session sends
/// anything that follows it. Returns the kinds of question the session can still be asked, where a
/// connection was stopped from answering, and `None` where who can answer is unchanged.
pub type ScopeWatch = Arc<dyn Fn(&str, &Widening) -> Option<Vec<Kind>> + Send + Sync>;
