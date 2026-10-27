//! MOUSE-1, MOUSE-34 and MOUSE-38: what a selection of the transcript holds as the pointer goes
//! beyond the text, and what Escape puts away.

use super::*;

/// A session whose transcript is one reply of two lines, so that most of the pane under it is blank,
/// drawn once at 120 by 24.
fn a_short_transcript() -> (Session, ratatui::buffer::Buffer) {
    let mut session = Session::new("none");
    session.transcript.push(crate::state::Entry::assistant(
        "short reply\nsecond line",
        Vec::new(),
    ));
    let buffer = drawn(&mut session);
    (session, buffer)
}

/// MOUSE-1 and MOUSE-34. Rejects: a drag that goes below the last row of a transcript that does not
/// fill its pane losing its selection, so that the copy at the release takes nothing. The reply is
/// followed by the blank row that holds entries apart, which holds no text, and the rows under it are
/// blank cells of the pane. The drag stops on one of them, the next frame is drawn before the
/// release, as the loops draw it, and the copy is read against it: it is what a drag to the end of
/// the last row that holds text holds, whichever blank row the pointer is on.
#[test]
fn a_drag_below_the_last_row_of_a_short_transcript_copies_down_to_the_end_of_it() {
    let copied = |below: u16| {
        let (mut session, buffer) = a_short_transcript();
        let (row, column) = find_cell(&buffer, "short reply");
        dragged_and_released_at_rest(&mut session, (120, 24), (row, column), (row + below, 50))
    };
    let to_the_end = vec!["short reply\n  second line".to_string()];
    for below in [1u16, 2, 3, 8, 14] {
        assert_eq!(copied(below), to_the_end, "dragged {below} rows below");
    }
}

/// MOUSE-1. Rejects: the overshoot being kept for good. The pointer goes below the text, a frame is
/// drawn, and it comes back to the first row: the selection is the words between the press and the
/// pointer as it is now.
#[test]
fn a_selection_that_went_below_the_text_follows_the_pointer_back_into_it() {
    let (mut session, buffer) = a_short_transcript();
    let (row, column) = find_cell(&buffer, "short reply");
    handle_mouse(
        &mut session,
        drag(MouseEventKind::Down(MouseButton::Left), row, column),
    );
    handle_mouse(
        &mut session,
        drag(MouseEventKind::Drag(MouseButton::Left), row + 9, 50),
    );
    drawn(&mut session);
    assert!(
        session.selection.is_some(),
        "the frame after the overshoot put the selection away"
    );
    handle_mouse(
        &mut session,
        drag(MouseEventKind::Drag(MouseButton::Left), row, column + 8),
    );
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 24)).expect("terminal");
    let mut handed = Vec::new();
    let up = drag(MouseEventKind::Up(MouseButton::Left), row, column + 8);
    assert_eq!(
        idle_action(&mut session, TermEvent::Mouse(up)),
        Action::Copy
    );
    copy_selection(&mut terminal, &mut session, |text| {
        handed.push(text.to_string());
        true
    })
    .expect("the copy was made");
    assert_eq!(handed, vec!["short re".to_string()]);
}

/// MOUSE-1. Rejects: a press on the blank rows under a short transcript, dragged up into the text,
/// being dropped at the next frame. It begins at the end of the last row that holds text, as a press
/// below the text does in a terminal, and the copy is the words from the pointer to there.
#[test]
fn a_drag_from_the_blank_rows_under_the_text_up_into_it_copies_from_the_end_of_the_text() {
    let (mut session, buffer) = a_short_transcript();
    let (row, column) = find_cell(&buffer, "short reply");
    let handed =
        dragged_and_released_at_rest(&mut session, (120, 24), (row + 6, 30), (row, column + 2));
    assert_eq!(handed, vec!["ort reply\n  second line".to_string()]);
}

/// MOUSE-1 and MOUSE-34, in the scroller, whose whole screen but the footer is the transcript.
/// Rejects: the overshoot dropping the selection in a view that is not the plain one.
#[test]
fn a_drag_below_the_last_row_in_the_scroller_copies_down_to_the_end_of_it() {
    let (mut session, _) = a_short_transcript();
    session.open_scroller();
    let buffer = drawn(&mut session);
    let (row, column) = find_cell(&buffer, "short reply");
    let handed =
        dragged_and_released_at_rest(&mut session, (120, 24), (row, column), (row + 9, 50));
    assert_eq!(handed, vec!["short reply\n  second line".to_string()]);
}

