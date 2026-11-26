//! Clicks on what the transcript draws: calls, delegates, reply markers, fenced blocks and the way back to the tail.

use super::*;

/// MOUSE-9: a click on a call's row in the scroller expands it as `x` does, a second click
/// collapses it, and neither leaves a selection or a copy behind.
#[test]
fn a_click_on_a_call_in_the_scroller_expands_and_collapses_it() {
    let mut session = scroller_over_one_call();
    let (row, column) = find_on_text(&session, "out 0");
    assert!(!render::as_text(&session).contains("out 29"));

    assert_eq!(click(&mut session, row, column), Action::Redraw);
    assert!(render::as_text(&session).contains("out 29"));
    assert!(session.selection.is_none());

    let (row, column) = find_on_text(&session, "out 3");
    assert_eq!(click(&mut session, row, column), Action::Redraw);
    assert!(!render::as_text(&session).contains("out 29"));
}

/// A click on a row that belongs to no call, and a drag that begins on one, expand nothing:
/// the drag is a selection and is copied as it always was.
#[test]
fn a_click_off_a_call_or_a_drag_over_one_expands_nothing() {
    let mut session = scroller_over_one_call();
    let (row, column) = find_on_text(&session, "out 0");
    let below = session.laid.height - 1;
    assert_ne!(below, row);
    assert_eq!(click(&mut session, below, 0), Action::Copy);
    assert!(!render::as_text(&session).contains("out 29"));

    swept(&mut session, (row, column), (row, column + 3));
    assert_eq!(
        handle_mouse(
            &mut session,
            drag(MouseEventKind::Up(MouseButton::Left), row, column + 3)
        ),
        Action::Copy
    );
    assert!(!render::as_text(&session).contains("out 29"));
}

/// With the key list open the click is on the list, which covers the call.
#[test]
fn a_click_with_the_scroller_help_open_expands_nothing() {
    let mut session = scroller_over_one_call();
    let (row, column) = find_on_text(&session, "out 0");
    session.toggle_scroller_help();
    click(&mut session, row, column);
    assert!(!render::as_text(&session).contains("out 29"));
}

/// A delegate's view draws its calls as one row, so a click where a call was is not on one.
#[test]
fn a_click_in_a_delegates_view_expands_nothing() {
    use bravebot_agent::report::{DelegateId, Delegation};
    let mut session = scroller_over_one_call();
    let (row, column) = find_on_text(&session, "out 0");
    let id = DelegateId::nth(1);
    session.delegate_started(Delegation {
        stop: Default::default(),
        id,
        kind: "reader".to_string(),
        task: "find the parser".to_string(),
    });
    assert!(session.watch());
    assert!(session.watched_delegate().is_some());

    click(&mut session, row, column);

    assert!(session.stop_watching());
    assert!(!render::as_text(&session).contains("out 29"));
}

/// MOUSE-10: a click on a delegate's block in the scroller opens the view of that delegate and
/// not of another, and the key that closes the view returns to the scroller.
#[test]
fn a_click_on_a_delegate_in_the_scroller_opens_that_delegates_view() {
    let mut session = scroller_over_two_delegates();
    let (row, column) = find_on_text(&session, "write the docs");

    assert_eq!(click(&mut session, row, column), Action::Redraw);

    let delegate = session.watched_delegate().expect("a delegate's view");
    assert_eq!(delegate.task, "write the docs");
    assert!(session.watching_a_delegate());
    assert!(session.selection.is_none());
    assert!(session.stop_watching());
    assert!(session.scrolling());
}

/// MOUSE-10 and MOUSE-23: a click opens a delegate's view without the list, so Escape from it
/// goes back to where the click was, the scroller or the conversation, and not to a list of the
/// delegates the person never opened.
#[test]
fn escape_from_a_clicked_delegate_returns_to_where_the_click_was() {
    let mut scrolling = scroller_over_two_delegates();
    let (row, column) = find_on_text(&scrolling, "write the docs");
    click(&mut scrolling, row, column);
    assert!(scrolling.watching_a_delegate(), "the click opened no view");

    handle_key(&mut scrolling, key(KeyCode::Esc));
    assert!(
        scrolling.watching().is_none(),
        "escape did not close the view: listing {}",
        scrolling.listing_delegates()
    );
    assert!(scrolling.scrolling(), "escape left the scroller");

    let (mut resting, buffer) = at_rest_over_two_delegates();
    let (row, column) = find_on(&buffer, "write the docs");
    click(&mut resting, row, column);
    assert!(resting.watching_a_delegate(), "the click opened no view");

    handle_key(&mut resting, key(KeyCode::Esc));
    assert!(
        resting.watching().is_none(),
        "escape did not close the view: listing {}",
        resting.listing_delegates()
    );
    assert!(!resting.scrolling());
}

/// MOUSE-10 and WATCH-7: the watch key opens from the scroller the view the click opens, the
/// list where there are several delegates and the one where there is one, and the key that
/// closes the view returns to the scroller. It does nothing where the session has started none.
/// The regression it rejects is the key answered nowhere in the scroller, which leaves the click
/// the only way to a delegate's view from there.
#[test]
fn the_watch_key_opens_the_delegates_from_the_scroller_and_closing_returns_to_it() {
    let mut session = scroller_over_two_delegates();
    assert_eq!(handle_key(&mut session, ctrl('l')), Action::Redraw);
    assert!(session.listing_delegates(), "no list of the delegates");
    assert!(session.scrolling(), "the scroller was left behind");
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert!(session.watching_a_delegate(), "no delegate's view");
    assert_eq!(handle_key(&mut session, ctrl('l')), Action::Redraw);
    assert!(session.watching().is_none(), "the view did not close");
    assert!(session.scrolling(), "closing the view left the scroller");

    let mut none = scroller_over_one_call();
    assert_eq!(handle_key(&mut none, ctrl('l')), Action::None);
    assert!(none.watching().is_none());
    assert!(none.scrolling());
}

/// A drag that begins on a delegate is a selection, and a click beside it opens nothing.
#[test]
fn a_drag_over_a_delegate_or_a_click_off_one_opens_no_view() {
    let mut session = scroller_over_two_delegates();
    let (row, column) = find_on_text(&session, "write the docs");
    swept(&mut session, (row, column), (row, column + 3));
    handle_mouse(
        &mut session,
        drag(MouseEventKind::Up(MouseButton::Left), row, column + 3),
    );
    assert!(session.watching().is_none());

    let (call, column) = find_on_text(&session, "out 0");
    click(&mut session, call, column);
    assert!(session.watching().is_none());
}

/// With the key list open the click is on the list, which covers the delegate.
#[test]
fn a_click_with_the_scroller_help_open_opens_no_delegate() {
    let mut session = scroller_over_two_delegates();
    let (row, column) = find_on_text(&session, "write the docs");
    session.toggle_scroller_help();
    click(&mut session, row, column);
    assert!(session.watching().is_none());
}

