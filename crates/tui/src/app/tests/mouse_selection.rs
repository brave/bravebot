//! Selecting text with the mouse: a pane, a word or a row, extending, wrapped and wide characters, and what is copied.

use super::*;

/// MOUSE-4: with `tui.mouse` off the interface never asks the terminal to report the mouse, so
/// the terminal's own selection and wheel keep working, and the rest of the takeover is
/// unchanged. With it on, the buttons, a drag, every motion and the coordinates in the form that
/// names a column past 223 (1000, 1002, 1003 and 1006) are asked for.
#[test]
fn a_session_with_the_mouse_off_never_asks_for_mouse_reporting() {
    let mut off = Vec::new();
    ask_for_modes(&mut off, false, false).expect("ask");
    let off = String::from_utf8(off).expect("utf8");
    let left_on = modes_left_on(&off);
    for mode in ["1000", "1002", "1003", "1006"] {
        assert!(
            !off.contains(&format!("\x1b[?{mode}h")),
            "mode {mode} asked for"
        );
        assert!(!left_on.contains(mode), "mode {mode} left on");
    }
    for mode in ["1049", "2004"] {
        assert!(
            left_on.contains(mode),
            "mode {mode} was dropped with the mouse"
        );
    }

    let on = modes_left_on(&asked_for(false));
    for mode in ["1000", "1002", "1003", "1006"] {
        assert!(
            on.contains(mode),
            "mode {mode} not asked for with the mouse on"
        );
    }
}

/// MOUSE-4: only a `false` turns it off, and it is on before any settings are read.
#[test]
fn the_mouse_is_captured_unless_the_settings_turn_it_off() {
    let mut session = Session::new(String::new());
    assert!(session.mouse_captured());
    session.adopt_mouse(Some(false));
    assert!(!session.mouse_captured());
    session.adopt_mouse(None);
    assert!(session.mouse_captured());
    session.adopt_mouse(Some(true));
    assert!(session.mouse_captured());
}

/// Two presses in a row on one cell select the word there, and a third the whole row, and
/// neither takes anything from the info panel beside it.
#[test]
fn a_double_click_selects_a_word_and_a_triple_click_a_row() {
    let (mut session, buffer) = two_panes();
    let (row, column) = find_on(&buffer, "one two");
    press(&mut session, row, column + 4, KeyModifiers::NONE);
    press(&mut session, row, column + 4, KeyModifiers::NONE);
    let word = session.selection.expect("selection");
    assert_eq!(crate::select::text(&buffer, &word), "two");
    press(&mut session, row, column + 4, KeyModifiers::NONE);
    let line = session.selection.expect("selection");
    let text = crate::select::text(&buffer, &line);
    assert!(text.contains("TRANSCRIPTMARK one two three"), "{text:?}");
    assert!(!text.contains("PANELMARK"), "{text:?}");
}

/// A press on another cell starts the count again, so two clicks on different words select
/// nothing in whole.
#[test]
fn two_presses_on_different_cells_are_not_a_double_click() {
    let (mut session, buffer) = two_panes();
    let (row, column) = find_on(&buffer, "one two");
    press(&mut session, row, column, KeyModifiers::NONE);
    press(&mut session, row, column + 4, KeyModifiers::NONE);
    assert!(session.selection.expect("selection").is_empty());
}

/// Shift-click moves the loose end of the selection held and leaves the anchor.
#[test]
fn a_shift_click_extends_the_selection_held() {
    let (mut session, buffer) = two_panes();
    let (row, column) = find_on(&buffer, "one two");
    swept(&mut session, (row, column), (row, column + 3));
    press(&mut session, row, column + 7, KeyModifiers::SHIFT);
    let selection = session.selection.expect("selection");
    assert_eq!(crate::select::text(&buffer, &selection), "one two");
}

/// A shift-click in another pane begins a selection there and does not stretch the first.
#[test]
fn a_shift_click_in_another_pane_begins_a_new_selection() {
    let (mut session, buffer) = two_panes();
    let (row, column) = find_on(&buffer, "one two");
    swept(&mut session, (row, column), (row, column + 3));
    let (prow, pcolumn) = find_on(&buffer, "PANELMARK");
    press(&mut session, prow, pcolumn, KeyModifiers::SHIFT);
    let selection = session.selection.expect("selection");
    assert_eq!(selection.pane(), crate::select::Pane::Panel);
    assert!(selection.is_empty());
}

/// A drag that starts in the transcript and sweeps right across the panel must not take the
/// panel's text on the rows it passes.
#[test]
fn a_drag_from_the_transcript_never_copies_the_panel() {
    let (mut session, buffer) = two_panes();
    swept(&mut session, (0, 0), (20, 119));
    let selection = session.selection.expect("selection");
    let text = crate::select::text(&buffer, &selection);
    assert!(text.contains("TRANSCRIPTMARK"), "copied {text:?}");
    assert!(!text.contains("PANELMARK"), "copied {text:?}");
}

/// The other direction: a drag that starts in the panel and sweeps left stops at the panel's
/// edge instead of taking the transcript.
#[test]
fn a_drag_from_the_panel_never_copies_the_transcript() {
    let (mut session, buffer) = two_panes();
    let panel = session
        .laid
        .panes
        .iter()
        .find(|(pane, _)| *pane == crate::select::Pane::Panel)
        .expect("panel recorded")
        .1;
    swept(&mut session, (20, panel.left() + 1), (0, 0));
    let selection = session.selection.expect("selection");
    let text = crate::select::text(&buffer, &selection);
    assert!(text.contains("PANELMARK"), "copied {text:?}");
    assert!(!text.contains("TRANSCRIPTMARK"), "copied {text:?}");
}

/// Dragging across the session id in the panel copies the id and nothing else: no border, no
/// indent, no padding, and no line break where the id is drawn over two rows.
#[test]
fn a_drag_over_the_session_id_copies_the_id_alone() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    const ID: &str = "d23f56fe-8328-429c-b51b-74cb8431957b";
    let (mut session, _) = two_panes();
    session.name_id(Some(ID));
    let mut terminal = Terminal::new(TestBackend::new(120, 24)).expect("terminal");
    let mut laid = crate::state::Laid::default();
    terminal
        .draw(|frame| laid = render::draw(frame, &session))
        .expect("draw");
    session.note_layout(laid);
    let buffer = terminal.backend().buffer().clone();
    let panel = session
        .laid
        .panes
        .iter()
        .find(|(pane, _)| *pane == crate::select::Pane::Panel)
        .expect("panel recorded")
        .1;
    let first = (panel.top()..panel.bottom())
        .find(|&row| {
            (panel.left()..panel.right())
                .map(|column| buffer[(column, row)].symbol())
                .collect::<String>()
                .contains("d23f56fe")
        })
        .expect("the id is drawn");
    // Press on the indent before the text and drag past the end of the second half of the id.
    // The border column is a resize handle and starts no selection.
    swept(
        &mut session,
        (first, panel.left() + 1),
        (first + 1, panel.right() - 1),
    );
    let selection = session.selection.expect("selection");
    assert_eq!(
        super::super::selected_text(&session, &buffer, &selection),
        ID
    );
}

/// A reply the transcript wrapped is one paragraph, and copies as one: the wrap is the width of
/// the screen and no line break of the author's. That holds in the view at rest and in the
/// scroller, which are drawn by different code.
#[test]
fn a_wrapped_reply_copies_as_one_paragraph() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    const REPLY: &str = "the quick brown fox jumps over the lazy dog and then keeps on running \
                             through the field until it reaches the far fence";
    // The first row ends in the last column, so its blank is dropped and the next row starts
    // with a word as one cut inside a word would.
    const FLUSH: &str = "aaaaaaaaa bbbbbbbbb ccccccccc dddddddd next words follow after it";
    const TURN_MARKER: &str = "\u{23fa}";
    for (scrolling, reply, from, to) in [
        (false, REPLY, "the quick", "far fence"),
        (true, REPLY, "the quick", "far fence"),
        (false, FLUSH, "aaaaaaaaa", "after it"),
        (true, FLUSH, "aaaaaaaaa", "after it"),
    ] {
        let mut session = Session::new("none");
        session.complete(reply.to_string(), Vec::new(), 0);
        if scrolling {
            session.open_scroller();
        }
        let mut terminal = Terminal::new(TestBackend::new(40, 20)).expect("terminal");
        let mut laid = crate::state::Laid::default();
        terminal
            .draw(|frame| laid = render::draw(frame, &session))
            .expect("draw");
        session.note_layout(laid);
        let buffer = terminal.backend().buffer().clone();
        let (first, _) = find_on(&buffer, from);
        let (last, _) = find_on(&buffer, to);
        assert!(last > first, "the reply was not wrapped");
        swept(&mut session, (first, 0), (last, 39));
        let selection = session.selection.expect("selection");
        let copied = super::super::selected_text(&session, &buffer, &selection);
        assert_eq!(
            copied,
            format!("{TURN_MARKER} {reply}"),
            "scrolling: {scrolling}"
        );
    }
}

/// The text of a drag from the transcript's first word to the far edge of the screen is what the
/// clipboard is handed, with nothing of the info panel beside it, and the notice counts it.
#[test]
fn a_release_over_the_transcript_hands_the_clipboard_its_text_and_none_of_the_panel() {
    let (mut session, buffer) = two_panes();
    let (row, column) = find_cell(&buffer, "TRANSCRIPTMARK");
    let handed = dragged_and_released(&mut session, (120, 24), (row, column), (row, 119));
    assert_eq!(handed, vec!["TRANSCRIPTMARK one two three".to_string()]);
    assert_eq!(session.copied, Some("TRANSCRIPTMARK one two three".len()));
}

/// At rest the release reaches the clipboard through the idle loop's handler and not the one a
/// running turn uses, and it hands over the same words.
#[test]
fn a_release_at_rest_hands_the_clipboard_what_was_swept() {
    let (mut session, buffer) = two_panes();
    let (row, column) = find_cell(&buffer, "TRANSCRIPTMARK");
    let handed = dragged_and_released_at_rest(&mut session, (120, 24), (row, column), (row, 119));
    assert_eq!(handed, vec!["TRANSCRIPTMARK one two three".to_string()]);

    let (row, column) = find_cell(&buffer, "PANELMARK");
    let handed = dragged_and_released_at_rest(&mut session, (120, 24), (row, column), (row, 119));
    assert_eq!(handed, vec!["PANELMARK".to_string()]);
}

/// The reverse: a drag across a row of the panel hands over the panel's words and none of the
/// transcript's, which are drawn on the same rows.
#[test]
fn a_release_over_the_panel_hands_the_clipboard_its_text_and_none_of_the_transcript() {
    let (mut session, buffer) = two_panes();
    let (row, column) = find_cell(&buffer, "PANELMARK");
    let handed = dragged_and_released(&mut session, (120, 24), (row, column), (row, 119));
    assert_eq!(handed, vec!["PANELMARK".to_string()]);
}

/// The same release while a turn is running, with a reply arriving under the words: the loop that
/// answers the mouse then is not the one at rest, and what it hands over has to be the same.
#[test]
fn a_release_while_a_turn_runs_hands_the_clipboard_what_was_swept() {
    let (mut session, _) = two_panes();
    session.status = Status::Working;
    session.streaming("a reply that is still arriving");
    let buffer = drawn(&mut session);
    let (row, column) = find_cell(&buffer, "TRANSCRIPTMARK");
    let handed = dragged_and_released(&mut session, (120, 24), (row, column), (row, 119));
    assert_eq!(handed, vec!["TRANSCRIPTMARK one two three".to_string()]);
}

/// In the scroller the whole screen is one pane, and a drag across a wrapped reply hands over one
/// paragraph.
#[test]
fn a_release_in_the_scroller_hands_the_clipboard_the_paragraph_that_was_swept() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    const REPLY: &str = "the quick brown fox jumps over the lazy dog and then keeps on running";
    let mut session = Session::new("none");
    session.complete(REPLY.to_string(), Vec::new(), 0);
    session.open_scroller();
    let mut terminal = Terminal::new(TestBackend::new(40, 20)).expect("terminal");
    let mut laid = crate::state::Laid::default();
    terminal
        .draw(|frame| laid = render::draw(frame, &session))
        .expect("draw");
    session.note_layout(laid);
    let buffer = terminal.backend().buffer().clone();
    let (first, _) = find_on(&buffer, "the quick");
    let (last, _) = find_on(&buffer, "running");
    let handed = dragged_and_released(&mut session, (40, 20), (first, 0), (last, 39));
    assert_eq!(handed, vec![format!("\u{23fa} {REPLY}")]);
}