/// MOUSE-34. Rejects: a selection at the tail of a transcript of more rows than a layout counts being
/// put away at every frame, so that a drag there copies nothing. The rows are counted in sixteen
/// bits, so the first row of the screen and the number of rows the transcript came to are both
/// stopped at the largest; the selection counts the rows of the screen there, as a pane that does
/// not scroll does, and the copy is what was on the screen.
#[test]
fn a_transcript_of_more_rows_than_a_layout_counts_is_selected_by_the_rows_of_the_screen() {
    let mut session = Session::new("none");
    for n in 0..34_000 {
        session.transcript.push(crate::state::Entry::assistant(
            format!("reply {n}"),
            Vec::new(),
        ));
    }
    let buffer = drawn(&mut session);
    assert_eq!(
        session.laid.rows,
        u16::MAX,
        "the transcript is not that long"
    );
    let (row, column) = find_cell(&buffer, "reply 33999");
    let handed = dragged_and_released_at_rest(&mut session, (120, 24), (row, column), (row, 119));
    assert_eq!(handed, vec!["reply 33999".to_string()]);
}

/// MOUSE-38. Rejects: Escape doing nothing while a shell command runs and a selection is held, which
/// is every press until a click takes the selection away, since a copy leaves it marked. The first
/// press puts the selection away and the command goes on, the second stops it, and Ctrl-C, which a
/// selection never takes, stops it at once.
#[test]
fn escape_puts_a_held_selection_away_before_it_stops_a_command_and_the_next_stops_it() {
    let mut session = with_a_selection_held();
    session.begin_command();
    let cancel = Cancel::new();

    command_key(&mut session, key(KeyCode::Esc), &cancel);
    assert!(
        session.selection.is_none(),
        "the selection was not put away"
    );
    assert!(
        !cancel.is_cancelled(),
        "the press that put it away stopped the command"
    );
    command_key(&mut session, key(KeyCode::Esc), &cancel);
    assert!(
        cancel.is_cancelled(),
        "the next press did not stop the command"
    );

    let mut session = with_a_selection_held();
    session.begin_command();
    let cancel = Cancel::new();
    command_key(&mut session, ctrl('c'), &cancel);
    assert!(cancel.is_cancelled(), "the selection took Ctrl-C");
    assert!(session.holds_a_selection());
}

/// MOUSE-38, in the scroller left open under a command, whose keys are answered before the stop
/// keys. Rejects: the scroller closing on the press that was to put the selection away, which is the
/// order every other loop has: the selection, then the mode, then what is running.
#[test]
fn escape_puts_a_held_selection_away_before_the_scroller_under_a_command_and_the_command() {
    let (mut session, _) = a_short_transcript();
    session.open_scroller();
    let buffer = drawn(&mut session);
    let (row, column) = find_cell(&buffer, "short reply");
    dragged_and_released(&mut session, (120, 24), (row, column), (row, column + 5));
    assert!(
        session.holds_a_selection(),
        "the fixture holds no selection"
    );
    session.begin_command();
    let cancel = Cancel::new();

    command_key(&mut session, key(KeyCode::Esc), &cancel);
    assert!(session.selection.is_none() && session.scrolling());
    command_key(&mut session, key(KeyCode::Esc), &cancel);
    assert!(
        !session.scrolling(),
        "the second press did not close the scroller"
    );
    assert!(!cancel.is_cancelled());
    command_key(&mut session, key(KeyCode::Esc), &cancel);
    assert!(
        cancel.is_cancelled(),
        "the third press did not stop the command"
    );
}

/// MOUSE-38, in the loops that read a summary's or an aside's keys and a goal check's: each reads the
/// stop keys against `stops_the_turn`, so the first Escape puts a held selection away and the second
/// stops what is out. Ctrl-C is not the selection's.
#[test]
fn escape_puts_a_held_selection_away_before_it_stops_a_request_or_a_goal_check() {
    for loop_key in [one_request_key, goal_check_key] {
        let mut session = with_a_selection_held();
        session.status = Status::Working;
        let cancel = Cancel::new();

        loop_key(&mut session, key(KeyCode::Esc), &cancel);
        assert!(
            session.selection.is_none(),
            "the selection was not put away"
        );
        assert!(
            !cancel.is_cancelled(),
            "the press that put it away stopped the request"
        );
        loop_key(&mut session, key(KeyCode::Esc), &cancel);
        assert!(cancel.is_cancelled(), "the next press did not stop it");

        let mut session = with_a_selection_held();
        session.status = Status::Working;
        let cancel = Cancel::new();
        loop_key(&mut session, ctrl('c'), &cancel);
        assert!(cancel.is_cancelled(), "the selection took Ctrl-C");
    }
}