/// MOUSE-23. A delegate is numbered from one in every turn, so a click on the block of the second
/// turn's delegate opens that delegate's view and not that of the first turn's, which has the
/// same number. The regression it rejects is a view found by the number the block holds, which
/// names the first delegate of any turn that started one. Both blocks are tried, so a view that
/// opens the last by number is not enough.
#[test]
fn a_click_opens_the_delegate_of_the_block_when_two_turns_number_theirs_alike() {
    for task in ["first turn task", "second turn task"] {
        let (mut session, buffer) = at_rest_over_a_delegate_in_each_of_two_turns();
        let (row, column) = find_on(&buffer, task);
        assert_eq!(click(&mut session, row, column), Action::Redraw);
        assert_eq!(
            session
                .watched_delegate()
                .map(|delegate| delegate.task.as_str()),
            Some(task)
        );
    }
}

/// MOUSE-23: at rest a click on a delegate's block opens the view of that delegate and not of
/// the one a row above it, and the view closes as the one the watch key opens does.
#[test]
fn a_click_on_a_delegate_at_rest_opens_that_delegates_view() {
    let (mut session, buffer) = at_rest_over_two_delegates();
    assert!(!session.scrolling());
    let (row, column) = find_on(&buffer, "write the docs");

    assert_eq!(click(&mut session, row, column), Action::Redraw);

    let delegate = session.watched_delegate().expect("a delegate's view");
    assert_eq!(delegate.task, "write the docs");
    assert!(session.watching_a_delegate());
    assert!(session.selection.is_none());
    assert!(session.stop_watching());
    assert!(!session.scrolling());

    let (first, column) = find_on(&buffer, "find the parser");
    assert_eq!(click(&mut session, first, column), Action::Redraw);
    assert_eq!(
        session.watched_delegate().map(|d| d.task.as_str()),
        Some("find the parser")
    );
}

/// MOUSE-23: a turn running is when a delegate is there to open, and the click reaches it
/// through the loop that runs while the turn does, which carries out nothing but a copy and so
/// has to find the view already open.
#[test]
fn a_click_on_a_delegate_while_a_turn_runs_opens_its_view() {
    use bravebot_agent::report::{DelegateId, Delegation};
    let mut session = Session::new("none");
    session.type_char('a');
    session.submit();
    assert!(session.a_turn_is_running());
    session.delegate_started(Delegation {
        stop: Default::default(),
        id: DelegateId::nth(1),
        kind: "reader".to_string(),
        task: "find the parser".to_string(),
    });
    let buffer = drawn(&mut session);
    let (row, column) = find_on(&buffer, "find the parser");

    assert_eq!(click(&mut session, row, column), Action::Redraw);
    assert_eq!(
        session.watched_delegate().map(|d| d.task.as_str()),
        Some("find the parser")
    );
}

/// MOUSE-23: the row between two blocks, the info panel on a row of a block, a drag from a
/// block, a double click on it and a click on the delegate's own view open nothing, each
/// against a fixture where the plain click does open the view.
#[test]
fn a_click_off_a_delegate_or_a_drag_over_one_opens_no_view_at_rest() {
    let (mut session, buffer) = at_rest_over_two_delegates();
    let (row, column) = find_on(&buffer, "find the parser");
    let panel = session
        .laid
        .panes
        .iter()
        .find(|(pane, _)| *pane == crate::select::Pane::Panel)
        .expect("panel recorded")
        .1
        .x;

    // The blank row the two blocks are held apart by.
    assert_eq!(click(&mut session, row + 1, column), Action::Copy);
    assert!(session.watching().is_none());
    // The panel is drawn on the same rows, and is not the transcript.
    assert_ne!(click(&mut session, row, panel + 4), Action::Redraw);
    assert!(session.watching().is_none());

    swept(&mut session, (row, column), (row, column + 3));
    let up = drag(MouseEventKind::Up(MouseButton::Left), row, column + 3);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);
    assert!(session.watching().is_none());

    session.last_click = None;
    press(&mut session, row, column, KeyModifiers::NONE);
    press(&mut session, row, column, KeyModifiers::NONE);
    let up = drag(MouseEventKind::Up(MouseButton::Left), row, column);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);
    assert!(session.watching().is_none());

    // A press that began elsewhere and was let go on the block is not a click on it.
    session.clear_selection();
    session.last_click = None;
    press(&mut session, 0, 0, KeyModifiers::NONE);
    let up = drag(MouseEventKind::Up(MouseButton::Left), row, column);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);
    assert!(session.watching().is_none());

    session.last_click = None;
    session.clear_selection();
    assert_eq!(click(&mut session, row, column), Action::Redraw);
    assert!(
        session.watching_a_delegate(),
        "the fixture's click opened nothing"
    );
}

/// MOUSE-23: a picker, a history search or the files state stands over the transcript and takes
/// the mouse's attention, so a click where a block was opens no view.
#[test]
fn a_click_on_a_delegate_opens_no_view_where_another_mode_is_in_use() {
    type Setup = fn(&mut Session);
    let modes: [(&str, Setup); 3] = [
        ("a history search", |session| {
            session.history =
                crate::history::History::from_entries(vec![bravebot_session::store::Entry::sent(
                    "old one", None,
                )]);
            assert!(session.open_history_search(), "no search opened");
        }),
        ("the files state", |session| {
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
            session.toggle_files();
            assert!(session.files_open(), "the files state did not open");
        }),
        ("a picker", |session| {
            session.open_picking(crate::picking::Picking::Effort(
                crate::effort_prompt::Picker::new(None),
            ));
        }),
    ];
    for (name, setup) in modes {
        let (mut session, buffer) = at_rest_over_two_delegates();
        let (row, column) = find_on(&buffer, "find the parser");
        setup(&mut session);
        let buffer = drawn(&mut session);
        let (row_now, column_now) = find_on(&buffer, "find the parser");
        assert_eq!(
            (row_now, column_now),
            (row, column),
            "{name}: the block moved"
        );
        click(&mut session, row, column);
        assert!(
            session.watching().is_none(),
            "{name}: a click opened a view"
        );
    }
}

/// MOUSE-24: a click on the way back drawn in the delegate's header leaves the view, and the
/// transcript it was opened over is where it was left.
#[test]
fn a_click_on_the_back_label_leaves_a_delegates_view() {
    let (mut session, buffer) = a_finished_delegates_view();
    let (row, column) = find_cell(&buffer, "\u{2039} back");
    assert_eq!(row, 0, "the way back is in the header");

    assert_eq!(click(&mut session, row, column + 2), Action::Redraw);

    assert!(session.watching().is_none());
    assert!(session.selection.is_none());
    let after = drawn(&mut session);
    assert!(find_on(&after, "find the parser").0 > 0);
}

/// MOUSE-24: a click on the sentence that closes the view leaves it too, and a click on the
/// words under it, on the header's task or on the header's own words leaves it open.
#[test]
fn a_click_on_the_sentence_that_closes_a_delegates_view_leaves_it() {
    let (mut session, buffer) = a_finished_delegates_view();
    for needle in [
        "it is in src/parser.rs",
        "write the docs",
        "reader delegate d2",
    ] {
        let (row, column) = find_on(&buffer, needle);
        click(&mut session, row, column + 1);
        assert!(session.watching_a_delegate(), "a click on {needle:?} left");
    }
    let (row, column) = find_on(&buffer, "answered after 3 rounds");
    assert_eq!(click(&mut session, row, column + 5), Action::Redraw);
    assert!(session.watching().is_none());
}