/// A click that swept nothing hands the clipboard nothing and puts the selection away, and a
/// copier that took nothing leaves no notice saying it did.
#[test]
fn a_click_hands_over_nothing_and_a_refused_copy_is_not_reported() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let (mut session, buffer) = two_panes();
    let (row, column) = find_cell(&buffer, "TRANSCRIPTMARK");
    let handed = dragged_and_released(&mut session, (120, 24), (row, column), (row, column));
    assert!(handed.is_empty(), "a click copied {handed:?}");
    assert!(session.selection.is_none());
    assert_eq!(session.copied, None);

    let mut terminal = Terminal::new(TestBackend::new(120, 24)).expect("terminal");
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Drag(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        let column = if kind == MouseEventKind::Down(MouseButton::Left) {
            column
        } else {
            119
        };
        answer_mouse(&mut terminal, &mut session, drag(kind, row, column), |_| {
            false
        })
        .expect("answered");
    }
    assert_eq!(session.copied, None, "a copy nothing took was reported");
}

/// MOUSE-34: a wheel turn with a selection held moves the highlight with the words it was put
/// on. A selection that stayed on its screen cells would cover whatever scrolled under them.
#[test]
fn a_wheel_turn_leaves_the_selection_on_the_text_it_was_put_on() {
    let _held = crate::theme::exclusive();
    let mut session = a_long_reply(60);
    let buffer = drawn_at(&mut session, 120, 24);
    let (row, column) = find_cell(&buffer, "row 50");
    swept(&mut session, (row, column), (row + 1, column + 6));
    let buffer = drawn_at(&mut session, 120, 24);
    let swept_over = vec!["row 50 of the reply".to_string(), "  row 51".to_string()];
    assert_eq!(highlighted(&buffer), swept_over);

    wheel_up(&mut session);
    let buffer = drawn_at(&mut session, 120, 24);
    assert_eq!(highlighted(&buffer), swept_over);
    assert_eq!(
        find_cell(&buffer, "row 50").0,
        row + session.wheel_rows(),
        "the view did not scroll"
    );

    // Further back, until the words have gone off the bottom of the transcript.
    for _ in 0..8 {
        wheel_up(&mut session);
    }
    let buffer = drawn_at(&mut session, 120, 24);
    assert_eq!(highlighted(&buffer), Vec::<String>::new());
    assert!(session.selection.is_some(), "scrolling away dropped it");
}

/// MOUSE-34, MOUSE-26, MOUSE-29 and MOUSE-16: a selection of the transcript held by its rows,
/// with the view scrolled so that no row of the transcript is the screen row it was, does not
/// stand in the way of a click that sweeps nothing. The click reaches the marker of a reply, a
/// row of the info panel and a place in the box, and puts the selection away as the click on
/// that target does without one. The regression it rejects is a press that leaves a selection
/// of rows for the release to find, so that the release is answered as the end of a drag, and a
/// target looked up by the screen row that the scroll offset makes the wrong row.
#[test]
fn a_click_that_sweeps_nothing_reaches_its_target_while_a_selection_of_rows_is_held() {
    let (mut session, _) = a_transcript_with_a_target_of_each_kind();
    // Drag over two rows of the long reply and let go. The copy is asked for and the selection
    // stays held, counting the transcript's rows.
    let hold_a_selection = |session: &mut Session| {
        let buffer = drawn_at(session, 120, 24);
        let (row, column) = find_cell(&buffer, "of the reply");
        swept(session, (row, column), (row + 1, column + 4));
        let up = drag(MouseEventKind::Up(MouseButton::Left), row + 1, column + 4);
        assert_eq!(handle_mouse(session, up), Action::Copy);
        assert!(
            session.holds_a_selection() && session.selection.is_some_and(|s| s.scrolls()),
            "no selection of rows was held"
        );
    };
    hold_a_selection(&mut session);
    wheel_up(&mut session);
    assert!(session.scroll > 0, "the view did not scroll back");
    let buffer = drawn_at(&mut session, 120, 24);
    assert!(session.holds_a_selection(), "scrolling put it away");

    // A reply's marker.
    let (row, column) = find_cell(&buffer, "Final answer.");
    assert_eq!(column, 2, "the marker leads the words");
    assert_eq!(
        click(&mut session, row, 0),
        Action::CopyReply(A_LAST_REPLY.to_string())
    );
    assert!(session.selection.is_none(), "the marker click kept it");

    // A row of the info panel, which says what the command typed says.
    hold_a_selection(&mut session);
    let buffer = drawn_at(&mut session, 120, 24);
    let effort = command_rows(&session)[1].0.clone();
    let (row, column) = find_cell(&buffer, &effort);
    assert_eq!(
        click(&mut session, row, column + 1),
        Action::ChooseEffort(crate::model_prompt::Scope::EverySession)
    );
    assert!(session.selection.is_none(), "the panel click kept it");

    // A place in the box.
    hold_a_selection(&mut session);
    let buffer = drawn_at(&mut session, 120, 24);
    let (row, column) = find_cell(&buffer, "draft text");
    click(&mut session, row, column + 3);
    assert_eq!(
        session.caret(),
        "dra".len(),
        "the box did not take the click"
    );
    assert!(session.selection.is_none(), "the box click kept it");
}

/// MOUSE-34 with MOUSE-27, MOUSE-29, MOUSE-22, MOUSE-40, MOUSE-41, MOUSE-42 and MOUSE-43: a
/// selection of the transcript held by its rows does not stand in the way of a click that sweeps
/// nothing on a web address (with ctrl), a file path (with ctrl for the editor, and plain to show
/// the file), the word that copies a block, the marker of a reply or a row of the info panel,
/// whichever arm of the handler answers it. The regression it rejects is an arm placed or
/// widened so that it takes another's cell, or a press that leaves the click nothing to answer.
/// A drag over the address is a selection that copies the address and marks nothing.
#[test]
fn a_click_that_sweeps_nothing_reaches_a_link_a_path_a_block_and_the_panel_while_rows_are_held() {
    use ratatui::style::Modifier;
    const GUIDE: &str = "https://example.test/guide";
    const PULL_REQUEST: &str = "https://github.com/brave/bravebot/pull/3200";
    const REPLY: &str = "Read https://example.test/guide and src/main.rs:7 first:\n```sh\nls -la\n```\nthen carry on.";
    let (mut session, _) = panel_of_command_rows(false);
    session.link(None, Some(PULL_REQUEST));
    session
        .transcript
        .push(crate::state::Entry::assistant(REPLY, Vec::new()));
    session.paste("draft text");

    // Drag over the first row, which is no target, and let go: the selection stays held.
    let hold_a_selection = |session: &mut Session| {
        let buffer = drawn_at(session, 120, 40);
        let (row, column) = find_cell(&buffer, "TRANSCRIPTMARK");
        swept(session, (row, column), (row, column + 10));
        let up = drag(MouseEventKind::Up(MouseButton::Left), row, column + 10);
        assert_eq!(handle_mouse(session, up), Action::Copy);
        assert!(
            session
                .selection
                .is_some_and(|selection| selection.scrolls()),
            "no selection of rows was held"
        );
        drawn_at(session, 120, 40)
    };
    let with_ctrl = |session: &mut Session, row: u16, column: u16| {
        for kind in [
            MouseEventKind::Down(MouseButton::Left),
            MouseEventKind::Up(MouseButton::Left),
        ] {
            let mut event = drag(kind, row, column);
            event.modifiers = KeyModifiers::CONTROL;
            let acted = handle_mouse(session, event);
            if kind == MouseEventKind::Up(MouseButton::Left) {
                return acted;
            }
        }
        unreachable!("the release answers")
    };
    let path = crate::text_links::PathRef {
        path: "src/main.rs".to_string(),
        line: Some(7),
        column: None,
    };

    // The address, with ctrl.
    let buffer = hold_a_selection(&mut session);
    let (row, column) = find_cell(&buffer, GUIDE);
    assert_eq!(
        with_ctrl(&mut session, row, column + 3),
        Action::OpenUrl(GUIDE.to_string())
    );
    assert!(session.selection.is_none(), "the link click kept it");

    // The path, with ctrl for the editor, and plain for the file once nothing follows it.
    let buffer = hold_a_selection(&mut session);
    let (row, column) = find_cell(&buffer, "src/main.rs");
    assert_eq!(
        with_ctrl(&mut session, row, column + 4),
        Action::EditFile(path.clone())
    );
    assert!(session.selection.is_none(), "the editor click kept it");
    let buffer = hold_a_selection(&mut session);
    let (row, column) = find_cell(&buffer, "src/main.rs");
    assert_eq!(click(&mut session, row, column + 4), Action::Redraw);
    assert!(session.selection.is_none(), "the path click kept it");
    let later = std::time::Instant::now() + std::time::Duration::from_millis(600);
    assert_eq!(session.take_the_click_due(later), Some(path));

    // The word that copies the block, and the marker of the reply.
    let buffer = hold_a_selection(&mut session);
    let (row, column) = cells_of(&buffer, "copy")[0];
    assert_eq!(
        click(&mut session, row, column),
        Action::CopyReply("ls -la".to_string())
    );
    assert!(session.selection.is_none(), "the block click kept it");
    let buffer = hold_a_selection(&mut session);
    let (row, _) = find_cell(&buffer, "Read");
    assert_eq!(
        click(&mut session, row, 0),
        Action::CopyReply(REPLY.to_string())
    );
    assert!(session.selection.is_none(), "the marker click kept it");

    // The address the panel was given, without ctrl, and a row that stands for a command.
    let buffer = hold_a_selection(&mut session);
    let (row, column) = find_cell(&buffer, "brave/bravebot#");
    assert_eq!(
        click(&mut session, row, column + 2),
        Action::OpenUrl(PULL_REQUEST.to_string())
    );
    assert!(session.selection.is_none(), "the panel link click kept it");
    let buffer = hold_a_selection(&mut session);
    let effort = command_rows(&session)[1].0.clone();
    let (row, column) = find_cell(&buffer, &effort);
    assert_eq!(
        click(&mut session, row, column + 1),
        Action::ChooseEffort(crate::model_prompt::Scope::EverySession)
    );

    // A place in the box.
    let buffer = hold_a_selection(&mut session);
    let (row, column) = find_cell(&buffer, "draft text");
    click(&mut session, row, column + 3);
    assert_eq!(
        session.caret(),
        "dra".len(),
        "the box did not take the click"
    );

    // And a drag over the address is a selection of it: the copy is the address, the highlight
    // covers its cells, and no mark the matcher put on them is left on the screen.
    let buffer = drawn_at(&mut session, 120, 40);
    let (row, column) = find_cell(&buffer, GUIDE);
    let end = column + GUIDE.len() as u16;
    let held = {
        let mut event = drag(MouseEventKind::Down(MouseButton::Left), row, column);
        event.modifiers = KeyModifiers::CONTROL;
        handle_mouse(&mut session, event);
        let mut event = drag(MouseEventKind::Drag(MouseButton::Left), row, end);
        event.modifiers = KeyModifiers::CONTROL;
        handle_mouse(&mut session, event);
        let mut up = drag(MouseEventKind::Up(MouseButton::Left), row, end);
        up.modifiers = KeyModifiers::CONTROL;
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).expect("terminal");
        let mut handed = Vec::new();
        let action = answer_mouse(&mut terminal, &mut session, up, |text| {
            handed.push(text.to_string());
            true
        })
        .expect("answered");
        assert_eq!(action, Action::None, "the sweep was answered as a click");
        handed
    };
    assert_eq!(held, vec![GUIDE.to_string()]);
    let after = drawn_at(&mut session, 120, 40);
    assert!(
        highlighted(&after).iter().any(|text| text == GUIDE),
        "{:?}",
        highlighted(&after)
    );
    for row in 0..after.area.height {
        for column in 0..after.area.width {
            assert!(
                !after[(column, row)]
                    .modifier
                    .intersects(Modifier::SLOW_BLINK | Modifier::RAPID_BLINK),
                "a mark was left on {row},{column}"
            );
        }
    }
}

