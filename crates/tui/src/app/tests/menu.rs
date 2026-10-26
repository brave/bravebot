//! MOUSE-52: the small menu a right click opens over a reply, a delegate's block or a held selection.

use super::*;
use crate::state::{Choice, Hit, Panel};

/// A right press at `cell`, and what it answered.
fn right_press(session: &mut Session, cell: (u16, u16)) -> Action {
    handle_mouse(
        session,
        drag(MouseEventKind::Down(MouseButton::Right), cell.0, cell.1),
    )
}

/// The items of the menu that is open, top to bottom, or none where none is.
fn choices(session: &Session) -> Vec<Choice> {
    session
        .menu()
        .map(|menu| menu.choices().to_vec())
        .unwrap_or_default()
}

/// The place in the transcript of the entry that holds `words`.
fn place_of(session: &Session, words: &str) -> usize {
    session
        .transcript
        .iter()
        .position(|entry| entry.text == words)
        .unwrap_or_else(|| panic!("no entry holds {words:?}"))
}

/// A press of the left button at `cell`, and what it answered.
fn press_on(session: &mut Session, cell: (u16, u16)) -> Action {
    handle_mouse(
        session,
        drag(MouseEventKind::Down(MouseButton::Left), cell.0, cell.1),
    )
}

/// A session at rest whose transcript is two replies and a prompt, drawn 120 by 24 with the info
/// panel beside it, with the cell of a word of the latest.
fn over_a_reply() -> (Session, (u16, u16)) {
    let (session, buffer) = at_rest_over_two_replies();
    let cell = find_on(&buffer, "The latest answer.");
    (session, cell)
}

/// MOUSE-52. Rejects: a menu that opens over anything but what it has items for, with items that are
/// not the ones for what is there: a reply (on its words, its marker and the blank of its row), a
/// delegate's block and a selection the pointer is inside. A right press on a prompt, on a blank
/// row, on the hint row, on a row of the panel or on a call opens none, and the release of the press
/// that opened it leaves it open.
#[test]
fn a_right_press_opens_the_menu_of_what_is_under_it_and_nowhere_else() {
    let (mut session, (row, column)) = over_a_reply();
    let latest = place_of(&session, "The latest answer.");
    let wanted = vec![
        Choice::Copy(latest),
        Choice::CopyAsText(latest),
        Choice::Branch,
    ];
    for cell in [(row, column), (row, 0), (row, 80)] {
        session.close_the_menu();
        assert_eq!(right_press(&mut session, cell), Action::Redraw, "{cell:?}");
        assert_eq!(choices(&session), wanted, "{cell:?}");
    }
    // A reply the screen wrapped over several rows is its words on each of them, and a reply that is
    // not the latest is its own.
    let (mut session, buffer) = at_rest_over_two_replies();
    let earlier = place_of(&session, AN_EARLIER_REPLY);
    let mut rows = std::collections::BTreeSet::new();
    for words in [
        "The first answer.",
        "paragraph with",
        "rows of the transcript.",
    ] {
        let cell = find_on(&buffer, words);
        rows.insert(cell.0);
        session.close_the_menu();
        right_press(&mut session, cell);
        assert_eq!(
            choices(&session)[0],
            Choice::Copy(earlier),
            "{words:?} is not the earlier reply's"
        );
    }
    assert!(
        rows.len() >= 3,
        "the fixture's reply is not wrapped: {rows:?}"
    );
    // The release of the press that opened it does not close it.
    let (mut session, (row, _)) = over_a_reply();
    right_press(&mut session, (row, 80));
    handle_mouse(
        &mut session,
        drag(MouseEventKind::Up(MouseButton::Right), row, 80),
    );
    assert!(session.menu_open(), "the release closed the menu");
    session.close_the_menu();

    // Nowhere else a reply is not.
    let (mut session, buffer) = at_rest_over_two_replies();
    for (what, cell) in [
        ("a prompt", find_on(&buffer, "and then")),
        ("a blank row", (1, 110)),
        ("the row of hints", (23, 3)),
    ] {
        assert_eq!(right_press(&mut session, cell), Action::None, "{what}");
        assert!(!session.menu_open(), "{what} opened a menu");
    }
    let (mut session, buffer) = panel_with_a_model();
    let cell = find_on(&buffer, "claude-sonnet");
    assert_eq!(right_press(&mut session, cell), Action::None);
    assert!(!session.menu_open(), "a row of the panel opened a menu");
    let session = scroller_over_one_call();
    let cell = find_on_text(&session, "out 0");
    let mut session = session;
    assert_eq!(right_press(&mut session, cell), Action::None);
    assert!(!session.menu_open(), "a call opened a menu");

    // A delegate's block opens the view of that delegate.
    let (mut session, buffer) = at_rest_over_two_delegates();
    let cell = find_on(&buffer, "write the docs");
    let block = session
        .transcript
        .iter()
        .rposition(|entry| entry.delegate.is_some())
        .expect("a delegate's block");
    assert_eq!(right_press(&mut session, cell), Action::Redraw);
    assert_eq!(choices(&session), vec![Choice::OpenDelegate(block)]);
}