/// MOUSE-24: a drag that begins on the way back, a double click on the sentence and a press
/// let go elsewhere are selections, and the view stays.
#[test]
fn a_drag_over_the_way_back_or_the_closing_sentence_leaves_the_view_open() {
    let (mut session, buffer) = a_finished_delegates_view();
    let (row, column) = find_cell(&buffer, "\u{2039} back");
    swept(&mut session, (row, column), (row, column + 3));
    let up = drag(MouseEventKind::Up(MouseButton::Left), row, column + 3);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);
    assert!(session.watching_a_delegate());

    let (closing, at) = find_on(&buffer, "answered after 3 rounds");
    session.last_click = None;
    press(&mut session, closing, at, KeyModifiers::NONE);
    press(&mut session, closing, at, KeyModifiers::NONE);
    let up = drag(MouseEventKind::Up(MouseButton::Left), closing, at);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);
    assert!(session.watching_a_delegate());

    session.clear_selection();
    session.last_click = None;
    press(&mut session, 5, 0, KeyModifiers::NONE);
    let up = drag(MouseEventKind::Up(MouseButton::Left), row, column);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);
    assert!(session.watching_a_delegate());

    session.clear_selection();
    session.last_click = None;
    assert_eq!(click(&mut session, row, column), Action::Redraw);
    assert!(
        session.watching().is_none(),
        "the fixture's click left nothing"
    );
}

/// MOUSE-24: the view a click in the scroller opened is left by a click on its closing
/// sentence, back to the scroller.
#[test]
fn a_click_on_the_closing_sentence_returns_to_the_scroller_it_was_opened_over() {
    use bravebot_agent::report::{DelegateId, Reported};
    let mut session = scroller_over_two_delegates();
    session.delegate_finished(
        DelegateId::nth(2),
        "a reader delegate answered after 3 rounds".to_string(),
        false,
        Some(Reported::Said("it is in src/parser.rs".to_string())),
    );
    session.note_layout(render::as_last_drawn(&session));
    let (row, column) = find_on_text(&session, "write the docs");
    click(&mut session, row, column);
    assert!(session.watching_a_delegate());
    session.last_click = None;
    session.note_layout(render::as_last_drawn(&session));

    let (row, column) = find_on_text(&session, "answered after 3 rounds");
    assert_eq!(click(&mut session, row, column + 2), Action::Redraw);
    assert!(session.watching().is_none());
    assert!(session.scrolling());
}

/// MOUSE-24: the label is the interface's own, so a drag along the header copies the words of
/// the header and not it.
#[test]
fn a_drag_along_the_header_copies_no_way_back() {
    let (mut session, buffer) = a_finished_delegates_view();
    swept(&mut session, (0, 0), (0, 119));
    let selection = session.selection.expect("a selection");
    let copied = selected_text(&session, &buffer, &selection);
    assert!(copied.contains("reader delegate d2"), "{copied:?}");
    assert!(!copied.contains("back"), "{copied:?}");
}

/// MOUSE-24: a header too narrow to hold the label beside its words draws none, and a click
/// where it would have been is not a way back.
#[test]
fn a_header_with_no_room_for_the_label_draws_none() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let (mut session, _) = a_finished_delegates_view();
    let mut terminal = Terminal::new(TestBackend::new(24, 12)).expect("terminal");
    let mut laid = crate::state::Laid::default();
    terminal
        .draw(|frame| laid = render::draw(frame, &session))
        .expect("draw");
    session.note_layout(laid);
    let on_the_screen: Vec<_> = session
        .laid
        .targets
        .iter()
        .filter(|target| target.space == crate::state::Space::Screen)
        .collect();
    assert!(on_the_screen.is_empty(), "{on_the_screen:?}");
    assert!(!render::as_text(&session).is_empty());
    let screen: String = (0..24)
        .map(|column| {
            terminal.backend().buffer()[(column, 0)]
                .symbol()
                .to_string()
        })
        .collect();
    assert!(!screen.contains("back"), "{screen:?}");
    click(&mut session, 0, 22);
    assert!(session.watching_a_delegate());
}

/// A delegate's view drawn `columns` wide, the width of the header's words, and the column
/// the way back is drawn at, or none when none is drawn.
fn header_at(columns: u16) -> (Session, usize, Option<u16>) {
    let (mut session, _) = a_finished_delegates_view();
    let buffer = drawn_at(&mut session, columns, 12);
    let cells: Vec<String> = (0..columns)
        .map(|column| buffer[(column, 0)].symbol().to_string())
        .collect();
    let label = cells.iter().position(|cell| cell == "\u{2039}");
    let words = cells[..label.unwrap_or(cells.len())]
        .concat()
        .trim_end()
        .chars()
        .count();
    (session, words, label.and_then(|at| u16::try_from(at).ok()))
}

/// MOUSE-24: the label is drawn when two blanks, the label and one blank after it fit beside
/// the words, and one column fewer draws none.
#[test]
fn the_label_is_drawn_when_two_blanks_and_one_more_cell_fit_beside_the_words() {
    let (_, words, label) = header_at(120);
    assert!(label.is_some(), "no label at 120 columns");
    let label = "\u{2039} back".chars().count();
    let exact = u16::try_from(words + 2 + label + 1).expect("a width");

    let (_, _, at_exact) = header_at(exact);
    assert_eq!(
        at_exact,
        Some(exact - 1 - u16::try_from(label).expect("a width")),
        "the label's column at the width that just fits"
    );
    let (_, _, one_short) = header_at(exact - 1);
    assert_eq!(one_short, None, "the label is drawn with a column too few");
}

/// MOUSE-24: the cells that answer as the way back are the label, the blank before it and the
/// last column after it, and the cell before the blank answers nothing.
#[test]
fn the_cells_that_answer_as_the_way_back_are_the_label_and_a_blank_either_side() {
    let (_, _, label) = header_at(120);
    let at = label.expect("the label");
    let wide = u16::try_from("\u{2039} back".chars().count()).expect("a width");
    for column in at - 1..=at + wide {
        let (mut session, _, _) = header_at(120);
        assert_eq!(
            click(&mut session, 0, column),
            Action::Redraw,
            "column {column} did not answer"
        );
        assert!(
            session.watching().is_none(),
            "column {column} left the view open"
        );
    }
    for column in [at - 2, at + wide + 1] {
        if column >= 120 {
            continue;
        }
        let (mut session, _, _) = header_at(120);
        click(&mut session, 0, column);
        assert!(
            session.watching_a_delegate(),
            "column {column} left the view"
        );
    }
}

/// MOUSE-24: a closing sentence that wraps over several rows answers on each of them.
#[test]
fn a_click_on_each_row_a_closing_sentence_wraps_over_leaves_the_view() {
    let (mut session, _) = a_finished_delegates_view();
    let buffer = drawn_at(&mut session, 16, 14);
    let rows_with = |word: &str| {
        (0..buffer.area.height)
            .filter(|&row| {
                let line: String = (0..16)
                    .map(|column| buffer[(column, row)].symbol().to_string())
                    .collect();
                line.contains(word)
            })
            .collect::<Vec<_>>()
    };
    let mut rows: Vec<u16> = ["answered", "rounds"]
        .iter()
        .flat_map(|word| rows_with(word))
        .collect();
    rows.sort_unstable();
    rows.dedup();
    assert!(
        rows.len() >= 2,
        "the sentence did not wrap at this width: {rows:?}"
    );
    for row in rows {
        let (mut session, _) = a_finished_delegates_view();
        drawn_at(&mut session, 16, 14);
        assert_eq!(click(&mut session, row, 1), Action::Redraw, "row {row}");
        assert!(session.watching().is_none(), "row {row} left the view open");
    }
}