/// MOUSE-34: the keys that scroll the transcript at rest, the arrows, the page keys and the ends
/// under Ctrl, move the view and the selection stays on its words, marked where they are in view
/// and held where they are not.
#[test]
fn the_keys_that_scroll_at_rest_leave_the_selection_on_the_text_it_was_put_on() {
    let _held = crate::theme::exclusive();
    let mut session = a_long_reply(80);
    drawn_at(&mut session, 120, 24);
    // Three turns of the wheel, so that the words are still above the button under the view
    // (VIEW-45) after a page key has moved them down.
    for _ in 0..3 {
        wheel_up(&mut session);
    }
    let buffer = drawn_at(&mut session, 120, 24);
    let (row, column) = find_cell(&buffer, "row 56");
    swept(&mut session, (row, column), (row + 1, column + 6));
    let swept_over = vec!["row 56 of the reply".to_string(), "  row 57".to_string()];
    assert_eq!(highlighted(&drawn_at(&mut session, 120, 24)), swept_over);

    let mut at = row;
    for (press, rows) in [
        (key(KeyCode::Up), 1),
        (key(KeyCode::PageUp), 10),
        (key(KeyCode::Down), -1),
        (key(KeyCode::PageDown), -10),
    ] {
        handle_key(&mut session, press);
        let buffer = drawn_at(&mut session, 120, 24);
        assert_eq!(highlighted(&buffer), swept_over, "{press:?}");
        at = (i32::from(at) + rows) as u16;
        assert_eq!(
            find_cell(&buffer, "row 56").0,
            at,
            "{press:?}: the view did not move as far as it was asked to"
        );
    }

    // The ends take the view past the words, which are then out of view and no longer marked, and
    // the selection is still held there, and marked again where the words come back into view.
    for end in [KeyCode::Home, KeyCode::End] {
        handle_key(&mut session, ctrl_key(end));
        let buffer = drawn_at(&mut session, 120, 24);
        assert!(highlighted(&buffer).is_empty(), "{end:?}");
        assert!(
            session.holds_a_selection(),
            "{end:?} put the selection away"
        );
    }
    for _ in 0..12 {
        if highlighted(&drawn_at(&mut session, 120, 24)) == swept_over {
            break;
        }
        wheel_up(&mut session);
    }
    assert_eq!(highlighted(&drawn_at(&mut session, 120, 24)), swept_over);
}

/// Two calls in the turn's own run, the second finished and the first still running above it.
fn two_calls_the_second_finished() -> Session {
    use bravebot_agent::report::Activity;
    let mut session = Session::new("kernel-enforced");
    session.status = Status::Working;
    session
        .transcript
        .push(crate::state::Entry::user("read both files"));
    session.start_activity(Activity::running("Read", "a.rs"));
    session.start_activity(Activity::running("Read", "b.rs"));
    session.finish_activity(Activity::running("Read", "b.rs").done("20 lines"));
    session
}

/// What the clipboard is handed is the text that was swept or nothing, never other text.
fn assert_swept_or_nothing(handed: &[String], swept: &str, what: &str) {
    assert!(
        handed.iter().all(|text| text == swept) && handed.len() <= 1,
        "{what}: the clipboard was handed {handed:?}, and {swept:?} was swept"
    );
}

/// MOUSE-34: a call that finishes above a selection gains rows, which moves every row below it,
/// so the rows the selection holds are rows of other text. The frame drawn after marks nothing
/// there, the selection goes, and a release copies what was swept or nothing.
#[test]
fn a_call_finishing_above_a_selection_does_not_leave_it_over_other_text() {
    use bravebot_agent::report::Activity;
    for scrolling in [false, true] {
        let _held = crate::theme::exclusive();
        let mut session = two_calls_the_second_finished();
        drawn_at(&mut session, 120, 24);
        if scrolling {
            session.open_scroller();
        }
        let buffer = drawn_at(&mut session, 120, 24);
        let (row, column) = find_cell(&buffer, "Read(b.rs)");
        swept(&mut session, (row, column), (row, column + 10));
        let swept_over = vec!["Read(b.rs)".to_string()];
        assert_eq!(highlighted(&drawn_at(&mut session, 120, 24)), swept_over);

        session.finish_activity(Activity::running("Read", "a.rs").done("40 lines"));
        let buffer = drawn_at(&mut session, 120, 24);
        let marked = highlighted(&buffer);
        assert!(
            marked.is_empty() || marked == swept_over,
            "scrolling: {scrolling}: the frame marked {marked:?}"
        );
        let handed = let_go(&mut session, (120, 24), row, column + 10);
        assert_swept_or_nothing(&handed, "Read(b.rs)", &format!("scrolling: {scrolling}"));
        assert!(session.selection.is_none(), "scrolling: {scrolling}");
    }
}

/// MOUSE-34: the same in a delegate's view, where the calls are the delegate's own lines.
#[test]
fn a_call_finishing_above_a_selection_in_a_delegates_view_does_not_leave_it_over_other_text() {
    use bravebot_agent::report::{Activity, DelegateId, Delegation};
    let _held = crate::theme::exclusive();
    let mut session = Session::new("kernel-enforced");
    let id = DelegateId::nth(1);
    session.delegate_started(Delegation {
        stop: Default::default(),
        id,
        kind: "reader".to_string(),
        task: "find the parser".to_string(),
    });
    session.reporting_for(Some(id));
    session.start_activity(Activity::running("Read", "a.rs"));
    session.start_activity(Activity::running("Read", "b.rs"));
    session.finish_activity(Activity::running("Read", "b.rs").done("20 lines"));
    assert!(session.watch());
    assert!(session.watching_a_delegate());
    drawn_at(&mut session, 120, 24);
    let buffer = drawn_at(&mut session, 120, 24);
    let (row, column) = find_cell(&buffer, "Read(b.rs)");
    swept(&mut session, (row, column), (row, column + 10));
    let swept_over = vec!["Read(b.rs)".to_string()];
    assert_eq!(highlighted(&drawn_at(&mut session, 120, 24)), swept_over);

    session.finish_activity(Activity::running("Read", "a.rs").done("40 lines"));
    let marked = highlighted(&drawn_at(&mut session, 120, 24));
    assert!(
        marked.is_empty() || marked == swept_over,
        "the frame marked {marked:?}"
    );
    let handed = let_go(&mut session, (120, 24), row, column + 10);
    assert_swept_or_nothing(&handed, "Read(b.rs)", "a delegate's view");
    assert!(session.selection.is_none());
}

/// A delegate with two calls, the second finished, and the view open on it.
fn watching_a_delegate_with_two_calls() -> Session {
    use bravebot_agent::report::{Activity, DelegateId, Delegation};
    let mut session = Session::new("kernel-enforced");
    let id = DelegateId::nth(1);
    session.delegate_started(Delegation {
        stop: Default::default(),
        id,
        kind: "reader".to_string(),
        task: "find the parser".to_string(),
    });
    session.reporting_for(Some(id));
    session.start_activity(Activity::running("Read", "a.rs"));
    session.start_activity(Activity::running("Read", "b.rs"));
    session.finish_activity(Activity::running("Read", "b.rs").done("20 lines"));
    assert!(session.watch());
    assert!(session.watching_a_delegate());
    session
}

/// MOUSE-34: a row put ahead of the delegate in the list the view opens moves the delegate's place
/// in it, and the delegate's lines stay where they are. The selection in them is kept, and what
/// is copied is what was swept.
#[test]
fn a_row_put_ahead_of_the_watched_delegate_leaves_a_selection_of_its_lines_alone() {
    let _held = crate::theme::exclusive();
    let mut session = watching_a_delegate_with_two_calls();
    drawn_at(&mut session, 120, 24);
    let buffer = drawn_at(&mut session, 120, 24);
    let (row, column) = find_cell(&buffer, "Read(b.rs)");
    swept(&mut session, (row, column), (row, column + 10));
    let swept_over = vec!["Read(b.rs)".to_string()];
    assert_eq!(highlighted(&drawn_at(&mut session, 120, 24)), swept_over);
    let place = session.watching().map(|watching| watching.at);

    session.put_an_aside_ahead_of_the_watched_row();
    assert_ne!(session.watching().map(|watching| watching.at), place);
    assert!(
        session.watching_a_delegate(),
        "the view moved off the delegate"
    );
    assert_eq!(highlighted(&drawn_at(&mut session, 120, 24)), swept_over);
    let handed = let_go(&mut session, (120, 24), row, column + 10);
    assert_eq!(handed, vec!["Read(b.rs)".to_string()]);
}

/// MOUSE-34: the view moved to another delegate holds other lines, so the selection goes.
#[test]
fn stepping_to_another_delegate_drops_a_selection_of_the_one_before() {
    use bravebot_agent::report::{DelegateId, Delegation};
    let _held = crate::theme::exclusive();
    let mut session = watching_a_delegate_with_two_calls();
    session.reporting_for(None);
    session.delegate_started(Delegation {
        stop: Default::default(),
        id: DelegateId::nth(2),
        kind: "reader".to_string(),
        task: "find the lexer".to_string(),
    });
    drawn_at(&mut session, 120, 24);
    let buffer = drawn_at(&mut session, 120, 24);
    let (row, column) = find_cell(&buffer, "Read(b.rs)");
    swept(&mut session, (row, column), (row, column + 10));
    assert!(session.selection.is_some());

    session.watch_next();
    drawn_at(&mut session, 120, 24);
    assert!(session.selection.is_none());
    let handed = let_go(&mut session, (120, 24), row, column + 10);
    assert!(handed.is_empty(), "copied {handed:?}");
}

/// MOUSE-34: what a turn does below a selection leaves it alone: a call finishing and gaining rows,
/// an entry arriving and a reply being written under it, in the plain view and in the scroller.
#[test]
fn calls_finishing_and_a_reply_arriving_below_a_selection_leave_it_alone() {
    use bravebot_agent::report::Activity;
    for scrolling in [false, true] {
        let _held = crate::theme::exclusive();
        let mut session = two_calls_the_second_finished();
        drawn_at(&mut session, 120, 24);
        if scrolling {
            session.open_scroller();
        }
        let buffer = drawn_at(&mut session, 120, 24);
        let (row, column) = find_cell(&buffer, "read both files");
        swept(&mut session, (row, column), (row, column + 15));
        let swept_over = vec!["read both files".to_string()];
        assert_eq!(highlighted(&drawn_at(&mut session, 120, 24)), swept_over);

        session.finish_activity(Activity::running("Read", "a.rs").done("40 lines"));
        session.transcript.push(crate::state::Entry::assistant(
            "both read\nand compared",
            Vec::new(),
        ));
        for words in [
            "a reply ",
            "being written ",
            "under the selection\n",
            "and going on",
        ] {
            session.streaming(words);
            let buffer = drawn_at(&mut session, 120, 24);
            assert_eq!(
                highlighted(&buffer),
                swept_over,
                "scrolling: {scrolling}: after {words:?}"
            );
        }
        let handed = let_go(&mut session, (120, 24), row, column + 15);
        assert_eq!(
            handed,
            vec!["read both files".to_string()],
            "scrolling: {scrolling}"
        );
    }
}

/// MOUSE-34: a selection that ends on the line a reply is being written on keeps it as the line
/// gets longer and wraps, since what it was put on, the words so far, is still where it was.
#[test]
fn a_selection_ending_on_a_line_still_being_written_survives_the_line_growing() {
    for scrolling in [false, true] {
        let _held = crate::theme::exclusive();
        let mut session = Session::new("none");
        session
            .transcript
            .push(crate::state::Entry::user("an earlier prompt"));
        session.status = Status::Working;
        session.streaming("alpha beta gamma");
        drawn_at(&mut session, 40, 20);
        if scrolling {
            session.open_scroller();
        }
        let buffer = drawn_at(&mut session, 40, 20);
        let (row, column) = find_cell(&buffer, "alpha");
        swept(&mut session, (row, column), (row, column + 5));
        let swept_over = vec!["alpha".to_string()];
        assert_eq!(highlighted(&drawn_at(&mut session, 40, 20)), swept_over);

        for more in [
            " delta",
            " epsilon zeta eta theta",
            " iota kappa lambda mu nu xi omicron",
        ] {
            session.streaming(more);
            assert_eq!(
                highlighted(&drawn_at(&mut session, 40, 20)),
                swept_over,
                "scrolling: {scrolling}: after {more:?}"
            );
        }
        let handed = let_go(&mut session, (40, 20), row, column + 5);
        assert_eq!(handed, vec!["alpha".to_string()], "scrolling: {scrolling}");
    }
}

/// MOUSE-34: a reply written at the tail becomes an entry in the same place and shape when its
/// round ends. With no thinking above it nothing moves, and a selection of it is kept.
#[test]
fn a_reply_becoming_an_entry_keeps_a_selection_of_its_rows() {
    let reply: String = (1..=6).map(|n| format!("reply {n:02}\n")).collect();
    let _held = crate::theme::exclusive();
    let mut session = Session::new("none");
    session
        .transcript
        .push(crate::state::Entry::user("an earlier prompt"));
    session.status = Status::Working;
    session.streaming(&reply);
    drawn_at(&mut session, 120, 24);
    let buffer = drawn_at(&mut session, 120, 24);
    let (row, column) = find_cell(&buffer, "reply 03");
    swept(&mut session, (row, column), (row, column + 8));
    let swept_over = vec!["reply 03".to_string()];
    assert_eq!(highlighted(&drawn_at(&mut session, 120, 24)), swept_over);

    session.narrate(reply.trim_end());
    assert_eq!(highlighted(&drawn_at(&mut session, 120, 24)), swept_over);
    let handed = let_go(&mut session, (120, 24), row, column + 8);
    assert_eq!(handed, vec!["reply 03".to_string()]);
}