/// MOUSE-52. Rejects: a held selection that the menu does not know of, so that "copy" copies the
/// reply and not what was swept, and one the pointer is outside of being offered a copy of itself.
#[test]
fn with_a_selection_held_and_the_pointer_inside_it_the_menu_copies_the_selection() {
    let mut session = a_long_reply(80);
    let buffer = drawn_at(&mut session, 120, 24);
    let (row, column) = find_cell(&buffer, "row 70");
    swept(&mut session, (row, column), (row + 1, column + 6));
    assert!(session.holds_a_selection());

    assert_eq!(right_press(&mut session, (row, column + 2)), Action::Redraw);
    assert_eq!(choices(&session), vec![Choice::CopySelection]);

    // Taken, it asks for the copy of the selection, which carries out as letting go of the button
    // did and hands the clipboard the words that were swept.
    let action = handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(action, Action::Copy);
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 24)).expect("terminal");
    let mut handed = Vec::new();
    copy_selection(&mut terminal, &mut session, |text| {
        handed.push(text.to_string());
        true
    })
    .expect("copied");
    assert_eq!(
        handed,
        vec!["row 70 of the reply\n  row 71".to_string()],
        "the menu did not copy what was swept"
    );

    // Outside it the menu is that of the reply under the pointer, and the selection is still held.
    let mut session = a_long_reply(80);
    let buffer = drawn_at(&mut session, 120, 24);
    let (row, column) = find_cell(&buffer, "row 70");
    swept(&mut session, (row, column), (row, column + 6));
    assert_eq!(
        right_press(&mut session, (row - 3, column + 2)),
        Action::Redraw
    );
    assert!(
        !choices(&session).contains(&Choice::CopySelection),
        "{:?}",
        choices(&session)
    );
    assert!(session.holds_a_selection());
}

/// MOUSE-52. Rejects: a menu drawn off the screen or wrapped, at any pointer: it is clamped inside the
/// frame, each item is on one row, and the rows an item is pressed on are the rows it is drawn on. The
/// pointer is put on the corners of the part of the transcript that is a reply and on its middle, so
/// that a menu opens at every size tried, the smallest included, and the clamping is asked of each.
#[test]
fn the_menu_is_drawn_inside_the_screen_wherever_the_pointer_is_and_wraps_nothing() {
    for (columns, lines) in [(120u16, 24u16), (30, 10), (12, 8)] {
        let mut session = a_long_reply(40);
        drawn_at(&mut session, columns, lines);
        let area = session.laid.window.expect("a transcript was drawn").area;
        // The last rows of the transcript are the turn's own line and a blank, which no reply owns.
        let last = area.bottom().saturating_sub(3).max(area.y);
        let corners = [
            (area.y, area.x),
            (area.y, area.right() - 1),
            (last, area.x),
            (last, area.right() - 1),
            (area.y + area.height / 2, area.x + area.width / 2),
        ];
        for cell in corners {
            let mut session = a_long_reply(40);
            drawn_at(&mut session, columns, lines);
            right_press(&mut session, cell);
            assert!(
                session.menu_open(),
                "{columns}x{lines} at {cell:?}: no menu opened over the transcript {area:?}"
            );
            let buffer = drawn_at(&mut session, columns, lines);
            let popup = session
                .laid
                .popup(Panel::Menu)
                .expect("the menu was not recorded");
            let screen = ratatui::layout::Rect::new(0, 0, columns, lines);
            assert_eq!(
                popup.area.intersection(screen),
                popup.area,
                "{columns}x{lines} at {cell:?}: the menu is off the screen: {:?}",
                popup.area
            );
            let list = popup.list.expect("the items were not recorded");
            assert!(popup.area.contains((list.x, list.y).into()));
            // Each item is one row, in order, and the cursor is on the first.
            let rows: Vec<String> = (list.y..list.bottom())
                .map(|y| {
                    (list.x..list.right())
                        .map(|x| buffer[(x, y)].symbol())
                        .collect::<String>()
                })
                .collect();
            assert!(
                rows[0].contains('\u{276f}'),
                "{columns}x{lines}: the cursor is not on the first item: {rows:?}"
            );
            assert!(usize::from(list.height) <= session.menu().expect("a menu").choices().len());
            if columns >= 30 && lines >= 10 {
                assert!(rows[0].contains(t!(menu_copy)), "{rows:?}");
                assert!(rows[1].contains(t!(menu_copy_text)), "{rows:?}");
                assert!(rows[2].contains(t!(menu_branch)), "{rows:?}");
            }
        }
    }

    // A screen with no row of a reply on it has nothing to hang a menu on, wherever it is pressed.
    let mut session = a_long_reply(40);
    drawn_at(&mut session, 12, 5);
    for row in 0..5 {
        for column in 0..12 {
            right_press(&mut session, (row, column));
            assert!(
                !session.menu_open(),
                "12x5: a menu opened at {:?} over no reply",
                (row, column)
            );
        }
    }
}