/// The functions of `app.rs` that are not tests, each as its name and its text, where a function is
/// what begins a line with `fn`, `pub fn`, `pub(super) fn` or `pub(crate) fn`.
fn functions_of_the_code() -> Vec<(&'static str, &'static str)> {
    let source = include_str!("../../app.rs");
    let code = source
        .split("\n#[cfg(test)]\nmod tests {")
        .next()
        .expect("the code comes before the tests");
    let starts: Vec<usize> = std::iter::once(0)
        .chain(code.match_indices('\n').map(|(at, _)| at + 1))
        .filter(|&at| {
            let line = code.get(at..).unwrap_or_default();
            ["fn ", "pub fn ", "pub(super) fn ", "pub(crate) fn "]
                .iter()
                .any(|opening| line.starts_with(opening))
        })
        .collect();
    starts
        .iter()
        .enumerate()
        .filter_map(|(index, &from)| {
            let to = starts.get(index + 1).copied().unwrap_or(code.len());
            let text = code.get(from..to)?;
            let name = text.split('(').next()?.rsplit(' ').next()?;
            Some((name, text))
        })
        .collect()
}

/// The loops that answer a mouse report, among `functions`: the one at rest through `idle_action`,
/// and those while work runs through `answer_mouse`.
fn the_loops_that_answer_the_mouse(
    functions: &[(&'static str, &'static str)],
) -> Vec<(&'static str, &'static str)> {
    let loops: Vec<_> = functions
        .iter()
        .filter(|(name, text)| {
            *name != "answer_mouse"
                && *name != "idle_action"
                && (text.contains("answer_mouse(") || text.contains("idle_action("))
        })
        .copied()
        .collect();
    let names: Vec<&str> = loops.iter().map(|(name, _)| *name).collect();
    for expected in ["pump_terminal", "run_command"] {
        assert!(
            names.contains(&expected),
            "{expected} is not read as a loop: {names:?}"
        );
    }
    assert!(
        loops.len() >= 3,
        "the loop at rest is not read as one: {names:?}"
    );
    loops
}

/// MOUSE-37. Rejects: a loop that reads the mouse and leaves a drag held below the transcript where it
/// was, so that the view scrolls at rest and not while a turn or a command runs, or the other way
/// round. A pointer held still sends no report, so each loop steps the view on its own clock, in every
/// pass whether or not an event came. A loop added to answer a report is held to the same step. They
/// read the terminal, which a test has no seam for in the loops themselves, so this reads what they
/// are made of.
#[test]
fn every_loop_that_answers_the_mouse_steps_a_drag_held_outside_the_transcript() {
    for (name, text) in the_loops_that_answer_the_mouse(&functions_of_the_code()) {
        assert!(
            text.contains("scroll_toward_the_pointer("),
            "{name} answers the mouse and never steps a held drag"
        );
    }
}

/// MOUSE-1. Rejects: a loop that handles the release of a drag and does not copy what it swept, at
/// rest, while a turn runs or while a command runs. A release is answered by `answer_mouse`, which
/// carries a copy out, or by the loop at rest, which carries it out itself.
#[test]
fn every_loop_that_answers_the_mouse_carries_out_the_copy_a_release_asks_for() {
    for (name, text) in the_loops_that_answer_the_mouse(&functions_of_the_code()) {
        assert!(
            text.contains("answer_mouse(") || text.contains("Action::Copy => copy_selection("),
            "{name} answers the mouse and never copies what a release swept"
        );
    }
}

