//! MOUSE-48 and MOUSE-14: a click on a hint of the row under the box does what the key it names does.

use super::*;
use crate::state::{Footer, Status};

/// The cell where `text` begins on the screen, for a session drawn now.
pub(super) fn located(build: fn() -> (Session, String)) -> (Session, (u16, u16)) {
    let (mut session, text) = build();
    let buffer = drawn(&mut session);
    let cell = find_cell(&buffer, &text);
    (session, cell)
}

/// A session with a finished turn that left a trail, so that the row offers to show it.
pub(super) fn with_a_trail() -> (Session, String) {
    use bravebot_core::capability::Capability;
    use bravebot_core::event::Event;
    use bravebot_core::label::Label;

    let mut session = Session::new("none");
    session.type_char('a');
    session.submit();
    session.complete(
        "reply",
        vec![bravebot_session::audit::as_line(
            &Event::Observed {
                capability: Capability::FileRead,
                label: Label::untrusted_private(),
            },
            None,
        )],
        0,
    );
    (session, "ctrl-t show trail".to_string())
}

pub(super) fn with_an_empty_box() -> (Session, String) {
    (Session::new("none"), "? for shortcuts".to_string())
}

pub(super) fn with_the_panel_closed() -> (Session, String) {
    (Session::new("none"), "ctrl-x info".to_string())
}

/// A session with one delegate, which the view the watch key opens holds.
pub(super) fn with_something_to_open() -> (Session, String) {
    let mut session = Session::new("kernel-enforced");
    session.delegate_started(bravebot_agent::report::Delegation {
        stop: Default::default(),
        id: bravebot_agent::report::DelegateId::nth(1),
        kind: "reader".to_string(),
        task: "find the parser".to_string(),
    });
    (session, "ctrl-l 1 to open".to_string())
}

/// A session that ran one command, which the view the outputs key opens holds.
pub(super) fn with_a_command_to_open() -> (Session, String) {
    let mut session = Session::new("kernel-enforced");
    session.command_printed(bravebot_agent::report::Printed {
        command: "cargo test".to_string(),
        lines: vec!["first".to_string()],
        total: 1,
        read_by_the_planner: false,
        outcome: bravebot_agent::report::Outcome::Succeeded,
        job: None,
    });
    (session, "alt-l or /outputs 1 to open".to_string())
}

/// A turn waiting on a command that can be moved to the background.
pub(super) fn with_a_command_to_move() -> (Session, String) {
    let mut session = Session::new("kernel-enforced");
    session.type_char('a');
    session.submit().expect("the prompt is sent");
    session.movable(bravebot_core::cancel::Handoff::new());
    (session, "ctrl-b to background".to_string())
}

/// A transcript long enough to scroll, with the view held back from the end.
pub(super) fn with_the_view_held() -> (Session, String) {
    let mut session = Session::new("none");
    for turn in 0..40 {
        session.type_char('q');
        session.submit();
        session.complete(format!("reply number {turn}"), Vec::new(), 0);
    }
    drawn(&mut session);
    session.scroll_up(60);
    // Laid out again, so that the rows below are measured against the view as it now stands.
    drawn(&mut session);
    let held = format!(
        "held, {} rows below  ·  ctrl-end to return",
        session.rows_below()
    );
    (session, held)
}

/// Shell mode armed over a line, at rest.
pub(super) fn in_shell_mode() -> (Session, String) {
    let mut session = Session::new("none");
    handle_key(&mut session, key(KeyCode::Char('!')));
    type_line(&mut session, "ls");
    assert!(session.shell, "the line did not arm shell mode");
    (session, "esc to cancel".to_string())
}

pub(super) fn with_a_picture_on_the_clipboard() -> (Session, String) {
    let mut session = Session::new("none");
    session.image_on_clipboard = true;
    (
        session,
        "image on clipboard  ·  ctrl-v to paste".to_string(),
    )
}

pub(super) fn with_a_mode() -> (Session, String) {
    let mut session = Session::new("none");
    session.cycle_permission_mode();
    let named = crate::status::named_mode(session.permission_mode(), false).expect("a named mode");
    (session, named.to_string())
}

/// A hint a click acts on: a session whose row draws it, the words it is drawn with, and whether
/// the session and the answer to the click show that the click did what the key does.
pub(super) struct Case {
    pub name: &'static str,
    pub footer: Footer,
    pub build: fn() -> (Session, String),
    pub acted: fn(&Session, &Action) -> bool,
}

/// Whether a click on each hint did what its key does, from the session after it and what the click
/// returned. Named so that the table of targets a panel may cover can read the same answers.
pub(super) fn trail_acted(session: &Session, _: &Action) -> bool {
    session.show_trail
}