/// MOUSE-25: at rest a click on a call that returned more than it draws opens the scroller on it
/// with the call expanded, and the heading stays on the row it was drawn on.
#[test]
fn a_click_on_a_call_at_rest_opens_the_scroller_on_it_expanded() {
    let (mut session, buffer) = at_rest_over_calls();
    assert!(!session.scrolling());
    let (row, column) = find_on(&buffer, "Run(make)");
    assert!(!buffer_text(&buffer).contains("out 13"));

    assert_eq!(click(&mut session, row + 2, column), Action::Redraw);

    assert!(session.scrolling());
    assert!(session.selection.is_none());
    let after = drawn_at(&mut session, 120, 40);
    assert!(
        buffer_text(&after).contains("out 13"),
        "the call was not expanded"
    );
    assert_eq!(find_on(&after, "Run(make)").0, row);

    // In the scroller a second click on the call collapses it, as it always did.
    session.last_click = None;
    let (row, column) = find_on(&after, "out 3");
    assert_eq!(click(&mut session, row, column), Action::Redraw);
    assert!(session.scrolling());
    let collapsed = drawn_at(&mut session, 120, 40);
    assert!(!buffer_text(&collapsed).contains("out 13"));
}

/// MOUSE-25: a write drawn past its twelve lines is a call with more to show, and the click
/// opens the scroller on it with every change drawn.
#[test]
fn a_click_on_a_long_write_at_rest_opens_the_scroller_on_every_change() {
    let (mut session, buffer) = at_rest_over_calls();
    let (row, column) = find_on(&buffer, "Write(out.txt)");
    assert!(!buffer_text(&buffer).contains("change 19"));

    assert_eq!(click(&mut session, row, column), Action::Redraw);

    assert!(session.scrolling());
    let after = drawn_at(&mut session, 120, 40);
    assert!(buffer_text(&after).contains("change 19"));
    // The write was drawn at the bottom of the screen, so it is shown whole from its heading.
    assert!(find_on(&after, "Write(out.txt)").0 <= row);
}

/// MOUSE-9: a click on each row of a call, down to the last, expands it, and the row after it
/// does not.
#[test]
fn a_click_on_every_row_of_a_call_in_the_scroller_expands_it() {
    let session = scroller_over_one_call();
    let (first, _) = find_on_text(&session, "Run(make)");
    let (last, _) = find_on_text(&session, "25 more lines");
    assert!(last > first + 1, "the call is drawn over {first} to {last}");
    for row in first..=last {
        let mut session = scroller_over_one_call();
        assert_eq!(click(&mut session, row, 4), Action::Redraw, "row {row}");
        assert!(
            render::as_text(&session).contains("out 29"),
            "a click on row {row} expanded nothing"
        );
    }
    let mut session = scroller_over_one_call();
    click(&mut session, last + 1, 4);
    assert!(!render::as_text(&session).contains("out 29"));
}

/// MOUSE-25: a click on each row of a call at rest, down to the last, opens the scroller on it
/// expanded, and the row after it does not.
#[test]
fn a_click_on_every_row_of_a_call_at_rest_opens_the_scroller_on_it() {
    for (heading, ending, unseen) in [
        ("Run(make)", "9 more lines", "out 13"),
        ("Write(out.txt)", "8 more lines", "change 19"),
    ] {
        let (_, buffer) = at_rest_over_calls();
        let (first, _) = find_on(&buffer, heading);
        let (last, _) = find_on(&buffer, ending);
        assert!(
            last > first + 1,
            "{heading} is drawn over {first} to {last}"
        );
        for row in first..=last {
            let (mut session, _) = at_rest_over_calls();
            assert_eq!(
                click(&mut session, row, 4),
                Action::Redraw,
                "{heading} {row}"
            );
            assert!(session.scrolling(), "{heading}: row {row} opened nothing");
            let after = drawn_at(&mut session, 120, 40);
            assert!(
                buffer_text(&after).contains(unseen),
                "{heading}: row {row} did not expand the call"
            );
        }
        let (mut session, _) = at_rest_over_calls();
        click(&mut session, last + 1, 4);
        assert!(
            !session.scrolling(),
            "{heading}: the row after it opened the scroller"
        );
    }
}

/// What a delegate was asked to do, long enough to be drawn over several rows beside the
/// heading, and ending in a word that is on no other row.
const A_LONG_TASK: &str = "find every place the parser reads a token and list the callers of \
        each one by name, with the file and the line, and say which of them hold a lock while \
        they do it, zebra";

/// The scroller over one call and a delegate whose block is drawn over more than one row, and
/// the frame at 80 by 40.
fn scroller_over_a_delegate_with_a_long_task() -> (Session, ratatui::buffer::Buffer) {
    use bravebot_agent::report::{DelegateId, Delegation};
    let mut session = scroller_over_one_call();
    session.delegate_started(Delegation {
        stop: Default::default(),
        id: DelegateId::nth(1),
        kind: "reader".to_string(),
        task: A_LONG_TASK.to_string(),
    });
    let buffer = drawn_at(&mut session, 80, 40);
    (session, buffer)
}

/// MOUSE-10: a click on each row of a delegate's block in the scroller, down to the last,
/// opens its view, and the row after it does not.
#[test]
fn a_click_on_every_row_of_a_delegates_block_in_the_scroller_opens_its_view() {
    let (_, buffer) = scroller_over_a_delegate_with_a_long_task();
    let (first, _) = find_on(&buffer, "reader delegate");
    let (last, _) = find_on(&buffer, "zebra");
    assert!(
        last > first + 1,
        "the block is drawn over {first} to {last}"
    );
    for row in first..=last {
        let (mut session, _) = scroller_over_a_delegate_with_a_long_task();
        assert_eq!(click(&mut session, row, 4), Action::Redraw, "row {row}");
        assert!(session.watching_a_delegate(), "row {row} opened no view");
    }
    let (mut session, _) = scroller_over_a_delegate_with_a_long_task();
    click(&mut session, last + 1, 4);
    assert!(session.watching().is_none());
}

/// MOUSE-23: a click on each row of a delegate's block at rest, down to the last, opens its
/// view, and the row after it does not.
#[test]
fn a_click_on_every_row_of_a_delegates_block_at_rest_opens_its_view() {
    use bravebot_agent::report::{DelegateId, Delegation};
    let at_rest = || {
        let (mut session, _) = two_panes();
        session.delegate_started(Delegation {
            stop: Default::default(),
            id: DelegateId::nth(1),
            kind: "reader".to_string(),
            task: A_LONG_TASK.to_string(),
        });
        let buffer = drawn(&mut session);
        (session, buffer)
    };
    let (_, buffer) = at_rest();
    let (first, _) = find_on(&buffer, "reader delegate");
    let (last, _) = find_on(&buffer, "zebra");
    assert!(
        last > first + 1,
        "the block is drawn over {first} to {last}"
    );
    for row in first..=last {
        let (mut session, _) = at_rest();
        assert_eq!(click(&mut session, row, 4), Action::Redraw, "row {row}");
        assert!(session.watching_a_delegate(), "row {row} opened no view");
    }
    let (mut session, _) = at_rest();
    click(&mut session, last + 1, 4);
    assert!(session.watching().is_none());
}