/// MOUSE-34. Rejects: a press or a drag handled before the frame that shows a wheel turn being placed
/// by the rows the view was on before it. The wheel and the drag arrive together, since a frame is
/// waited out while reports stream in, and the pointer is over the text the wheel has moved under it,
/// so that is what it selects. The button goes down on a row, the wheel turns, and the drag ends on
/// the same cell with no frame drawn between: it has gone back over the rows the wheel moved the view
/// by.
#[test]
fn a_drag_straight_after_a_wheel_turn_selects_the_text_under_the_pointer_now() {
    let mut session = a_long_reply(60);
    let buffer = drawn_at(&mut session, 120, 24);
    let (row, column) = find_cell(&buffer, "row 55");
    let moved = session.wheel_rows();
    assert!(moved >= 2, "the wheel does not move the view");

    handle_mouse(
        &mut session,
        drag(MouseEventKind::Down(MouseButton::Left), row, column),
    );
    handle_mouse(&mut session, drag(MouseEventKind::ScrollUp, row, column));
    handle_mouse(
        &mut session,
        drag(MouseEventKind::Drag(MouseButton::Left), row, column),
    );
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 24)).expect("terminal");
    let mut handed = Vec::new();
    answer_mouse(
        &mut terminal,
        &mut session,
        drag(MouseEventKind::Up(MouseButton::Left), row, column),
        |text| {
            handed.push(text.to_string());
            true
        },
    )
    .expect("answered");

    let first = 55 - usize::from(moved);
    let expected: Vec<String> = (first..55)
        .map(|n| format!("{}row {n} of the reply", if n == first { "" } else { "  " }))
        .collect();
    assert_eq!(handed, vec![format!("{}\n", expected.join("\n"))]);
}

/// MOUSE-34. Rejects: the same for a press, which is placed on the row the wheel has moved under the
/// pointer and not on the one that was there.
#[test]
fn a_press_straight_after_a_wheel_turn_begins_on_the_text_under_the_pointer_now() {
    let mut session = a_long_reply(60);
    let buffer = drawn_at(&mut session, 120, 24);
    let (row, column) = find_cell(&buffer, "row 55");
    let moved = usize::from(session.wheel_rows());

    handle_mouse(&mut session, drag(MouseEventKind::ScrollUp, row, column));
    let handed = {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 24)).expect("terminal");
        let mut handed = Vec::new();
        for event in [
            drag(MouseEventKind::Down(MouseButton::Left), row, column),
            drag(MouseEventKind::Drag(MouseButton::Left), row, column + 6),
            drag(MouseEventKind::Up(MouseButton::Left), row, column + 6),
        ] {
            answer_mouse(&mut terminal, &mut session, event, |text| {
                handed.push(text.to_string());
                true
            })
            .expect("answered");
        }
        handed
    };
    assert_eq!(handed, vec![format!("row {}", 55 - moved)]);
}

/// MOUSE-1. Rejects: the last character of a row of the info panel being left out of a drag that
/// ends at the last column of the screen, which is where the panel ends, so that the pointer can go
/// no further. A directory too long for the panel is cut with an ellipsis at the left, and the copy of
/// the row is the row as it is drawn, to its last character.
#[test]
fn a_drag_to_the_last_column_of_the_screen_copies_the_last_character_of_a_panel_row() {
    let (mut session, _) = two_panes();
    session.identify(
        "PANELMARK",
        "/work/dir/with/a/path/far/too/long/for/the/panel/to/hold/all/of/it/END".to_string(),
        None,
    );
    let buffer = drawn_at(&mut session, 120, 24);
    let (row, column) = find_cell(&buffer, "\u{2026}");
    let drawn: String = (column..120)
        .map(|at| buffer[(at, row)].symbol().to_string())
        .collect();
    assert!(drawn.trim_end().ends_with("END"), "{drawn:?}");

    let handed = dragged_and_released_at_rest(&mut session, (120, 24), (row, column), (row, 119));
    assert_eq!(handed, vec![drawn.trim_end().to_string()]);
}

