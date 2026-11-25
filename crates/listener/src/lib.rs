// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! The host's authenticated network listener.
//!
//! [`identity`] makes the keys and names a key by its hash, [`registry`] holds the devices the
//! person paired, and [`server`] accepts connections over TLS 1.3 and serves each device the
//! registry allows through a [`server::Service`]. This crate depends on nothing that opens a
//! session, so what a device is answered is what the caller of [`server::Listener::start`] gives it.

mod der;
pub mod identity;
pub mod invitation;
pub mod limits;
pub mod pairing;
#[cfg(test)]
mod pairing_tests;
#[cfg(any(test, feature = "testing"))]
pub mod phone;
pub mod registry;
pub mod server;
#[cfg(test)]
mod serving_tests;
mod tls;
