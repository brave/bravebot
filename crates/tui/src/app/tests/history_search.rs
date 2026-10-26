//! MOUSE-15, MOUSE-46 and MOUSE-47 for the search over the prompts already sent: a press on a match,
//! the wheel, and a press outside the panel.

use super::*;

/// A session whose history holds twenty prompts, `prompt 01` the oldest, with the search open.
fn searching_twenty() -> Session {
    searching(20)
}

/// A session whose history holds `count` prompts, `prompt 01` the oldest, with the search open.
fn searching(count: usize) -> Session {
    let mut session = Session::new("none");
    session.history = crate::history::History::from_entries(
        (1..=count)
            .map(|n| bravebot_session::store::Entry::sent(format!("prompt {n:02}"), None))
            .collect(),
    );
    assert!(session.open_history_search(), "no search opened");
    session
}

fn cursor(session: &Session) -> Option<String> {
    session.history_match().map(|entry| entry.prompt.clone())
}

/// The cells of each row of the list of matches the last frame drew, top to bottom.
fn rows_of_the_matches(session: &Session) -> Vec<ratatui::layout::Rect> {
    session
        .laid
        .targets
        .iter()
        .filter(|target| matches!(target.hit, crate::state::Hit::Match(_)))
        .map(|target| target.area)
        .collect()
}

/// The row and a column of the text a match is drawn with.
fn row_of(buffer: &ratatui::buffer::Buffer, text: &str) -> (u16, u16) {
    let (row, column) = find_on(buffer, text);
    (row, column + 1)
}

/// MOUSE-15. Rejects: a press on a match that moves nothing, or takes it at once, a second press
/// that leaves the search open or sends the prompt, and a prompt that goes anywhere but the box.
#[test]
fn a_press_on_a_prompt_moves_the_cursor_and_a_second_takes_it_into_the_box() {
    let mut session = searching_twenty();
    let buffer = drawn(&mut session);
    assert_eq!(cursor(&session).as_deref(), Some("prompt 20"));

    let (row, column) = row_of(&buffer, "prompt 17");
    press(&mut session, row, column, KeyModifiers::NONE);
    assert_eq!(cursor(&session).as_deref(), Some("prompt 17"));
    assert!(
        session.searching_history(),
        "the first press took the prompt"
    );
    assert_eq!(session.input(), "");

    let action = {
        press(&mut session, row, column, KeyModifiers::NONE);
        handle_mouse(
            &mut session,
            drag(MouseEventKind::Up(MouseButton::Left), row, column),
        )
    };
    assert!(
        !session.searching_history(),
        "the second press left it open"
    );
    assert_eq!(
        session.input(),
        "prompt 17",
        "the prompt did not go to the box"
    );
    assert!(
        !matches!(action, Action::Submit(_)),
        "the prompt was sent: {action:?}"
    );
}

/// MOUSE-15. Rejects: a second press on another match, or after the window, taking a prompt.
#[test]
fn two_presses_on_different_prompts_or_far_apart_take_nothing() {
    let mut session = searching_twenty();
    let buffer = drawn(&mut session);
    let (row, column) = row_of(&buffer, "prompt 18");
    let (other, other_column) = row_of(&buffer, "prompt 16");
    press(&mut session, row, column, KeyModifiers::NONE);
    press(&mut session, other, other_column, KeyModifiers::NONE);
    assert!(session.searching_history(), "two prompts took one");
    assert_eq!(cursor(&session).as_deref(), Some("prompt 16"));

    // A press the interval has passed since is not the first of a double click.
    let long_ago = std::time::Instant::now()
        .checked_sub(crate::select::MULTI_CLICK * 2)
        .expect("the machine has been up for a moment");
    session.pick_click = Some(crate::select::count_click(None, long_ago, other, other_column).0);
    press(&mut session, other, other_column, KeyModifiers::NONE);
    assert!(session.searching_history(), "a late second press took it");
}