/// MOUSE-38, over the two modes the scroller test does not reach: the files state, which has its own
/// Escape, and a delegate's view, which has two. Rejects: the selection being looked for after the
/// mode, so that the first press takes the mode and leaves the selection marked under it. Each mode is
/// opened over a selection held, with no frame between, as a key opens it.
#[test]
fn escape_puts_a_held_selection_away_before_the_files_state_and_a_delegates_view_take_it() {
    // The files state.
    let mut session = Session::new("none");
    let diff = bravebot_agent::diff::Diff::compute("old\n", "new\n");
    session.transcript.push(crate::state::Entry::tool(
        bravebot_agent::report::Activity::running("Update", "a.rs")
            .of_tool("edit_file")
            .with_changes(diff.condensed(3))
            .with_written(Some(bravebot_agent::report::Written {
                landed: "a.rs".into(),
                added: diff.added(),
                removed: diff.removed(),
                numbers: diff.condensed_numbers(3),
            }))
            .done("done"),
    ));
    session.laid.columns = 120;
    drawn_at(&mut session, 120, 24);
    swept(&mut session, (0, 0), (0, 5));
    assert!(
        session.holds_a_selection(),
        "the fixture holds no selection"
    );
    session.toggle_files();
    assert!(session.files_open());
    handle_key(&mut session, key(KeyCode::Esc));
    assert!(
        session.selection.is_none() && session.files_open(),
        "the first press did not put the selection away before the files state"
    );
    handle_key(&mut session, key(KeyCode::Esc));
    assert!(!session.files_open());

    // A delegate's view.
    let (mut session, _) = at_rest_over_two_delegates();
    swept(&mut session, (0, 0), (0, 5));
    assert!(
        session.holds_a_selection(),
        "the fixture holds no selection"
    );
    handle_key(&mut session, ctrl('l'));
    assert!(session.watching().is_some(), "the view did not open");
    handle_key(&mut session, key(KeyCode::Esc));
    assert!(
        session.selection.is_none() && session.watching().is_some(),
        "the first press did not put the selection away before the view"
    );
}

/// MOUSE-35. Rejects: the first row of a window the rows are drawn in being taken for the start of a
/// line, when it is the continuation of one the window before ended in. One line of three thousand
/// words wraps over more rows than the two hundred and fifty-six that are drawn at a time, and is
/// swept from its first row to its last with the mouse, through the real rendering. The copy is the
/// line, with no line break where a window ended.
#[test]
fn a_line_that_wraps_over_several_windows_of_rows_copies_as_one_line() {
    let words: Vec<String> = (0..3000).map(|n| format!("w{n:04}")).collect();
    let mut session = Session::new("none");
    session.complete(words.join(" "), Vec::new(), 0);
    drawn_at(&mut session, 30, 24);
    assert!(
        session.laid.rows > 520,
        "the line is too short to cross two windows: {} rows",
        session.laid.rows
    );
    session.scroll_up(u16::MAX);
    drawn_at(&mut session, 30, 24);
    let top = find_cell(&drawn_at(&mut session, 30, 24), "w0000");
    press(&mut session, top.0, top.1, KeyModifiers::NONE);

    session.scroll_down(u16::MAX);
    drawn_at(&mut session, 30, 24);
    let last_row = session.laid.panes[0].1.bottom() - 1;
    press(&mut session, last_row, 28, KeyModifiers::SHIFT);

    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(30, 24)).expect("terminal");
    let mut handed = Vec::new();
    copy_selection(&mut terminal, &mut session, |text| {
        handed.push(text.to_string());
        true
    })
    .expect("the copy was made");
    let expected = words.join(" ");
    // The blank row that holds this reply from the next is the last row of the transcript.
    let copied = handed
        .first()
        .map(|text| text.trim_end_matches('\n').to_string())
        .unwrap_or_default();
    let differs_at = copied
        .chars()
        .zip(expected.chars())
        .position(|(got, wanted)| got != wanted);
    assert!(
        handed.len() == 1 && copied == expected,
        "{} copies, {} characters against {}, the first difference at {differs_at:?}",
        handed.len(),
        copied.chars().count(),
        expected.chars().count()
    );
}

