// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! A QR code for a short piece of text, and the lines that draw it in a terminal.
//!
//! The host shows one to a person so a phone can read the address, the key hash and the one-time
//! secret of an invitation in a single look. It is the QR Code Model 2 of ISO/IEC 18004 in byte
//! mode at error correction level M, in versions 1 to 10, which holds up to 213 bytes. A longer text
//! is refused, since the host's own text is far shorter and a larger code is harder to scan from a
//! terminal.
//!
//! This crate does no I/O and takes no input but the bytes it is given.

mod galois;
mod layout;
mod render;

pub use render::render;

/// The most bytes a code holds: version 10 at level M.
pub const MAX_BYTES: usize = 213;

/// The text was longer than [`MAX_BYTES`].
#[derive(Debug, PartialEq, Eq)]
pub struct TooLong;

impl std::fmt::Display for TooLong {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "a QR code here holds at most {MAX_BYTES} bytes")
    }
}

impl std::error::Error for TooLong {}

/// A QR code: a square of dark and light modules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Code {
    version: u8,
    size: usize,
    modules: Vec<bool>,
}

impl Code {
    /// The smallest code in versions 1 to 10 that holds `data`.
    pub fn encode(data: &[u8]) -> Result<Self, TooLong> {
        let version = layout::smallest_version(data.len()).ok_or(TooLong)?;
        let codewords = layout::codewords(data, version);
        let (size, modules) = layout::draw(version, &codewords);
        Ok(Self {
            version,
            size,
            modules,
        })
    }

    /// The version, 1 to 10.
    pub fn version(&self) -> u8 {
        self.version
    }

    /// Modules along one side.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Whether the module at column `x`, row `y` is dark. A place outside the code is light, as the
    /// quiet zone around it is.
    pub fn is_dark(&self, x: usize, y: usize) -> bool {
        x < self.size && y < self.size && self.modules[y * self.size + x]
    }
}

#[cfg(test)]
mod tests;
