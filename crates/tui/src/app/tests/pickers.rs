//! MOUSE-15, MOUSE-46 and MOUSE-47 for the pickers a turn in flight opens as a mode of the session:
//! a press on a row, the wheel, and a press outside the panel.

use super::*;
use crate::picking::{Picked, Picking};
use bravebot_aichat::protocol::Effort;

/// The session with the info panel drawn at 120 by 24, so that the picker, which is centred, leaves
/// the panel's rows uncovered beside it, and the cell of the panel's context reading.
fn a_session_beside_the_panel() -> (Session, (u16, u16)) {
    let (session, buffer) = panel_with_a_context();
    let cell = find_on(&buffer, &render::context_reading(&session));
    (session, cell)
}

/// The cell of row `index` of the list the open picker drew, as the last frame recorded it.
fn row_of_the_picker(session: &Session, index: u16) -> (u16, u16) {
    let list = session
        .laid
        .popup(crate::state::Panel::Picker)
        .and_then(|popup| popup.list)
        .expect("a picker that lists rows was recorded");
    (list.y + index, list.x + 3)
}

fn open_effort(session: &mut Session) {
    session.open_picking(Picking::Effort(crate::effort_prompt::Picker::new(None)));
    drawn(session);
}

fn cursor(session: &Session) -> Option<Option<Effort>> {
    match session.picking() {
        Some(Picking::Effort(picker)) => picker.chosen().map(|row| row.0),
        _ => None,
    }
}

/// MOUSE-15. Rejects: a press on a row of a picker opened mid-turn that moves nothing, that takes
/// the row at once, or whose second press leaves the picker open or the row untaken.
#[test]
fn a_press_on_a_row_moves_the_cursor_and_a_second_takes_it_in_a_picker_opened_mid_turn() {
    let (mut session, _) = a_session_beside_the_panel();
    open_effort(&mut session);
    assert_eq!(cursor(&session), Some(None));

    // The rows are the unset one, then low, medium, high, xhigh and max.
    let (row, column) = row_of_the_picker(&session, 3);
    press(&mut session, row, column, KeyModifiers::NONE);
    assert_eq!(cursor(&session), Some(Some(Effort::High)));
    assert!(
        session.take_pick().is_none(),
        "the first press took the row"
    );

    press(&mut session, row, column, KeyModifiers::NONE);
    assert!(session.picking().is_none(), "the second press left it open");
    match session.take_pick() {
        Some(Picked::Effort(row)) => assert_eq!(row.0, Some(Effort::High)),
        other => panic!("the row taken was {other:?}"),
    }
}

/// A `/theme` picker opened as a mode of the session, drawn, with the cursor on `current`.
fn open_theme(session: &mut Session, current: &str) {
    session.open_picking(Picking::Theme(crate::theme_prompt::opened(
        crate::theme::builtins(),
        current,
    )));
    drawn(session);
}

/// MOUSE-17. Rejects: a press on a row of the `/theme` picker opened mid-turn that moves the cursor
/// without putting the theme under it in force, a second press that leaves the picker open or takes
/// another theme, and a press outside that leaves the preview in force.
#[test]
fn a_press_on_a_row_of_the_theme_picker_opened_mid_turn_previews_it_and_a_press_outside_puts_the_old_one_back()
 {
    let _held = crate::theme::exclusive();
    crate::theme::apply_brave();
    let themes = crate::theme::builtins();
    let (mut session, (outside_row, outside_column)) = a_session_beside_the_panel();
    open_theme(&mut session, crate::theme::BRAVE);

    let (row, column) = row_of_the_picker(&session, 2);
    press(&mut session, row, column, KeyModifiers::NONE);
    assert_eq!(
        crate::theme::name(),
        themes[2].name,
        "the row was not previewed"
    );
    assert!(
        session.take_pick().is_none(),
        "the first press took the row"
    );

    press(&mut session, row, column, KeyModifiers::NONE);
    assert!(session.picking().is_none(), "the second press left it open");
    match session.take_pick() {
        Some(Picked::Theme(theme)) => assert_eq!(theme.name, themes[2].name),
        other => panic!("the row taken was {other:?}"),
    }
    assert_eq!(
        crate::theme::name(),
        themes[2].name,
        "taking the row dropped it"
    );

    crate::theme::apply_brave();
    open_theme(&mut session, crate::theme::BRAVE);
    let (row, column) = row_of_the_picker(&session, 3);
    press(&mut session, row, column, KeyModifiers::NONE);
    assert_eq!(crate::theme::name(), themes[3].name);
    click(&mut session, outside_row, outside_column);
    assert!(
        session.picking().is_none(),
        "the press outside left it open"
    );
    assert_eq!(
        crate::theme::name(),
        crate::theme::BRAVE,
        "the press outside kept the preview"
    );
    crate::theme::apply_brave();
}