/// MOUSE-38, for a selection of the status line and one of the list offered under the box, which are
/// panes of their own and not the transcript. Rejects: Escape taking a held selection of either only
/// where it is in the transcript, so that the first press clears the box or stops the turn with the
/// selection still marked.
#[test]
fn escape_puts_away_a_selection_of_the_status_line_or_the_offered_list_first() {
    // The list offered under the box as a line is typed.
    let mut session = Session::new("none");
    for c in "/he".chars() {
        session.type_char(c);
    }
    let buffer = drawn_at(&mut session, 120, 24);
    let offered = session
        .laid
        .panes
        .iter()
        .find(|(pane, _)| *pane == crate::select::Pane::Offered)
        .expect("nothing was offered under the box")
        .1;
    let row = offered.y;
    let (_, column) = find_cell(&buffer, "/help");
    swept(&mut session, (row, column), (row, column + 4));
    let held = session.selection.expect("a selection");
    assert_eq!(held.pane(), crate::select::Pane::Offered);
    assert!(session.holds_a_selection());
    handle_key(&mut session, key(KeyCode::Esc));
    assert!(
        session.selection.is_none(),
        "the selection was not put away"
    );
    assert_eq!(session.input(), "/he", "the press cleared the box");
    handle_key(&mut session, key(KeyCode::Esc));
    assert_eq!(session.input(), "", "the next press did not clear it");

    // The status line while a turn runs.
    let mut session = Session::new("none");
    session.type_char('a');
    session.submit();
    assert!(session.a_turn_is_running());
    drawn_at(&mut session, 120, 24);
    let status = session
        .laid
        .panes
        .iter()
        .find(|(pane, _)| *pane == crate::select::Pane::Status)
        .expect("no status line while a turn runs")
        .1;
    swept(&mut session, (status.y, 2), (status.y, 8));
    let held = session.selection.expect("a selection");
    assert_eq!(held.pane(), crate::select::Pane::Status);
    assert!(session.holds_a_selection());
    let escape = key(KeyCode::Esc);
    assert!(!stops_the_turn(&session, escape));
    handle_key_while_working(&mut session, escape);
    assert!(
        session.selection.is_none(),
        "the selection was not put away"
    );
    assert!(
        stops_the_turn(&session, escape),
        "the turn could not be stopped"
    );
}

/// MOUSE-7. Rejects: a press with shift held on the blank before a row of the info panel's text
/// beginning a selection of its own, which put away the one held. The blank is no text and is part of
/// the pane, so the press moves the loose end to the first column of the text on its row, and the
/// selection copies from where it began to there.
#[test]
fn a_shift_click_on_the_indent_of_a_panel_row_extends_the_selection_held() {
    let (mut session, buffer) = two_panes();
    let (row, column) = find_cell(&buffer, "PANELMARK");
    let panel = session
        .laid
        .panes
        .iter()
        .find(|(pane, _)| *pane == crate::select::Pane::Panel)
        .expect("the panel is drawn")
        .1;
    assert_eq!(
        column,
        panel.x + crate::panel::INSET,
        "the text is not after the indent"
    );

    swept(&mut session, (row, column), (row, column + 4));
    let held = session.selection.expect("a selection");
    // The blank after the border, on the row of the directory two below, which is another row of text.
    let (below, _) = find_cell(&buffer, "/work/dir");
    let indent = (below, panel.x + 1);
    assert!(panel.contains((indent.1, indent.0).into()));
    press(&mut session, indent.0, indent.1, KeyModifiers::SHIFT);

    let extended = session.selection.expect("the press put the selection away");
    assert_ne!(extended, held, "the loose end did not move");
    let buffer = drawn_at(&mut session, 120, 24);
    let copied = selected_text(&session, &buffer, &extended);
    assert!(copied.starts_with("PANELMARK"), "{copied:?}");
    assert!(!copied.contains("/work/dir"), "{copied:?}");
    assert!(
        copied.contains('\n'),
        "the selection did not reach the next row: {copied:?}"
    );
}

/// MOUSE-1. Rejects: a selection held when the scroller's key list is opened being marked over the
/// list, whose cells are the interface's and no text of the transcript the selection is of. The
/// selection runs under where the list is drawn, which is the middle of the screen, and the cells it
/// still covers are marked outside the list.
#[test]
fn a_selection_held_under_the_scrollers_key_list_is_not_marked_over_it() {
    let _held = crate::theme::exclusive();
    let mut session = Session::new("none");
    session.complete(
        (0..40)
            .map(|n| format!("row {n:02} of the reply, long enough to run well past the list"))
            .collect::<Vec<_>>()
            .join("\n"),
        Vec::new(),
        0,
    );
    drawn_at(&mut session, 120, 24);
    session.open_scroller();
    drawn_at(&mut session, 120, 24);
    dragged_and_released(&mut session, (120, 24), (1, 0), (21, 110));
    assert!(session.holds_a_selection());

    session.toggle_scroller_help();
    let buffer = drawn_at(&mut session, 120, 24);
    let list = session
        .laid
        .popup(crate::state::Panel::KeyList)
        .expect("the list was not drawn")
        .area;
    let marked = crate::theme::picked_out().bg;
    let (mut inside, mut outside) = (0, 0);
    for row in 0..buffer.area.height {
        for column in 0..buffer.area.width {
            if buffer[(column, row)].style().bg == marked {
                if list.contains((column, row).into()) {
                    inside += 1;
                } else {
                    outside += 1;
                }
            }
        }
    }
    assert_eq!(inside, 0, "{inside} cells of the key list are marked");
    assert!(outside > 100, "the selection is not marked outside it");
}

