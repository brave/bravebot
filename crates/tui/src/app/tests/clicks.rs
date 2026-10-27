//! MOUSE-50: what makes a release the end of a click, for every kind of target.

use super::popups::{click_held, targets};
use super::*;

/// A mouse event at `cell` with `modifiers` held.
fn at(kind: MouseEventKind, cell: (u16, u16), modifiers: KeyModifiers) -> MouseEvent {
    let mut event = drag(kind, cell.0, cell.1);
    event.modifiers = modifiers;
    event
}

/// MOUSE-9, MOUSE-10 and the clauses of the other kinds. Rejects: a release that is not on the cell
/// the press was on ending a click on the target the press was on, which a terminal that reports no
/// motion with a button held produces when the pointer moves between a press and its release. Every
/// kind of target the table of fixtures holds is pressed on and released one cell beside, and none
/// acts, where the same press and release on one cell act. The scrollbar is left out because a press
/// on it moves the view and no release is waited for.
#[test]
fn a_release_on_another_cell_than_the_press_ends_no_click_on_any_target() {
    for fixture in targets() {
        if fixture.name == "the scrollbar beside the transcript" {
            continue;
        }
        let (mut session, (row, column)) = (fixture.build)();
        let mut control = (fixture.build)().0;
        let action = click_held(&mut control, (row, column), fixture.modifiers);
        assert!(
            (fixture.acted)(&control, &action),
            "{}: a press and a release on one cell did nothing",
            fixture.name
        );

        handle_mouse(
            &mut session,
            at(
                MouseEventKind::Down(MouseButton::Left),
                (row, column),
                fixture.modifiers,
            ),
        );
        let action = handle_mouse(
            &mut session,
            at(
                MouseEventKind::Up(MouseButton::Left),
                (row, column + 1),
                fixture.modifiers,
            ),
        );
        assert!(
            !(fixture.acted)(&session, &action),
            "{}: a release one cell from the press acted ({action:?})",
            fixture.name
        );
        assert_eq!(
            action,
            Action::Copy,
            "{}: the release was not the end of a selection",
            fixture.name
        );
    }
}

/// MOUSE-50. A release ends a click on the target under it only where three things hold: it is on the
/// cell the press was on, the pointer moved the loose end of the selection nowhere between them, and
/// the press was a single click. The test above holds the first of them to every kind of target, and
/// this holds the other two. Rejects: a click made of a release that is on the press's cell after
/// the pointer swept elsewhere, which a terminal that reports motion with the button held and drops
/// the report of the return produces, and a click made of the second press of a double click.
#[test]
fn a_release_after_a_sweep_or_a_second_press_ends_no_click_on_any_target() {
    let mut swept_away = 0;
    for fixture in targets() {
        // A press on the scrollbar acts on the press, and the bar's own tests hold its drag.
        if fixture.name == "the scrollbar beside the transcript" {
            continue;
        }
        let down = |cell| {
            at(
                MouseEventKind::Down(MouseButton::Left),
                cell,
                fixture.modifiers,
            )
        };
        let up = |cell| {
            at(
                MouseEventKind::Up(MouseButton::Left),
                cell,
                fixture.modifiers,
            )
        };

        // Swept away from the cell, and let go where the press was with no report of the return.
        let (mut session, (row, column)) = (fixture.build)();
        handle_mouse(&mut session, down((row, column)));
        handle_mouse(
            &mut session,
            at(
                MouseEventKind::Drag(MouseButton::Left),
                (row, column + 2),
                fixture.modifiers,
            ),
        );
        if session
            .selection
            .is_some_and(|selection| !selection.held_still())
        {
            let action = handle_mouse(&mut session, up((row, column)));
            assert!(
                !(fixture.acted)(&session, &action),
                "{}: a release on the press's cell after a sweep acted ({action:?})",
                fixture.name
            );
            assert_eq!(
                action,
                Action::Copy,
                "{}: the release was not the end of the sweep",
                fixture.name
            );
            swept_away += 1;
        }

        // The second press of two, let go: a double click is a selection and not a click.
        let (mut session, cell) = (fixture.build)();
        handle_mouse(&mut session, down(cell));
        handle_mouse(&mut session, down(cell));
        let action = handle_mouse(&mut session, up(cell));
        assert!(
            !(fixture.acted)(&session, &action),
            "{}: the second press of a double click acted ({action:?})",
            fixture.name
        );
    }
    // A target at the edge of its pane cannot be swept off, so not every one is tried, but most are.
    assert!(
        swept_away >= 20,
        "only {swept_away} kinds of target were swept away from"
    );
}