pub(super) fn shortcuts_acted(session: &Session, _: &Action) -> bool {
    matches!(session.offered(), crate::state::Offered::Shortcuts)
}

pub(super) fn panel_acted(session: &Session, _: &Action) -> bool {
    session.panel_open()
}

pub(super) fn watch_acted(session: &Session, _: &Action) -> bool {
    session
        .watching()
        .is_some_and(|watching| watching.scope == crate::state::WatchScope::Agents)
}

pub(super) fn outputs_acted(session: &Session, _: &Action) -> bool {
    session
        .watching()
        .is_some_and(|watching| watching.scope == crate::state::WatchScope::Outputs)
}

pub(super) fn background_acted(session: &Session, _: &Action) -> bool {
    !session.can_move_to_background()
}

pub(super) fn held_acted(session: &Session, _: &Action) -> bool {
    session.rows_below() == 0
}

pub(super) fn cancel_acted(session: &Session, _: &Action) -> bool {
    !session.shell && session.input().is_empty()
}

pub(super) fn paste_acted(_: &Session, action: &Action) -> bool {
    *action == Action::Paste
}

/// Every hint of the row under the box that names a key. A hint added to the row is a row here.
pub(super) fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "the permission mode",
            footer: Footer::Mode,
            build: with_a_mode,
            acted: |session, _| {
                session.permission_mode() != bravebot_agent::PermissionMode::AcceptEdits
            },
        },
        Case {
            name: "show trail",
            footer: Footer::Trail,
            build: with_a_trail,
            acted: trail_acted,
        },
        Case {
            name: "? for shortcuts",
            footer: Footer::Shortcuts,
            build: with_an_empty_box,
            acted: shortcuts_acted,
        },
        Case {
            name: "ctrl-x info",
            footer: Footer::Info,
            build: with_the_panel_closed,
            acted: panel_acted,
        },
        Case {
            name: "ctrl-l to open",
            footer: Footer::Watch,
            build: with_something_to_open,
            acted: watch_acted,
        },
        Case {
            name: "alt-l or /outputs to open",
            footer: Footer::Outputs,
            build: with_a_command_to_open,
            acted: outputs_acted,
        },
        Case {
            name: "ctrl-b to background",
            footer: Footer::Background,
            build: with_a_command_to_move,
            acted: background_acted,
        },
        Case {
            name: "ctrl-end to return",
            footer: Footer::Held,
            build: with_the_view_held,
            acted: held_acted,
        },
        Case {
            name: "esc to cancel",
            footer: Footer::CancelShell,
            build: in_shell_mode,
            acted: cancel_acted,
        },
        Case {
            name: "ctrl-v to paste",
            footer: Footer::Paste,
            build: with_a_picture_on_the_clipboard,
            acted: paste_acted,
        },
    ]
}

/// The row of the hints: the last of the 24 the frame is drawn in.
fn hint_row(buffer: &ratatui::buffer::Buffer) -> u16 {
    buffer.area.height - 1
}

/// MOUSE-48. Rejects: a hint that names a key and is not a click target, a click that does something
/// other than what the key does, and a target that is only part of the words, which leaves the rest
/// of what the hint says to do nothing. The first, the middle and the last cell of each hint are
/// clicked, each on a session of its own.
#[test]
fn a_click_on_each_hint_does_what_its_key_does() {
    for case in cases() {
        let (control, _) = (case.build)();
        assert!(
            !(case.acted)(&control, &Action::None),
            "{}: the session had acted before any click",
            case.name
        );
        let width = (case.build)().1.chars().count() as u16;
        for offset in [0, width / 2, width - 1] {
            let (mut session, (row, column)) = located(case.build);
            let action = click(&mut session, row, column + offset);
            assert!(
                (case.acted)(&session, &action),
                "{}: a click {offset} cells in did not act ({action:?})",
                case.name
            );
            assert!(session.selection.is_none(), "{}", case.name);
            assert_ne!(action, Action::Copy, "{}", case.name);
        }
    }
}