/// A session with a turn running, whose reply so far is `rows` numbered rows and the beginning of
/// the next, `row NN of the re`, and the frame drawn at 120 by 24.
fn a_reply_being_written(rows: usize) -> Session {
    let mut session = Session::new("none");
    session
        .transcript
        .push(crate::state::Entry::user("write it out"));
    session.status = Status::Working;
    let written: Vec<String> = (0..rows)
        .map(|n| format!("row {n:02} of the reply"))
        .collect();
    session.streaming(&format!("{}\nrow {rows:02} of the re", written.join("\n")));
    drawn_at(&mut session, 120, 24);
    session
}

/// The model writes `words` more of the reply and the frame is drawn, as the loop does for each chunk.
fn written_on(session: &mut Session, words: &str) {
    session.streaming(words);
    drawn_at(session, 120, 24);
}

/// What the clipboard is handed when the button is let go at `row` and `column`, which is one string.
fn handed_at_the_release(session: &mut Session, row: u16, column: u16) -> String {
    let mut handed = let_go(session, (120, 24), row, column);
    assert_eq!(handed.len(), 1, "the clipboard was handed {handed:?}");
    handed.remove(0)
}

/// MOUSE-1 and MOUSE-34. Rejects: a selection whose loose end is on the blank row that closes the
/// transcript, or on the cells under it, being put away at the next words of a reply that is still
/// arriving. The words are added to the row above that blank row, and what tells the blank row from
/// any other covers every row above it. The reply grows between the drag and the release, as it does
/// for as long as the model writes, and what is copied is the words from the press to the end of the
/// row as it is now.
#[test]
fn a_selection_ended_below_a_reply_that_is_being_written_survives_its_next_words() {
    // The pointer is below the pane, and on the pane's last row, which is the blank one.
    for from_the_bottom in [0u16, 1] {
        let mut session = a_reply_being_written(80);
        let pane = the_transcript_pane(&session);
        let buffer = drawn_at(&mut session, 120, 24);
        let (row, column) = find_cell(&buffer, "row 70");
        let to = pane.bottom() - from_the_bottom;
        swept(&mut session, (row, column), (to, 8));
        drawn_at(&mut session, 120, 24);
        assert!(session.selection.is_some(), "ended on {to}: put away");

        written_on(&mut session, "ply and more");
        assert!(
            session.selection.is_some(),
            "pointer {from_the_bottom} rows from the bottom: the next words of the reply put the selection away"
        );
        let copied = handed_at_the_release(&mut session, to, 8);
        assert!(
            copied.starts_with("row 70 of the reply\n  row 71 of the reply\n"),
            "{from_the_bottom}: {copied:?}"
        );
        assert!(
            copied.ends_with("\n  row 79 of the reply\n  row 80 of the reply and more"),
            "{from_the_bottom}: {copied:?}"
        );
    }
}

/// MOUSE-1 and MOUSE-34. Rejects: the same loss when the reply is long enough to wrap, so that the
/// words that arrive give the row the end is on a row after it.
#[test]
fn a_selection_ended_below_a_reply_that_wraps_as_it_is_written_survives_it() {
    let mut session = a_reply_being_written(80);
    let pane = the_transcript_pane(&session);
    let buffer = drawn_at(&mut session, 120, 24);
    let (row, column) = find_cell(&buffer, "row 70");
    swept(&mut session, (row, column), (pane.bottom(), 8));
    drawn_at(&mut session, 120, 24);

    written_on(&mut session, &format!("ply {}", "and more ".repeat(20)));
    assert!(
        session.selection.is_some(),
        "the words that wrapped the row put the selection away"
    );
    let copied = handed_at_the_release(&mut session, pane.bottom(), 8);
    assert!(copied.starts_with("row 70 of the reply\n"), "{copied:?}");
    assert!(
        copied.contains("\n  row 80 of the reply and more"),
        "{copied:?}"
    );
}

