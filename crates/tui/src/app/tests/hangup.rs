// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! INPUT-51 at the end of `run`: a terminal that cannot be handed back is asked whether it hung up
//! while the run still holds it. INPUT-33 at the same place: a run that ends by failing hands the
//! terminal back as one that is left does.

use super::*;
use ratatui::layout::Rect;
use ratatui::{TerminalOptions, Viewport};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// A terminal that keeps what it was written, and can be read while the terminal that owns it is
/// still alive.
#[derive(Clone, Default)]
struct Written(Rc<RefCell<Vec<u8>>>);

impl Write for Written {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

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

/// The way out the tests below hand a terminal back by: the real [`hand_back`] but for the tty, so
/// the modes it writes are in `written` and whether raw mode was left is in `left_raw_mode`.
fn handed_back(
    left_raw_mode: &Cell<bool>,
) -> impl FnOnce(&mut CrosstermBackend<Written>) -> io::Result<()> + '_ {
    move |backend| {
        hand_back(backend, false, || {
            left_raw_mode.set(true);
            Ok(())
        })
    }
}

/// INPUT-33: a session that ends by failing gives the terminal back as one that is left does, and
/// returns the failure it ended with. Rejects an end of the run that returns the session's failure
/// before the terminal is handed back, which leaves the person in raw mode on a screen that is not
/// their own with mouse reporting still on.
#[test]
fn a_session_that_ends_by_failing_has_handed_the_terminal_back_and_says_how_it_failed() {
    let written = Written::default();
    let left_raw_mode = Cell::new(false);
    let asked = Cell::new(0);

    let ended = finish_run(
        &mut terminal(written.clone()),
        Err::<u8, _>(io::Error::other("the interface failed")),
        handed_back(&left_raw_mode),
        || asked.set(asked.get() + 1),
    );

    let sent = String::from_utf8_lossy(&written.0.borrow()).into_owned();
    assert!(
        sent.contains("\x1b[?1049l"),
        "the screen of its own was not left: {sent:?}"
    );
    assert!(
        sent.contains("\x1b[?2004l") && sent.contains("\x1b[?1004l"),
        "paste and focus reporting were not given back: {sent:?}"
    );
    assert!(left_raw_mode.get(), "raw mode was not left");
    assert_eq!(
        ended.expect_err("the failure is returned").to_string(),
        "the interface failed"
    );
    assert_eq!(asked.get(), 0, "a terminal that was handed back asked");
}

/// A session that ends by being left is handed back the same way and returns what it ended with.
#[test]
fn a_session_that_is_left_has_handed_the_terminal_back_and_returns_what_it_ended_with() {
    let written = Written::default();
    let left_raw_mode = Cell::new(false);

    let ended = finish_run(
        &mut terminal(written.clone()),
        Ok(7_u8),
        handed_back(&left_raw_mode),
        || {},
    );

    assert!(String::from_utf8_lossy(&written.0.borrow()).contains("\x1b[?1049l"));
    assert!(left_raw_mode.get(), "raw mode was not left");
    assert_eq!(ended.expect("the session ended well"), 7);
}

/// Where the session failed and the terminal cannot be handed back either, the failure of the
/// hand-back is the one returned, since it is the one that says the terminal is still held, and it
/// is asked about once. The session's own is dropped. Rejects an end of the run that returns the
/// session's failure and loses the one that left the terminal held.
#[test]
fn where_the_session_failed_and_the_terminal_cannot_be_handed_back_that_failure_is_returned() {
    let asked = Cell::new(0);

    let ended = finish_run(
        &mut terminal(Written::default()),
        Err::<u8, _>(io::Error::other("the interface failed")),
        |_| Err(io::Error::other("the terminal would not be given back")),
        || asked.set(asked.get() + 1),
    );

    assert_eq!(
        ended.expect_err("a failure is returned").to_string(),
        "the terminal would not be given back"
    );
    assert_eq!(
        asked.get(),
        1,
        "a terminal that was not handed back did not ask"
    );
}