/// A roster of `count` models named `model-0`, `model-1` and on, all under one service.
fn a_long_roster(count: usize) -> Vec<bravebot_aichat::models::Model> {
    (0..count)
        .map(|n| bravebot_aichat::models::Model {
            key: format!("openrouter/model-{n}"),
            display_name: format!("model-{n}"),
            premium: false,
            provider: Some("OpenRouter".to_string()),
            conversation_tokens: None,
            reads_effort: true,
            advertised: bravebot_aichat::models::Advertised::default(),
        })
        .collect()
}

/// MOUSE-15, in a picker a session holds. Rejects: a double click in place that does not take the
/// model pointed at, because a press that moved the cursor to a row already drawn slid the list, so
/// the second press landed on another model. With a roster that fits the panel, and with one longer
/// than it opened on its last model, a press and a release, a frame, and a press and a release on one
/// cell put the cursor on the model drawn there, leave the rows where they were, and take it.
#[test]
fn a_double_click_in_place_takes_the_model_pointed_at_on_every_row_of_a_list_scrolled_or_not() {
    let open = |session: &mut Session, count: usize| {
        let current = format!("openrouter/model-{}", count - 1);
        session.open_picking(Picking::Model(crate::model_prompt::Picker::new(
            a_long_roster(count),
            Some(current.as_str()),
        )));
        drawn(session)
    };
    let model_on = |buffer: &ratatui::buffer::Buffer, row: u16| -> Option<String> {
        let words: String = (0..buffer.area.width)
            .map(|x| buffer[(x, row)].symbol())
            .collect();
        words
            .split_whitespace()
            .find(|word| word.starts_with("model-"))
            .map(str::to_string)
    };
    let listed = |buffer: &ratatui::buffer::Buffer, session: &Session| -> Vec<Option<String>> {
        let list = session
            .laid
            .popup(crate::state::Panel::Picker)
            .and_then(|popup| popup.list)
            .expect("the list was recorded");
        (list.y..list.bottom())
            .map(|row| model_on(buffer, row))
            .collect()
    };
    for (count, scrolled) in [(3, false), (40, true)] {
        let (mut session, _) = a_session_beside_the_panel();
        let buffer = open(&mut session, count);
        let rows = listed(&buffer, &session).len();
        assert_eq!(
            listed(&buffer, &session).iter().flatten().count() < count,
            scrolled
        );
        for slot in 0..rows {
            let (mut session, _) = a_session_beside_the_panel();
            let buffer = open(&mut session, count);
            let before = listed(&buffer, &session);
            let Some(named) = before[slot].clone() else {
                continue;
            };
            let (row, column) = row_of_the_picker(&session, slot as u16);

            click(&mut session, row, column);
            assert_eq!(
                model_under_the_cursor(&session).as_deref(),
                Some(named.as_str()),
                "{count} models, row {slot}"
            );
            let buffer = drawn(&mut session);
            let after_the_press = listed(&buffer, &session);
            click(&mut session, row, column);
            assert!(
                session.picking().is_none(),
                "{count} models, row {slot}: the second press left it open"
            );
            match session.take_pick() {
                Some(Picked::Model(model)) => assert_eq!(model.display_name, named),
                other => panic!("{count} models, row {slot}: the row taken was {other:?}"),
            }
            assert_eq!(
                after_the_press, before,
                "{count} models, row {slot}: the press slid the list"
            );
        }
    }
}

/// MOUSE-15. Rejects: a second press on another row, and one after the window, taking a row, and
/// a click on the border or below the last row moving the cursor or leaving.
#[test]
fn presses_on_two_rows_or_off_the_rows_take_nothing_in_a_picker_opened_mid_turn() {
    let (mut session, _) = a_session_beside_the_panel();
    open_effort(&mut session);
    let (row, column) = row_of_the_picker(&session, 2);
    let (other, _) = row_of_the_picker(&session, 3);
    press(&mut session, row, column, KeyModifiers::NONE);
    press(&mut session, other, column, KeyModifiers::NONE);
    assert!(session.picking().is_some(), "two rows took one");
    assert_eq!(cursor(&session), Some(Some(Effort::High)));

    let popup = session
        .laid
        .popup(crate::state::Panel::Picker)
        .expect("the picker is recorded");
    for cell in [
        (popup.area.y, popup.area.x + 4),
        (popup.area.y + 3, popup.area.x),
        (row_of_the_picker(&session, 6).0, column),
        (popup.area.bottom() - 2, column),
    ] {
        click(&mut session, cell.0, cell.1);
        click(&mut session, cell.0, cell.1);
        assert!(session.picking().is_some(), "{cell:?} left the picker");
        assert_eq!(cursor(&session), Some(Some(Effort::High)), "{cell:?}");
    }
    assert!(session.take_pick().is_none());
}