/// MOUSE-1, MOUSE-34 and MOUSE-37. Rejects: a drag held below the transcript with the view scrolled
/// back, which scrolls to the tail and ends on the row that closes the transcript there, losing the
/// selection at the next words of the reply.
#[test]
fn a_drag_held_below_a_reply_being_written_that_scrolls_to_its_tail_keeps_the_selection() {
    let mut session = a_reply_being_written(80);
    for _ in 0..4 {
        wheel_up(&mut session);
    }
    let buffer = drawn_at(&mut session, 120, 24);
    let pane = the_transcript_pane(&session);
    let (row, column) = find_cell(&buffer, "row 60");
    swept(&mut session, (row, column), (pane.bottom(), 8));

    let start = Instant::now();
    let mut steps = 0;
    while session.scroll_toward_the_pointer(at_a_step(start, steps)) {
        steps += 1;
        drawn_at(&mut session, 120, 24);
        assert!(steps < 20, "the view never reached the tail");
    }
    assert_eq!(session.scroll, 0, "the view stopped short of the tail");
    drawn_at(&mut session, 120, 24);
    assert!(session.selection.is_some());

    written_on(&mut session, "ply and more");
    assert!(
        session.selection.is_some(),
        "the next words of the reply put the selection away at the tail"
    );
    let copied = handed_at_the_release(&mut session, pane.bottom(), 8);
    assert!(copied.starts_with("row 60 of the reply\n"), "{copied:?}");
    assert!(
        copied.ends_with("\n  row 80 of the reply and more"),
        "{copied:?}"
    );
}

/// MOUSE-1 and MOUSE-34, in the scroller, with a transcript that fills the screen and with one that
/// leaves blank cells under it. Rejects: the same loss in a view that is not the plain one.
#[test]
fn a_selection_ended_below_a_reply_being_written_survives_its_next_words_in_the_scroller() {
    for rows in [80, 6] {
        let mut session = a_reply_being_written(rows);
        session.open_scroller();
        let buffer = drawn_at(&mut session, 120, 24);
        let pane = the_transcript_pane(&session);
        let first = format!("row {:02}", rows - 5);
        let (row, column) = find_cell(&buffer, &first);
        swept(&mut session, (row, column), (pane.bottom(), 8));
        drawn_at(&mut session, 120, 24);
        assert!(session.selection.is_some(), "{rows}: put away at once");

        written_on(&mut session, "ply and more");
        assert!(
            session.selection.is_some(),
            "{rows} rows: the next words of the reply put the selection away in the scroller"
        );
        let copied = handed_at_the_release(&mut session, pane.bottom(), 8);
        assert!(copied.starts_with(&first), "{rows}: {copied:?}");
        assert!(
            copied.ends_with(&format!("\n  row {rows:02} of the reply and more")),
            "{rows}: {copied:?}"
        );
    }
}

/// MOUSE-34. The fingerprint still does what it is for: a line put in above the end, or a change to
/// the words the end's own line begins with, puts the selection away, whether the end is below the
/// text or on it, in the plain view and in the scroller.
#[test]
fn a_change_above_an_end_below_a_reply_being_written_still_puts_the_selection_away() {
    for scroller in [false, true] {
        for ended_below in [true, false] {
            let held = || {
                let mut session = a_reply_being_written(80);
                if scroller {
                    session.open_scroller();
                }
                let buffer = drawn_at(&mut session, 120, 24);
                let pane = the_transcript_pane(&session);
                let (row, column) = find_cell(&buffer, "row 70");
                let to = if ended_below {
                    pane.bottom()
                } else {
                    pane.bottom() - 2
                };
                swept(&mut session, (row, column), (to, 8));
                drawn_at(&mut session, 120, 24);
                assert!(session.selection.is_some());
                session
            };
            let label = format!("scroller {scroller}, ended below the text {ended_below}");

            // A line put in above everything the selection is of.
            let mut session = held();
            session
                .transcript
                .insert(0, crate::state::Entry::user("an earlier question"));
            written_on(&mut session, "ply");
            assert!(session.selection.is_none(), "{label}: a line put in above");

            // A reply that is not the one the selection was made on, written under the same text.
            let mut session = held();
            session.transcript[0] = crate::state::Entry::user("another question");
            written_on(&mut session, "ply");
            assert!(session.selection.is_none(), "{label}: a line changed above");
        }
    }
}