/// MOUSE-34 and MOUSE-27: the word that copies a fenced block is drawn on the block's header
/// only once the reply is an entry, so the header gains it when the round ends. It is no text of
/// the reply, so a selection from the header down, in the plain view and in the scroller, keeps
/// its words and still copies them. The regression it rejects is a mark that reads the word and
/// the blanks before it, which puts the selection away when the reply ends.
#[test]
fn a_reply_with_a_block_becoming_an_entry_keeps_a_selection_from_its_header_down() {
    let reply: String = format!(
        "Here:\n```sh\nls -la\n```\n{}",
        (1..=6)
            .map(|n| format!("reply {n:02}\n"))
            .collect::<String>()
    );
    for scrolling in [false, true] {
        let _held = crate::theme::exclusive();
        let mut session = Session::new("none");
        session
            .transcript
            .push(crate::state::Entry::user("an earlier prompt"));
        session.status = Status::Working;
        session.streaming(&reply);
        if scrolling {
            session.open_scroller();
        }
        drawn_at(&mut session, 120, 24);
        let buffer = drawn_at(&mut session, 120, 24);
        assert!(
            cells_of(&buffer, "copy").is_empty(),
            "scrolling {scrolling}: the word was drawn before the reply ended"
        );
        let (header, _) = find_cell(&buffer, "sh");
        let (row, column) = find_cell(&buffer, "reply 03");
        swept(&mut session, (header, 2), (row, column + 8));
        let swept_over = highlighted(&drawn_at(&mut session, 120, 24));
        assert!(
            swept_over.len() > 4,
            "scrolling {scrolling}: {swept_over:?}"
        );

        session.narrate(reply.trim_end());
        let after = drawn_at(&mut session, 120, 24);
        assert_eq!(
            cells_of(&after, "copy").len(),
            1,
            "scrolling {scrolling}: the word was not drawn once the reply ended"
        );
        assert_eq!(
            highlighted(&after),
            swept_over,
            "scrolling {scrolling}: the selection was put away or moved"
        );
        let handed = let_go(&mut session, (120, 24), row, column + 8);
        let [copied] = handed.as_slice() else {
            panic!("scrolling {scrolling}: the clipboard was handed {handed:?}");
        };
        assert!(
            copied.starts_with("sh\n") && copied.ends_with("reply 03"),
            "scrolling {scrolling}: {copied:?}"
        );
        assert!(
            !copied.contains("copy"),
            "scrolling {scrolling}: {copied:?}"
        );
    }
}

/// MOUSE-34 and MOUSE-28: a prompt queued behind the turn is drawn under the box and not among
/// the transcript's lines, so putting one there leaves a selection of the transcript held on its
/// words, though the room the transcript has shrinks by the rows it takes. The info panel's
/// glyph, the `back` label of a delegate's view and the rows of the transcript's own lines are
/// drawn beside those lines and not in them, which is why none of them is a case here.
#[test]
fn a_prompt_queued_under_the_box_leaves_a_selection_of_the_transcript_held() {
    let _held = crate::theme::exclusive();
    let mut session = a_long_reply(40);
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));
    assert!(session.a_turn_is_running());
    let buffer = drawn_at(&mut session, 120, 24);
    let (row, column) = find_cell(&buffer, "row 30");
    swept(&mut session, (row, column), (row + 1, column + 6));
    let swept_over = highlighted(&drawn_at(&mut session, 120, 24));
    assert_eq!(swept_over.len(), 2, "{swept_over:?}");

    wait_as(&mut session, "second");
    assert_eq!(waiting_prompts(&session), vec!["second"]);
    let after = drawn_at(&mut session, 120, 24);
    assert!(
        session.holds_a_selection(),
        "the queued prompt put the selection away"
    );
    // The badge on the queued prompt is painted in the ink a selection is, so it is read too.
    let marked: Vec<String> = highlighted(&after)
        .into_iter()
        .filter(|row| !row.contains("QUEUED"))
        .collect();
    assert_eq!(marked, swept_over);
}

/// MOUSE-34: a search in the scroller draws over the text it matches, splitting the spans that
/// hold it, and that is a change of what is drawn and not of the text, so a selection held while
/// a search runs and is cleared is kept.
#[test]
fn a_search_over_a_selection_does_not_drop_it() {
    let _held = crate::theme::exclusive();
    let mut session = a_long_reply(40);
    drawn_at(&mut session, 120, 24);
    session.open_scroller();
    let buffer = drawn_at(&mut session, 120, 24);
    let (row, column) = find_cell(&buffer, "row 30");
    swept(&mut session, (row, column), (row, column + 6));
    let swept_over = vec!["row 30".to_string()];
    assert_eq!(highlighted(&drawn_at(&mut session, 120, 24)), swept_over);

    handle_key(&mut session, key(KeyCode::Char('/')));
    for c in "of the".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_key(&mut session, key(KeyCode::Enter));
    let buffer = drawn_at(&mut session, 120, 24);
    assert!(
        session.selection.is_some(),
        "the search dropped the selection"
    );
    let (searched, _) = find_cell(&buffer, "row 30");
    assert_eq!(
        highlighted(&buffer).first().map(String::as_str),
        Some("row 30"),
        "row {searched}"
    );
    let handed = let_go(&mut session, (120, 24), searched, column + 6);
    assert_eq!(handed, vec!["row 30".to_string()]);
}

/// MOUSE-34: the opening screen is the mark with the face on it, which glances about from frame to
/// frame, so what is selected under it stays selected. When the first entry ends the screen its
/// rows hold other text, and the selection goes.
#[test]
fn a_selection_on_the_opening_screen_goes_when_the_opening_ends() {
    let _held = crate::theme::exclusive();
    let mut session = Session::new("none");
    drawn_at(&mut session, 120, 30);
    let buffer = drawn_at(&mut session, 120, 30);
    let (row, column) = find_cell(&buffer, "Ask a question");
    swept(&mut session, (row, column), (row, column + 14));
    let swept_over = vec!["Ask a question".to_string()];
    for _ in 0..3 {
        assert_eq!(highlighted(&drawn_at(&mut session, 120, 30)), swept_over);
    }

    let reply: String = (1..=40).map(|n| format!("reply {n:02}\n")).collect();
    session
        .transcript
        .push(crate::state::Entry::assistant(reply, Vec::new()));
    let marked = highlighted(&drawn_at(&mut session, 120, 30));
    assert!(
        marked.is_empty() || marked == swept_over,
        "the frame marked {marked:?}"
    );
    let handed = let_go(&mut session, (120, 30), row, column + 14);
    assert_swept_or_nothing(&handed, "Ask a question", "the opening screen");
    assert!(session.selection.is_none());
}

/// MOUSE-34: the model's thinking is drawn above its reply while it is shown, and is taken down
/// when the round ends and the reply becomes an entry, which moves the reply's rows up. A
/// selection of the reply is dropped then, and what is copied is what was swept or nothing.
#[test]
fn thinking_taken_down_above_a_selection_does_not_leave_it_over_other_text() {
    let reply: String = (1..=12).map(|n| format!("reply {n:02}\n")).collect();
    for scrolling in [false, true] {
        let _held = crate::theme::exclusive();
        let mut session = Session::new("none");
        session
            .transcript
            .push(crate::state::Entry::user("an earlier prompt"));
        session.toggle_thinking();
        session.status = Status::Working;
        session.thinking("first thought\nsecond thought\nthird thought");
        session.streaming(&reply);
        drawn_at(&mut session, 120, 30);
        if scrolling {
            session.open_scroller();
        }
        let buffer = drawn_at(&mut session, 120, 30);
        let (row, column) = find_cell(&buffer, "reply 03");
        swept(&mut session, (row, column), (row, column + 8));
        let swept_over = vec!["reply 03".to_string()];
        assert_eq!(highlighted(&drawn_at(&mut session, 120, 30)), swept_over);

        session.narrate(reply.trim_end());
        let buffer = drawn_at(&mut session, 120, 30);
        assert!(
            !(0..30).any(|row| {
                (0..120)
                    .map(|column| buffer[(column, row)].symbol().to_string())
                    .collect::<String>()
                    .contains("second thought")
            }),
            "the thinking is still drawn"
        );
        let marked = highlighted(&buffer);
        assert!(
            marked.is_empty() || marked == swept_over,
            "scrolling: {scrolling}: the frame marked {marked:?}"
        );
        let handed = let_go(&mut session, (120, 30), row, column + 8);
        assert_swept_or_nothing(&handed, "reply 03", &format!("scrolling: {scrolling}"));
        assert!(session.selection.is_none(), "scrolling: {scrolling}");
    }
}

/// MOUSE-34: the scroller's keys move the view and the selection stays on its words.
#[test]
fn the_scrollers_keys_leave_the_selection_on_the_text_it_was_put_on() {
    let _held = crate::theme::exclusive();
    let mut session = a_long_reply(60);
    drawn_at(&mut session, 120, 24);
    session.open_scroller();
    let buffer = drawn_at(&mut session, 120, 24);
    let (row, column) = find_cell(&buffer, "row 40");
    swept(&mut session, (row, column), (row + 1, column + 6));
    let swept_over = vec!["row 40 of the reply".to_string(), "  row 41".to_string()];
    assert_eq!(highlighted(&drawn_at(&mut session, 120, 24)), swept_over);

    handle_key(&mut session, key(KeyCode::Char('k')));
    let buffer = drawn_at(&mut session, 120, 24);
    assert_eq!(highlighted(&buffer), swept_over);
    assert_eq!(find_cell(&buffer, "row 40").0, row + 1);

    handle_key(&mut session, ctrl('u'));
    let buffer = drawn_at(&mut session, 120, 24);
    assert_eq!(highlighted(&buffer), swept_over);
    assert_eq!(find_cell(&buffer, "row 40").0, row + 1 + 11);
}

/// MOUSE-34: rows that a turn adds below a selection move the view when it follows the tail,
/// and the highlight with it, and leave both alone in the scroller, which holds the view.
#[test]
fn rows_a_turn_adds_below_do_not_move_the_selection_off_its_text() {
    let _held = crate::theme::exclusive();
    for scrolling in [false, true] {
        let mut session = a_long_reply(60);
        session.status = Status::Working;
        drawn_at(&mut session, 120, 24);
        if scrolling {
            session.open_scroller();
        }
        let buffer = drawn_at(&mut session, 120, 24);
        let (row, column) = find_cell(&buffer, "row 50");
        swept(&mut session, (row, column), (row, column + 6));
        let swept_over = vec!["row 50".to_string()];
        assert_eq!(highlighted(&drawn_at(&mut session, 120, 24)), swept_over);

        session.transcript.push(crate::state::Entry::assistant(
            "later one\nlater two",
            Vec::new(),
        ));
        let buffer = drawn_at(&mut session, 120, 24);
        assert_eq!(
            highlighted(&buffer),
            swept_over,
            "scrolling: {scrolling}: the selection left its words"
        );
        let moved = find_cell(&buffer, "row 50").0;
        if scrolling {
            assert_eq!(moved, row, "the scroller's view moved");
        } else {
            assert_eq!(moved + 3, row, "the view did not follow the tail");
        }
    }
}

/// MOUSE-3: opening or closing the info panel changes the width the transcript wraps at, so the
/// rows a selection of it holds are rows of other text. It is dropped by the frame that is drawn
/// at the new width, in both directions, and a release after it copies nothing.
#[test]
fn toggling_the_panel_drops_a_selection_of_the_transcript_it_would_leave_over_other_text() {
    let mut session = Session::new("none");
    session.laid.columns = 120;
    session.complete(
        "the quick brown fox jumps over the lazy dog and then keeps on running through the \
             field until it reaches the far fence beside the old stone wall at the edge of the farm \
             where the grass is long",
        Vec::new(),
        0,
    );
    for opening in [true, false] {
        if !opening {
            session.toggle_panel();
        }
        let buffer = drawn_at(&mut session, 120, 24);
        let (row, column) = find_cell(&buffer, "the quick");
        swept(&mut session, (row, column), (row + 2, 30));
        assert!(session.selection.is_some());

        session.toggle_panel();
        drawn_at(&mut session, 120, 24);
        assert!(
            session.selection.is_none(),
            "opening: {opening}: the selection stayed over text that was wrapped again"
        );
        let up = drag(MouseEventKind::Up(MouseButton::Left), row + 2, 30);
        assert_eq!(handle_mouse(&mut session, up), Action::Copy);
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 24)).expect("terminal");
        let mut handed = Vec::new();
        copy_selection(&mut terminal, &mut session, |text| {
            handed.push(text.to_string());
            true
        })
        .expect("copy");
        assert!(handed.is_empty(), "opening: {opening}: copied {handed:?}");
        if opening {
            session.toggle_panel();
        }
    }
}