/// MOUSE-48. Rejects: a click on the separator beside a hint, a drag that begins or ends on it, a
/// press of another button, a press and a release on two cells, and the second click of a double
/// click acting. The separator cells are the ones just before the first cell and just after the
/// last, which on the first hint is the indent.
#[test]
fn only_a_single_click_of_the_left_button_on_a_hint_acts() {
    for case in cases() {
        let width = (case.build)().1.chars().count() as u16;

        for beside in [-1i16, width as i16] {
            let (mut session, (row, column)) = located(case.build);
            let action = click(&mut session, row, (column as i16 + beside) as u16);
            assert!(
                !(case.acted)(&session, &action),
                "{}: a click {beside} cells from the start acted",
                case.name
            );
        }

        // A drag across the hint selects, and copies when it is let go.
        let (mut session, (row, column)) = located(case.build);
        swept(&mut session, (row, column), (row, column + 2));
        let up = drag(MouseEventKind::Up(MouseButton::Left), row, column + 2);
        let action = handle_mouse(&mut session, up);
        assert_eq!(action, Action::Copy, "{}", case.name);
        assert!(
            !(case.acted)(&session, &action),
            "{}: a drag acted",
            case.name
        );

        // Pressed on the hint and let go on the next cell, which is not a click on one cell.
        let (mut session, (row, column)) = located(case.build);
        handle_mouse(
            &mut session,
            drag(MouseEventKind::Down(MouseButton::Left), row, column),
        );
        let action = handle_mouse(
            &mut session,
            drag(MouseEventKind::Up(MouseButton::Left), row, column + 1),
        );
        assert!(
            !(case.acted)(&session, &action),
            "{}: a press and a release on two cells acted",
            case.name
        );

        for button in [MouseButton::Right, MouseButton::Middle] {
            let (mut session, (row, column)) = located(case.build);
            handle_mouse(
                &mut session,
                drag(MouseEventKind::Down(button), row, column),
            );
            let action = handle_mouse(&mut session, drag(MouseEventKind::Up(button), row, column));
            assert!(
                !(case.acted)(&session, &action),
                "{}: the {button:?} button acted",
                case.name
            );
        }

        // The first click of a double click acts and the rest of it does nothing, so a toggle is
        // not undone by it (MOUSE-50).
        let (mut session, (row, column)) = located(case.build);
        let first = click(&mut session, row, column);
        let second = click(&mut session, row, column);
        assert_eq!(
            second,
            Action::None,
            "{}: the second click acted",
            case.name
        );
        assert!(
            (case.acted)(&session, &first),
            "{}: the second click undid the first",
            case.name
        );
    }
}

/// MOUSE-48. Rejects: the hints being targets while a turn runs only because they are targets at
/// rest, or only at rest because the key is refused at some moments: the trail, the shortcuts, the
/// panel, the view the watch key opens, the way to the end of a held view, and the move to the
/// background all act in the middle of a turn, as their keys do.
#[test]
fn a_click_on_a_hint_acts_while_a_turn_runs() {
    for case in cases() {
        let mut working = match case.footer {
            // Not offered while a turn runs, or refused by it.
            Footer::CancelShell | Footer::Mode => continue,
            _ => (case.build)().0,
        };
        working.status = Status::Working;
        let buffer = drawn(&mut working);
        let text = (case.build)().1;
        let (row, column) = find_cell(&buffer, &text);
        let action = click(&mut working, row, column);
        assert!(
            (case.acted)(&working, &action),
            "{}: a click while a turn runs did not act ({action:?})",
            case.name
        );
    }
}

/// MOUSE-48. Rejects: a hint that is drawn while the key it names is refused being a target. `?`
/// types a question mark in a line that has words in it and is vi's own key in NORMAL mode, so the
/// words are not offered a list there, and a click on the hint must not type one. `esc` leaves
/// shell mode only at rest and only for the box everybody has. While a command the person typed
/// runs the keys of the row are read by nothing.
#[test]
fn a_hint_that_is_drawn_while_its_key_is_refused_is_not_a_target() {
    // Words in the box.
    let mut session = typed_into("a question");
    let buffer = drawn(&mut session);
    let (row, column) = find_cell(&buffer, "? for shortcuts");
    click(&mut session, row, column);
    assert_eq!(session.input(), "a question");
    assert!(!matches!(
        session.offered(),
        crate::state::Offered::Shortcuts
    ));

    // vi's NORMAL mode, on an empty line.
    let mut session = editing_vis_way();
    handle_key(&mut session, key(KeyCode::Esc));
    assert!(session.vi_normal());
    let buffer = drawn(&mut session);
    let (row, column) = find_cell(&buffer, "? for shortcuts");
    click(&mut session, row, column);
    assert!(!matches!(
        session.offered(),
        crate::state::Offered::Shortcuts
    ));
    assert_eq!(session.input(), "");

    // Shell mode, where the row draws the shell line and `esc to cancel` is its own.
    let mut session = editing_vis_way();
    handle_key(&mut session, key(KeyCode::Char('!')));
    assert!(session.shell);
    let buffer = drawn(&mut session);
    let (row, column) = find_cell(&buffer, "esc to cancel");
    click(&mut session, row, column);
    assert!(
        session.shell,
        "a click took the vi box out of shell mode, where Esc is vi's"
    );

    let (mut session, _) = in_shell_mode();
    session.status = Status::Working;
    let buffer = drawn(&mut session);
    let (row, column) = find_cell(&buffer, "esc to cancel");
    click(&mut session, row, column);
    assert!(session.shell, "a click left shell mode while a turn runs");
    assert_eq!(session.input(), "ls");

    // A command the person typed is running.
    for case in cases() {
        if matches!(case.footer, Footer::CancelShell) {
            continue;
        }
        let (mut session, _) = (case.build)();
        let text = (case.build)().1;
        session.status = Status::Running;
        let buffer = drawn(&mut session);
        let (row, column) = find_cell(&buffer, &text);
        let action = click(&mut session, row, column);
        assert!(
            !(case.acted)(&session, &action),
            "{}: a click acted while a command runs",
            case.name
        );
    }
}