/// MOUSE-25: a call with nothing more to show, the info panel on the call's row, a drag, a
/// double click and a press let go elsewhere open nothing, each against a fixture where the
/// plain click on the call does open the scroller.
#[test]
fn a_click_off_a_call_or_a_drag_over_one_opens_no_scroller_at_rest() {
    let (mut session, buffer) = at_rest_over_calls();
    let panel = session
        .laid
        .panes
        .iter()
        .find(|(pane, _)| *pane == crate::select::Pane::Panel)
        .expect("panel recorded")
        .1
        .x;

    let (short, short_column) = find_on(&buffer, "Read(short.txt)");
    assert_eq!(click(&mut session, short, short_column), Action::Copy);
    assert_eq!(click(&mut session, short + 1, short_column), Action::Copy);
    assert!(!session.scrolling(), "a call with nothing more opened it");
    // The sandbox row of the panel is a click target, so it is turned off for the click on the
    // panel's cell on the call's row, which is blank or text without a target.
    session.set_panel_sections(
        crate::panel::Sections::default().toggled(crate::panel::Section::Sandbox),
    );
    let buffer = drawn_at(&mut session, 120, 40);
    let (row, column) = find_on(&buffer, "Run(make)");
    assert_ne!(click(&mut session, row, panel + 4), Action::Redraw);
    assert!(!session.scrolling());

    swept(&mut session, (row, column), (row, column + 3));
    let up = drag(MouseEventKind::Up(MouseButton::Left), row, column + 3);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);
    assert!(!session.scrolling());

    session.last_click = None;
    press(&mut session, row, column, KeyModifiers::NONE);
    press(&mut session, row, column, KeyModifiers::NONE);
    let up = drag(MouseEventKind::Up(MouseButton::Left), row, column);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);
    assert!(!session.scrolling());

    session.clear_selection();
    session.last_click = None;
    press(&mut session, 0, 0, KeyModifiers::NONE);
    let up = drag(MouseEventKind::Up(MouseButton::Left), row, column);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);
    assert!(!session.scrolling());

    session.clear_selection();
    session.last_click = None;
    assert_eq!(click(&mut session, row, column), Action::Redraw);
    assert!(session.scrolling(), "the fixture's click opened nothing");
}

/// MOUSE-25: a picker, a history search, the files state and a delegate's view each stand over
/// the transcript, and a click on a call opens no scroller under them.
#[test]
fn a_click_on_a_call_opens_no_scroller_where_another_mode_is_in_use() {
    use bravebot_agent::report::{DelegateId, Delegation};
    type Setup = fn(&mut Session);
    let modes: [(&str, Setup); 4] = [
        ("a history search", |session| {
            session.history =
                crate::history::History::from_entries(vec![bravebot_session::store::Entry::sent(
                    "old one", None,
                )]);
            assert!(session.open_history_search(), "no search opened");
        }),
        ("the files state", |session| {
            session.toggle_files();
            assert!(session.files_open(), "the files state did not open");
        }),
        ("a picker", |session| {
            session.open_picking(crate::picking::Picking::Effort(
                crate::effort_prompt::Picker::new(None),
            ));
        }),
        ("a delegate's view", |session| {
            session.delegate_started(Delegation {
                stop: Default::default(),
                id: DelegateId::nth(1),
                kind: "reader".to_string(),
                task: "find the parser".to_string(),
            });
            assert!(session.watch(), "no view opened");
        }),
    ];
    for (name, setup) in modes {
        let (mut session, buffer) = at_rest_over_calls();
        let (row, column) = find_on(&buffer, "Run(make)");
        // Written files are what the state lists, so the fixture has one.
        session.transcript.push(crate::state::Entry::tool(
            bravebot_agent::report::Activity::running("Update", "a.rs")
                .of_tool("edit_file")
                .with_written(Some(bravebot_agent::report::Written {
                    landed: "a.rs".into(),
                    added: 1,
                    removed: 0,
                    numbers: Vec::new(),
                }))
                .done("done"),
        ));
        setup(&mut session);
        let _ = drawn_at(&mut session, 120, 40);
        click(&mut session, row, column);
        assert!(!session.scrolling(), "{name}: a click opened the scroller");
    }
}

/// MOUSE-25: a turn running is when a person most wants to read what a call returned, and the
/// click works there as it does at rest.
#[test]
fn a_click_on_a_call_while_a_turn_runs_opens_the_scroller_on_it() {
    let (mut session, _) = at_rest_over_calls();
    session.type_char('a');
    session.submit();
    assert!(session.a_turn_is_running());
    let buffer = drawn_at(&mut session, 120, 40);
    let (row, column) = find_on(&buffer, "Run(make)");

    assert_eq!(click(&mut session, row, column), Action::Redraw);
    assert!(session.scrolling());
    let after = drawn_at(&mut session, 120, 40);
    assert!(buffer_text(&after).contains("out 13"));
}

/// MOUSE-26: a click on the marker that leads a reply copies that reply, and copies exactly what
/// `/copy` copies for it: the latest for the latest and an earlier one for the earlier, with the
/// control character left out and the paragraph break and the tab kept.
#[test]
fn a_click_on_a_replys_marker_copies_that_reply_as_copy_does() {
    let (mut session, buffer) = at_rest_over_two_replies();
    let (row, column) = find_cell(&buffer, "A named answer.");
    assert_eq!(column, 2, "the marker leads the words");
    let latest = reply_to_copy(&session, "").expect("a latest reply");
    assert_eq!(latest, "A named answer.");
    for marker in [0, 1] {
        let action = click(&mut session, row, marker);
        assert_eq!(action, Action::CopyReply(latest.clone()), "column {marker}");
        assert!(session.selection.is_none());
        session.last_click = None;
    }
    // The line that says which definition answered is above the marker and is not it.
    assert_eq!(click(&mut session, row - 1, 0), Action::Copy);

    let (row, _) = find_cell(&buffer, "The first answer.");
    let earlier = reply_to_copy(&session, "3").expect("an earlier reply");
    assert_eq!(
        reply_to_copy(&session, "2").as_deref(),
        Ok("The latest answer."),
        "the blank reply is not one"
    );
    assert_ne!(earlier, latest);
    assert!(
        earlier.contains("\n\n") && earlier.contains('\t'),
        "{earlier:?}"
    );
    assert!(!earlier.contains('\u{1b}'), "{earlier:?}");
    assert_eq!(click(&mut session, row, 0), Action::CopyReply(earlier));
}