/// MOUSE-3: a resize in the middle of a drag, in either direction or both, drops the selection,
/// and what the drag does afterwards neither panics nor copies cells that hold other text.
#[test]
fn a_resize_in_the_middle_of_a_drag_leaves_nothing_to_extend_or_copy() {
    for resized in [(100, 24), (120, 20), (90, 15), (200, 60), (30, 5), (1, 1)] {
        let mut session = a_long_reply(40);
        let buffer = drawn_at(&mut session, 120, 24);
        let (row, column) = find_cell(&buffer, "row 30");
        handle_mouse(
            &mut session,
            drag(MouseEventKind::Down(MouseButton::Left), row, column),
        );
        handle_mouse(
            &mut session,
            drag(MouseEventKind::Drag(MouseButton::Left), row + 1, column + 8),
        );
        drawn_at(&mut session, resized.0, resized.1);
        assert!(
            session.selection.is_none(),
            "{resized:?}: the drag survived"
        );

        let handed = {
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(resized.0, resized.1))
                    .expect("terminal");
            let mut handed = Vec::new();
            for event in [
                drag(
                    MouseEventKind::Drag(MouseButton::Left),
                    row + 2,
                    column + 20,
                ),
                drag(MouseEventKind::Up(MouseButton::Left), row + 2, column + 20),
            ] {
                answer_mouse(&mut terminal, &mut session, event, |text| {
                    handed.push(text.to_string());
                    true
                })
                .expect("answered");
            }
            handed
        };
        assert!(handed.is_empty(), "{resized:?}: copied {handed:?}");
    }
}

/// MOUSE-34: a transcript that has fewer rows than a selection reaches no longer has the text
/// the selection was put on, so the selection goes; one that still has every row it reaches
/// stays, and copies what it did.
#[test]
fn a_transcript_that_shrinks_drops_a_selection_that_reached_what_went() {
    let mut session = Session::new("none");
    session
        .transcript
        .push(crate::state::Entry::user("first prompt"));
    session
        .transcript
        .push(crate::state::Entry::assistant("first answer", Vec::new()));
    session
        .transcript
        .push(crate::state::Entry::user("second prompt"));
    session
        .transcript
        .push(crate::state::Entry::assistant("second answer", Vec::new()));
    let buffer = drawn_at(&mut session, 120, 24);
    let (early, early_column) = find_cell(&buffer, "first prompt");
    let (late, late_column) = find_cell(&buffer, "second answer");

    swept(
        &mut session,
        (early, early_column),
        (early, early_column + 5),
    );
    swept(&mut session, (late, late_column), (late, late_column + 6));
    assert!(session.selection.is_some());
    session.transcript.truncate(2);
    drawn_at(&mut session, 120, 24);
    assert!(
        session.selection.is_none(),
        "the selection stayed over rows the transcript no longer has"
    );

    swept(
        &mut session,
        (early, early_column),
        (early, early_column + 5),
    );
    session.transcript.truncate(2);
    drawn_at(&mut session, 120, 24);
    let selection = session.selection.expect("the rows are still there");
    assert!(selection.covers(early, early_column));
}

/// MOUSE-34: a terminal with one row, or one column, draws what little it can, and a mouse
/// report anywhere on it, or at the far corner of the coordinates, does not panic.
#[test]
fn a_terminal_one_row_high_takes_a_mouse_report_anywhere_without_panicking() {
    for size in [(80, 1), (1, 1), (1, 24), (3, 2), (40, 3)] {
        for scrolling in [false, true] {
            let mut session = a_long_reply(30);
            drawn_at(&mut session, 120, 24);
            if scrolling {
                session.open_scroller();
            }
            drawn_at(&mut session, size.0, size.1);
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(size.0, size.1))
                    .expect("terminal");
            let corners = [0, 1, size.1.saturating_sub(1), size.1, u16::MAX - 1];
            for row in corners {
                for column in [0, size.0.saturating_sub(1), size.0, u16::MAX - 1] {
                    // A drag, and then a triple click and a drag from it, which take whole words
                    // and rows and read the cells there.
                    let down = drag(MouseEventKind::Down(MouseButton::Left), row, column);
                    let up = drag(MouseEventKind::Up(MouseButton::Left), row, column);
                    for event in [
                        down,
                        drag(MouseEventKind::Drag(MouseButton::Left), 0, 0),
                        drag(MouseEventKind::Drag(MouseButton::Left), row, column),
                        up,
                        down,
                        up,
                        down,
                        up,
                        down,
                        drag(MouseEventKind::Drag(MouseButton::Left), 0, 0),
                        up,
                    ] {
                        answer_mouse(&mut terminal, &mut session, event, |_| true)
                            .expect("answered");
                        drawn_at(&mut session, size.0, size.1);
                    }
                }
            }
        }
    }
}

/// MOUSE-34: a drag that began with the view scrolled back selects the rows under the pointer
/// then, and a release hands over those rows and not the ones that were at those screen rows when
/// the view was at the tail.
#[test]
fn a_drag_that_began_with_the_view_scrolled_back_selects_the_rows_under_it() {
    let mut session = a_long_reply(60);
    drawn_at(&mut session, 120, 24);
    for _ in 0..4 {
        wheel_up(&mut session);
    }
    let buffer = drawn_at(&mut session, 120, 24);
    let (row, column) = find_cell(&buffer, "row 40");
    let (last, _) = find_cell(&buffer, "row 42");
    let handed = dragged_and_released(&mut session, (120, 24), (row, column), (last, column + 6));
    assert_eq!(
        handed,
        vec!["row 40 of the reply\n  row 41 of the reply\n  row 42".to_string()]
    );
}

/// MOUSE-34: expanding a call with `x` changes how many rows it takes, so the rows below it
/// move and a selection held over them would be over other text. It is dropped.
#[test]
fn expanding_a_call_drops_a_selection_held_in_the_scroller() {
    let mut session = scroller_over_one_call();
    let buffer = drawn_at(&mut session, 80, 11);
    let (row, column) = find_cell(&buffer, "out 3");
    swept(&mut session, (row, column), (row, column + 5));
    assert!(session.selection.is_some());

    handle_key(&mut session, key(KeyCode::Char('x')));
    assert!(
        render::as_text(&session).contains("out 29"),
        "the call was not expanded"
    );
    assert!(session.selection.is_none());
}

/// MOUSE-34: the scroller counts some calls' rows differently from the plain view, so a selection
/// held when it opens holds rows of other text there. It is dropped, with no info panel to have
/// changed the width.
#[test]
fn opening_the_scroller_drops_a_selection_held_in_the_plain_view() {
    let mut session = a_long_reply(60);
    let buffer = drawn_at(&mut session, 120, 24);
    let (row, column) = find_cell(&buffer, "row 50");
    swept(&mut session, (row, column), (row, column + 6));
    assert!(session.selection.is_some());
    session.open_scroller();
    drawn_at(&mut session, 120, 24);
    assert!(session.selection.is_none());
}

/// MOUSE-34: the button is let go before the frame that shows a wheel turn has been drawn. What
/// is copied is the text that was swept, not the text now on the rows it was on.
#[test]
fn a_release_straight_after_a_wheel_turn_copies_the_text_that_was_swept() {
    let mut session = a_long_reply(60);
    let buffer = drawn_at(&mut session, 120, 24);
    let (row, column) = find_cell(&buffer, "row 50");
    swept(&mut session, (row, column), (row, column + 6));
    wheel_up(&mut session);

    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 24)).expect("terminal");
    let mut handed = Vec::new();
    answer_mouse(
        &mut terminal,
        &mut session,
        drag(MouseEventKind::Up(MouseButton::Left), row, column + 6),
        |text| {
            handed.push(text.to_string());
            true
        },
    )
    .expect("answered");
    assert_eq!(handed, vec!["row 50".to_string()]);
}

/// The scroller draws the transcript on every row but the footer, so a terminal that is shorter
/// by itself changes which rows are in view without changing a column. The selection goes.
#[test]
fn a_height_only_resize_in_the_scroller_drops_the_selection() {
    let mut session = a_long_reply(60);
    drawn_at(&mut session, 120, 24);
    session.open_scroller();
    let buffer = drawn_at(&mut session, 120, 24);
    let (row, column) = find_cell(&buffer, "row 50");
    swept(&mut session, (row, column), (row, column + 6));
    assert!(session.selection.is_some());
    drawn_at(&mut session, 120, 20);
    assert!(session.selection.is_none());
}

/// What `row_range` rows of a reply `row 00` to `row N` read, as a copy of them from the start
/// of the first to the end of the sixth character of the last gives them.
fn the_reply_rows(first: usize, last: usize) -> String {
    let mut text = format!("row {first:02} of the reply");
    for row in first + 1..last {
        text.push_str(&format!("\n  row {row:02} of the reply"));
    }
    text.push_str(&format!("\n  row {last:02}"));
    text
}

/// MOUSE-35: a selection that reaches past the screen copies every row it reaches, in order, the
/// ones scrolled out of view as much as the ones in it. The ends are set by a press, the wheel
/// and a shift-click, so no row of it was ever on the screen together with both ends.
#[test]
fn a_selection_longer_than_the_screen_copies_in_full() {
    for scrolling in [false, true] {
        let mut session = a_long_reply(80);
        drawn_at(&mut session, 120, 24);
        if scrolling {
            session.open_scroller();
        }
        let buffer = drawn_at(&mut session, 120, 24);
        let (row, column) = find_cell(&buffer, "row 79");
        handle_mouse(
            &mut session,
            drag(MouseEventKind::Down(MouseButton::Left), row, column + 6),
        );
        handle_mouse(
            &mut session,
            drag(MouseEventKind::Drag(MouseButton::Left), row, column + 6),
        );
        for _ in 0..16 {
            wheel_up(&mut session);
        }
        let buffer = drawn_at(&mut session, 120, 24);
        let (first, column) = find_cell(&buffer, "row 30");
        press(&mut session, first, column, KeyModifiers::SHIFT);

        let handed = let_go(&mut session, (120, 24), first, column);
        assert_eq!(
            handed,
            vec![the_reply_rows(30, 79)],
            "scrolling: {scrolling}"
        );
    }
}

/// MOUSE-35: the rows copied are the ones that were swept even when every one of them has
/// scrolled out of view by the time the button is let go, and none of the panel beside the
/// transcript is in them.
#[test]
fn a_selection_wholly_out_of_view_copies_its_rows_and_nothing_of_the_panel() {
    let mut session = a_long_reply(80);
    session.laid.columns = 120;
    session.toggle_panel();
    session.identify("PANELMARK", "/work/dir".to_string(), None);
    let buffer = drawn_at(&mut session, 120, 24);
    let (row, column) = find_cell(&buffer, "row 70");
    swept(&mut session, (row, column), (row + 3, column + 6));
    for _ in 0..12 {
        wheel_up(&mut session);
    }
    let buffer = drawn_at(&mut session, 120, 24);
    assert!(
        !(0..buffer.area.height).any(|r| (0..buffer.area.width)
            .map(|c| buffer[(c, r)].symbol().to_string())
            .collect::<String>()
            .contains("row 70")),
        "the rows swept are still in view"
    );
    let handed = let_go(&mut session, (120, 24), row, column);
    assert_eq!(handed, vec![the_reply_rows(70, 73)]);
}

/// What a transcript's rows read as when they are drawn on the screen and when they are drawn on
/// their own, row by row, for the rows in view.
fn the_rows_in_view_read_alike(session: &mut Session, size: (u16, u16), what: &str) {
    let buffer = drawn_at(session, size.0, size.1);
    let window = session.laid.window.expect("the transcript was drawn");
    let rows = render::Rows::new(session, window);
    let rendered = rows.window(window.top, window.top + window.area.height - 1);
    for at in 0..window.area.height {
        let read = |buffer: &ratatui::buffer::Buffer, row: u16| {
            (window.area.left()..window.area.right())
                .map(|column| crate::hyperlink::shown(buffer[(column, row)].symbol()))
                .collect::<String>()
                .trim_end()
                .to_string()
        };
        assert_eq!(
            read(&rendered.buffer, at),
            read(&buffer, window.area.top() + at),
            "{what}: row {} of the transcript",
            window.top + at
        );
    }
}

/// A transcript with a prompt, a quarantined result with an escape in it, a reply with a
/// heading, wrapped paragraphs, a list, a code block and wide characters.
fn a_varied_transcript() -> Session {
    use bravebot_agent::report::{Activity, Landing, Reach, Shown};
    let mut session = Session::new("kernel-enforced");
    session.transcript.push(crate::state::Entry::user(
        "explain the layout of the repository in detail, naming every directory and what it holds",
    ));
    session.finish_activity(Activity::running("Read", "README.md").done("40 lines"));
    session.landed(Landing::Quarantined);
    session.show(Shown {
        as_the_planner_had_it: None,
        origin: "README.md".to_string(),
        reach: Reach::NotThePlanner,
        label: "(U,priv)".to_string(),
        preview: vec![
            "the haystack holds a needle that is long enough to wrap onto another row".to_string(),
            "behind \u{1b}[31m\u{1b}[2J an escape".to_string(),
        ],
        lines: 2,
    });
    session.complete(
        "# A heading\n\nA paragraph that is long enough to need wrapping by the transcript at \
             forty columns, with \u{65e5}\u{672c}\u{8a9e} in it.\n\n- the first item\n- a second \
             item that is long enough to wrap as well\n\n```\ncode line one\ncode line two   \n```\n\n\
             the last words",
        Vec::new(),
        0,
    );
    session
}