/// The cells of each hint the last frame recorded and the key it names, in the order they were drawn.
fn recorded(session: &Session) -> Vec<(ratatui::layout::Rect, Footer)> {
    session
        .laid
        .targets
        .iter()
        .filter_map(|target| match target.hit {
            crate::state::Hit::Footer(footer) => Some((target.area, footer)),
            _ => None,
        })
        .collect()
}

/// Whether no hint is recorded over the cells `text` is drawn in on the row under the box.
fn is_no_target(session: &mut Session, text: &str) -> bool {
    let buffer = drawn(session);
    assert!(
        !recorded(session).is_empty(),
        "no hint was recorded, so the reading below proves nothing"
    );
    let (row, column) = find_cell(&buffer, text);
    session.footer_at(row, column).is_none()
}

/// MOUSE-48. Rejects: a part of the row that describes and names no key being a target, which are
/// the context, the words that say the way out is a second press and the count of what a copy took,
/// and a click anywhere on the line of an outside link's prompt, whose key sends it.
#[test]
fn what_the_row_only_describes_is_not_a_target() {
    let mut session = Session::new("none");
    assert!(is_no_target(&mut session, "context not yet measured"));

    session.copied = Some(12);
    assert!(is_no_target(&mut session, "12 chars to clipboard"));
    session.copied = None;

    session.cleared_by_interrupt = true;
    assert!(is_no_target(&mut session, "ctrl-c again to exit"));
    session.cleared_by_interrupt = false;

    session.offered_to_leave = true;
    assert!(is_no_target(&mut session, "ctrl-c again to exit"));

    // The line of the prompt an outside link wrote: no part of it is a target, and a click on it
    // sends nothing and changes nothing in the box.
    let mut session = Session::new("none");
    session.prefill_from_link("fix the test");
    let buffer = drawn(&mut session);
    assert!(recorded(&session).is_empty());
    let row = hint_row(&buffer);
    for column in 0..buffer.area.width {
        let action = click(&mut session, row, column);
        assert!(
            matches!(action, Action::Redraw | Action::None | Action::Copy),
            "a click at column {column} did more than select: {action:?}"
        );
    }
    assert_eq!(session.input(), "fix the test");
}

/// The cells of `area` as the words drawn in them.
fn read(buffer: &ratatui::buffer::Buffer, area: ratatui::layout::Rect) -> String {
    (area.x..area.right())
        .map(|column| buffer[(column, area.y)].symbol())
        .collect()
}

/// MOUSE-48. Rejects: the cells a hint is recorded in being counted from the wrong place: in bytes,
/// with the separator left out, or from the start of the line and not of the part. At each width the
/// row is drawn in, the cells of every hint recorded read as the words of that hint and nothing
/// more, the hints do not overlap, and a hint the width had no room for is not recorded.
#[test]
fn the_cells_recorded_for_a_hint_read_as_its_words_at_every_width() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    for width in [48u16, 60, 80, 100, 120, 160, 200] {
        for case in cases() {
            let (mut session, text) = (case.build)();
            let mut terminal = Terminal::new(TestBackend::new(width, 24)).expect("terminal");
            let mut laid = crate::state::Laid::default();
            terminal
                .draw(|frame| laid = render::draw(frame, &session))
                .expect("draw");
            session.note_layout(laid);
            let buffer = terminal.backend().buffer().clone();
            let row: String = read(
                &buffer,
                ratatui::layout::Rect::new(0, hint_row(&buffer), width, 1),
            );

            let hints = recorded(&session);
            let mine: Vec<_> = hints
                .iter()
                .filter(|(_, footer)| *footer == case.footer)
                .collect();
            if row.contains(&text) {
                assert_eq!(mine.len(), 1, "{} at {width}: {hints:?}", case.name);
                assert_eq!(read(&buffer, mine[0].0), text, "{} at {width}", case.name);
            } else {
                assert!(
                    mine.is_empty(),
                    "{} at {width}: recorded where the row does not say it: {mine:?} in {row:?}",
                    case.name
                );
            }

            let mut cells: Vec<_> = hints.iter().map(|(area, _)| *area).collect();
            cells.sort_by_key(|area| area.x);
            for pair in cells.windows(2) {
                assert!(
                    pair[0].right() <= pair[1].x,
                    "{} at {width}: {cells:?}",
                    case.name
                );
            }
        }
    }
}