/// The fixtures whose second press of a double click is not the rest of a click that acted: the
/// scrollbar acts on the press and no release is waited for, the text of the box puts the caret and a
/// double click in text selects the word as it does anywhere, and a path waits for the second press
/// to say whether the first was one (MOUSE-42).
const THE_SECOND_PRESS_IS_ITS_OWN: [&str; 3] = [
    "the scrollbar beside the transcript",
    "a place in the text of the box",
    "a file path in a reply",
];

/// MOUSE-50. A terminal reports the two clicks of a double click as a press, a release, a press and a
/// release, and the first release is a click that acts. Rejects: the second press beginning a
/// selection of the word under it, or its release copying one, after the click that acted: a double
/// click on the model row asking for the picker and then copying a word of the panel, one on the glyph
/// that hides the panel copying a word of the transcript that lay beneath it, and one on a mark or a
/// hint doing what it does twice. Every kind of target the table of fixtures holds is double clicked
/// as a terminal reports it and acts once, with the second press and its release answering nothing
/// and holding no selection, a triple click does the same, and a fourth press is a press again.
#[test]
fn a_double_click_on_a_target_acts_once_and_selects_nothing() {
    for fixture in targets() {
        if THE_SECOND_PRESS_IS_ITS_OWN.contains(&fixture.name) {
            continue;
        }
        let (mut session, cell) = (fixture.build)();
        let acted = click_held(&mut session, cell, fixture.modifiers);
        assert!(
            (fixture.acted)(&session, &acted),
            "{}: the first click did nothing",
            fixture.name
        );

        for press in [2, 3] {
            let down = handle_mouse(
                &mut session,
                at(
                    MouseEventKind::Down(MouseButton::Left),
                    cell,
                    fixture.modifiers,
                ),
            );
            let up = handle_mouse(
                &mut session,
                at(
                    MouseEventKind::Up(MouseButton::Left),
                    cell,
                    fixture.modifiers,
                ),
            );
            assert_eq!(
                (down, up),
                (Action::None, Action::None),
                "{}: press {press} of the run answered something",
                fixture.name
            );
            assert!(
                session.selection.is_none(),
                "{}: press {press} began a selection",
                fixture.name
            );
            assert!(
                (fixture.acted)(&session, &acted),
                "{}: press {press} undid the click",
                fixture.name
            );
        }

        // The press after a triple click starts the count again, so it is a press like any other.
        let fourth = handle_mouse(
            &mut session,
            at(
                MouseEventKind::Down(MouseButton::Left),
                cell,
                fixture.modifiers,
            ),
        );
        assert_eq!(
            fourth,
            Action::Redraw,
            "{}: the fourth press began nothing",
            fixture.name
        );
    }
}