/// MOUSE-52. Rejects: a menu with no keyboard, one whose keys also reach the box, one that cannot be
/// closed with no effect, and a cursor that runs past an end. Up and Down, `k` and `j` move the
/// cursor and stop at the first item and the last, Enter takes the item, Escape and Ctrl-C close the
/// menu and change nothing, and every other key does nothing at all, the box included.
#[test]
fn the_menu_is_walked_taken_and_closed_with_the_keys() {
    let (mut session, cell) = over_a_reply();
    right_press(&mut session, cell);
    let cursor = |session: &Session| session.menu().map(|menu| menu.cursor());
    assert_eq!(cursor(&session), Some(0));

    for (code, from, to) in [
        (KeyCode::Down, 0, 1),
        (KeyCode::Char('j'), 1, 2),
        (KeyCode::Down, 2, 2),
        (KeyCode::Up, 2, 1),
        (KeyCode::Char('k'), 1, 0),
        (KeyCode::Up, 0, 0),
    ] {
        assert_eq!(cursor(&session), Some(from));
        assert_eq!(handle_key(&mut session, key(code)), Action::Redraw);
        assert_eq!(cursor(&session), Some(to), "{code:?}");
    }

    // Keys the menu has no use for change nothing, and are not typed into the box under it.
    for code in [
        KeyCode::Char('a'),
        KeyCode::Tab,
        KeyCode::Backspace,
        KeyCode::Left,
        KeyCode::Char('/'),
    ] {
        assert_eq!(
            handle_key(&mut session, key(code)),
            Action::None,
            "{code:?}"
        );
    }
    assert_eq!(session.input(), "");
    assert!(session.menu_open());

    // Escape and Ctrl-C close it and do nothing else: the box, the transcript and the clipboard are
    // as they were.
    for close in [
        key(KeyCode::Esc),
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
    ] {
        right_press(&mut session, cell);
        assert!(session.menu_open());
        let before = session.transcript.len();
        assert_eq!(handle_key(&mut session, close), Action::Redraw, "{close:?}");
        assert!(!session.menu_open(), "{close:?} left the menu up");
        assert_eq!(session.input(), "");
        assert_eq!(session.transcript.len(), before);
        assert!(session.copied.is_none());
        assert_eq!(session.status, crate::state::Status::Idle);
    }

    // With no menu the same keys are the box's again.
    handle_key(&mut session, key(KeyCode::Char('a')));
    assert_eq!(session.input(), "a");
}

/// MOUSE-52. The same two delegates numbered alike, taken through the menu: the item that opens
/// the delegate of a block opens the delegate of that block and not the first one that has its
/// number, whichever block the right click was on.
#[test]
fn the_menu_opens_the_delegate_of_the_block_when_two_turns_number_theirs_alike() {
    for task in ["first turn task", "second turn task"] {
        let (mut session, buffer) = at_rest_over_a_delegate_in_each_of_two_turns();
        let (row, column) = find_on(&buffer, task);
        right_press(&mut session, (row, column));
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Redraw
        );
        assert_eq!(
            session
                .watched_delegate()
                .map(|delegate| delegate.task.as_str()),
            Some(task)
        );
    }
}

/// MOUSE-52. Rejects: a typed `/copy text` that copies another text than the menu's `copy as text`
/// item does for the same reply, which would leave the keyboard with a route to something else than the
/// click: the two put the same words on the clipboard.
#[test]
fn copy_as_text_and_the_typed_command_put_the_same_words_on_the_clipboard() {
    let mut session = Session::new("none");
    session.transcript.push(crate::state::Entry::assistant(
        "# A heading\n\nSome **bold**, a [link](https://example.test/a) and `code` in a paragraph \
         that the screen would wrap over several rows if it were drawn in a narrow transcript.\n\n\
         1. one\n2. two\n\n> quoted",
        Vec::new(),
    ));
    let buffer = drawn_at(&mut session, 60, 24);
    right_press(&mut session, find_on(&buffer, "A heading"));
    handle_key(&mut session, key(KeyCode::Down));
    let Action::CopyReply(by_the_menu) = handle_key(&mut session, key(KeyCode::Enter)) else {
        panic!("the item did not copy");
    };
    assert!(by_the_menu.contains("Some bold"), "{by_the_menu:?}");

    type_line(&mut session, "/copy text");
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::CopyReply(by_the_menu)
    );
}

