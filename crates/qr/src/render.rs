// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! A code as lines of text for a terminal.
//!
//! Two rows of modules share one line of text, in the half blocks, so a code is about as tall as it
//! is wide. A scanner reads dark modules on a light ground, and a terminal's own colours may be the
//! other way round, so each line sets black on white itself and resets afterwards, and the quiet zone
//! of four modules around the code is white too.

use crate::Code;

/// Modules of light on each side, as the standard asks.
const QUIET: usize = 4;

const BLACK_ON_WHITE: &str = "\u{1b}[30;47m";
const RESET: &str = "\u{1b}[0m";

/// The code as lines, with no newline after the last.
pub fn render(code: &Code) -> String {
    let span = code.size() + 2 * QUIET;
    let dark = |x: usize, y: usize| {
        x.checked_sub(QUIET)
            .zip(y.checked_sub(QUIET))
            .is_some_and(|(x, y)| code.is_dark(x, y))
    };
    let mut lines = Vec::new();
    for top in (0..span).step_by(2) {
        let mut line = String::from(BLACK_ON_WHITE);
        for x in 0..span {
            line.push(match (dark(x, top), dark(x, top + 1)) {
                (true, true) => '\u{2588}',
                (true, false) => '\u{2580}',
                (false, true) => '\u{2584}',
                (false, false) => ' ',
            });
        }
        line.push_str(RESET);
        lines.push(line);
    }
    lines.join("\n")
}
