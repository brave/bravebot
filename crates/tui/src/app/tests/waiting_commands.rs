// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! CMD-8: what a command typed while a turn runs does when it waits, and what holds one back.
//! The commands that rewrite the conversation or the workspace wait, Ctrl-Enter on one stops the
//! turn and carries it out, `/exit` waits and stops nothing, a file `/open` numbers is refused
//! mid-turn and never queued, and a waiting `/btw` holds a later `/btw` back.

use super::*;

/// Ctrl-Enter on a waiting command asks for the turn to stop, as it does for a waiting prompt, and
/// the command is carried out once the turn has ended. Every command that waits is tried, since each
/// reaches the queue by the same arm and a press that only queued the line would leave the person
/// with no way to hurry it. The stopped prompt stays sent, marked stopped, because a line waits
/// behind it (INPUT-4).
///
/// Rejects a Ctrl-Enter on a command that queues the line and asks for nothing more, a command
/// dropped by the stop, and one carried out as some other action.
#[test]
fn ctrl_enter_on_a_waiting_command_stops_the_turn_and_carries_the_command_out() {
    for (line, then) in [
        (CLEAR_COMMAND, Action::Clear),
        (COMPACT_COMMAND, Action::Compact(String::new())),
        (UNDO_COMMAND, Action::Undo),
        (REWIND_COMMAND, Action::Rewind(String::new())),
        (
            "/cd crates/tui",
            Action::ChangeDirectory("crates/tui".to_string()),
        ),
        (
            "/add-dir ../other",
            Action::AddDirectory("../other".to_string()),
        ),
        (EXIT_COMMAND, Action::Quit),
    ] {
        let mut session = a_turn_running_on("first");

        let cancel =
            nothing_beside(|beside| typed_during_a_turn(&mut session, line, ctrl_enter(), beside));

        assert!(
            cancel.is_cancelled(),
            "Ctrl-Enter on {line} left the turn running"
        );
        assert_eq!(
            session.commands_waiting().collect::<Vec<_>>(),
            [line],
            "{line} was not left waiting for the turn to end"
        );

        finish_cancelled_turn(&mut session, "first", Some(0));

        assert_eq!(
            queued_next(&mut session),
            Some(then),
            "{line} was not carried out once the turn had stopped"
        );
        assert!(session.queued.is_empty(), "{line} left something waiting");
        assert_eq!(
            session.is_quitting(),
            line == EXIT_COMMAND,
            "{line}: whether the session was left"
        );
        assert_eq!(session.input(), "", "{line}: the stopped prompt moved");
        assert!(
            session
                .transcript
                .last()
                .is_some_and(|entry| entry.speaker == crate::state::Speaker::Stopped),
            "{line}: nothing recorded that the turn stopped"
        );
    }
}

/// `/exit` typed beside a running turn waits for the turn to end and stops nothing: no stop is
/// asked for, the session is not left, the running turn is not handed the line, and the line says
/// why it waits. It leaves once the turn is over.
///
/// Rejects an `/exit` that cancels the turn, one that leaves at once, and one the turn is handed.
#[test]
fn exit_typed_mid_turn_waits_for_the_turn_and_does_not_stop_it() {
    let mut session = a_turn_running_on("first");

    let cancel = nothing_beside(|beside| {
        typed_during_a_turn(&mut session, EXIT_COMMAND, key(KeyCode::Enter), beside)
    });

    assert!(!cancel.is_cancelled(), "/exit stopped the turn");
    assert!(!session.is_quitting(), "/exit left while the turn ran");
    assert!(session.a_turn_is_running(), "the turn did not go on");
    assert_eq!(
        session.commands_waiting().collect::<Vec<_>>(),
        [EXIT_COMMAND]
    );
    assert_eq!(
        session.interjections().take(),
        None,
        "the running turn was handed /exit"
    );
    let why = t!(command_waits_for_the_turn, command = EXIT_COMMAND);
    assert_eq!(said_under_the_turn(&session), vec![why.as_str()]);

    session.complete("answered", Vec::new(), 0);
    assert!(
        !session.is_quitting(),
        "/exit left before it was carried out"
    );
    assert_eq!(queued_next(&mut session), Some(Action::Quit));
    assert!(session.is_quitting());
}

/// A file the last `/open` listed is neither shown nor opened in the editor by a number typed while
/// a turn runs, with or without `edit`: each is refused with its own sentence and nothing waits,
/// since the editor takes the terminal the turn is drawing on. An address in the same list still
/// opens, so the refusal is about the file and not the command.
///
/// Rejects a preview or an editor started mid-turn, and a line queued to be opened later.
#[test]
fn a_file_the_open_command_numbers_is_refused_while_a_turn_runs_and_not_queued() {
    let mut session = a_turn_running_on("first");
    session.transcript.push(crate::state::Entry::assistant(
        "read https://example.test/a and src/main.rs:3",
        Vec::new(),
    ));
    type_while_working(&mut session, "/open");
    assert_eq!(session.listed_links().len(), 2, "the list was not made");

    assert_eq!(
        type_while_working(&mut session, "/open 1"),
        Action::OpenUrl("https://example.test/a".to_string()),
        "the address in the list did not open"
    );
    for (line, said) in [
        ("/open 2", t!(open_not_while_working).to_string()),
        ("/open edit 2", t!(open_edit_not_while_working).to_string()),
    ] {
        let asked = type_while_working(&mut session, line);

        assert!(
            !matches!(asked, Action::Preview(_) | Action::EditFile(_)),
            "{line} was carried out mid-turn: {asked:?}"
        );
        assert_eq!(
            said_under_the_turn(&session)
                .last()
                .map(ToString::to_string),
            Some(said),
            "{line}"
        );
        assert!(session.queued.is_empty(), "{line} waited");
        assert_eq!(session.input(), "", "{line} was left in the box");
    }
}

/// A `/btw` waiting in the queue holds a later `/btw` back: the later one waits behind it, says so,
/// and is asked after it, so the two questions are asked in the order they were typed. With a `/btw`
/// ahead of it the later one is not asked at once, though a `/btw` is carried out as typed behind
/// any other waiting line.
///
/// Rejects a `/btw` that jumps ahead of a waiting `/btw`.
#[test]
fn a_btw_typed_behind_a_waiting_btw_waits_behind_it() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut session = a_turn_that_has_sent_a_request();
    wait_as(&mut session, "/btw one");
    assert_eq!(waiting_prompts(&session), vec!["/btw one"]);

    typed_beside_a_turn(&mut session, "/btw two");

    let asked_at_once = session.aside_in_flight();
    if asked_at_once {
        end_aside_beside_a_turn(&mut session);
    }
    assert!(
        !asked_at_once,
        "/btw two was asked ahead of the /btw waiting before it"
    );
    assert_eq!(waiting_prompts(&session), vec!["/btw one", "/btw two"]);
    let why = t!(
        command_waits_behind,
        command = BTW_COMMAND,
        line = "/btw one"
    );
    assert_eq!(said_under_the_turn(&session), vec![why.as_str()]);

    session.complete("answered", Vec::new(), 0);
    let carried_out: Vec<String> = std::iter::from_fn(|| session.take_queued_command())
        .map(|commanded| commanded.line)
        .collect();
    assert_eq!(carried_out, ["/btw one", "/btw two"]);
}
