// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! INPUT-51 at the end of `run`: a terminal that cannot be handed back is asked whether it hung up
//! while the run still holds it.

use super::*;
use ratatui::layout::Rect;
use ratatui::{TerminalOptions, Viewport};
use std::cell::Cell;

/// A terminal whose other end has closed: every write fails as a closed pty's does.
struct Closed;

impl Write for Closed {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::from_raw_os_error(5))
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::from_raw_os_error(5))
    }
}

fn terminal<W: Write>(out: W) -> Terminal<CrosstermBackend<W>> {
    Terminal::with_options(
        CrosstermBackend::new(out),
        TerminalOptions {
            viewport: Viewport::Fixed(Rect::new(0, 0, 20, 5)),
        },
    )
    .expect("a terminal")
}

/// The check is made when the terminal is not handed back, whether the hand-back or the cursor
/// after it fails, and not when the run leaves normally. Rejects an end of the run that returns its
/// failure without asking, which drops the terminal and lets its drop write to the closed terminal
/// and panic before the watch ends the session with 129.
#[test]
fn a_run_whose_terminal_cannot_be_handed_back_asks_whether_it_hung_up() {
    let asked = Cell::new(0);

    let failed = end_the_run(
        &mut terminal(Vec::new()),
        |_| Err(io::Error::from_raw_os_error(5)),
        || asked.set(asked.get() + 1),
    );
    assert!(failed.is_err());
    assert_eq!(asked.get(), 1, "a failed hand-back did not ask");

    let failed = end_the_run(
        &mut terminal(Closed),
        |_| Ok(()),
        || asked.set(asked.get() + 1),
    );
    assert!(failed.is_err());
    assert_eq!(
        asked.get(),
        2,
        "a cursor that could not be shown did not ask"
    );

    end_the_run(
        &mut terminal(Vec::new()),
        |_| Ok(()),
        || asked.set(asked.get() + 1),
    )
    .expect("a terminal that takes writes is handed back");
    assert_eq!(asked.get(), 2, "a run that left normally asked");
}
