// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! INPUT-33 around each handover of the terminal: the editor a prompt is written in, the viewer a
//! transcript is read in, the editor opened on a file, the editor a write's body is changed in, and
//! the shell a stop leaves the terminal to. The real entry points need a tty, so the order is
//! watched through [`handed_over`], which they all go through, and a scan of `terminal.rs` shows
//! that they do.

use super::*;
use std::cell::RefCell;

/// A handover that logs each of its three steps, and whose steps succeed or fail as it is told.
fn handover(
    log: &RefCell<Vec<&'static str>>,
    gives_back: io::Result<()>,
    run_returns: Result<&'static str, &'static str>,
    takes_again: io::Result<()>,
) -> io::Result<Result<&'static str, &'static str>> {
    handed_over(
        &mut (),
        |_| {
            log.borrow_mut().push("given back");
            gives_back
        },
        || {
            log.borrow_mut().push("ran");
            run_returns
        },
        |_| {
            log.borrow_mut().push("taken again");
            takes_again
        },
    )
}

/// The program has the terminal only for as long as it runs: the modes are given back before it and
/// taken again after it. Rejects a handover that runs the program first, which leaves it drawing
/// under a screen of its own with mouse reporting on, and one that never takes the terminal again,
/// which leaves a session nobody draws on.
#[test]
fn the_terminal_is_given_back_before_the_program_runs_and_taken_again_after_it() {
    let log = RefCell::new(Vec::new());

    let returned = handover(&log, Ok(()), Ok("saved"), Ok(())).expect("the sequence");

    assert_eq!(*log.borrow(), ["given back", "ran", "taken again"]);
    assert_eq!(returned, Ok("saved"), "what the program returned was lost");
}

/// An editor that would not start, or that failed, is said by the caller. The terminal is still
/// taken again, since a session that returns from a failed editor with the terminal still given
/// away is a screen nobody draws on, and the failure is carried back for the caller to say.
/// Rejects a handover that returns the program's failure before taking the terminal again.
#[test]
fn the_terminal_is_taken_again_when_the_program_failed() {
    let log = RefCell::new(Vec::new());

    let returned = handover(&log, Ok(()), Err("no editor"), Ok(())).expect("the sequence");

    assert_eq!(*log.borrow(), ["given back", "ran", "taken again"]);
    assert_eq!(
        returned,
        Err("no editor"),
        "the failure was not carried back"
    );
}

/// A terminal that could not be given back is not handed over: the program would be given raw mode
/// and a screen it did not ask for, and nothing would take them back. Nothing runs after the
/// failure, the terminal is not taken again, and the failure is the one returned. Rejects a
/// handover that goes on after a failed give-back.
#[test]
fn nothing_runs_after_a_terminal_that_could_not_be_given_back() {
    let log = RefCell::new(Vec::new());

    let returned = handover(
        &log,
        Err(io::Error::other("the terminal would not be given back")),
        Ok("saved"),
        Ok(()),
    );

    assert_eq!(*log.borrow(), ["given back"], "went on after a failure");
    assert_eq!(
        returned.expect_err("the failure is returned").to_string(),
        "the terminal would not be given back"
    );
}

/// A terminal that could not be taken again is a failure of the session, which ends with it, and
/// not a success that carries the program's result.
#[test]
fn a_terminal_that_could_not_be_taken_again_is_returned_as_a_failure() {
    let log = RefCell::new(Vec::new());

    let returned = handover(
        &log,
        Ok(()),
        Ok("saved"),
        Err(io::Error::other("the terminal would not be taken again")),
    );

    assert_eq!(*log.borrow(), ["given back", "ran", "taken again"]);
    assert_eq!(
        returned.expect_err("the failure is returned").to_string(),
        "the terminal would not be taken again"
    );
}