/// MOUSE-26: the cell after the marker, the rows a wrapped reply continues on, the prompt's own
/// marker above, the info panel on the marker's row, a drag, a double click and a press that began
/// elsewhere copy no reply, each against a fixture where the plain click on the marker does.
#[test]
fn a_click_off_a_replys_marker_copies_no_reply() {
    let (mut session, buffer) = at_rest_over_two_replies();
    let (row, _) = find_cell(&buffer, "The first answer.");
    let (prompt, _) = find_cell(&buffer, "and then");
    let (wrapped, _) = find_cell(&buffer, "transcript.");
    assert!(wrapped > row + 2, "the paragraph was not wrapped");
    let panel = session
        .laid
        .panes
        .iter()
        .find(|(pane, _)| *pane == crate::select::Pane::Panel)
        .expect("panel recorded")
        .1
        .x;
    let copies_a_reply = |action: Action| matches!(action, Action::CopyReply(_));

    assert!(!copies_a_reply(click(&mut session, row, 2)));
    assert!(!copies_a_reply(click(&mut session, row, 5)));
    assert!(!copies_a_reply(click(&mut session, wrapped, 0)));
    assert!(!copies_a_reply(click(&mut session, wrapped, 1)));
    assert!(!copies_a_reply(click(&mut session, prompt, 0)));
    assert!(!copies_a_reply(click(&mut session, prompt, 1)));
    // The blank reply draws a marker of its own and has nothing to copy, as `/copy` finds.
    assert_eq!(buffer[(0, prompt - 2)].symbol(), "\u{23fa}");
    assert!(!copies_a_reply(click(&mut session, prompt - 2, 0)));
    assert!(!copies_a_reply(click(&mut session, row, panel + 1)));

    swept(&mut session, (row, 0), (row, 8));
    let up = drag(MouseEventKind::Up(MouseButton::Left), row, 8);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);

    session.last_click = None;
    press(&mut session, row, 0, KeyModifiers::NONE);
    press(&mut session, row, 0, KeyModifiers::NONE);
    let up = drag(MouseEventKind::Up(MouseButton::Left), row, 0);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);

    session.clear_selection();
    session.last_click = None;
    press(&mut session, row + 1, 30, KeyModifiers::NONE);
    let up = drag(MouseEventKind::Up(MouseButton::Left), row, 0);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);

    session.clear_selection();
    session.last_click = None;
    assert!(
        copies_a_reply(click(&mut session, row, 0)),
        "the fixture's click copied nothing"
    );
}

/// MOUSE-26: a reply still arriving is not a reply yet, so its marker copies nothing until the
/// round ends and the words are an entry. `/copy` copies finished replies only.
#[test]
fn a_reply_still_arriving_has_no_marker_to_click() {
    let (mut session, _) = at_rest_over_two_replies();
    session.type_char('a');
    session.submit();
    session.streaming("An answer that is still being written");
    let buffer = drawn(&mut session);
    let (row, column) = find_cell(&buffer, "An answer that is still");
    assert_eq!(column, 2);
    assert_eq!(click(&mut session, row, 0), Action::Copy);
    session.last_click = None;

    session.complete("An answer that is still being written", Vec::new(), 0);
    let buffer = drawn(&mut session);
    let (row, _) = find_cell(&buffer, "An answer that is still");
    assert_eq!(
        click(&mut session, row, 0),
        Action::CopyReply("An answer that is still being written".to_string())
    );
}

/// MOUSE-26: an earlier reply is copied while a turn runs, and in the scroller, where the same
/// marker leads it.
#[test]
fn a_reply_is_copied_by_its_marker_while_a_turn_runs_and_in_the_scroller() {
    let (mut session, _) = at_rest_over_two_replies();
    session.type_char('a');
    session.submit();
    assert!(session.a_turn_is_running());
    let buffer = drawn(&mut session);
    let (row, _) = find_cell(&buffer, "A named answer.");
    let latest = "A named answer.".to_string();
    assert_eq!(
        click(&mut session, row, 0),
        Action::CopyReply(latest.clone())
    );

    session.last_click = None;
    session.open_scroller();
    let buffer = drawn_at(&mut session, 120, 40);
    let (row, _) = find_cell(&buffer, "A named answer.");
    assert_eq!(click(&mut session, row, 0), Action::CopyReply(latest));
}

/// MOUSE-26: a picker, a history search, the files state and a delegate's view each stand over
/// the transcript, and a click on a marker copies nothing under them.
#[test]
fn a_click_on_a_replys_marker_copies_nothing_where_another_mode_is_in_use() {
    use bravebot_agent::report::{DelegateId, Delegation};
    type Setup = fn(&mut Session);
    let modes: [(&str, Setup); 4] = [
        ("a history search", |session| {
            session.history =
                crate::history::History::from_entries(vec![bravebot_session::store::Entry::sent(
                    "old one", None,
                )]);
            assert!(session.open_history_search(), "no search opened");
        }),
        ("the files state", |session| {
            session.toggle_files();
            assert!(session.files_open(), "the files state did not open");
        }),
        ("a picker", |session| {
            session.open_picking(crate::picking::Picking::Effort(
                crate::effort_prompt::Picker::new(None),
            ));
        }),
        ("a delegate's view", |session| {
            session.delegate_started(Delegation {
                stop: Default::default(),
                id: DelegateId::nth(1),
                kind: "reader".to_string(),
                task: "find the parser".to_string(),
            });
            assert!(session.watch(), "no view opened");
        }),
    ];
    for (name, setup) in modes {
        let (mut session, buffer) = at_rest_over_two_replies();
        let (row, _) = find_cell(&buffer, "A named answer.");
        session.transcript.push(crate::state::Entry::tool(
            bravebot_agent::report::Activity::running("Update", "a.rs")
                .of_tool("edit_file")
                .with_written(Some(bravebot_agent::report::Written {
                    landed: "a.rs".into(),
                    added: 1,
                    removed: 0,
                    numbers: Vec::new(),
                }))
                .done("done"),
        ));
        setup(&mut session);
        let _ = drawn(&mut session);
        let action = click(&mut session, row, 0);
        assert!(
            !matches!(action, Action::CopyReply(_)),
            "{name}: a click copied a reply"
        );
    }
}

/// MOUSE-26: the loops that run while work does carry out the copy a click on a marker asked
/// for, say how much was copied, leave the view where it was, and carry out nothing else a mouse
/// report returned.
#[test]
fn the_loops_that_run_while_work_does_copy_the_reply_a_click_named() {
    let mut session = Session::new("none");
    session.scroll = 7;
    let mut handed = Vec::new();
    copy_a_reply_clicked(
        &mut session,
        Action::CopyReply("déjà vu".to_string()),
        |text| {
            handed.push(text.to_string());
            true
        },
    );
    assert_eq!(handed, ["déjà vu"]);
    assert_eq!(session.copied, Some(7));
    assert_eq!(session.scroll, 7, "the view was taken to the tail");

    copy_a_reply_clicked(
        &mut session,
        Action::ChooseEffort(crate::model_prompt::Scope::EverySession),
        |text| {
            handed.push(text.to_string());
            true
        },
    );
    copy_a_reply_clicked(&mut session, Action::Redraw, |text| {
        handed.push(text.to_string());
        true
    });
    assert_eq!(handed.len(), 1, "an action that is not a copy copied");

    copy_a_reply_clicked(&mut session, Action::CopyReply("gone".to_string()), |_| {
        false
    });
    assert_eq!(session.copied, None);
    assert_eq!(
        said_in_the_transcript(&session).last().map(String::as_str),
        Some(t!(session_copy_failed))
    );
}

/// The words of the first block as the reply wrote them: no fence, the tab kept, the markers
/// kept, and the long line one line though the screen drew it over two rows.
const THE_FIRST_BLOCK: &str = "cargo test -p bravebot-tui --lib\n\techo \"a *star* and a_b_c\" # and a comment long enough that the transcript has to wrap it over a second row";