/// MOUSE-35: a selection of the whole transcript, taken from its first row to its last in a view
/// a screenful of rows tall, reads a wrapped line as one line and a code block without the blanks
/// that pad it, for the rows that were never on the screen at all as for the ones that were.
#[test]
fn a_whole_transcript_copies_as_the_lines_it_is_made_of() {
    let mut session = a_varied_transcript();
    drawn_at(&mut session, 40, 12);
    session.open_scroller();
    drawn_at(&mut session, 40, 12);
    assert!(render::as_text(&session).contains("code line two"));
    handle_key(&mut session, key(KeyCode::Char('g')));
    let buffer = drawn_at(&mut session, 40, 12);
    let (row, column) = find_cell(&buffer, "> explain");
    handle_mouse(
        &mut session,
        drag(MouseEventKind::Down(MouseButton::Left), row, column),
    );
    handle_mouse(
        &mut session,
        drag(MouseEventKind::Drag(MouseButton::Left), row, column + 2),
    );
    handle_key(&mut session, key(KeyCode::Char('G')));
    let buffer = drawn_at(&mut session, 40, 12);
    let (last, _) = find_cell(&buffer, "the last words");
    press(&mut session, last, 39, KeyModifiers::SHIFT);

    let handed = let_go(&mut session, (40, 12), last, 39);
    let [copied] = handed.as_slice() else {
        panic!("the clipboard was handed {handed:?}");
    };
    let lines: Vec<&str> = copied.lines().collect();
    assert!(
        lines
            .iter()
            .any(|line| line.contains("A paragraph that is long enough to need wrapping by the transcript at forty columns, with \u{65e5}\u{672c}\u{8a9e} in it.")),
        "{copied}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.ends_with("a second item that is long enough to wrap as well")),
        "{copied}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.trim_start() == "code line two"),
        "{copied}"
    );
    assert!(
        lines.iter().all(|line| line.trim_end() == *line),
        "{copied}"
    );
    let at = |needle: &str| {
        lines
            .iter()
            .position(|line| line.contains(needle))
            .expect(needle)
    };
    assert!(at("> explain") < at("A heading"));
    assert!(at("A heading") < at("code line one"));
    assert!(at("code line one") < at("the last words"));
}

/// A drag across a line of wide characters in the box hands over each character once, as the
/// copy of the transcript does, so the line is pasted as it was typed.
#[test]
fn a_drag_over_wide_characters_in_the_box_copies_them_once() {
    let (mut session, buffer) = typing("\u{65e5}\u{672c}\u{8a9e} end");
    let (row, column) = find_cell(&buffer, "\u{65e5}");
    // The last column of the box is its border, which the drag stops short of.
    let handed = dragged_and_released(&mut session, (120, 24), (row, column), (row, 118));
    assert_eq!(handed, vec!["\u{65e5}\u{672c}\u{8a9e} end".to_string()]);
}

/// The number `NN` of the `row NN` that the row `at` of the drawn frame shows.
fn the_reply_row_on(buffer: &ratatui::buffer::Buffer, at: u16) -> usize {
    let text: String = (0..buffer.area.width)
        .map(|column| buffer[(column, at)].symbol().to_string())
        .collect();
    let found = text
        .find("row ")
        .unwrap_or_else(|| panic!("no row of the reply on {text:?}"));
    text[found + 4..found + 6].parse().expect("a row number")
}

/// MOUSE-37: a drag held below the transcript scrolls it on and extends the selection to the
/// row that comes into view, each step, until the button is let go. What is copied runs from the
/// row the drag began on to the last row in view then, which is more than one screen of rows.
#[test]
fn a_drag_held_below_the_transcript_scrolls_it_and_extends_the_selection() {
    for scrolling in [false, true] {
        let mut session = a_long_reply(80);
        drawn_at(&mut session, 120, 24);
        if scrolling {
            session.open_scroller();
        }
        for _ in 0..12 {
            wheel_up(&mut session);
        }
        let buffer = drawn_at(&mut session, 120, 24);
        let pane = the_transcript_pane(&session);
        let (row, column) = find_cell(&buffer, "row 40");
        handle_mouse(
            &mut session,
            drag(MouseEventKind::Down(MouseButton::Left), row, column),
        );
        handle_mouse(
            &mut session,
            drag(MouseEventKind::Drag(MouseButton::Left), pane.bottom(), 8),
        );
        assert!(session.pulling_at_an_edge(), "scrolling: {scrolling}");

        let start = Instant::now();
        for step in 0..25 {
            assert!(
                session.scroll_toward_the_pointer(at_a_step(start, step)),
                "scrolling: {scrolling}: step {step}"
            );
            drawn_at(&mut session, 120, 24);
        }
        let buffer = drawn_at(&mut session, 120, 24);
        let last = the_reply_row_on(&buffer, pane.bottom() - 1);
        assert!(
            last - 40 + 1 > usize::from(pane.height),
            "scrolling: {scrolling}: the selection is not longer than the screen"
        );

        let handed = let_go(&mut session, (120, 24), pane.bottom(), 8);
        assert_eq!(
            handed,
            vec![the_reply_rows(40, last)],
            "scrolling: {scrolling}"
        );
    }
}

/// MOUSE-37: the same above the transcript, where the first row of the screen is the edge since a
/// pointer dragged out of the window is reported at the nearest cell inside it.
#[test]
fn a_drag_held_on_the_first_row_scrolls_the_transcript_back_and_extends_the_selection() {
    for scrolling in [false, true] {
        let mut session = a_long_reply(80);
        drawn_at(&mut session, 120, 24);
        if scrolling {
            session.open_scroller();
        }
        let buffer = drawn_at(&mut session, 120, 24);
        let pane = the_transcript_pane(&session);
        let (row, column) = find_cell(&buffer, "row 70");
        handle_mouse(
            &mut session,
            drag(MouseEventKind::Down(MouseButton::Left), row, column + 6),
        );
        handle_mouse(
            &mut session,
            drag(MouseEventKind::Drag(MouseButton::Left), 0, column),
        );
        assert_eq!(pane.top(), 0);

        let start = Instant::now();
        for step in 0..25 {
            assert!(
                session.scroll_toward_the_pointer(at_a_step(start, step)),
                "scrolling: {scrolling}: step {step}"
            );
            drawn_at(&mut session, 120, 24);
        }
        let buffer = drawn_at(&mut session, 120, 24);
        let first = the_reply_row_on(&buffer, 0);
        assert!(70 - first + 1 > usize::from(pane.height), "{first}");

        let handed = let_go(&mut session, (120, 24), 0, column);
        assert_eq!(
            handed,
            vec![the_reply_rows(first, 70)],
            "scrolling: {scrolling}"
        );
    }
}

/// A session scrolled back from the tail of a long reply, with a selection begun on "row 40" and
/// dragged on to `to`, and the frame drawn.
fn dragged_from_row_40(to: (u16, u16)) -> Session {
    let mut session = a_long_reply(80);
    drawn_at(&mut session, 120, 24);
    for _ in 0..12 {
        wheel_up(&mut session);
    }
    let buffer = drawn_at(&mut session, 120, 24);
    let (row, column) = find_cell(&buffer, "row 40");
    swept(&mut session, (row, column), to);
    drawn_at(&mut session, 120, 24);
    session
}

/// MOUSE-37: the view moves only while the left button is held and has moved a selection of the
/// transcript, and the pointer is past an edge. A pointer inside the transcript, a press that
/// has not moved, a button let go, a move with no button, another button and a window that lost
/// focus each leave it where it is, with a pointer that would otherwise have scrolled it.
#[test]
fn nothing_scrolls_unless_a_held_drag_is_past_an_edge_of_the_transcript() {
    let outside = the_transcript_pane(&dragged_from_row_40((5, 5))).bottom();
    let scrolled = |session: &mut Session| session.scroll_toward_the_pointer(Instant::now());

    // Inside the transcript.
    let mut session = dragged_from_row_40((5, 5));
    let before = session.scroll;
    assert!(!scrolled(&mut session));
    assert_eq!(session.scroll, before);

    // A press that has not moved, with the pointer outside: it is a click.
    let mut session = dragged_from_row_40((5, 5));
    handle_mouse(
        &mut session,
        drag(MouseEventKind::Down(MouseButton::Left), outside, 8),
    );
    assert!(!scrolled(&mut session));

    type Ending = fn(&mut Session, u16);
    let ends: [(&str, Ending); 5] = [
        ("the button is let go", |session, row| {
            handle_mouse(session, drag(MouseEventKind::Up(MouseButton::Left), row, 8));
        }),
        ("a move with no button", |session, row| {
            handle_mouse(session, drag(MouseEventKind::Moved, row, 8));
        }),
        ("another button", |session, row| {
            handle_mouse(
                session,
                drag(MouseEventKind::Down(MouseButton::Right), row, 8),
            );
        }),
        ("a drag of another button", |session, row| {
            handle_mouse(
                session,
                drag(MouseEventKind::Drag(MouseButton::Middle), row, 8),
            );
        }),
        ("the window loses focus", |session, _| {
            took_input(session, &TermEvent::FocusLost);
        }),
    ];
    for (what, end) in ends {
        let mut session = dragged_from_row_40((outside, 8));
        assert!(
            session.pulling_at_an_edge(),
            "{what}: the fixture was not pulling"
        );
        end(&mut session, outside);
        assert!(!session.pulling_at_an_edge(), "{what}");
        assert!(!scrolled(&mut session), "{what}");
    }

    // A drag that began in the info panel or the box has no rows of the transcript to scroll,
    // with rows of it above and below the view to scroll to.
    let mut session = a_long_reply(80);
    session.laid.columns = 120;
    session.toggle_panel();
    session.identify("PANELMARK", "/work/dir".to_string(), None);
    session.paste("hello world");
    drawn_at(&mut session, 120, 24);
    for _ in 0..6 {
        wheel_up(&mut session);
    }
    let buffer = drawn_at(&mut session, 120, 24);
    assert!(session.rows_below() > 0 && session.top_row() > 0);
    for (needle, name) in [("PANELMARK", "the panel"), ("hello", "the box")] {
        let (row, column) = find_cell(&buffer, needle);
        swept(&mut session, (row, column), (row, column + 3));
        for past in [0, 23] {
            handle_mouse(
                &mut session,
                drag(MouseEventKind::Drag(MouseButton::Left), past, column + 3),
            );
            assert!(!session.pulling_at_an_edge(), "{name}: row {past}");
        }
    }
}

/// MOUSE-37: a view that cannot go the way the pointer is has nothing to scroll: at the first row
/// a drag on the first row of the screen does nothing, and at the tail a drag below does nothing.
#[test]
fn a_view_at_the_end_the_drag_is_past_stays_where_it_is() {
    let mut session = a_long_reply(80);
    let buffer = drawn_at(&mut session, 120, 24);
    let pane = the_transcript_pane(&session);
    let (row, column) = find_cell(&buffer, "row 70");
    swept(&mut session, (row, column), (pane.bottom(), 8));
    assert!(!session.pulling_at_an_edge(), "the view is at the tail");
    assert!(!session.scroll_toward_the_pointer(Instant::now()));
    assert_eq!(session.scroll, 0);

    session.scroller_to_first_row();
    let buffer = drawn_at(&mut session, 120, 24);
    assert_eq!(session.top_row(), 0);
    let (row, column) = find_cell(&buffer, "row 05");
    swept(&mut session, (row, column), (0, 8));
    assert!(
        !session.pulling_at_an_edge(),
        "the view is at the first row"
    );
    assert!(!session.scroll_toward_the_pointer(Instant::now()));
}

/// MOUSE-37: a step is owed for each tick that has passed, so a loop that wakes less often scrolls
/// as far, up to a few; two calls at one instant take one step; and the further past the edge the
/// pointer, the more rows a step moves, up to four.
#[test]
fn the_steps_owed_follow_the_time_and_the_distance_past_the_edge() {
    let from_the_pane = |past: u16| {
        let pane = the_transcript_pane(&dragged_from_row_40((5, 5)));
        dragged_from_row_40((pane.bottom() + past - 1, 8))
    };
    let top = |session: &Session| session.top_row();
    let start = Instant::now();

    let mut session = from_the_pane(1);
    let before = top(&session);
    assert!(session.scroll_toward_the_pointer(start));
    assert_eq!(top(&session), before + 1, "the first step is taken at once");
    assert!(!session.scroll_toward_the_pointer(start));
    assert!(!session.scroll_toward_the_pointer(start + select::EDGE_TICK / 2));
    assert!(session.scroll_toward_the_pointer(at_a_step(start, 1)));
    assert_eq!(top(&session), before + 2);

    // A pass that came back after ten ticks makes up three.
    assert!(session.scroll_toward_the_pointer(at_a_step(start, 11)));
    assert_eq!(top(&session), before + 5);

    for (past, rows) in [(2, 2), (3, 3), (4, 4), (9, 4)] {
        let mut session = from_the_pane(past);
        let before = top(&session);
        assert!(session.scroll_toward_the_pointer(start));
        assert_eq!(top(&session), before + rows, "{past} rows past the edge");
    }
}

