//! MOUSE-15 and MOUSE-46 for the lists of what could be typed that are drawn under the box: a click
//! on a row, and the wheel over them.

use super::*;

/// The names of the commands a half-typed `/c` offers, in the order they are listed.
fn offered_commands(session: &Session) -> Vec<&'static str> {
    session
        .completions()
        .iter()
        .map(|command| command.name)
        .collect()
}

/// The cells of each row of the list under the box the last frame drew, with the item it stands for.
fn rows_drawn(session: &Session) -> Vec<(ratatui::layout::Rect, usize)> {
    session
        .laid
        .targets
        .iter()
        .filter_map(|target| match target.hit {
            crate::state::Hit::Item(index) => Some((target.area, index)),
            _ => None,
        })
        .collect()
}

/// The cell at the start of the text of row `index` of the list, as the last frame drew it.
fn row_of_item(session: &Session, index: usize) -> (u16, u16) {
    let rows = rows_drawn(session);
    let (row, _) = rows
        .iter()
        .find(|(_, item)| *item == index)
        .unwrap_or_else(|| panic!("item {index} is not drawn: {rows:?}"));
    (row.y, row.x + 6)
}

/// Whether the key that takes what is offered, and nothing else, was what the second click did: the
/// line is what Tab leaves.
fn what_tab_leaves(session: &Session, item: usize) -> String {
    let mut by_key = typed_into(session.input());
    by_key.select_completion(item);
    handle_key(&mut by_key, key(KeyCode::Tab));
    by_key.input().to_string()
}

/// MOUSE-15. Rejects: a click on a row of the list that moves nothing, takes the row at once, or
/// runs the command it names: the first click puts the cursor on the row and the second takes it
/// into the box as Tab does, and Enter is still what runs it.
#[test]
fn a_click_on_a_command_moves_the_cursor_and_a_second_takes_it_into_the_box() {
    let mut session = typed_into("/c");
    drawn(&mut session);
    let commands = offered_commands(&session);
    assert!(commands.len() >= 3, "{commands:?}");
    assert_eq!(
        session.highlighted_completion().map(|c| c.name),
        Some(commands[0])
    );

    let (row, column) = row_of_item(&session, 2);
    assert_eq!(click(&mut session, row, column), Action::Redraw);
    assert_eq!(
        session.highlighted_completion().map(|c| c.name),
        Some(commands[2])
    );
    assert_eq!(session.input(), "/c", "the first click took the row");

    // The second click, as a terminal sends it: a press and a release on the same cell, quickly.
    let expected = what_tab_leaves(&session, 2);
    assert_eq!(click(&mut session, row, column), Action::Redraw);
    assert_eq!(session.input(), expected);
    assert_ne!(session.input(), "/c");
    assert!(
        session.selection.is_none(),
        "the second click left a word selected"
    );
}

/// MOUSE-15. Rejects: a double click in place that does not take the row pointed at, with a frame
/// drawn between the two presses as the loop draws it. On every row drawn of a list that fits the room
/// under the box and of one that does not, with the cursor on the first item and moved down the list,
/// a press and a release, a frame, and a press and a release on one cell put the cursor on the row
/// drawn there, leave the rows where they were, and take it into the box as Tab does.
#[test]
fn a_double_click_in_place_takes_the_row_pointed_at_on_every_row_of_a_list_whole_or_cut() {
    for (typed, moved_down) in [("/c", 0), ("/", 0), ("/", 12)] {
        let opened = || {
            let mut session = typed_into(typed);
            for _ in 0..moved_down {
                handle_key(&mut session, key(KeyCode::Down));
            }
            drawn(&mut session);
            session
        };
        let rows = rows_drawn(&opened()).len();
        assert!(rows >= 3, "{typed:?}: {rows} rows");
        // The list a lone slash offers is longer than the room under the box.
        assert_eq!(
            opened().offered_count() > rows,
            typed == "/",
            "{typed:?}: {rows} rows"
        );
        for item in 0..rows {
            let mut session = opened();
            let before: Vec<usize> = rows_drawn(&session)
                .iter()
                .map(|(_, index)| *index)
                .collect();
            let item = before[item];
            let (row, column) = row_of_item(&session, item);
            let expected = what_tab_leaves(&session, item);

            click(&mut session, row, column);
            assert_eq!(
                session.input(),
                typed,
                "{typed:?}, item {item}: the first click took it"
            );
            drawn(&mut session);
            let after: Vec<usize> = rows_drawn(&session)
                .iter()
                .map(|(_, index)| *index)
                .collect();
            assert_eq!(
                after, before,
                "{typed:?}, item {item}: the click slid the list"
            );
            click(&mut session, row, column);
            assert_eq!(
                session.input(),
                expected,
                "{typed:?}, item {item}: the second did not take it"
            );
        }
    }
}