/// MOUSE-27: a click on the word at the right of a block's header copies that block alone, as the
/// reply wrote it, and each block copies its own.
#[test]
fn a_click_on_a_blocks_copy_word_copies_the_block_alone() {
    let (mut session, buffer) = at_rest_over_a_reply_with_blocks();
    let words = cells_of(&buffer, "copy");
    assert_eq!(words.len(), 2, "one word to a block: {words:?}");

    for cell in 0..4 {
        let action = click(&mut session, words[0].0, words[0].1 + cell);
        assert_eq!(
            action,
            Action::CopyReply(THE_FIRST_BLOCK.to_string()),
            "cell {cell}"
        );
        assert!(session.selection.is_none());
        session.last_click = None;
    }
    assert_eq!(
        click(&mut session, words[1].0, words[1].1),
        Action::CopyReply("the second block".to_string())
    );
}

/// MOUSE-27: the cells beside the word, the name the fence gave, the block's own rows, the info
/// panel on the header's row, a drag, a double click and a press that began elsewhere copy no
/// block, each against a fixture where the plain click on the word does.
#[test]
fn a_click_off_a_blocks_copy_word_copies_no_block() {
    let (mut session, buffer) = at_rest_over_a_reply_with_blocks();
    let (row, column) = cells_of(&buffer, "copy")[0];
    let (name, name_column) = cells_of(&buffer, "sh")[0];
    assert_eq!(name, row, "the name is on the header");
    let panel = session
        .laid
        .panes
        .iter()
        .find(|(pane, _)| *pane == crate::select::Pane::Panel)
        .expect("panel recorded")
        .1
        .x;
    let copies = |action: Action| matches!(action, Action::CopyReply(_));

    assert!(!copies(click(&mut session, row, column - 1)));
    assert!(!copies(click(&mut session, row, column + 4)));
    assert!(!copies(click(&mut session, row, name_column)));
    assert!(!copies(click(&mut session, row + 1, column)));
    assert!(!copies(click(&mut session, row + 2, column)));
    assert!(!copies(click(&mut session, row, panel + 1)));
    assert!(!copies(click(&mut session, row - 1, column)));

    swept(&mut session, (row, column), (row, column + 3));
    let up = drag(MouseEventKind::Up(MouseButton::Left), row, column + 3);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);

    session.last_click = None;
    press(&mut session, row, column, KeyModifiers::NONE);
    press(&mut session, row, column, KeyModifiers::NONE);
    let up = drag(MouseEventKind::Up(MouseButton::Left), row, column);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);

    session.clear_selection();
    session.last_click = None;
    press(&mut session, row + 1, 30, KeyModifiers::NONE);
    let up = drag(MouseEventKind::Up(MouseButton::Left), row, column);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);

    session.clear_selection();
    session.last_click = None;
    assert!(
        copies(click(&mut session, row, column)),
        "the fixture's click copied nothing"
    );
}

/// MOUSE-27: the word is the interface's own, so a drag over the header copies what the fence
/// named and a drag over the block copies its lines, and neither takes the word.
#[test]
fn a_drag_over_a_block_does_not_copy_its_copy_word() {
    let (mut session, buffer) = at_rest_over_a_reply_with_blocks();
    let (row, _) = cells_of(&buffer, "copy")[0];
    swept(&mut session, (row, 0), (row + 2, 83));
    let selection = session.selection.expect("a selection");
    let copied = selected_text(&session, &buffer, &selection);
    assert!(copied.contains("sh\n"), "{copied:?}");
    assert!(
        copied.contains("cargo test -p bravebot-tui --lib"),
        "{copied:?}"
    );
    assert!(!copied.contains("copy"), "{copied:?}");
}

/// MOUSE-27: a reply still arriving draws a block without the word, and a click where it would
/// be copies nothing. The word is there when the round ends.
#[test]
fn a_block_in_a_reply_still_arriving_has_no_copy_word_until_it_ends() {
    let (mut session, _) = at_rest_over_two_replies();
    session.type_char('a');
    session.submit();
    session.streaming("Here:\n```sh\nls -la\n");
    let buffer = drawn_at(&mut session, 120, 40);
    assert!(cells_of(&buffer, "copy").is_empty());
    let (row, _) = cells_of(&buffer, "ls -la")[0];
    assert_eq!(click(&mut session, row - 1, 80), Action::Copy);
    session.last_click = None;

    session.complete("Here:\n```sh\nls -la\n", Vec::new(), 0);
    let buffer = drawn_at(&mut session, 120, 40);
    let (row, column) = cells_of(&buffer, "copy")[0];
    assert_eq!(
        click(&mut session, row, column),
        Action::CopyReply("ls -la".to_string()),
        "a block nobody closed is the rest of the reply"
    );
}

/// MOUSE-27: the word works in the scroller and while a turn runs.
#[test]
fn a_blocks_copy_word_works_in_the_scroller_and_while_a_turn_runs() {
    let (mut session, _) = at_rest_over_a_reply_with_blocks();
    session.type_char('a');
    session.submit();
    assert!(session.a_turn_is_running());
    let buffer = drawn_at(&mut session, 120, 40);
    let (row, column) = cells_of(&buffer, "copy")[1];
    assert_eq!(
        click(&mut session, row, column),
        Action::CopyReply("the second block".to_string())
    );

    session.last_click = None;
    session.open_scroller();
    let buffer = drawn_at(&mut session, 120, 40);
    let (row, column) = cells_of(&buffer, "copy")[0];
    assert_eq!(
        click(&mut session, row, column),
        Action::CopyReply(THE_FIRST_BLOCK.to_string())
    );
}

/// MOUSE-27: a picker, a history search, the files state and a delegate's view each stand over
/// the transcript, and a click on a block's word copies nothing under them.
#[test]
fn a_click_on_a_blocks_copy_word_copies_nothing_where_another_mode_is_in_use() {
    use bravebot_agent::report::{DelegateId, Delegation};
    type Setup = fn(&mut Session);
    let modes: [(&str, Setup); 4] = [
        ("a history search", |session| {
            session.history =
                crate::history::History::from_entries(vec![bravebot_session::store::Entry::sent(
                    "old one", None,
                )]);
            assert!(session.open_history_search(), "no search opened");
        }),
        ("the files state", |session| {
            session.toggle_files();
            assert!(session.files_open(), "the files state did not open");
        }),
        ("a picker", |session| {
            session.open_picking(crate::picking::Picking::Effort(
                crate::effort_prompt::Picker::new(None),
            ));
        }),
        ("a delegate's view", |session| {
            session.delegate_started(Delegation {
                stop: Default::default(),
                id: DelegateId::nth(1),
                kind: "reader".to_string(),
                task: "find the parser".to_string(),
            });
            assert!(session.watch(), "no view opened");
        }),
    ];
    for (name, setup) in modes {
        let (mut session, buffer) = at_rest_over_a_reply_with_blocks();
        let (row, column) = cells_of(&buffer, "copy")[0];
        session.transcript.push(crate::state::Entry::tool(
            bravebot_agent::report::Activity::running("Update", "a.rs")
                .of_tool("edit_file")
                .with_written(Some(bravebot_agent::report::Written {
                    landed: "a.rs".into(),
                    added: 1,
                    removed: 0,
                    numbers: Vec::new(),
                }))
                .done("done"),
        ));
        setup(&mut session);
        let _ = drawn_at(&mut session, 120, 40);
        let action = click(&mut session, row, column);
        assert!(
            !matches!(action, Action::CopyReply(_)),
            "{name}: a click copied a block"
        );
    }
}