/// MOUSE-15. Rejects: a press on a row of a window that has slid, counted from the top of the list
/// the history makes and not from the top of the rows drawn. Each row drawn is the prompt a press on
/// it puts the cursor on, with eight of twenty showing and after a search has narrowed them.
#[test]
fn each_row_drawn_is_the_prompt_a_press_on_it_picks_in_a_window_that_has_slid() {
    for typed in ["", "prompt 1"] {
        for older in [0, 5, 11] {
            let mut session = searching_twenty();
            for c in typed.chars() {
                session.type_into_history_search(c);
            }
            for _ in 0..older {
                session.history_search_older();
            }
            let buffer = drawn(&mut session);
            let picks = rows_of_the_matches(&session);
            assert!(!picks.is_empty(), "typed {typed:?}, {older} older");
            for row in picks {
                let words: String = (row.x..row.right())
                    .map(|x| buffer[(x, row.y)].symbol())
                    .collect();
                let name = words
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .windows(2)
                    .find(|pair| pair[0] == "prompt")
                    .map(|pair| format!("prompt {}", pair[1]))
                    .unwrap_or_else(|| panic!("row {} holds no prompt: {words:?}", row.y));
                let mut fresh = searching_twenty();
                for c in typed.chars() {
                    fresh.type_into_history_search(c);
                }
                for _ in 0..older {
                    fresh.history_search_older();
                }
                drawn(&mut fresh);
                press(&mut fresh, row.y, row.x + 2, KeyModifiers::NONE);
                assert_eq!(
                    cursor(&fresh).as_deref(),
                    Some(name.as_str()),
                    "typed {typed:?}, {older} older, row {}: {words:?}",
                    row.y
                );
            }
        }
    }
}

/// The prompt a row of the frame draws, as `prompt NN`.
fn prompt_on(buffer: &ratatui::buffer::Buffer, row: u16) -> String {
    let words: String = (0..buffer.area.width)
        .map(|x| buffer[(x, row)].symbol())
        .collect();
    let words: Vec<&str> = words.split_whitespace().collect();
    words
        .windows(2)
        .find(|pair| pair[0] == "prompt")
        .map(|pair| format!("prompt {}", pair[1]))
        .unwrap_or_else(|| panic!("row {row} holds no prompt: {words:?}"))
}

/// A press and a release on a cell, as a terminal sends them.
fn click_there(session: &mut Session, row: u16, column: u16) {
    press(session, row, column, KeyModifiers::NONE);
    handle_mouse(
        session,
        drag(MouseEventKind::Up(MouseButton::Left), row, column),
    );
}

/// The prompts the rows of the list draw, top to bottom.
fn listed(session: &Session, buffer: &ratatui::buffer::Buffer) -> Vec<String> {
    rows_of_the_matches(session)
        .iter()
        .map(|row| prompt_on(buffer, row.y))
        .collect()
}

/// MOUSE-15. Rejects: a double click in place that does not take the prompt pointed at, because a
/// press that moved the cursor to a row already drawn slid the window, so the second press landed on
/// another prompt. On every row of a list that has not scrolled, and of one that has, a press and a
/// release, a frame, and a press and a release on the same cell put the cursor on the prompt drawn
/// there, leave the rows where they were, and take the prompt into the box.
#[test]
fn a_double_click_in_place_takes_the_prompt_pointed_at_on_every_row_of_a_list_scrolled_or_not() {
    for (count, scrolled) in [(5, false), (20, true)] {
        let rows = rows_of_the_matches(&{
            let mut session = searching(count);
            drawn(&mut session);
            session
        })
        .len();
        assert_eq!(rows < count, scrolled, "{count} prompts, {rows} rows");
        for slot in 0..rows {
            let mut session = searching(count);
            let buffer = drawn(&mut session);
            let before = listed(&session, &buffer);
            let cell = rows_of_the_matches(&session)[slot];
            let (row, column) = (cell.y, cell.x + 2);

            click_there(&mut session, row, column);
            assert!(session.searching_history(), "{count} prompts, row {slot}");
            assert_eq!(cursor(&session).as_deref(), Some(before[slot].as_str()));
            let buffer = drawn(&mut session);
            let after_the_press = listed(&session, &buffer);

            click_there(&mut session, row, column);
            assert!(
                !session.searching_history(),
                "{count} prompts, row {slot}: the second press left it open"
            );
            assert_eq!(session.input(), before[slot], "{count} prompts, row {slot}");
            assert_eq!(
                after_the_press, before,
                "{count} prompts, row {slot}: the press slid the list"
            );
        }
    }
}

