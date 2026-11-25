// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! How much of the host a connection may use before it has said who it is.
//!
//! The limits are global and not per address: loopback is one address however many programs use
//! it, and each device on a tailnet has an address of its own, so a per-address count would stop
//! nothing it was meant to. A program that fills them can keep a device from connecting, which the
//! person finds when pairing fails. It cannot be taken for a device.

use std::time::Duration;

/// What a listener holds a connection to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Connections open at once, whoever they are. A connection past this is closed as it arrives.
    pub connections: usize,
    /// Connections that have not finished the handshake. One past this is closed as it arrives.
    pub handshakes: usize,
    /// How long a handshake may take from the moment the connection is accepted, and how long the
    /// first line of a device that is not paired may take after it, taken together.
    pub handshake: Duration,
    /// The longest line read from a connection, newline excluded. A line that reaches it without
    /// ending ends the connection.
    pub line: usize,
    /// The longest first line a device that is not paired may send, newline excluded. It is read
    /// within the time of the handshake.
    pub first_line: usize,
    /// The longest a write to a device may wait before the connection is given up.
    pub write: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            connections: 16,
            handshakes: 4,
            handshake: Duration::from_secs(10),
            line: 8 * 1024 * 1024,
            first_line: 4096,
            write: Duration::from_secs(5),
        }
    }
}