/// MOUSE-52. Rejects: an item that does not do what the command or the click it stands for does, or
/// does it by another path: "copy" is what the marker's click copies and what `/copy` copies, "copy as
/// text" is the same reply as the screen shows it with none of its markdown marks and none of the
/// marker, "branch this session" is the command `/branch`, and "open this delegate" is the click on the
/// block. A copy is carried out whichever way the item is taken.
#[test]
fn each_item_does_what_the_command_or_the_click_it_stands_for_does() {
    // The words a click on the marker copies, which is what `/copy` copies for the reply.
    let (mut session, buffer) = at_rest_over_two_replies();
    let (named_row, named_column) = find_on(&buffer, "A named answer.");
    let clicked = {
        let (mut other, _) = at_rest_over_two_replies();
        match click(&mut other, named_row, 0) {
            Action::CopyReply(words) => words,
            other => panic!("the click on the marker did not copy a reply: {other:?}"),
        }
    };
    assert_eq!(clicked, "A named answer.");

    right_press(&mut session, (named_row, named_column));
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::CopyReply(clicked.clone()),
        "the item is not what the marker copies"
    );
    assert!(!session.menu_open());

    // Taken by a press on its row rather than by a key it is the same, and the press is the one that
    // answers: the release that follows it is the menu's and clicks nothing beneath.
    right_press(&mut session, (named_row, named_column));
    drawn(&mut session);
    let list = session
        .laid
        .popup(Panel::Menu)
        .and_then(|popup| popup.list)
        .expect("the items were not recorded");
    assert_eq!(
        press_on(&mut session, (list.y, list.x + 3)),
        Action::CopyReply(clicked),
        "a press on the item did not take it"
    );
    assert!(!session.menu_open());
    assert_eq!(
        handle_mouse(
            &mut session,
            drag(MouseEventKind::Up(MouseButton::Left), list.y, list.x + 3)
        ),
        Action::None
    );

    // The text of a reply as the screen shows it has none of the marks of the markdown it was
    // written in, and does not wrap the paragraphs the markdown wrote as one line.
    let mut session = Session::new("none");
    session.transcript.push(crate::state::Entry::assistant(
        "# A heading\n\nSome **bold** and `code` and a long paragraph that the screen would wrap over \
         several rows if it were drawn in a narrow transcript, kept as one line.\n\n- one\n- two",
        Vec::new(),
    ));
    let buffer = drawn_at(&mut session, 60, 20);
    right_press(&mut session, find_on(&buffer, "A heading"));
    handle_key(&mut session, key(KeyCode::Down));
    let Action::CopyReply(text) = handle_key(&mut session, key(KeyCode::Enter)) else {
        panic!("the item did not copy");
    };
    assert!(text.contains("A heading"), "{text:?}");
    assert!(
        text.contains("Some bold and code and a long paragraph"),
        "{text:?}"
    );
    assert!(text.contains("kept as one line."), "{text:?}");
    for mark in ["# ", "**", "`", "\u{23fa}"] {
        assert!(!text.contains(mark), "{mark:?} is in {text:?}");
    }
    assert!(
        !text
            .lines()
            .any(|line| line.starts_with("  ") && line.contains("Some")),
        "the margin of the screen is in the text: {text:?}"
    );

    // Branching is the command: at the plain prompt it asks for what typing it asks for.
    let mut typed = Session::new("none");
    type_line(&mut typed, "/branch");
    let by_the_command = handle_key(&mut typed, key(KeyCode::Enter));
    let (mut session, cell) = over_a_reply();
    right_press(&mut session, cell);
    handle_key(&mut session, key(KeyCode::Down));
    handle_key(&mut session, key(KeyCode::Down));
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        by_the_command,
        "the item is not the command"
    );
    assert_eq!(by_the_command, Action::Branch(String::new()));

    // The block of a delegate opens that delegate's view, which the click on it opens.
    let (mut session, buffer) = at_rest_over_two_delegates();
    right_press(&mut session, find_on(&buffer, "write the docs"));
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert_eq!(
        session
            .watched_delegate()
            .map(|delegate| delegate.task.as_str()),
        Some("write the docs")
    );
}