/// MOUSE-15. Rejects: a second press counted with the first when the list moved between them, which
/// takes a prompt the first press did not name: the wheel moves the cursor and the window with it, so
/// the cell holds another prompt, and a press there is a first press on that prompt. A press where
/// the prompt first named is drawn now is the second press on it, and takes it.
#[test]
fn a_press_where_the_wheel_has_since_drawn_another_prompt_is_a_first_press_on_it() {
    let mut session = searching_twenty();
    let buffer = drawn(&mut session);
    let top = rows_of_the_matches(&session)[0];
    let (row, column) = (top.y, top.x + 2);
    let named = prompt_on(&buffer, row);

    click_there(&mut session, row, column);
    assert_eq!(cursor(&session).as_deref(), Some(named.as_str()));
    handle_mouse(&mut session, drag(MouseEventKind::ScrollUp, row, column));
    let buffer = drawn(&mut session);
    let drawn_there_now = prompt_on(&buffer, row);
    assert_ne!(drawn_there_now, named, "the wheel did not move the list");

    click_there(&mut session, row, column);
    assert!(
        session.searching_history(),
        "a prompt nobody pressed first was taken"
    );
    assert_eq!(session.input(), "");
    assert_eq!(cursor(&session).as_deref(), Some(drawn_there_now.as_str()));

    let mut session = searching_twenty();
    drawn(&mut session);
    click_there(&mut session, row, column);
    handle_mouse(&mut session, drag(MouseEventKind::ScrollDown, row, column));
    let buffer = drawn(&mut session);
    let moved_to = rows_of_the_matches(&session)
        .into_iter()
        .find(|rows| prompt_on(&buffer, rows.y) == named)
        .expect("the prompt first named is still drawn");
    click_there(&mut session, moved_to.y, moved_to.x + 2);
    assert!(
        !session.searching_history(),
        "the prompt first named was not taken"
    );
    assert_eq!(session.input(), named);
}

/// MOUSE-15. Rejects: a window that stays where a press left it once a key moves the cursor: the keys
/// and the wheel move it as they always did, with the cursor on the last row drawn, so the search a
/// press and a key left is drawn as the search the keys alone reach the same prompt in.
#[test]
fn a_key_after_a_press_moves_the_window_as_the_keys_alone_do() {
    let mut pressed = searching_twenty();
    let buffer = drawn(&mut pressed);
    let top = rows_of_the_matches(&pressed)[0];
    click_there(&mut pressed, top.y, top.x + 2);
    drawn(&mut pressed);
    // To a newer prompt, which the held window still draws and the window the keys alone leave
    // does not.
    pressed.history_search_newer();
    let after_a_key = drawn(&mut pressed);

    let named = prompt_on(&buffer, top.y);
    let mut keys = searching_twenty();
    drawn(&mut keys);
    while cursor(&keys).as_deref() != Some(named.as_str()) {
        keys.history_search_older();
    }
    keys.history_search_newer();
    let by_keys = drawn(&mut keys);

    assert_eq!(cursor(&pressed), cursor(&keys));
    assert_eq!(listed(&pressed, &after_a_key), listed(&keys, &by_keys));
}