/// Each handover takes the terminal again with the mouse choice the session holds, and the one
/// function that takes it again passes that choice on. A handover that takes it again with
/// `true` or `false` written in, or with some other value, turns capture on for a person who turned
/// it off with `tui.mouse`, or leaves the wheel scrolling the window behind the session after an
/// editor. A scan of the source, since the entry points need a tty to run.
///
/// Rejects `take_again_from_a_program` handing `take_over_terminal` a mouse value of its own, and
/// a handover that reads the choice from anywhere but the session or its caller.
#[test]
fn every_handover_takes_the_terminal_again_with_the_mouse_choice_the_session_holds() {
    let terminal = include_str!("../terminal.rs");

    let taken_again = body_of(terminal, "take_again_from_a_program");
    assert!(
        taken_again.contains("take_over_terminal(terminal.backend_mut(), mouse)"),
        "the terminal is not taken again with the mouse choice it was given: {taken_again}"
    );
    assert!(
        !taken_again.contains("true") && !taken_again.contains("false"),
        "a mouse choice is written into the function: {taken_again}"
    );

    for entry in ["edit_prompt", "show_transcript", "open_file_in_editor"] {
        assert!(
            body_of(terminal, entry)
                .contains("take_again_from_a_program(terminal, session.mouse_captured())"),
            "`{entry}` does not take the terminal again with the session's mouse choice"
        );
    }
    let edit_body = body_of(terminal, "edit_body");
    assert!(
        edit_body.contains("take_again_from_a_program(terminal, mouse)"),
        "`edit_body` does not take the terminal again with the choice its caller read: {edit_body}"
    );
    let suspend = body_of(terminal, "suspend_session");
    assert!(
        suspend.contains("let mouse = session.mouse_captured();")
            && suspend.contains("take_over_terminal(terminal.backend_mut(), mouse)"),
        "`suspend_session` does not take the terminal again with the session's mouse choice: {suspend}"
    );
}

/// The text of the function named `name` in `source`, from its signature to the line that closes it.
fn body_of<'a>(source: &'a str, name: &str) -> &'a str {
    let from = source
        .find(&format!("fn {name}("))
        .unwrap_or_else(|| panic!("`fn {name}(` is not in terminal.rs"));
    let rest = &source[from..];
    let to = rest
        .find("\n}\n")
        .unwrap_or_else(|| panic!("`fn {name}` is not closed"));
    &rest[..to]
}

/// Each way the terminal is given to another program goes through `handed_over`, and what it gives
/// back and takes again are the interface's own definitions, so the modes taken again are the ones
/// that were given back. A scan of the source, since the entry points need a tty to run: it fails
/// when a handover goes back to writing its own order, or to a way of giving back or taking again
/// that leaves a mode out.
#[test]
fn every_handover_goes_through_the_one_that_gives_back_and_takes_again() {
    let terminal = include_str!("../terminal.rs");

    for entry in [
        "edit_prompt",
        "show_transcript",
        "open_file_in_editor",
        "edit_body",
        "suspend_session",
    ] {
        let body = body_of(terminal, entry);
        assert!(
            body.contains("handed_over("),
            "`{entry}` does not go through `handed_over`"
        );
        assert!(
            body.contains("give_back_for_a_program") || body.contains("hand_back_terminal("),
            "`{entry}` does not give the terminal back as the interface does"
        );
        assert!(
            body.contains("take_again_from_a_program") || body.contains("take_over_terminal("),
            "`{entry}` does not take the terminal again as the interface does"
        );
    }

    let given_back = body_of(terminal, "give_back_for_a_program");
    assert!(
        given_back.contains("hand_back_terminal(") && given_back.contains("show_cursor()"),
        "a program is not given the terminal as the interface gives it back: {given_back}"
    );

    let taken_again = body_of(terminal, "take_again_from_a_program");
    assert!(
        taken_again.contains("take_over_terminal(") && taken_again.contains("clear()"),
        "the terminal is not taken again as the interface takes it: {taken_again}"
    );
}
