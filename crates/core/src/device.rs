// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! Whether a paired device may do what it asks.
//!
//! The host decides this once for every line a device sends, from what the person did on the host
//! and nothing the device sent: whether the device is paired, whether the person revoked it, and
//! what grant they gave it (decision 16 in `docs/design/mobile/decisions-and-tradeoffs.md`).
//! [`Standing::authorizes`] is that decision and takes no other input, so no byte of a request
//! can change its answer.
//!
//! # Fails closed
//!
//! A device the host holds no record of is refused, and so is one the person revoked, whatever
//! grant it held before. A `read` grant never allows a `control` operation. Only a device that is
//! paired with a grant that covers the operation is allowed.

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// What the person let a device do when they paired it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify, derive(Structural))]
pub enum Grant {
    /// Watch a session: its view, its questions and its events.
    Read,
    /// Watch, and also send, cancel, answer and close.
    Control,
}

/// The kind of thing a request asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify, derive(Structural))]
pub enum Operation {
    /// Look at a session.
    Read,
    /// Change what a session does.
    Control,
}

/// What the host holds about a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify, derive(Structural))]
pub enum Standing {
    /// No record: never paired, or the host restarted since.
    Unknown,
    /// The person removed it on the host.
    Revoked,
    /// Paired, with the grant the person gave.
    Paired(Grant),
}

/// The answer, with the reason a request is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify, derive(Structural))]
pub enum Verdict {
    Allowed,
    NotPaired,
    Revoked,
    GrantTooNarrow,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl Grant {
    /// Whether a grant covers an operation: `read` covers only reading, `control` both.
    #[cfg_attr(verus_keep_ghost, verus_spec(r =>
        ensures
            r == crate::spec::grant_allows(self, operation),
    ))]
    pub fn allows(self, operation: Operation) -> bool {
        if operation == Operation::Read {
            true
        } else {
            self == Self::Control
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl Standing {
    /// Whether a device in this standing may do `operation`.
    #[cfg_attr(verus_keep_ghost, verus_spec(r =>
        ensures
            r == crate::spec::device_verdict(self, operation),
    ))]
    pub fn authorizes(self, operation: Operation) -> Verdict {
        if let Self::Paired(grant) = self {
            if grant.allows(operation) {
                Verdict::Allowed
            } else {
                Verdict::GrantTooNarrow
            }
        } else if self == Self::Revoked {
            Verdict::Revoked
        } else {
            Verdict::NotPaired
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPERATIONS: [Operation; 2] = [Operation::Read, Operation::Control];

    /// Every state against every operation, with the answers written out, so a change to how
    /// either axis is read shows as a changed row and not as a changed rule.
    #[test]
    fn each_standing_gets_the_verdict_the_table_gives() {
        let table = [
            (Standing::Unknown, Operation::Read, Verdict::NotPaired),
            (Standing::Unknown, Operation::Control, Verdict::NotPaired),
            (Standing::Revoked, Operation::Read, Verdict::Revoked),
            (Standing::Revoked, Operation::Control, Verdict::Revoked),
            (
                Standing::Paired(Grant::Read),
                Operation::Read,
                Verdict::Allowed,
            ),
            (
                Standing::Paired(Grant::Read),
                Operation::Control,
                Verdict::GrantTooNarrow,
            ),
            (
                Standing::Paired(Grant::Control),
                Operation::Read,
                Verdict::Allowed,
            ),
            (
                Standing::Paired(Grant::Control),
                Operation::Control,
                Verdict::Allowed,
            ),
        ];
        for (standing, operation, expected) in table {
            assert_eq!(
                standing.authorizes(operation),
                expected,
                "{standing:?} asking for {operation:?}"
            );
        }
    }

    /// Only a paired device can be allowed, so no record and a revoked record never are.
    #[test]
    fn a_device_the_host_holds_no_grant_for_is_never_allowed() {
        for operation in OPERATIONS {
            for standing in [Standing::Unknown, Standing::Revoked] {
                assert_ne!(standing.authorizes(operation), Verdict::Allowed);
            }
        }
    }

    /// A read grant allows reading and nothing that changes a session.
    #[test]
    fn a_read_grant_never_allows_control() {
        let refused = Standing::Paired(Grant::Read).authorizes(Operation::Control);
        assert_eq!(refused, Verdict::GrantTooNarrow);
        assert_eq!(
            Standing::Paired(Grant::Read).authorizes(Operation::Read),
            Verdict::Allowed
        );
    }
}