/// MOUSE-15. Rejects: a press on the title, the box the search is typed in, the line of keys, the
/// prompt shown in full beside the list or the border moving the cursor or taking a prompt, even
/// pressed twice, and a press of another button, a drag or a release acting.
#[test]
fn a_press_on_the_panel_off_the_matches_and_any_other_event_leave_the_search_alone() {
    let mut session = searching_twenty();
    let buffer = drawn(&mut session);
    let panel = session
        .laid
        .popup(crate::state::Panel::HistorySearch)
        .expect("the search is recorded")
        .area;
    let list = rows_of_the_matches(&session)[0];
    let before = cursor(&session);
    let beside = (list.y + 1, panel.right() - 3);
    for (row, column) in [
        (panel.y, panel.x + 3),
        (find_on(&buffer, "⌕").0, panel.x + 6),
        (panel.bottom() - 1, panel.x + 3),
        beside,
        (panel.y, panel.right() - 1),
    ] {
        for _ in 0..2 {
            press(&mut session, row, column, KeyModifiers::NONE);
        }
        assert!(session.searching_history(), "({row}, {column})");
        assert_eq!(cursor(&session), before, "({row}, {column})");
    }
    for kind in [
        MouseEventKind::Down(MouseButton::Right),
        MouseEventKind::Down(MouseButton::Middle),
        MouseEventKind::Up(MouseButton::Left),
        MouseEventKind::Drag(MouseButton::Left),
        MouseEventKind::Moved,
    ] {
        handle_mouse(&mut session, drag(kind, list.y, list.x + 2));
        handle_mouse(&mut session, drag(kind, list.y, list.x + 2));
        assert!(session.searching_history(), "{kind:?}");
        assert_eq!(cursor(&session), before, "{kind:?}");
    }
}

/// MOUSE-46. Rejects: a wheel turn that moves the cursor the wrong way, by another step than
/// `tui.wheelRows`, past the oldest match or the newest, that scrolls the transcript beneath, or that
/// takes or closes the search.
#[test]
fn the_wheel_walks_the_prompts_by_the_wheel_rows_and_stops_at_both_ends() {
    let mut session = searching_twenty();
    for n in 0..40 {
        session.transcript.push(crate::state::Entry::user(format!(
            "a line to scroll through {n}"
        )));
    }
    drawn(&mut session);
    let scrolled = session.scroll;
    let step = usize::from(session.wheel_rows());
    let name = |n: usize| format!("prompt {n:02}");

    handle_mouse(&mut session, drag(MouseEventKind::ScrollUp, 2, 2));
    assert_eq!(cursor(&session), Some(name(20 - step)), "up is older");
    handle_mouse(&mut session, drag(MouseEventKind::ScrollDown, 2, 2));
    assert_eq!(cursor(&session), Some(name(20)), "down is newer");
    for _ in 0..30 {
        handle_mouse(&mut session, drag(MouseEventKind::ScrollUp, 2, 2));
    }
    assert_eq!(cursor(&session), Some(name(1)), "past the oldest");
    for _ in 0..30 {
        handle_mouse(&mut session, drag(MouseEventKind::ScrollDown, 2, 2));
    }
    assert_eq!(cursor(&session), Some(name(20)), "past the newest");
    assert!(session.searching_history());
    assert_eq!(session.input(), "");
    assert_eq!(
        session.scroll, scrolled,
        "the wheel scrolled the transcript"
    );
}

/// MOUSE-47. Rejects: a left press outside the panel leaving the search open, or taking the prompt
/// under the cursor, a press of another button closing it, and the release that follows beginning a
/// copy of what the transcript beneath says. What was typed in the box stays as it was.
#[test]
fn a_press_outside_the_search_closes_it_and_leaves_the_box_as_it_was() {
    let mut session = searching_twenty();
    session.close_history_search();
    type_line(&mut session, "prompt");
    assert!(session.open_history_search());
    session
        .transcript
        .push(crate::state::Entry::user("a line of the transcript"));
    let buffer = drawn(&mut session);
    let (row, column) = find_on(&buffer, "a line of the transcript");

    handle_mouse(
        &mut session,
        drag(MouseEventKind::Down(MouseButton::Right), row, column),
    );
    assert!(session.searching_history(), "a right press closed it");

    let action = click(&mut session, row, column);
    assert!(
        !session.searching_history(),
        "the press outside left it open"
    );
    assert_eq!(session.input(), "prompt", "the box was changed");
    assert_ne!(action, Action::Copy, "the release was left to the frame");
    assert!(session.selection.is_none());
}