/// MOUSE-52. Rejects: an item that does what its command refuses mid-turn, or that does it with another
/// note: `/branch` waits for the turn to end and says why, as it does typed, and the menu is open and
/// answers its keys while the turn runs.
#[test]
fn branching_from_the_menu_while_a_turn_runs_waits_as_the_typed_command_does() {
    let working = || {
        let (mut session, buffer) = at_rest_over_two_replies();
        session.type_char('a');
        session.submit();
        let buffer = {
            let _ = buffer;
            drawn_at(&mut session, 120, 24)
        };
        (session, buffer)
    };

    let (mut typed, _) = working();
    for c in "/branch".chars() {
        handle_key_while_working(&mut typed, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut typed, key(KeyCode::Enter));

    let (mut session, buffer) = working();
    let cell = find_on(&buffer, "The latest answer.");
    assert_eq!(right_press(&mut session, cell), Action::Redraw);
    assert!(session.menu_open(), "no menu opened while a turn runs");
    handle_key_while_working(&mut session, key(KeyCode::Down));
    handle_key_while_working(&mut session, key(KeyCode::Down));
    handle_key_while_working(&mut session, key(KeyCode::Enter));
    assert!(!session.menu_open());

    assert_eq!(
        session.queued.len(),
        typed.queued.len(),
        "not queued as typed"
    );
    assert!(session.queued.len() == 1, "{:?}", session.queued);
    let said = |session: &Session| {
        session
            .transcript
            .iter()
            .map(|entry| entry.text.clone())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let note = said(&typed);
    assert!(
        said(&session).ends_with(note.rsplit('\n').next().expect("a line")),
        "the note a typed `/branch` leaves is not the note the item leaves"
    );
}

/// MOUSE-52 and MOUSE-47. Rejects: a press outside the menu that does anything but close it, a press
/// of another button, a drag or a release that closes it, a press on its border that takes an item
/// or closes it, and the wheel scrolling the transcript beneath it, where a turn of the wheel moves
/// its cursor by `tui.wheelRows` and stops at the ends.
#[test]
fn a_press_outside_the_menu_closes_it_and_nothing_else_does() {
    let (mut session, cell) = over_a_reply();
    right_press(&mut session, cell);
    drawn(&mut session);
    let popup = session.laid.popup(Panel::Menu).expect("a menu");
    let list = popup.list.expect("items");
    // The top right corner of the screen, which the menu does not reach from a pointer in the
    // transcript.
    let outside = (0, 119);
    assert!(
        !popup
            .area
            .contains(ratatui::layout::Position::new(outside.1, outside.0)),
        "the menu {:?} covers the cell the press is to be outside it at",
        popup.area
    );

    // Another button elsewhere, a drag and a release do not close it, and a right press does not open
    // another where the pointer is.
    for kind in [
        MouseEventKind::Down(MouseButton::Right),
        MouseEventKind::Down(MouseButton::Middle),
        MouseEventKind::Drag(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
        MouseEventKind::Moved,
    ] {
        handle_mouse(&mut session, drag(kind, outside.0, outside.1));
        assert!(session.menu_open(), "{kind:?} closed the menu");
    }
    // The border and the blank between its rows are neither an item nor outside.
    handle_mouse(
        &mut session,
        drag(
            MouseEventKind::Down(MouseButton::Left),
            popup.area.y,
            popup.area.x + 2,
        ),
    );
    assert!(session.menu_open(), "a press on the border closed it");

    // The wheel moves the cursor and leaves the transcript where it is.
    let scrolled = session.scroll;
    handle_mouse(&mut session, drag(MouseEventKind::ScrollDown, 5, 5));
    assert_eq!(session.menu().map(|menu| menu.cursor()), Some(2));
    assert_eq!(
        session.scroll, scrolled,
        "the wheel scrolled what is beneath"
    );
    handle_mouse(&mut session, drag(MouseEventKind::ScrollUp, 5, 5));
    assert_eq!(session.menu().map(|menu| menu.cursor()), Some(0));

    // A left press on an item takes it and moves nothing else: the row under the menu is not clicked.
    let action = press_on(&mut session, (list.y + 1, list.x + 3));
    assert!(matches!(action, Action::CopyReply(_)), "{action:?}");
    assert!(!session.menu_open());

    // A left press outside closes it with no other effect. The frame that follows the item taken is
    // drawn first, as a loop draws it, so the menu that was is no longer a panel the press lands on.
    drawn(&mut session);
    right_press(&mut session, cell);
    drawn(&mut session);
    let before = (session.input().to_string(), session.transcript.len());
    let action = handle_mouse(
        &mut session,
        drag(
            MouseEventKind::Down(MouseButton::Left),
            outside.0,
            outside.1,
        ),
    );
    assert_eq!(action, Action::Redraw);
    assert!(!session.menu_open());
    assert_eq!(
        before,
        (session.input().to_string(), session.transcript.len())
    );
    // And the release that ends that press is the panel's.
    let action = handle_mouse(
        &mut session,
        drag(MouseEventKind::Up(MouseButton::Left), outside.0, outside.1),
    );
    assert_eq!(action, Action::None);
}

/// MOUSE-52. Rejects: a menu that opens where it must not, which is while a panel that holds the input
/// is up, while a command the person typed runs, where the setting turned it off, in a delegate's view
/// whose entries are the delegate's own, and one that outlives the terminal's size. The setting turns
/// it off only as a boolean `false`, and leaves the mouse on.
#[test]
fn the_menu_does_not_open_where_it_must_not_and_is_dropped_on_a_resize() {
    use ratatui::crossterm::event::Event as TermEvent;
    // The setting.
    let off = bravebot_config::Settings::parse(r#"{"tui": {"contextMenu": false}}"#);
    let on = bravebot_config::Settings::parse(r#"{"tui": {"contextMenu": true}}"#);
    let (mut session, cell) = over_a_reply();
    adopt_settings(&mut session, &off, None);
    assert!(
        session.mouse_captured(),
        "the menu's setting took the mouse"
    );
    assert_eq!(right_press(&mut session, cell), Action::None);
    assert!(!session.menu_open(), "the setting did not turn it off");
    adopt_settings(&mut session, &on, None);
    assert_eq!(right_press(&mut session, cell), Action::Redraw);
    assert!(session.menu_open());
    session.close_the_menu();
    let (mut unnamed, cell) = over_a_reply();
    adopt_settings(&mut unnamed, &bravebot_config::Settings::parse("{}"), None);
    assert_eq!(right_press(&mut unnamed, cell), Action::Redraw);

    // A command that is running reads no key of the menu's.
    let (mut session, cell) = over_a_reply();
    session.status = crate::state::Status::Running;
    assert_eq!(right_press(&mut session, cell), Action::None);
    assert!(!session.menu_open());

    // A picker holds the input.
    let (mut session, cell) = over_a_reply();
    session.open_picking(crate::picking::Picking::Effort(
        crate::effort_prompt::Picker::new(None),
    ));
    drawn(&mut session);
    assert_eq!(right_press(&mut session, cell), Action::Redraw);
    assert!(!session.menu_open(), "a menu opened over a picker");
    assert!(session.picking().is_some(), "the press closed the picker");

    // The files state draws other rows beside the transcript and owns the keys.
    let (mut session, cell) = over_a_reply();
    super::standing::enter_files(&mut session);
    assert_eq!(right_press(&mut session, cell), Action::None);
    assert!(!session.menu_open());

    // A delegate's view draws the delegate's own entries.
    let (mut session, cell) = over_a_reply();
    session.delegate_started(bravebot_agent::report::Delegation {
        stop: Default::default(),
        id: bravebot_agent::report::DelegateId::nth(1),
        kind: "reader".to_string(),
        task: "find the parser".to_string(),
    });
    assert!(session.watch());
    assert_eq!(right_press(&mut session, cell), Action::None);
    assert!(!session.menu_open());

    // A resize drops it.
    let (mut session, cell) = over_a_reply();
    right_press(&mut session, cell);
    assert!(session.menu_open());
    crate::app::keys::took_input(&mut session, &TermEvent::Resize(100, 30));
    assert!(!session.menu_open(), "the menu outlived a resize");
}

/// A scroller over a reply and the call that follows it, which returned thirty lines and draws five,
/// at 80 by 10 with the frame recorded, and the cell of the reply's words.
fn a_scroller_over_a_reply_and_a_call() -> (Session, (u16, u16)) {
    use bravebot_agent::report::{Activity, Landing, Returned};
    let mut session = Session::new("kernel-enforced");
    session.complete("The build output follows.", Vec::new(), 0);
    session.finish_activity(Activity::running("Run", "make").done("ok"));
    session.landed(Landing::Context);
    let lines: Vec<String> = (0..30).map(|n| format!("out {n}")).collect();
    session.returned(Returned {
        lines: lines[..5].to_vec(),
        total: 30,
        from_the_end: false,
        whole: lines,
    });
    session.open_scroller();
    let buffer = drawn_at(&mut session, 80, 10);
    let cell = find_cell(&buffer, "The build output");
    (session, cell)
}

/// MOUSE-52. Rejects: the menu opening in the scroller with items that act on a transcript that is not
/// the scroller's, and a menu that is not a panel that holds the input. The scroller's reply is a
/// reply, so the menu opens over it, and the rows it is drawn over include the call that follows it,
/// which a click expands in the scroller. A left press on an item of the menu takes the item and
/// leaves the call collapsed, where the same press with no menu expands it.
#[test]
fn the_menu_opens_in_the_scroller_and_a_press_on_it_reaches_nothing_beneath() {
    let (mut session, cell) = a_scroller_over_a_reply_and_a_call();
    let buffer = drawn_at(&mut session, 80, 10);
    assert!(!render::as_text(&session).contains("out 29"));

    // The control: a click on the call's row, where the menu will be drawn, expands the call.
    let call_row = find_cell(&buffer, "Run(make)").0;
    let mut control = a_scroller_over_a_reply_and_a_call().0;
    assert_eq!(click(&mut control, call_row, cell.1 + 3), Action::Redraw);
    assert!(
        render::as_text(&control).contains("out 29"),
        "a click on the call did not expand it, so the fixture shows nothing"
    );

    assert_eq!(right_press(&mut session, cell), Action::Redraw);
    assert!(session.menu_open(), "no menu opened in the scroller");
    let wanted = choices(&session);
    assert_eq!(wanted.len(), 3, "{wanted:?}");
    drawn_at(&mut session, 80, 10);

    let popup = session.laid.popup(Panel::Menu).expect("a menu");
    let list = popup.list.expect("items");
    let mut over_the_call = None;
    for y in list.y..list.bottom() {
        let found = session.target_at(y, list.x + 2).map(|target| target.hit);
        assert!(
            matches!(found, Some(Hit::PopupRows(Panel::Menu))),
            "row {y} is not the menu's: {found:?}"
        );
        if y == call_row {
            over_the_call = Some(y);
        }
    }
    let over_the_call = over_the_call.expect("the menu is not drawn over the call's row");

    // The press takes the item drawn there, which is the one of that row, and not the call.
    let action = press_on(&mut session, (over_the_call, list.x + 3));
    let taken = usize::from(over_the_call - list.y);
    assert_eq!(wanted[taken], Choice::CopyAsText(0));
    assert!(
        matches!(action, Action::CopyReply(_)),
        "the press on item {taken} answered {action:?}"
    );
    assert!(!session.menu_open(), "the press did not take the item");
    handle_mouse(
        &mut session,
        drag(
            MouseEventKind::Up(MouseButton::Left),
            over_the_call,
            list.x + 3,
        ),
    );
    assert!(
        !render::as_text(&session).contains("out 29"),
        "the press reached the call beneath the menu"
    );
}

/// MOUSE-52. Rejects: the key that closes the menu also stopping the turn that is running, which a
/// person pressing Escape to put away a menu did not ask for, and a copy a key asked for while work
/// runs being dropped by a loop that has no use for it: the loops that carry out a key hand a copy on
/// to the one that holds the terminal.
#[test]
fn a_key_that_closes_or_takes_from_the_menu_while_work_runs_does_not_stop_it_and_the_copy_is_handed_on()
 {
    use super::super::keys::stops_the_turn;
    let (mut session, buffer) = at_rest_over_two_replies();
    session.type_char('a');
    session.submit();
    let cell = find_on(&drawn_at(&mut session, 120, 24), "The latest answer.");
    let _ = buffer;
    let escape = key(KeyCode::Esc);
    let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert!(
        stops_the_turn(&session, escape),
        "the fixture's turn does not stop"
    );
    right_press(&mut session, cell);
    for stop in [escape, ctrl_c] {
        assert!(
            !stops_the_turn(&session, stop),
            "{stop:?} stops the turn while the menu is up"
        );
    }
    handle_key_while_working(&mut session, escape);
    assert!(!session.menu_open());
    assert!(
        session.a_turn_is_running(),
        "closing the menu stopped the turn"
    );
    assert!(stops_the_turn(&session, escape));

    // What a key asked for that needs the terminal is not dropped by `act_while_working`.
    for asked in [Action::Copy, Action::CopyReply("words".to_string())] {
        let handed = act_while_working(&mut session, asked, || crate::clipboard::Pasted::Nothing);
        assert!(
            matches!(handed, Action::Copy | Action::CopyReply(_)),
            "{handed:?}"
        );
    }
    assert_eq!(
        act_while_working(&mut session, Action::Redraw, || {
            crate::clipboard::Pasted::Nothing
        }),
        Action::None
    );
}

/// The words of the selection that "row 70" begins in `a_long_reply(80)`, swept from there to the next
/// row, with the selection held and the menu it opened drawn once so that its items are recorded.
fn a_menu_over_a_held_selection() -> (Session, (u16, u16), ratatui::layout::Rect) {
    let mut session = a_long_reply(80);
    let buffer = drawn_at(&mut session, 120, 24);
    let (row, column) = find_cell(&buffer, "row 70");
    swept(&mut session, (row, column), (row + 1, column + 6));
    assert!(session.holds_a_selection());
    assert_eq!(right_press(&mut session, (row, column + 2)), Action::Redraw);
    assert_eq!(choices(&session), vec![Choice::CopySelection]);
    drawn_at(&mut session, 120, 24);
    let list = session
        .laid
        .popup(Panel::Menu)
        .and_then(|popup| popup.list)
        .expect("the items were not recorded");
    (session, (row, column), list)
}

/// What a copy of the held selection hands the clipboard, carried out as the loop carries it out.
fn handed_for(session: &mut Session) -> Vec<String> {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 24)).expect("terminal");
    let mut handed = Vec::new();
    copy_selection(&mut terminal, session, |text| {
        handed.push(text.to_string());
        true
    })
    .expect("copied");
    handed
}

/// MOUSE-52. Rejects: a press on the menu's copy item that clears the selection the menu was opened
/// for before the item runs, so that the item copies nothing. The press is the one a person makes, on
/// the item as drawn, and what is copied is the words that were swept. A press of another button
/// while the menu is up, and the press outside it that closes it, leave the selection held as it was,
/// since the menu opened for it and closing the menu changes nothing else.
#[test]
fn a_press_on_the_item_that_copies_a_held_selection_copies_it() {
    let (mut session, _, list) = a_menu_over_a_held_selection();
    assert_eq!(
        press_on(&mut session, (list.y, list.x + 3)),
        Action::Copy,
        "a press on the item did not ask for the copy"
    );
    assert!(
        session.holds_a_selection(),
        "the press dropped the selection"
    );
    assert_eq!(
        handed_for(&mut session),
        vec!["row 70 of the reply\n  row 71".to_string()],
        "the item copied nothing, or other words than were swept"
    );

    // Another button, while the menu is up, and the press that closes it.
    for button in [MouseButton::Right, MouseButton::Middle] {
        let (mut session, _, _) = a_menu_over_a_held_selection();
        handle_mouse(&mut session, drag(MouseEventKind::Down(button), 0, 119));
        assert!(session.menu_open(), "{button:?} closed the menu");
        assert!(
            session.holds_a_selection(),
            "a press of {button:?} dropped the selection"
        );
    }
    let (mut session, _, _) = a_menu_over_a_held_selection();
    handle_mouse(
        &mut session,
        drag(MouseEventKind::Down(MouseButton::Left), 0, 119),
    );
    assert!(!session.menu_open());
    assert!(
        session.holds_a_selection(),
        "the press that closed the menu dropped the selection"
    );
}

/// MOUSE-46 and MOUSE-52. The idle loop draws a frame only for an event that returns something, so a
/// turn of the wheel that moves a cursor and returns nothing leaves the screen showing the cursor
/// where it was. Rejects: the wheel walking the menu, the search over earlier prompts and a picker
/// left open at rest with no frame asked for, each tried through the function the idle loop answers an
/// event with, and the frame that is then drawn showing the cursor on the next item.
#[test]
fn a_turn_of_the_wheel_over_a_panel_that_holds_the_input_asks_for_the_frame_that_shows_it() {
    use ratatui::crossterm::event::Event as TermEvent;
    let wheel = TermEvent::Mouse(drag(MouseEventKind::ScrollDown, 5, 5));

    // The menu: the cursor, which a turn of the wheel moves by `tui.wheelRows` and so to the last of
    // its three items, is drawn there afterwards.
    let (mut session, cell) = over_a_reply();
    right_press(&mut session, cell);
    drawn(&mut session);
    assert_eq!(idle_action(&mut session, wheel.clone()), Action::Redraw);
    let buffer = drawn(&mut session);
    let (row, column) = find_cell(&buffer, t!(menu_branch));
    assert_eq!(buffer[(column - 2, row)].symbol(), "\u{276f}");

    // The search over the prompts already sent.
    let mut session = Session::new("none");
    session.history = crate::history::History::from_entries(
        (0..20)
            .map(|n| bravebot_session::store::Entry::sent(format!("prompt {n:02}"), None))
            .collect(),
    );
    assert!(session.open_history_search(), "no search opened");
    drawn(&mut session);
    let cursor = |session: &Session| {
        let matches = session.history_matches().len();
        session
            .history_search()
            .and_then(|search| search.at(matches))
    };
    let before = cursor(&session);
    let wheel_up = TermEvent::Mouse(drag(MouseEventKind::ScrollUp, 5, 5));
    assert_eq!(idle_action(&mut session, wheel_up), Action::Redraw);
    assert_ne!(
        cursor(&session),
        before,
        "the wheel did not move the cursor of the search"
    );

    // A picker a turn opened and that is still open at rest.
    let (mut session, _) = panel_with_a_context();
    session.open_picking(crate::picking::Picking::Effort(
        crate::effort_prompt::Picker::new(None),
    ));
    drawn(&mut session);
    assert_eq!(idle_action(&mut session, wheel), Action::Redraw);
}

/// MOUSE-52. Rejects: a menu that survives into a command the person typed. A menu is opened while a
/// turn runs, a shell line queued behind the turn starts as the turn ends, and the menu stays up over
/// it: Escape and Ctrl-C then close nothing and stop nothing, since the loop that runs a command
/// reads only the keys that stop it, and a press on an item returns an action that loop drops. The
/// menu is closed when the command begins and the two keys reach the command.
#[test]
fn a_menu_left_open_when_a_command_begins_is_closed_and_the_keys_reach_the_command() {
    for stop in [
        key(KeyCode::Esc),
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
    ] {
        let (mut session, buffer) = at_rest_over_two_replies();
        session.type_char('a');
        session.submit();
        let cell = find_on(&drawn_at(&mut session, 120, 24), "The latest answer.");
        let _ = buffer;
        right_press(&mut session, cell);
        assert!(session.menu_open(), "no menu opened while the turn ran");

        session.begin_command();
        assert!(
            !session.menu_open(),
            "the menu outlived the start of a command"
        );

        let cancel = Cancel::new();
        command_key(&mut session, stop, &cancel);
        assert!(
            cancel.is_cancelled(),
            "{stop:?} did not stop the command with the menu left up"
        );
    }
}

/// MOUSE-52. Rejects: the menu of a held selection opening in a delegate's view or in the files
/// state, which the clause says have none: the selection is checked before the state, so the menu
/// opened over rows that are not the session's. A selection held at the plain prompt still opens it,
/// which is the fixture's control.
#[test]
fn the_menu_of_a_held_selection_does_not_open_in_a_delegates_view_or_the_files_state() {
    for state in ["a delegate's view", "the files state"] {
        let (mut session, cell) = over_a_reply();
        match state {
            "a delegate's view" => {
                session.delegate_started(bravebot_agent::report::Delegation {
                    stop: Default::default(),
                    id: bravebot_agent::report::DelegateId::nth(1),
                    kind: "reader".to_string(),
                    task: "find the parser".to_string(),
                });
                assert!(session.watch());
            }
            _ => super::standing::enter_files(&mut session),
        }
        let buffer = drawn(&mut session);
        let (row, column) = match state {
            "a delegate's view" => find_on(&buffer, "find the parser"),
            _ => cell,
        };
        swept(&mut session, (row, column), (row, column + 6));
        assert!(session.holds_a_selection(), "{state}: no selection held");

        assert_eq!(
            right_press(&mut session, (row, column + 2)),
            Action::None,
            "{state}"
        );
        assert!(
            !session.menu_open(),
            "{state}: a menu opened over a selection"
        );
    }

    let (mut session, cell) = over_a_reply();
    swept(&mut session, cell, (cell.0, cell.1 + 6));
    assert_eq!(
        right_press(&mut session, (cell.0, cell.1 + 2)),
        Action::Redraw
    );
    assert!(session.menu_open(), "the control opened no menu");
}

/// MOUSE-52. Rejects: a paste reaching the box under the open menu, which holds the input: the words
/// are in the box and nothing says so, as a paste under a picker or the search would not be. The
/// words are pasted at rest and while a turn runs, and the box is as it was.
#[test]
fn a_paste_under_the_open_menu_is_not_typed_into_the_box() {
    let (mut session, cell) = over_a_reply();
    right_press(&mut session, cell);
    assert_eq!(handle_paste(&mut session, "pasted words"), Action::None);
    assert_eq!(
        session.input(),
        "",
        "the paste reached the box under the menu"
    );

    let (mut session, buffer) = at_rest_over_two_replies();
    session.type_char('a');
    session.submit();
    let cell = find_on(&drawn_at(&mut session, 120, 24), "The latest answer.");
    let _ = buffer;
    right_press(&mut session, cell);
    assert!(session.menu_open());
    assert_eq!(
        handle_paste_while_working(&mut session, "pasted words"),
        Action::None
    );
    assert_eq!(
        session.input(),
        "",
        "the paste reached the box under the menu"
    );
}