/// MOUSE-27: `/copy block` is the keyboard route to the word that copies a block. Typed, it puts
/// on the clipboard exactly the words the click on that block's word puts there, the last block
/// of the latest reply first and the one before it for `2`, so the two cannot disagree about
/// which block is meant or what it holds.
#[test]
fn a_typed_block_command_copies_the_words_the_click_on_that_block_copies() {
    let (mut session, buffer) = at_rest_over_a_reply_with_blocks();
    let words = cells_of(&buffer, "copy");
    assert_eq!(words.len(), 2);
    let mut clicked = Vec::new();
    for (row, column) in words {
        let Action::CopyReply(text) = click(&mut session, row, column) else {
            panic!("a click on a block's word copied nothing");
        };
        clicked.push(text);
        session.last_click = None;
    }

    assert_eq!(clicked[1], "the second block");
    assert_eq!(
        copied_by(&mut session, "/copy block"),
        Ok(clicked[1].clone())
    );
    assert_eq!(
        copied_by(&mut session, "/copy block 1"),
        Ok(clicked[1].clone())
    );
    assert_eq!(
        copied_by(&mut session, "/copy block 2"),
        Ok(clicked[0].clone())
    );
    assert!(session.input().is_empty(), "the command stayed on the line");
}

/// MOUSE-27: blocks are counted back from the latest through every reply, as replies are, a
/// reply with none is passed over, a block with nothing in it is not one, and a control
/// character is left out of what is copied as it is for a reply.
#[test]
fn a_typed_block_command_counts_blocks_back_through_the_replies() {
    let mut session = Session::new("none");
    answered(&mut session, "one", "intro\n```sh\nrm\u{1b}[0m -rf\tx\n```");
    answered(&mut session, "two", "a reply with no block");
    answered(
        &mut session,
        "three",
        "text\n```\nfirst of three\n```\n```\n```\n~~~py\nsecond of three\n~~~",
    );

    assert_eq!(
        copied_by(&mut session, "/copy block"),
        Ok("second of three".to_string())
    );
    assert_eq!(
        copied_by(&mut session, "/copy block 2"),
        Ok("first of three".to_string())
    );
    assert_eq!(
        copied_by(&mut session, "/copy block 3"),
        Ok("rm[0m -rf\tx".to_string())
    );
    // The reply command is as it was, and a block is not a reply back.
    assert_eq!(
        copied_by(&mut session, "/copy 2"),
        Ok("a reply with no block".to_string())
    );
}

/// MOUSE-27: `/copy block` refuses as `/copy` does. With no block anywhere, a number that is not
/// one, zero, and a number past the oldest block each say so in a note and copy nothing, and the
/// last says how many blocks there are.
#[test]
fn a_typed_block_command_refuses_what_it_cannot_take_and_copies_nothing() {
    let mut session = Session::new("none");
    assert_eq!(
        copied_by(&mut session, "/copy block"),
        Err(t!(session_copy_no_block).to_string()),
        "no reply at all"
    );
    answered(
        &mut session,
        "one",
        "a reply with no block, only `inline` code",
    );
    assert_eq!(
        copied_by(&mut session, "/copy block 2"),
        Err(t!(session_copy_no_block).to_string()),
        "a reply with no block"
    );

    answered(&mut session, "two", "```\nonly\n```");
    for line in [
        "/copy block 0",
        "/copy block x",
        "/copy block -1",
        "/copy block 2x",
        "/copy block 1 2",
    ] {
        assert_eq!(
            copied_by(&mut session, line),
            Err(t!(session_copy_block_needs_a_number).to_string()),
            "{line}"
        );
    }
    assert_eq!(
        copied_by(&mut session, "/copy block 2"),
        Err(t!(session_copy_block_goes_no_further, blocks = 1)),
        "a number past the oldest block"
    );
    assert_eq!(
        copied_by(&mut session, "/copy block 99999999999999999999999999"),
        Err(t!(session_copy_block_goes_no_further, blocks = 1)),
        "a number too long to hold"
    );
    // A longer word is not the word, so it is read as a reply count and refused as one.
    assert_eq!(
        copied_by(&mut session, "/copy blocks"),
        Err(t!(session_copy_needs_a_number).to_string())
    );
    assert_eq!(session.copied, None, "a refusal reported a copy");
    assert_eq!(
        copied_by(&mut session, "/copy block"),
        Ok("only".to_string())
    );
}

/// MOUSE-27: a block is wanted on the clipboard while a turn runs as a reply is, so the command
/// does not wait, and a block with nothing in it, or only blank lines, has no word to click and no
/// number.
#[test]
fn a_typed_block_command_runs_mid_turn_and_counts_the_blocks_that_have_a_word() {
    let (mut session, buffer) = at_rest_over_a_reply_with_blocks();
    assert_eq!(cells_of(&buffer, "copy").len(), 2);
    session.transcript.push(crate::state::Entry::assistant(
        "```\nA\n```\n```\n```\n```\n\n  \n```\n```\nC\n```",
        Vec::new(),
    ));
    let buffer = drawn_at(&mut session, 120, 40);
    let words = cells_of(&buffer, "copy");
    assert_eq!(
        words.len(),
        4,
        "a block with nothing in it drew a word: {words:?}"
    );

    session.type_char('a');
    session.submit();
    assert!(session.a_turn_is_running());
    assert_eq!(
        type_while_working(&mut session, "/copy block"),
        Action::CopyReply("C".to_string())
    );
    assert_eq!(
        type_while_working(&mut session, "/copy block 2"),
        Action::CopyReply("A".to_string())
    );
    // The click on the last word is the same block.
    let (row, column) = words[3];
    assert_eq!(
        click(&mut session, row, column),
        Action::CopyReply("C".to_string())
    );
}

/// MOUSE-27: Tab finishes the word after `/copy`, and leaves a number or a word it does not know.
#[test]
fn tab_finishes_block_after_the_copy_command_and_leaves_a_number_alone() {
    for (typed, finished) in [("/copy b", "/copy block"), ("/copy bl", "/copy block")] {
        let mut session = Session::new("none");
        for c in typed.chars() {
            session.type_char(c);
        }
        assert_eq!(handle_key(&mut session, key(KeyCode::Tab)), Action::Redraw);
        assert_eq!(session.input(), finished, "{typed:?}");
    }
    for typed in ["/copy 2", "/copy block", "/copy x", "/copy "] {
        let mut session = Session::new("none");
        for c in typed.chars() {
            session.type_char(c);
        }
        handle_key(&mut session, key(KeyCode::Tab));
        assert_eq!(session.input(), typed, "{typed:?}");
    }
}

/// MOUSE-27: the command names the word in the list of commands, and the words after `/copy` are
/// still `/copy`'s, so a line with `block` in it is not sent as a prompt.
#[test]
fn the_copy_command_names_block_and_keeps_the_words_after_it() {
    let all = commands();
    let command = all
        .iter()
        .find(|command| command.name == COPY_COMMAND)
        .expect("the copy command");
    assert_eq!(command.argument, "[n | block [n] | text [n]]");
    assert!(
        command.description.contains("block"),
        "{}",
        command.description
    );
    for line in ["/copy block", "/copy block 2", "/copy 3"] {
        assert_eq!(command_typed(line), Some(COPY_COMMAND), "{line}");
    }
}