/// MOUSE-37: while a turn runs and writes under a view that is held, a drag held below the
/// transcript scrolls toward the tail and stops there. At the tail the view follows what the turn
/// writes, and the end of the selection stays on the last row in view, so what is copied reaches
/// the rows the turn went on to add.
#[test]
fn a_drag_held_below_a_transcript_a_turn_is_writing_under_follows_it_to_the_tail() {
    let mut session = a_long_reply(80);
    session.status = Status::Working;
    drawn_at(&mut session, 120, 24);
    for _ in 0..4 {
        wheel_up(&mut session);
    }
    let buffer = drawn_at(&mut session, 120, 24);
    let pane = the_transcript_pane(&session);
    let (row, column) = find_cell(&buffer, "row 60");
    // The row under the one the button is drawn in, which is below the transcript at the tail
    // as well (VIEW-45).
    let below = pane.bottom() + 1;
    swept(&mut session, (row, column), (below, 8));

    let start = Instant::now();
    let mut steps = 0;
    while session.scroll_toward_the_pointer(at_a_step(start, steps)) {
        steps += 1;
        drawn_at(&mut session, 120, 24);
        assert!(steps < 20, "the view never reached the tail");
    }
    assert_eq!(session.scroll, 0, "the view stopped short of the tail");
    drawn_at(&mut session, 120, 24);

    // The turn writes two rows more, and the view follows them to the tail.
    session.transcript.push(crate::state::Entry::assistant(
        "later one\nlater two",
        Vec::new(),
    ));
    drawn_at(&mut session, 120, 24);
    assert!(
        session.scroll_toward_the_pointer(at_a_step(start, steps + 1)),
        "the end of the selection stayed on the row it was on"
    );
    assert!(!session.scroll_toward_the_pointer(at_a_step(start, steps + 2)));

    let handed = let_go(&mut session, (120, 24), below, 8);
    let [copied] = handed.as_slice() else {
        panic!("the clipboard was handed {handed:?}");
    };
    assert!(
        copied.starts_with(&format!("{} of the reply\n", the_reply_rows(60, 79))),
        "{copied:?}"
    );
    assert!(copied.contains("later two"), "{copied:?}");
}

/// MOUSE-38: Escape puts a selection held away and does nothing else, so the box keeps the line
/// in it for the press after, which is the one that clears it. Whatever the style of the box.
#[test]
fn escape_puts_a_held_selection_away_before_it_clears_the_box() {
    for editing in [crate::vim::Editing::Ordinary, crate::vim::Editing::Vi] {
        let mut session = with_a_selection_held();
        session.choose_editing(editing);
        session.paste("half a thought");

        assert_eq!(handle_key(&mut session, key(KeyCode::Esc)), Action::Redraw);
        assert!(session.selection.is_none(), "{editing:?}");
        assert_eq!(session.copied, None, "{editing:?}: the count stayed up");
        assert_eq!(session.input(), "half a thought", "{editing:?}");
        if editing == crate::vim::Editing::Vi {
            assert_eq!(
                session.vi_mode(),
                Some(crate::vim::Mode::Insert),
                "the press also left INSERT"
            );
        }

        handle_key(&mut session, key(KeyCode::Esc));
        if editing == crate::vim::Editing::Ordinary {
            assert_eq!(
                session.input(),
                "",
                "the second press did not clear the box"
            );
        } else {
            assert_eq!(session.vi_mode(), Some(crate::vim::Mode::Normal));
        }
    }
}

/// MOUSE-38: with a turn running the first Escape puts the selection away and the turn goes on,
/// and the second stops it, in the loops that read the keys against `stops_the_turn`. Ctrl-C is
/// not taken by the selection, and stops the turn on the first press.
#[test]
fn escape_puts_a_held_selection_away_before_it_stops_a_turn_and_the_next_stops_it() {
    let mut session = with_a_selection_held();
    session.status = Status::Working;
    let escape = key(KeyCode::Esc);

    assert!(
        !stops_the_turn(&session, escape),
        "the selection did not take the key"
    );
    assert_eq!(
        handle_key_while_working(&mut session, escape),
        Action::Redraw
    );
    assert!(session.selection.is_none());
    assert_eq!(session.status, Status::Working);
    assert!(
        stops_the_turn(&session, escape),
        "the turn could not be stopped"
    );

    let session = with_a_selection_held();
    assert!(
        stops_the_turn(&session, ctrl('c')),
        "the selection took Ctrl-C"
    );
    assert!(session.holds_a_selection());
}

/// MOUSE-38: the same order over the modes that stand over the box, which each have an Escape of
/// their own: the scroller closes on the press after the selection goes, a history search too.
#[test]
fn escape_puts_a_held_selection_away_before_a_mode_takes_it() {
    // The scroller.
    let mut session = a_long_reply(40);
    drawn_at(&mut session, 120, 24);
    session.open_scroller();
    let buffer = drawn_at(&mut session, 120, 24);
    let (row, column) = find_cell(&buffer, "row 30");
    dragged_and_released(&mut session, (120, 24), (row, column), (row, column + 6));
    assert!(session.holds_a_selection());
    handle_key(&mut session, key(KeyCode::Esc));
    assert!(session.selection.is_none() && session.scrolling());
    handle_key(&mut session, key(KeyCode::Esc));
    assert!(!session.scrolling());

    // A history search, with the mouse captured as it is unless the settings turn it off. The
    // search holds the mouse (MOUSE-47), so the selection is the one held when it opened. It is
    // drawn with no scrollbar (VIEW-39), so the transcript keeps its width across it, and the
    // selection with it, only where it had none either: a transcript that fits its pane. The
    // selection is marked under the search, and is the first thing Escape puts away.
    let mut session = Session::new("none");
    session.complete("row 30 of the reply".to_string(), Vec::new(), 0);
    session.history =
        crate::history::History::from_entries(vec![bravebot_session::store::Entry::sent(
            "old one", None,
        )]);
    assert!(session.mouse_captured());
    let buffer = drawn_at(&mut session, 120, 24);
    let (row, column) = find_cell(&buffer, "row 30");
    dragged_and_released(&mut session, (120, 24), (row, column), (row, column + 6));
    assert!(session.open_history_search());
    let buffer = drawn_at(&mut session, 120, 24);
    assert!(session.holds_a_selection());
    assert_eq!(
        highlighted(&buffer),
        ["row 30"],
        "the selection held under the search is not marked"
    );
    handle_key(&mut session, key(KeyCode::Esc));
    assert!(session.selection.is_none() && session.searching_history());
    handle_key(&mut session, key(KeyCode::Esc));
    assert!(!session.searching_history());
}

/// MOUSE-38: a press that swept nothing holds no selection, so Escape does what it always did,
/// and a picker opened over a selection takes the key before it.
#[test]
fn escape_is_not_taken_by_a_click_and_a_picker_takes_it_before_a_selection() {
    let (mut session, buffer) = two_panes();
    session.paste("half a thought");
    let (row, column) = find_cell(&buffer, "TRANSCRIPTMARK");
    press(&mut session, row, column, KeyModifiers::NONE);
    assert!(session.selection.is_some() && !session.holds_a_selection());
    handle_key(&mut session, key(KeyCode::Esc));
    assert_eq!(session.input(), "", "Escape did not clear the box");

    let mut session = with_a_selection_held();
    session.open_picking(crate::picking::Picking::Effort(
        crate::effort_prompt::Picker::new(None),
    ));
    handle_key(&mut session, key(KeyCode::Esc));
    assert!(session.picking().is_none(), "the picker was not closed");
    assert!(session.holds_a_selection(), "the selection took the key");
}

/// MOUSE-2: the files state draws the panel's border where the panel's own state does, and a
/// press on it begins no selection, clears the one held, and copies nothing on release. A press
/// on the cell after it is on the panel, and begins one.
#[test]
fn a_press_on_the_panels_border_starts_no_selection_in_the_files_state() {
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
    session.toggle_files();
    assert!(session.files_open(), "the files state did not open");
    drawn_at(&mut session, 120, 24);
    let panel = session
        .laid
        .panes
        .iter()
        .find(|(pane, _)| *pane == crate::select::Pane::Panel)
        .expect("the files state records the panel")
        .1;
    let row = panel.y + 1;

    // One held in the transcript is put away by the press.
    swept(&mut session, (0, 0), (0, 5));
    assert!(session.selection.is_some());
    press(&mut session, row, panel.x, KeyModifiers::NONE);
    assert!(
        session.selection.is_none(),
        "the border press began a selection"
    );
    assert!(!session.dragging_panel(), "the files state is not resized");
    let up = drag(MouseEventKind::Up(MouseButton::Left), row, panel.x);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);
    assert!(let_go(&mut session, (120, 24), row, panel.x).is_empty());

    // And the next cell, the first of the panel's own.
    press(&mut session, row, panel.x + 1, KeyModifiers::NONE);
    assert_eq!(
        session.selection.map(|selection| selection.pane()),
        Some(crate::select::Pane::Panel)
    );
}

/// The text of the footer row of a drawn frame.
fn the_footer_of(buffer: &ratatui::buffer::Buffer) -> String {
    let row = buffer.area.height - 1;
    (0..buffer.area.width)
        .map(|column| buffer[(column, row)].symbol().to_string())
        .collect()
}

/// MOUSE-39: through the release, the notice names the pane a drag in the panel, the box or the
/// footer began in, keeps the count of what was copied, and says nothing of the pane for the
/// transcript. A reply copied with `/copy` after one of them does not name it either, and a click
/// takes the notice down with the name.
#[test]
fn the_notice_of_a_copy_names_the_pane_it_was_taken_from() {
    let (mut session, _) = two_panes();
    session.paste("text in the box");
    let buffer2 = drawn_at(&mut session, 120, 24);

    let (row, column) = find_cell(&buffer2, "PANELMARK");
    let handed = dragged_and_released(&mut session, (120, 24), (row, column), (row, 119));
    assert_eq!(handed, vec!["PANELMARK".to_string()]);
    let footer = the_footer_of(&drawn_at(&mut session, 120, 24));
    assert!(
        footer.contains("9 chars to clipboard, from the panel"),
        "{footer}"
    );

    let (row, column) = find_cell(&buffer2, "text in");
    dragged_and_released(&mut session, (120, 24), (row, column), (row, column + 7));
    let footer = the_footer_of(&drawn_at(&mut session, 120, 24));
    assert!(
        footer.contains("7 chars to clipboard, from the box"),
        "{footer}"
    );

    let footer_row = 23;
    let (_, column) = find_cell(&buffer2, "? for shortcuts");
    dragged_and_released(
        &mut session,
        (120, 24),
        (footer_row, column),
        (footer_row, column + 5),
    );
    let footer = the_footer_of(&drawn_at(&mut session, 120, 24));
    assert!(
        footer.contains("5 chars to clipboard, from the footer"),
        "{footer}"
    );

    let (row, column) = find_cell(&buffer2, "TRANSCRIPTMARK");
    dragged_and_released(&mut session, (120, 24), (row, column), (row, column + 6));
    let footer = the_footer_of(&drawn_at(&mut session, 120, 24));
    assert!(footer.contains("6 chars to clipboard"), "{footer}");
    assert!(!footer.contains("from the"), "{footer}");

    // A reply copied with /copy after a copy from the panel does not carry the panel's name.
    let (row, column) = find_cell(&buffer2, "PANELMARK");
    dragged_and_released(&mut session, (120, 24), (row, column), (row, 119));
    copy_reply(&mut session, "a reply", |_| true);
    let footer = the_footer_of(&drawn_at(&mut session, 120, 24));
    assert!(footer.contains("7 chars to clipboard"), "{footer}");
    assert!(!footer.contains("from the"), "{footer}");

    // And a click takes the notice down altogether.
    let (row, column) = find_cell(&buffer2, "PANELMARK");
    dragged_and_released(&mut session, (120, 24), (row, column), (row, 119));
    click(&mut session, 0, 0);
    let footer = the_footer_of(&drawn_at(&mut session, 120, 24));
    assert!(!footer.contains("to clipboard"), "{footer}");
}