/// MOUSE-46. Rejects: the wheel scrolling the transcript beneath a picker, moving the cursor a
/// different number of rows from `tui.wheelRows`, going past an end, or taking a row.
#[test]
fn the_wheel_walks_a_picker_opened_mid_turn_by_the_wheel_rows() {
    let (mut session, _) = a_session_beside_the_panel();
    for n in 0..40 {
        session.transcript.push(crate::state::Entry::user(format!(
            "a line to scroll through {n}"
        )));
    }
    drawn(&mut session);
    open_effort(&mut session);
    let scrolled = session.scroll;
    let rows = [
        None,
        Some(Effort::Low),
        Some(Effort::Medium),
        Some(Effort::High),
        Some(Effort::Xhigh),
        Some(Effort::Max),
    ];
    let step = usize::from(session.wheel_rows());
    assert!(step >= 1 && step < rows.len(), "the default step is {step}");

    handle_mouse(&mut session, drag(MouseEventKind::ScrollDown, 2, 2));
    assert_eq!(cursor(&session), Some(rows[step]));
    handle_mouse(&mut session, drag(MouseEventKind::ScrollUp, 2, 2));
    assert_eq!(cursor(&session), Some(rows[0]));
    for _ in 0..10 {
        handle_mouse(&mut session, drag(MouseEventKind::ScrollDown, 2, 2));
    }
    assert_eq!(
        cursor(&session),
        Some(rows[5]),
        "the wheel went past the last row"
    );
    for _ in 0..10 {
        handle_mouse(&mut session, drag(MouseEventKind::ScrollUp, 2, 2));
    }
    assert_eq!(
        cursor(&session),
        Some(rows[0]),
        "the wheel went past the first row"
    );
    assert!(session.picking().is_some() && session.take_pick().is_none());
    assert_eq!(
        session.scroll, scrolled,
        "the wheel scrolled the transcript"
    );
}

/// MOUSE-47. Rejects: a press outside a picker opened mid-turn that leaves it open, that takes the
/// row under its cursor, or whose release is completed as a click on the target it ended on. The
/// press is on the context reading of the info panel, and the release is the click's.
#[test]
fn a_press_outside_a_picker_opened_mid_turn_closes_it_and_clicks_nothing_beneath() {
    let (mut session, (row, column)) = a_session_beside_the_panel();
    open_effort(&mut session);
    assert!(
        !session
            .laid
            .popup(crate::state::Panel::Picker)
            .expect("the picker is recorded")
            .area
            .contains(ratatui::layout::Position::new(column, row)),
        "the picker covers the context reading, so a press on it is not outside"
    );

    let action = click(&mut session, row, column);
    assert!(
        session.picking().is_none(),
        "the press outside left it open"
    );
    assert!(
        session.take_pick().is_none(),
        "the press outside took a row"
    );
    assert_ne!(action, Action::Copy, "the release was left to the frame");
    assert!(
        context_rows(&session).is_empty(),
        "the release reported the context"
    );
    assert!(session.selection.is_none());

    // The picker is gone, so a click on the row now is the click it always was.
    click(&mut session, row, column);
    assert!(!context_rows(&session).is_empty());
}

/// MOUSE-47. Rejects: a press of the right or the middle button outside the panel, or a release,
/// closing it.
#[test]
fn only_a_left_press_outside_a_picker_opened_mid_turn_closes_it() {
    let (mut session, (row, column)) = a_session_beside_the_panel();
    open_effort(&mut session);
    for kind in [
        MouseEventKind::Down(MouseButton::Right),
        MouseEventKind::Down(MouseButton::Middle),
        MouseEventKind::Up(MouseButton::Left),
        MouseEventKind::Drag(MouseButton::Left),
        MouseEventKind::Moved,
    ] {
        handle_mouse(&mut session, drag(kind, row, column));
        assert!(session.picking().is_some(), "{kind:?} closed the picker");
    }
}