/// MOUSE-50. The press after a click that acted begins nothing only on the cell of the click and
/// within the interval that makes a double click: a press on another cell is a press of its own, and
/// a click that did not act leaves the double click that follows it what it always was, a word.
/// Rejects: a memory of the click that outlives the run, so a press elsewhere begins nothing, and one
/// that is kept for a click that did nothing.
#[test]
fn only_the_rest_of_a_run_that_began_with_a_click_that_acted_begins_nothing() {
    let (mut session, (row, column)) = (targets()
        .into_iter()
        .find(|fixture| fixture.name == "the model row of the info panel")
        .expect("the fixture")
        .build)();
    assert_eq!(click(&mut session, row, column), Action::ChooseModel);

    // Another cell is not the rest of that run.
    let down = handle_mouse(
        &mut session,
        at(
            MouseEventKind::Down(MouseButton::Left),
            (row + 5, column),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        down,
        Action::Redraw,
        "a press on another cell began nothing"
    );
    assert!(session.selection.is_some());

    // A click that did nothing leaves the double click that follows it a selection of the word. A
    // web address answers only a click with ctrl held, so a click without it finds the target and
    // acts on nothing.
    let (mut session, cell) = (targets()
        .into_iter()
        .find(|fixture| fixture.name == "a web address in a reply")
        .expect("the fixture")
        .build)();
    assert_eq!(
        click_held(&mut session, cell, KeyModifiers::NONE),
        Action::Copy,
        "a click without ctrl on an address did something"
    );
    let down = handle_mouse(
        &mut session,
        at(
            MouseEventKind::Down(MouseButton::Left),
            cell,
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(down, Action::Redraw);
    assert!(session.selection.is_some());
}

/// MOUSE-6, MOUSE-16, MOUSE-42. The three places whose second press is its own are still a double
/// click: the text of the box selects the word it is on, and a path that waited for a second press
/// selects its name and shows nothing. Rejects: the rule that keeps the second press of a double click
/// from beginning anything being applied to them, which takes a double click in the box away from
/// the person.
#[test]
fn a_double_click_in_the_text_of_the_box_or_on_a_path_still_selects_a_word() {
    for name in ["a place in the text of the box", "a file path in a reply"] {
        let fixture = targets()
            .into_iter()
            .find(|fixture| fixture.name == name)
            .expect("the fixture");
        let (mut session, cell) = (fixture.build)();
        click_held(&mut session, cell, fixture.modifiers);
        let down = handle_mouse(
            &mut session,
            at(
                MouseEventKind::Down(MouseButton::Left),
                cell,
                fixture.modifiers,
            ),
        );
        assert_eq!(
            down,
            Action::Redraw,
            "{name}: the second press began nothing"
        );
        let up = handle_mouse(
            &mut session,
            at(
                MouseEventKind::Up(MouseButton::Left),
                cell,
                fixture.modifiers,
            ),
        );
        assert_eq!(
            up,
            Action::Copy,
            "{name}: the double click selected nothing"
        );
        assert!(
            session
                .selection
                .is_some_and(|selection| !selection.is_empty()),
            "{name}: the selection holds no word"
        );
    }
}

/// MOUSE-50. A press and its release are one click on what the press stood on. A frame drawn between
/// them can put another target under the cell: a reply arriving at the tail moves every row up, and
/// the cell holds the marker of the next reply. Rejects: the release acting on whatever is under it
/// in the frame that is on the screen then, which copies a reply the person did not press on. The
/// replies are a row each with a blank between, so the next reply's marker is where the one pressed
/// on was, and the release is the end of a press that began on a target that has moved.
#[test]
fn a_release_over_a_target_a_frame_since_the_press_put_there_ends_no_click() {
    let a_session = || {
        let mut session = Session::new("none");
        for n in 0..20 {
            session.transcript.push(crate::state::Entry::assistant(
                format!("answer {n:02}"),
                Vec::new(),
            ));
        }
        let buffer = drawn(&mut session);
        (session, buffer)
    };
    let (mut session, buffer) = a_session();
    let (row, _) = find_cell(&buffer, "answer 15");

    // The control: with no frame between them the release copies the reply that was pressed.
    let mut control = a_session().0;
    assert_eq!(
        click(&mut control, row, 0),
        Action::CopyReply("answer 15".to_string())
    );

    handle_mouse(
        &mut session,
        drag(MouseEventKind::Down(MouseButton::Left), row, 0),
    );
    session
        .transcript
        .push(crate::state::Entry::assistant("answer 20", Vec::new()));
    let later = drawn(&mut session);
    assert_eq!(
        find_cell(&later, "answer 16").0,
        row,
        "the cell does not hold the next reply, so the fixture shows nothing"
    );
    let action = handle_mouse(
        &mut session,
        drag(MouseEventKind::Up(MouseButton::Left), row, 0),
    );
    assert!(
        !matches!(action, Action::CopyReply(_)),
        "the release copied a reply the press was not on: {action:?}"
    );
}