/// MOUSE-5: the info panel draws a long session id over two rows and a copy of the panel reads
/// them as one line. The transcript beside it is drawn on the same rows, and its lines are not
/// joined there: they are the lines their author broke.
#[test]
fn a_copy_of_the_transcript_beside_the_session_id_keeps_its_lines_apart() {
    let mut session = a_long_reply(40);
    session.laid.columns = 120;
    session.toggle_panel();
    session.name_id(Some("d23f56fe-8328-429c-b51b-74cb8431957b"));
    let buffer = drawn_at(&mut session, 120, 24);
    let panel = session
        .laid
        .panes
        .iter()
        .find(|(pane, _)| *pane == crate::select::Pane::Panel)
        .expect("panel recorded")
        .1;
    let joined = crate::panel::continued_rows(&session, panel);
    assert!(!joined.is_empty(), "the id is not drawn over two rows");

    let first = joined[0];
    let (top, column) = (first.saturating_sub(1), 0);
    let handed = dragged_and_released(&mut session, (120, 24), (top, column), (first + 2, 119));
    let [copied] = handed.as_slice() else {
        panic!("the clipboard was handed {handed:?}");
    };
    let _ = buffer;
    assert_eq!(copied.lines().count(), 4, "{copied:?}");
    assert!(
        copied.lines().all(|line| line.contains("of the reply")),
        "{copied:?}"
    );
}

/// MOUSE-1 and MOUSE-47: a history search holds the mouse, so a drag in it marks nothing and a
/// release copies nothing, which is what a drag that marked nothing copies. A press above the
/// panel closes the search and begins no selection, and a drag over a prompt of the list selects
/// none of it. The regression it rejects is a drag in the search that is a selection beneath a
/// panel that has the input.
#[test]
fn a_drag_in_a_history_search_begins_no_selection_and_copies_nothing() {
    let _held = crate::theme::exclusive();
    let mut session = a_long_reply(40);
    session.history = crate::history::History::from_entries(vec![
        bravebot_session::store::Entry::sent("old one", None),
        bravebot_session::store::Entry::sent("another prompt", None),
    ]);
    drawn_at(&mut session, 120, 24);
    assert!(session.open_history_search());
    let buffer = drawn_at(&mut session, 120, 24);

    let (row, column) = find_cell(&buffer, "row 30");
    swept(&mut session, (row, column), (row + 1, column + 6));
    assert!(
        session.selection.is_none(),
        "a drag above the search began a selection"
    );
    assert!(
        !session.searching_history(),
        "a press above the search did not close it"
    );
    assert!(highlighted(&drawn_at(&mut session, 120, 24)).is_empty());
    assert!(let_go(&mut session, (120, 24), row + 1, column + 6).is_empty());

    assert!(session.open_history_search());
    let buffer = drawn_at(&mut session, 120, 24);
    let (row, column) = find_cell(&buffer, "another prompt");
    swept(&mut session, (row, column), (row, column + 7));
    assert!(
        session.selection.is_none(),
        "a drag over a prompt began a selection"
    );
    assert!(
        session.searching_history(),
        "a press on a prompt closed the search"
    );
    assert!(highlighted(&drawn_at(&mut session, 120, 24)).is_empty());
    assert!(let_go(&mut session, (120, 24), row, column + 7).is_empty());
}

/// MOUSE-35: the same row, read off the screen where it is in view and from the transcript's own
/// rendering where it is not, gives the same text, at every position of the view, in the plain
/// view and in the scroller, with and without the panel beside it, and at two widths.
#[test]
fn a_row_reads_the_same_on_the_screen_and_off_it() {
    for (size, panel) in [((40, 16), false), ((120, 16), false), ((120, 16), true)] {
        for scrolling in [false, true] {
            let mut session = a_varied_transcript();
            session.laid.columns = size.0;
            if panel {
                session.toggle_panel();
            }
            drawn_at(&mut session, size.0, size.1);
            if scrolling {
                session.open_scroller();
            }
            drawn_at(&mut session, size.0, size.1);
            for step in 0..30 {
                the_rows_in_view_read_alike(
                    &mut session,
                    size,
                    &format!("{size:?} panel {panel} scrolling {scrolling} step {step}"),
                );
                if scrolling {
                    session.scroller_back(2);
                } else {
                    session.scroll_up(2);
                }
            }
        }
    }
}

/// Dragging past the bottom of the transcript stops at its last row, which is above the box. With
/// the view scrolled back the last row of the pane is a row of text, and at the tail the last row
/// that holds text is the one above the blank row that closes the transcript.
#[test]
fn a_drag_past_the_bottom_stops_at_the_last_row_of_the_pane() {
    let (mut session, _) = two_panes();
    for n in 0..30 {
        session
            .transcript
            .push(crate::state::Entry::user(format!("row {n}")));
    }
    drawn_at(&mut session, 120, 24);
    let transcript = session.laid.panes[0].1;

    swept(&mut session, (0, 0), (23, 10));
    let selection = session.selection.expect("selection");
    assert!(selection.covers(transcript.bottom() - 2, 0));
    assert!(!selection.covers(transcript.bottom() - 1, 0));

    // The row the button returning to the tail is drawn in is the last row of the pane, and is no
    // row of the transcript to select (VIEW-45).
    wheel_up(&mut session);
    drawn_at(&mut session, 120, 24);
    let transcript = session.laid.panes[0].1;
    assert_eq!(transcript.bottom(), session.laid.jump.expect("a button").y);
    swept(&mut session, (0, 0), (23, 10));
    let selection = session.selection.expect("selection");
    assert!(selection.covers(transcript.bottom() - 1, 0));
    assert!(!selection.covers(transcript.bottom(), 0));
}

/// Closing the panel while a selection of it is held leaves nothing selected.
#[test]
fn closing_the_panel_drops_a_selection_inside_it() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let (mut session, _) = two_panes();
    let panel = session.laid.panes.last().expect("panel").1;
    swept(
        &mut session,
        (panel.top(), panel.left() + 1),
        (panel.top() + 1, panel.left() + 5),
    );
    assert!(session.selection.is_some());
    session.toggle_panel();
    let mut terminal = Terminal::new(TestBackend::new(120, 24)).expect("terminal");
    let mut laid = crate::state::Laid::default();
    terminal
        .draw(|frame| laid = render::draw(frame, &session))
        .expect("draw");
    session.note_layout(laid);
    assert!(session.selection.is_none());
}

/// The cells a selection named hold other text once the terminal changes width.
#[test]
fn a_width_change_drops_the_selection() {
    let (mut session, _) = two_panes();
    swept(&mut session, (0, 0), (0, 10));
    assert!(session.selection.is_some());
    let mut laid = session.laid.clone();
    laid.columns = 110;
    session.note_layout(laid);
    assert!(session.selection.is_none());
}

/// The same for a terminal that is shorter, drawn at the new size.
#[test]
fn a_height_change_drops_the_selection() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let (mut session, _) = two_panes();
    swept(&mut session, (0, 0), (0, 10));
    assert!(session.selection.is_some());
    let mut terminal = Terminal::new(TestBackend::new(120, 20)).expect("terminal");
    let mut laid = crate::state::Laid::default();
    terminal
        .draw(|frame| laid = render::draw(frame, &session))
        .expect("draw");
    session.note_layout(laid);
    assert!(session.selection.is_none());
}

/// A press where the layout recorded no region starts nothing.
#[test]
fn a_press_outside_every_pane_starts_no_selection() {
    let (mut session, _) = two_panes();
    swept(&mut session, (0, 0), (0, 10));
    assert!(session.selection.is_some());
    session
        .laid
        .panes
        .retain(|(pane, _)| *pane != crate::select::Pane::Hint);
    let row = 23;
    handle_mouse(
        &mut session,
        drag(MouseEventKind::Down(MouseButton::Left), row, 3),
    );
    assert!(session.selection.is_none());
}

/// The sequence has to leave a terminal that keeps one tracking state in the mode that
/// reports drags, or the wheel stops reaching the session and scrolls the window behind it
/// instead. What is asked for last is what such a terminal ends up in.
#[test]
fn the_mouse_request_ends_in_the_mode_that_reports_a_drag() {
    assert!(
        TRACK_MOTION_ONLY_WHILE_DRAGGING.ends_with("\x1b[?1002h"),
        "a terminal with one tracking state would be left without drags"
    );
    assert!(
        TRACK_MOTION_ONLY_WHILE_DRAGGING.starts_with("\x1b[?1003l"),
        "all-motion reporting was never turned off"
    );
    assert!(
        TRACK_MOTION_ONLY_WHILE_DRAGGING.contains("\x1b[?1000h"),
        "button reporting, which carries the wheel, was not asked for again"
    );
}

/// A pointer crossing the window is not input. Terminals report that motion by default, and
/// answering each report with a redraw is what made a drag crawl: the queue outran the
/// drawing and the highlight trailed behind the pointer.
#[test]
fn moving_the_pointer_without_a_button_asks_for_nothing() {
    let mut session = Session::new("none");
    session.begin_selection(2, 2);
    session.extend_selection(2, 8);

    let action = handle_mouse(&mut session, drag(MouseEventKind::Moved, 9, 40));

    assert_eq!(action, Action::None, "a bare move asked for work");
    let selection = session.selection.expect("the selection was disturbed");
    assert!(
        selection.covers(2, 4),
        "the selection moved with the pointer"
    );
}

/// Selecting has nothing to do with the turn: what is on the screen is already there, and a
/// long turn is exactly when someone wants to copy part of it.
#[test]
fn selecting_works_while_a_turn_is_running() {
    let mut session = Session::new("none");
    session.type_char('x');
    session.submit();
    assert_eq!(session.status, Status::Working);

    handle_mouse(
        &mut session,
        drag(MouseEventKind::Down(MouseButton::Left), 1, 0),
    );
    handle_mouse(
        &mut session,
        drag(MouseEventKind::Drag(MouseButton::Left), 1, 5),
    );
    assert!(session.selection.is_some());
}

/// MOUSE-4: `tui.mouse` reaches the session through `adopt_settings`. The editor and the pager
/// take the terminal back with the session's choice, so a session that kept the default after
/// its settings turned capture off would capture the mouse again after the first round trip.
#[test]
fn adopting_settings_puts_the_mouse_choice_in_force() {
    let off = bravebot_config::Settings::parse(r#"{"tui": {"mouse": false}}"#);
    let on = bravebot_config::Settings::parse(r#"{"tui": {"mouse": true}}"#);
    let unset = bravebot_config::Settings::parse("{}");

    let mut turned_off = Session::new("none");
    adopt_settings(&mut turned_off, &off, None);
    let mut turned_on = Session::new("none");
    adopt_settings(&mut turned_on, &on, None);
    let mut unnamed = Session::new("none");
    adopt_settings(&mut unnamed, &unset, None);

    assert!(
        !turned_off.mouse_captured(),
        "tui.mouse off was not adopted"
    );
    assert!(turned_on.mouse_captured());
    assert!(unnamed.mouse_captured(), "an absent key turned capture off");
}

/// MOUSE-4, MOUSE-52, BACKEND-24: a session started in a checkout whose settings file has a `tui`
/// block keeps the person's mouse capture and right-click menu off, whether the block names the
/// wheel only or turns either on, and one where the person stated nothing or `true` still
/// captures the mouse and opens the menu.
#[test]
fn a_checkouts_tui_block_does_not_turn_the_mouse_or_the_menu_back_on() {
    let root = crate::testutil::scratch_dir("bravebot-app-checkout-tui");
    let _ = std::fs::remove_dir_all(&root);
    let home = root.join("home");
    let cwd = root.join("cwd");
    std::fs::create_dir_all(&home).expect("scratch home");
    std::fs::create_dir_all(cwd.join(".bravebot")).expect("scratch project");
    std::fs::create_dir_all(cwd.join(".git")).expect("scratch checkout");
    let session_in = |home_text: &str, project_text: &str| {
        std::fs::write(home.join("settings.json"), home_text).expect("write the home layer");
        std::fs::write(cwd.join(".bravebot").join("settings.json"), project_text)
            .expect("write the project layer");
        let settings = bravebot_config::Settings::layered(Some(home.clone()), Some(&cwd), None);
        let mut session = Session::new("none");
        adopt_settings(&mut session, &settings, None);
        (session.mouse_captured(), session.context_menu_on())
    };
    let off = r#"{"tui": {"mouse": false, "contextMenu": false, "wheelRows": 5}}"#;

    assert_eq!(
        session_in(off, r#"{"tui": {"wheelRows": 3}}"#),
        (false, false),
        "a block naming only the wheel"
    );
    assert_eq!(
        session_in(off, r#"{"tui": {"mouse": true, "contextMenu": true}}"#),
        (false, false),
        "a block turning both on"
    );
    assert_eq!(
        session_in("{}", r#"{"tui": {"mouse": false, "contextMenu": false}}"#),
        (false, false),
        "a checkout turning both off"
    );
    assert_eq!(
        session_in("{}", r#"{"tui": {"mouse": true}}"#),
        (true, true),
        "over nothing"
    );
    assert_eq!(
        session_in(
            r#"{"tui": {"mouse": true, "contextMenu": true}}"#,
            r#"{"tui": {"wheelRows": 3}}"#
        ),
        (true, true),
        "the person's own true"
    );
}