/// MOUSE-15, MOUSE-46. Rejects: the other pickers a turn opens, which are the same list, not being
/// given the mouse.
#[test]
fn the_preferences_picker_opened_mid_turn_takes_a_press_and_the_wheel_too() {
    let (mut session, _) = a_session_beside_the_panel();
    session.open_picking(Picking::Editing(crate::config_prompt::Picker::new(
        crate::vim::Editing::Ordinary,
    )));
    drawn(&mut session);
    handle_mouse(&mut session, drag(MouseEventKind::ScrollDown, 1, 1));
    let (row, column) = row_of_the_picker(&session, 1);
    click(&mut session, row, column);
    click(&mut session, row, column);
    assert!(session.picking().is_none());
    assert!(matches!(session.take_pick(), Some(Picked::Editing(_))));
}

/// MOUSE-45, MOUSE-47. Rejects: the release of the press that closed a picker, which the session
/// reads after the picker's own loop has returned, being completed as a click on the row of the
/// info panel it landed on, so that clicking the model row to leave a picker opened by clicking it
/// opens it again.
#[test]
fn the_release_of_a_press_that_closed_a_picker_clicks_nothing_in_the_session() {
    let (mut session, buffer) = panel_with_a_model();
    let (row, column) = find_on(&buffer, "claude-sonnet");
    // The click that opened the picker, which the session remembers.
    assert_eq!(click(&mut session, row, column), Action::ChooseModel);
    // The picker read the next press itself and closed on it, and says so.
    session.end_a_pickers_press();
    let release = drag(MouseEventKind::Up(MouseButton::Left), row, column);
    assert_ne!(handle_mouse(&mut session, release), Action::ChooseModel);
    // The next click is an ordinary one again.
    assert_eq!(click(&mut session, row, column), Action::ChooseModel);
}

fn a_roster() -> Vec<bravebot_aichat::models::Model> {
    let served = |key: &str, name: &str| bravebot_aichat::models::Model {
        key: key.to_string(),
        display_name: name.to_string(),
        premium: false,
        provider: Some("OpenRouter".to_string()),
        conversation_tokens: None,
        reads_effort: true,
        advertised: bravebot_aichat::models::Advertised::default(),
    };
    vec![
        served("openrouter/first", "first-model"),
        served("openrouter/second", "second-model"),
        served("openrouter/third", "third-model"),
    ]
}

fn open_models(session: &mut Session) {
    session.open_picking(Picking::Model(crate::model_prompt::Picker::new(
        a_roster(),
        None,
    )));
}

fn model_under_the_cursor(session: &Session) -> Option<String> {
    match session.picking() {
        Some(Picking::Model(picker)) => picker.chosen().map(|model| model.display_name.clone()),
        _ => None,
    }
}

/// MOUSE-15, MOUSE-46, MOUSE-47 for the model picker a turn opens: a press on the row of a model
/// drawn under its service's heading, a second press, the wheel, and a press outside the panel.
/// Rejects: the model picker being left out of the pickers that take the mouse, and a press read
/// against the rows of a list that has headings in it.
#[test]
fn the_model_picker_opened_mid_turn_takes_a_press_the_wheel_and_a_press_outside() {
    let (mut session, (outside, outside_column)) = a_session_beside_the_panel();
    open_models(&mut session);
    let buffer = drawn(&mut session);
    assert_eq!(
        model_under_the_cursor(&session).as_deref(),
        Some("first-model")
    );

    let (row, column) = find_on(&buffer, "second-model");
    press(&mut session, row, column, KeyModifiers::NONE);
    assert_eq!(
        model_under_the_cursor(&session).as_deref(),
        Some("second-model")
    );
    assert!(
        session.take_pick().is_none(),
        "the first press took the model"
    );

    handle_mouse(&mut session, drag(MouseEventKind::ScrollDown, 1, 1));
    assert_eq!(
        model_under_the_cursor(&session).as_deref(),
        Some("third-model")
    );
    handle_mouse(&mut session, drag(MouseEventKind::ScrollUp, 1, 1));
    handle_mouse(&mut session, drag(MouseEventKind::ScrollUp, 1, 1));
    assert_eq!(
        model_under_the_cursor(&session).as_deref(),
        Some("first-model")
    );

    let (row, column) = find_on(&buffer, "third-model");
    press(&mut session, row, column, KeyModifiers::NONE);
    press(&mut session, row, column, KeyModifiers::NONE);
    assert!(session.picking().is_none(), "the second press left it open");
    match session.take_pick() {
        Some(Picked::Model(model)) => assert_eq!(model.display_name, "third-model"),
        other => panic!("the row taken was {other:?}"),
    }

    open_models(&mut session);
    drawn(&mut session);
    click(&mut session, outside, outside_column);
    assert!(
        session.picking().is_none(),
        "the press outside left it open"
    );
    assert!(
        session.take_pick().is_none(),
        "the press outside took a model"
    );
}