/// MOUSE-48. Rejects: the hints of the line that is not the ordinary one being left out, which are
/// the shell line's, or those of the ordinary line being recorded while the shell line is drawn:
/// the cells of the shell line's hints read as their words, and the `?` is not offered there.
#[test]
fn the_shell_line_records_its_own_hints_and_not_the_ordinary_ones() {
    let (mut session, _) = in_shell_mode();
    session.type_char('x');
    let buffer = drawn(&mut session);
    let hints = recorded(&session);
    let targets: Vec<Footer> = hints.iter().map(|(_, footer)| *footer).collect();
    assert_eq!(targets, vec![Footer::CancelShell], "{hints:?}");
    assert_eq!(read(&buffer, hints[0].0), "esc to cancel");
    assert!(
        !read(
            &buffer,
            ratatui::layout::Rect::new(0, hint_row(&buffer), buffer.area.width, 1)
        )
        .contains("? for shortcuts")
    );
}

/// MOUSE-48. Rejects: the hint that offers the paste key being a target for the picture's note and
/// for the other notes of the same cells: the count of what a copy took replaces it, so a click
/// there is not a request for the clipboard.
#[test]
fn the_note_a_copy_leaves_replaces_the_paste_hint_and_its_target() {
    let (mut session, text) = with_a_picture_on_the_clipboard();
    let buffer = drawn(&mut session);
    let (row, column) = find_cell(&buffer, &text);
    assert_eq!(click(&mut session, row, column), Action::Paste);

    session.copied = Some(12);
    let buffer = drawn(&mut session);
    assert!(
        recorded(&session)
            .iter()
            .all(|(_, footer)| *footer != Footer::Paste)
    );
    let (row, column) = find_cell(&buffer, "12 chars to clipboard");
    assert_ne!(click(&mut session, row, column), Action::Paste);
}

/// MOUSE-48. The note that names the key that pastes a picture is a hint, and a hint the width of the
/// terminal had no room for is not drawn and is not a target. Rejects: the note drawn cut at the
/// right of a row too short for it, with its target over the cells of the cut row, so that a click
/// on half a sentence pastes. At every width from the narrowest to one past what the note needs,
/// the note is either drawn whole with a target over its words and no more, or neither.
#[test]
fn a_note_that_names_a_key_is_drawn_whole_with_its_target_or_not_at_all() {
    let (_, words) = with_a_picture_on_the_clipboard();
    let needs = words.chars().count() as u16;
    let mut whole = 0;
    for width in 4..=needs + 6 {
        let (mut session, _) = with_a_picture_on_the_clipboard();
        let buffer = drawn_at(&mut session, width, 12);
        let row_of_hints = (0..12u16)
            .map(|row| {
                (0..width)
                    .map(|column| buffer[(column, row)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();
        let pastes: Vec<ratatui::layout::Rect> = recorded(&session)
            .into_iter()
            .filter(|(_, footer)| *footer == Footer::Paste)
            .map(|(area, _)| area)
            .collect();
        let drawn_whole = row_of_hints.iter().any(|row| row.contains(&words));
        let drawn_in_part = row_of_hints
            .iter()
            .any(|row| row.contains("image on") || row.contains("to paste"));
        if drawn_whole {
            whole += 1;
            assert_eq!(pastes.len(), 1, "width {width}: {pastes:?}");
            let area = pastes[0];
            let row = &row_of_hints[usize::from(area.y)];
            let under: String = row
                .chars()
                .skip(usize::from(area.x))
                .take(usize::from(area.width))
                .collect();
            assert_eq!(
                under, words,
                "width {width}: the target is not over the words"
            );
        } else {
            assert!(
                pastes.is_empty(),
                "width {width}: a target over a note that is not drawn whole: {pastes:?}"
            );
            assert!(
                !drawn_in_part,
                "width {width}: the note is drawn cut: {row_of_hints:?}"
            );
        }
    }
    assert!(
        whole >= 4,
        "the note was drawn whole at only {whole} widths"
    );
}