/// MOUSE-15. Rejects: a second click on another row, or a drag over the rows, taking one.
#[test]
fn clicks_on_two_rows_or_a_drag_over_the_list_take_nothing() {
    let mut session = typed_into("/c");
    drawn(&mut session);
    let (first, column) = row_of_item(&session, 1);
    let (second, _) = row_of_item(&session, 2);
    click(&mut session, first, column);
    click(&mut session, second, column);
    assert_eq!(session.input(), "/c", "two rows took one");
    assert_eq!(
        session.highlighted_completion().map(|c| c.name),
        Some(offered_commands(&session)[2])
    );

    swept(&mut session, (first, column), (second, column + 4));
    let up = drag(MouseEventKind::Up(MouseButton::Left), second, column + 4);
    assert_eq!(handle_mouse(&mut session, up), Action::Copy);
    assert_eq!(session.input(), "/c", "a drag took a row");
}

/// MOUSE-15. Rejects: a click below the last row, on the box, or of another button, or a click
/// while nothing is offered, acting on the list.
#[test]
fn a_click_off_the_rows_and_a_click_of_another_button_take_nothing() {
    let mut session = typed_into("/c");
    drawn(&mut session);
    let (last, column) = row_of_item(&session, session.offered_count() - 1);
    let before = session.highlighted_completion().map(|c| c.name);
    for row in [last + 1, last + 2] {
        click(&mut session, row, column);
        click(&mut session, row, column);
    }
    for kind in [MouseButton::Right, MouseButton::Middle] {
        let (row, column) = row_of_item(&session, 1);
        for _ in 0..2 {
            handle_mouse(&mut session, drag(MouseEventKind::Down(kind), row, column));
            handle_mouse(&mut session, drag(MouseEventKind::Up(kind), row, column));
        }
    }
    assert_eq!(session.input(), "/c");
    assert_eq!(session.highlighted_completion().map(|c| c.name), before);
}

/// MOUSE-15. Rejects: a press on the keys listed by `?` taking one of them, since there is nothing
/// to take among them, and a list that is gone being taken from.
#[test]
fn the_list_of_shortcuts_and_a_list_that_has_gone_are_not_rows_to_take() {
    let mut session = typed_into("");
    session.type_char('?');
    let buffer = drawn(&mut session);
    assert!(
        rows_drawn(&session).is_empty(),
        "the shortcuts drew rows to pick"
    );
    let (row, column) = find_on(&buffer, "run a shell command");
    click(&mut session, row, column);
    click(&mut session, row, column);
    assert_eq!(session.input(), "");

    let mut session = typed_into("/c");
    drawn(&mut session);
    let (row, column) = row_of_item(&session, 1);
    session.clear_input();
    click(&mut session, row, column);
    click(&mut session, row, column);
    assert_eq!(session.input(), "", "a list that was gone took a row");
}

/// MOUSE-15 and MOUSE-50. Rejects: a row recorded in the frame for a list that has since gone, as
/// when the line was cleared by a key, still being a target. A click there is a plain click, which
/// ends in the copy that finds nothing to copy, and not the list's.
#[test]
fn a_row_of_a_list_that_has_gone_since_the_frame_is_no_target() {
    let mut session = typed_into("/c");
    drawn(&mut session);
    let (row, column) = row_of_item(&session, 1);
    assert!(session.completion_at(row, column).is_some());

    session.clear_input();
    assert_eq!(session.completion_at(row, column), None);
    assert_eq!(click(&mut session, row, column), Action::Copy);
}

/// MOUSE-15. Rejects: the rows being counted from the top of the pane when other lines are drawn
/// above them, which a stashed line is: each row drawn is the command a click on it picks, and no row
/// past what the pane had room for is there to click.
#[test]
fn each_row_drawn_is_the_item_a_click_on_it_picks_under_other_rows_and_in_little_room() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    for (rows, stashed) in [(24u16, false), (24, true), (9, false), (9, true)] {
        let mut session = typed_into("");
        if stashed {
            type_line(&mut session, "put away");
            session.stash();
        }
        type_line(&mut session, "/");
        let mut terminal = Terminal::new(TestBackend::new(100, rows)).expect("terminal");
        let mut laid = crate::state::Laid::default();
        terminal
            .draw(|frame| laid = render::draw(frame, &session))
            .expect("draw");
        session.note_layout(laid);
        let buffer = terminal.backend().buffer().clone();
        let commands = offered_commands(&session);
        let drawn_rows = rows_drawn(&session);
        assert!(!drawn_rows.is_empty(), "{rows} rows, stashed {stashed}");
        assert!(
            drawn_rows.len() <= commands.len(),
            "{rows} rows, stashed {stashed}: more rows than commands"
        );
        for (cells, index) in drawn_rows {
            let text: String = (cells.x..cells.right())
                .map(|x| buffer[(x, cells.y)].symbol())
                .collect();
            assert!(
                text.contains(commands[index]),
                "{rows} rows, stashed {stashed}: item {index} is {:?} and the row reads {text:?}",
                commands[index]
            );
            // The press picks what the row says, which `highlighted_completion` answers for. The
            // press before it is forgotten, so that two rows are not the run of one double click.
            session.last_click = None;
            session.select_completion(0);
            click(&mut session, cells.y, cells.x + 6);
            assert_eq!(
                session.highlighted_completion().map(|c| c.name),
                Some(commands[index]),
                "{rows} rows, stashed {stashed}"
            );
        }
    }
}

/// MOUSE-15. Rejects: the list that is offered while a turn runs, which is the commands alone, not
/// taking a click, since a command typed then is queued or carried out and so is listed.
#[test]
fn a_click_on_a_row_of_the_list_offered_while_a_turn_runs_takes_it_the_same() {
    let mut session = typed_into("/c");
    session.status = crate::state::Status::Working;
    drawn(&mut session);
    let commands = offered_commands(&session);
    assert!(commands.len() >= 2, "{commands:?}");
    let (row, column) = row_of_item(&session, 1);
    click(&mut session, row, column);
    assert_eq!(
        session.highlighted_completion().map(|c| c.name),
        Some(commands[1])
    );
    click(&mut session, row, column);
    assert_ne!(session.input(), "/c");
}

/// MOUSE-15. Rejects: the list of files after an `@` being left out: a click on a file moves the
/// cursor to it and a second writes it into the line.
#[test]
fn a_click_on_a_file_moves_the_cursor_and_a_second_writes_it_after_the_at() {
    let root = crate::testutil::scratch_dir("bravebot-click-a-file-completion-test");
    std::fs::create_dir_all(&root).expect("scratch");
    for name in ["alpha.txt", "beta.txt", "gamma.txt"] {
        std::fs::write(root.join(name), "text").expect("a file");
    }
    let mut session = Session::new("none").in_workspace(&root);
    type_line(&mut session, "read @");
    drawn(&mut session);
    let crate::state::Offered::Files(entries) = session.offered() else {
        panic!("files were not offered");
    };
    let at = entries
        .iter()
        .position(|entry| entry.path == "beta.txt")
        .expect("beta is offered");

    let (row, column) = row_of_item(&session, at);
    click(&mut session, row, column);
    assert_eq!(
        session.highlighted_entry().map(|entry| entry.path),
        Some("beta.txt".to_string())
    );
    assert_eq!(session.input(), "read @");
    click(&mut session, row, column);
    assert!(
        session.input().starts_with("read @beta.txt"),
        "{:?}",
        session.input()
    );
    std::fs::remove_dir_all(&root).ok();
}

/// MOUSE-46. Rejects: the wheel over the list scrolling the transcript beneath it, moving the cursor
/// by another step than `tui.wheelRows`, in the wrong direction or past either end, and the wheel
/// elsewhere moving the cursor.
#[test]
fn the_wheel_over_the_list_walks_it_and_elsewhere_scrolls_the_transcript() {
    let mut session = typed_into("/");
    for n in 0..60 {
        session.transcript.push(crate::state::Entry::user(format!(
            "a line to scroll through {n}"
        )));
    }
    drawn(&mut session);
    let commands = offered_commands(&session);
    let step = usize::from(session.wheel_rows());
    assert!(commands.len() > 2 * step + 1, "{}", commands.len());
    let (row, column) = row_of_item(&session, 0);
    let cursor = |session: &Session| session.highlighted_completion().map(|c| c.name);
    let scrolled = session.scroll;

    handle_mouse(&mut session, drag(MouseEventKind::ScrollDown, row, column));
    assert_eq!(cursor(&session), Some(commands[step]));
    assert_eq!(
        session.scroll, scrolled,
        "the wheel over the list scrolled the transcript"
    );
    handle_mouse(&mut session, drag(MouseEventKind::ScrollUp, row, column));
    assert_eq!(cursor(&session), Some(commands[0]));
    handle_mouse(&mut session, drag(MouseEventKind::ScrollUp, row, column));
    assert_eq!(cursor(&session), Some(commands[0]), "past the first row");
    for _ in 0..200 {
        handle_mouse(&mut session, drag(MouseEventKind::ScrollDown, row, column));
    }
    assert_eq!(
        cursor(&session),
        commands.last().copied(),
        "past the last row"
    );

    handle_mouse(&mut session, drag(MouseEventKind::ScrollUp, 1, 1));
    assert_eq!(
        cursor(&session),
        commands.last().copied(),
        "the wheel elsewhere moved the list"
    );
    assert_ne!(
        session.scroll, scrolled,
        "the wheel elsewhere did not scroll the transcript"
    );
}
