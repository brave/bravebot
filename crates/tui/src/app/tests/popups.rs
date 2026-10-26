//! MOUSE-45: a press that lands on a panel drawn over the frame is the panel's, and reaches nothing
//! drawn beneath it.

use super::hints;
use super::scrollbar;
use super::*;
use crate::state::Popup;
use ratatui::layout::Rect;

/// A click target of the frame: a session drawn so that the target is on the screen, the cell of
/// it, and whether a click on that cell did what the target does.
pub(super) struct Target {
    pub(super) name: &'static str,
    pub(super) build: fn() -> (Session, (u16, u16)),
    /// Whether the click acted: given the session after it and what `handle_mouse` returned.
    pub(super) acted: fn(&Session, &Action) -> bool,
    /// What is held while the click is made: a target that answers only a click with ctrl held is
    /// shown with it held, since a click without it is a selection and acts on nothing.
    pub(super) modifiers: KeyModifiers,
}

impl Target {
    /// A target a click answers with nothing held.
    fn plain(
        name: &'static str,
        build: fn() -> (Session, (u16, u16)),
        acted: fn(&Session, &Action) -> bool,
    ) -> Target {
        Target {
            name,
            build,
            acted,
            modifiers: KeyModifiers::NONE,
        }
    }

    /// A target a click answers with ctrl held.
    fn with_ctrl(
        name: &'static str,
        build: fn() -> (Session, (u16, u16)),
        acted: fn(&Session, &Action) -> bool,
    ) -> Target {
        Target {
            modifiers: KeyModifiers::CONTROL,
            ..Target::plain(name, build, acted)
        }
    }
}

/// A mouse event at `cell` with `modifiers` held.
fn mouse_at(kind: MouseEventKind, cell: (u16, u16), modifiers: KeyModifiers) -> MouseEvent {
    let mut event = drag(kind, cell.0, cell.1);
    event.modifiers = modifiers;
    event
}

/// A press and a release on `cell` with `modifiers` held, and what the release answered.
pub(super) fn click_held(
    session: &mut Session,
    cell: (u16, u16),
    modifiers: KeyModifiers,
) -> Action {
    handle_mouse(
        session,
        mouse_at(MouseEventKind::Down(MouseButton::Left), cell, modifiers),
    );
    handle_mouse(
        session,
        mouse_at(MouseEventKind::Up(MouseButton::Left), cell, modifiers),
    )
}

/// Every click target there is, one row each. A target added to the frame is a row here, so that a
/// panel drawn over it is shown to take the press first.
pub(super) fn targets() -> Vec<Target> {
    vec![
        Target::plain(
            "the model row of the info panel",
            || {
                let (session, buffer) = panel_with_a_model();
                let cell = find_on(&buffer, "claude-sonnet");
                (session, cell)
            },
            |_, action| *action == Action::ChooseModel,
        ),
        Target::plain(
            "the effort row of the info panel",
            || {
                let (session, buffer) = panel_with_a_model();
                let cell = find_on(&buffer, "effort");
                (session, cell)
            },
            |_, action| *action == Action::ChooseEffort,
        ),
        Target::plain(
            "the context reading of the info panel",
            || {
                let (session, buffer) = panel_with_a_context();
                let cell = find_on(&buffer, &render::context_reading(&session));
                (session, cell)
            },
            |session, _| !context_rows(session).is_empty(),
        ),
        Target::plain(
            "a started server's row of the info panel",
            || {
                let (mut session, _) = panel_with_a_context();
                session.servers.started = vec!["notes-server".to_string()];
                let buffer = drawn(&mut session);
                let cell = find_on(&buffer, "notes-server");
                (session, cell)
            },
            |_, action| *action == Action::Mcp(String::new()),
        ),
        Target::plain(
            "the permission mode under the box",
            || {
                let mut session = Session::new("none");
                session.cycle_permission_mode();
                let mode = session.permission_mode();
                let buffer = drawn(&mut session);
                let cell = find_on(
                    &buffer,
                    crate::status::named_mode(mode, false).expect("a mode with a name"),
                );
                (session, cell)
            },
            |session, _| session.permission_mode() != bravebot_agent::PermissionMode::AcceptEdits,
        ),
        Target::plain(
            "a call in the scroller",
            || {
                let session = scroller_over_one_call();
                let cell = find_on_text(&session, "out 0");
                (session, cell)
            },
            |session, _| render::as_text(session).contains("out 29"),
        ),
        Target::plain(
            "a delegate in the scroller",
            || {
                let session = scroller_over_two_delegates();
                let cell = find_on_text(&session, "write the docs");
                (session, cell)
            },
            |session, _| session.watching().is_some(),
        ),
        Target::plain(
            "the show trail hint",
            || hints::located(hints::with_a_trail),
            hints::trail_acted,
        ),
        Target::plain(
            "the shortcuts hint",
            || hints::located(hints::with_an_empty_box),
            hints::shortcuts_acted,
        ),
        Target::plain(
            "the info panel hint",
            || hints::located(hints::with_the_panel_closed),
            hints::panel_acted,
        ),
        Target::plain(
            "the hint that opens the delegate view",
            || hints::located(hints::with_something_to_open),
            hints::watch_acted,
        ),
        Target::plain(
            "the hint that opens the view of what commands printed",
            || hints::located(hints::with_a_command_to_open),
            hints::outputs_acted,
        ),
        Target::plain(
            "the hint that moves a command to the background",
            || hints::located(hints::with_a_command_to_move),
            hints::background_acted,
        ),
        Target::plain(
            "the hint that returns a held view to its end",
            || hints::located(hints::with_the_view_held),
            hints::held_acted,
        ),
        Target::plain(
            "the hint that leaves shell mode",
            || hints::located(hints::in_shell_mode),
            hints::cancel_acted,
        ),
        Target::plain(
            "the hint that pastes a picture",
            || hints::located(hints::with_a_picture_on_the_clipboard),
            hints::paste_acted,
        ),
        Target::plain(
            "the scrollbar beside the transcript",
            scrollbar::located_in_the_track,
            scrollbar::bar_acted,
        ),
        Target::plain(
            "a place in the text of the box",
            || {
                let mut session = typed_into("a line of words to click in");
                let buffer = drawn(&mut session);
                let (row, column) = find_on(&buffer, "words");
                (session, (row, column))
            },
            |session, _| session.caret() != "a line of words to click in".len(),
        ),
        Target::plain(
            "a delegate's block at rest",
            || {
                let (session, buffer) = at_rest_over_two_delegates();
                let cell = find_on(&buffer, "write the docs");
                (session, cell)
            },
            |session, _| session.watching().is_some(),
        ),
        Target::plain(
            "a call at rest",
            || {
                let (session, buffer) = at_rest_over_calls();
                let (row, column) = find_on(&buffer, "Run(make)");
                (session, (row + 2, column))
            },
            |session, _| session.scrolling(),
        ),
        Target::plain(
            "the marker that leads a reply",
            || {
                let (session, buffer) = at_rest_over_two_replies();
                let (row, _) = find_cell(&buffer, "A named answer.");
                (session, (row, 0))
            },
            |_, action| matches!(action, Action::CopyReply(_)),
        ),
        Target::plain(
            "the word that copies a fenced block",
            || {
                let (session, buffer) = at_rest_over_a_reply_with_blocks();
                let cell = cells_of(&buffer, "copy")[0];
                (session, cell)
            },
            |_, action| matches!(action, Action::CopyReply(_)),
        ),
        Target::plain(
            "a prompt waiting behind a turn",
            || {
                let (session, buffer) = working_with_three_waiting();
                let cell = find_cell(&buffer, "third question");
                (session, cell)
            },
            |session, _| waiting(session).len() < 3,
        ),
        Target::plain(
            "the way back of a delegate's view",
            || {
                let (session, buffer) = a_finished_delegates_view();
                let (row, column) = find_cell(&buffer, "\u{2039} back");
                (session, (row, column + 2))
            },
            |session, _| session.watching().is_none(),
        ),
        Target::plain(
            "the sentence that closes a delegate's view",
            || {
                let (session, buffer) = a_finished_delegates_view();
                let (row, column) = find_on(&buffer, "answered after 3 rounds");
                (session, (row, column + 5))
            },
            |session, _| session.watching().is_none(),
        ),
        Target::plain(
            "the session's name in the info panel",
            || {
                let (mut session, _) = two_panes();
                session.identify("tidy the parser", "/work/dir".to_string(), None);
                let buffer = drawn(&mut session);
                let cell = find_cell(&buffer, "tidy");
                (session, cell)
            },
            |session, _| session.input().starts_with(RENAME_COMMAND),
        ),
        Target::plain(
            "the hide row of the info panel",
            || {
                let (session, buffer) = panel_of_command_rows(false);
                let cell = on_the_hide_row(&buffer);
                (session, cell)
            },
            |session, _| !session.panel_open(),
        ),
        Target::plain(
            "the Files section of the info panel",
            || {
                let mut session = wrote_two_files(2);
                session.toggle_panel();
                let buffer = draw_at(&mut session, 120, 40);
                let cell = find_on(&buffer, t!(panel_files));
                (session, cell)
            },
            |session, _| session.files_open(),
        ),
        Target::plain(
            "a file's row of the files state",
            || {
                let session = in_the_files_state();
                let cell = a_panel_target(&session, crate::panel::Target::File(0));
                (session, cell)
            },
            |session, _| {
                session
                    .files_state()
                    .is_some_and(|state| state.selected == 0)
            },
        ),
        Target::plain(
            "the close control of the files state",
            || {
                let session = in_the_files_state();
                let cell = a_panel_target(&session, crate::panel::Target::CloseFiles);
                (session, cell)
            },
            |session, _| !session.files_open(),
        ),
        Target::plain(
            "the control for the next edit in the files state",
            || {
                let mut session = in_the_files_state();
                session.files_select(0);
                draw_at(&mut session, 120, 40);
                let cell = a_panel_target(&session, crate::panel::Target::NextEdit);
                (session, cell)
            },
            |session, _| session.files_state().is_some_and(|state| state.scroll > 0),
        ),
        Target::plain(
            "the control for the previous edit in the files state",
            || {
                let mut session = in_the_files_state();
                session.files_select(0);
                draw_at(&mut session, 120, 40);
                session.files_to_edit(true);
                draw_at(&mut session, 120, 40);
                let cell = a_panel_target(&session, crate::panel::Target::PreviousEdit);
                (session, cell)
            },
            |session, _| session.files_state().is_some_and(|state| state.scroll == 0),
        ),
        Target::plain(
            "a language server's row of the info panel",
            || {
                let (session, buffer) = panel_of_command_rows(false);
                let cell = find_cell(&buffer, "rust-analyzer");
                (session, cell)
            },
            |session, _| {
                said_in_the_transcript(session).contains(&t!(lsp_started, count = 1).to_string())
            },
        ),
        Target::plain(
            "a task of the plan in the info panel",
            || {
                let mut session = a_plan_a_long_way_up();
                let buffer = draw_at(&mut session, 120, 30);
                let cell = find_on(&buffer, "Fix it");
                (session, cell)
            },
            |session, _| session.rows_below() > 0,
        ),
        Target::plain(
            "the mark of a finished task in the info panel",
            || {
                let mut session = a_plan_a_long_way_up();
                let buffer = draw_at(&mut session, 120, 30);
                let (row, _) = find_on(&buffer, "Read the tokenizer");
                let mark = the_panel(&session).x + 2;
                (session, (row, mark))
            },
            |session, _| !session.plan_detail().is_empty(),
        ),
        Target::with_ctrl(
            "a web address in a reply",
            || {
                let (session, buffer) =
                    a_reply_of("The docs are at https://example.test/guide now");
                (session, shown_at(&buffer, "https://"))
            },
            |_, action| matches!(action, Action::OpenUrl(_)),
        ),
        Target::plain(
            "a file path in a reply",
            || {
                let (session, buffer) = a_reply_of("The fix is in src/main.rs:3 now");
                (session, shown_at(&buffer, "src/main.rs"))
            },
            |session, _| {
                session
                    .the_click_due_in(std::time::Instant::now())
                    .is_some()
            },
        ),
        Target::with_ctrl(
            "a file path in a reply, to the editor",
            || {
                let (session, buffer) = a_reply_of("The fix is in src/main.rs:3 now");
                (session, shown_at(&buffer, "src/main.rs"))
            },
            |_, action| matches!(action, Action::EditFile(_)),
        ),
        Target::plain(
            "the pull request row of the info panel",
            || {
                let (session, buffer) = with_a_pull_request_and_an_issue();
                (session, shown_at(&buffer, "Pull request"))
            },
            |_, action| matches!(action, Action::OpenUrl(_)),
        ),
        Target::plain(
            "the issue row of the info panel",
            || {
                let (session, buffer) = with_a_pull_request_and_an_issue();
                (session, shown_at(&buffer, "Issue"))
            },
            |_, action| matches!(action, Action::OpenUrl(_)),
        ),
    ]
}

/// A session, 120 columns wide, whose write calls changed `a.rs` `edits` times and then `b.rs` once.
fn wrote_two_files(edits: usize) -> Session {
    let mut session = Session::new("none");
    session.laid.columns = 120;
    let mut text = String::new();
    let wrote = |session: &mut Session, path: &str, before: &str, after: &str| {
        let diff = bravebot_agent::diff::Diff::compute(before, after);
        session.transcript.push(crate::state::Entry::tool(
            bravebot_agent::report::Activity::running("Update", path)
                .of_tool("edit_file")
                .with_changes(diff.condensed(3))
                .with_written(Some(bravebot_agent::report::Written {
                    landed: path.into(),
                    added: diff.added(),
                    removed: diff.removed(),
                    numbers: diff.condensed_numbers(3),
                }))
                .done("done"),
        ));
    };
    for edit in 0..edits {
        let before = text.clone();
        text.push_str(&format!("line {edit}\n"));
        wrote(&mut session, "a.rs", &before, &text);
    }
    wrote(&mut session, "b.rs", "p\n", "q\n");
    session
}

/// A session with the panel in its files state over two files, the second highlighted, `a.rs` with
/// two edits and `b.rs` with one, drawn 120 columns by 40 rows.
pub(super) fn in_the_files_state() -> Session {
    let mut session = wrote_two_files(2);
    session.toggle_files();
    assert!(session.files_open(), "the files state did not open");
    draw_at(&mut session, 120, 40);
    session
}

/// The rectangle the panel was drawn in by the last frame.
fn the_panel(session: &Session) -> Rect {
    session
        .laid
        .panes
        .iter()
        .find(|(pane, _)| *pane == crate::select::Pane::Panel)
        .expect("the panel is recorded")
        .1
}

/// The cell of a target of the panel, found by asking every cell of it.
pub(super) fn a_panel_target(session: &Session, wanted: crate::panel::Target) -> (u16, u16) {
    the_panels_targets(session)
        .into_iter()
        .find_map(|(target, row, column)| (target == wanted).then_some((row, column)))
        .unwrap_or_else(|| panic!("{wanted:?} is not a target of the panel"))
}

/// A session at rest whose transcript is one reply of `words`, drawn 100 columns by 12 rows.
pub(super) fn a_reply_of(words: &str) -> (Session, ratatui::buffer::Buffer) {
    let mut session = Session::new("kernel-enforced");
    session
        .transcript
        .push(crate::state::Entry::assistant(words, Vec::new()));
    let buffer = draw_at(&mut session, 100, 12);
    (session, buffer)
}

/// The row and column where `needle` is drawn, read through the escapes that carry an address.
pub(super) fn shown_at(buffer: &ratatui::buffer::Buffer, needle: &str) -> (u16, u16) {
    (0..buffer.area.height)
        .find_map(|row| {
            let line: String = (0..buffer.area.width)
                .map(|column| crate::hyperlink::shown(buffer[(column, row)].symbol()).to_string())
                .collect();
            line.find(needle)
                .map(|at| (row, line[..at].chars().count() as u16))
        })
        .unwrap_or_else(|| panic!("{needle:?} is not on the screen"))
}

/// A session with the info panel open, naming a pull request and an issue, drawn 120 columns by 24
/// rows.
pub(super) fn with_a_pull_request_and_an_issue() -> (Session, ratatui::buffer::Buffer) {
    let mut session = Session::new("kernel-enforced");
    session.laid.columns = 120;
    session.toggle_panel();
    session.identify("a session", "/work/dir".to_string(), Some("main"));
    session.link(
        Some("https://github.com/brave/bravebot/issues/2685"),
        Some("https://github.com/brave/bravebot/pull/3200"),
    );
    let buffer = draw_at(&mut session, 120, 24);
    (session, buffer)
}

/// A session with the info panel open whose plan has a finished task the panel's row cuts short, one
/// finished that fits and one in progress, written by a turn that is a long way above the end of the
/// transcript.
fn a_plan_a_long_way_up() -> Session {
    use bravebot_core::todo::{Item, List, Status};
    let mut session = Session::new("none");
    session.type_char('a');
    session.submit();
    session.set_todos(bravebot_core::todo::rows(&List::new(vec![
        Item::new(
            "Read the tokenizer and list every place a token is consumed twice, then write down what each does",
            Status::Done,
        ),
        Item::new("Fix it", Status::Done),
        Item::new("Write the regression test", Status::Active),
    ])));
    session.complete("the first answer", Vec::new(), 0);
    for _ in 0..2 {
        session.type_char('x');
        session.submit();
        let long: Vec<String> = (1..=30).map(|n| format!("answer line {n}")).collect();
        session.complete(long.join("\n\n"), Vec::new(), 0);
    }
    draw_at(&mut session, 120, 30);
    session.toggle_panel();
    session
}

/// A small panel over `cell` and the cells around it.
fn covering(cell: (u16, u16)) -> Popup {
    Popup {
        panel: crate::state::Panel::Picker,
        area: Rect::new(cell.1.saturating_sub(2), cell.0.saturating_sub(1), 6, 3),
        list: None,
    }
}

/// Open the picker the panel over `cell` is, which a panel stands for only while it is open, and
/// record the cells it covers as a frame that drew it would.
fn put_a_panel_over(session: &mut Session, cell: (u16, u16)) -> Popup {
    session.open_picking(crate::picking::Picking::Effort(
        crate::effort_prompt::Picker::new(None),
    ));
    let popup = covering(cell);
    session.laid.targets.extend(popup.recorded());
    popup
}

/// MOUSE-45. Rejects: a press on a panel drawn over a click target that is answered by the target
/// beneath it. Each row acts on a click with nothing over it, which shows the fixture draws the
/// target where the click lands, and does nothing when a panel is recorded over its cell.
#[test]
fn a_click_on_a_panel_reaches_no_target_beneath_it() {
    for target in targets() {
        let (mut session, (row, column)) = (target.build)();
        let mut control = (target.build)().0;
        let action = click_held(&mut control, (row, column), target.modifiers);
        assert!(
            (target.acted)(&control, &action),
            "{}: a click with nothing over it did nothing",
            target.name
        );

        // The person clicked this cell a moment ago, so a release there is a click unless the
        // panel took the press that began it.
        session.last_click =
            Some(crate::select::count_click(None, std::time::Instant::now(), row, column).0);
        put_a_panel_over(&mut session, (row, column));
        let action = click_held(&mut session, (row, column), target.modifiers);
        assert!(
            !(target.acted)(&session, &action),
            "{}: a click on a panel drawn over it acted on it ({action:?})",
            target.name
        );
        assert_eq!(
            action,
            Action::None,
            "{}: the panel did not take the release of a press on it",
            target.name
        );
        assert!(
            session.selection.is_none(),
            "{}: the press began a selection under the panel",
            target.name
        );
    }
}

/// MOUSE-45. Rejects: a drag that begins on a panel and is let go on a target, or begins on a
/// target and is let go on a panel, that acts on the target as a click would, and a release on a
/// panel that follows a press elsewhere being taken by the panel.
#[test]
fn a_press_and_its_release_on_different_sides_of_a_panel_click_nothing() {
    for target in targets() {
        let (mut session, (row, column)) = (target.build)();
        // Somewhere that is not the panel and not the target, and not on a column that is one: four
        // rows from it and a dozen columns in.
        let elsewhere = (
            if row >= 4 { row - 4 } else { row + 4 },
            column.saturating_sub(12),
        );
        let popup = put_a_panel_over(&mut session, (row, column));
        assert!(
            !popup
                .area
                .contains(ratatui::layout::Position::new(elsewhere.1, elsewhere.0))
        );

        // Pressed on the panel and let go on the target's cell beside it, off the panel.
        let beside = (row, popup.area.right());
        handle_mouse(
            &mut session,
            mouse_at(
                MouseEventKind::Down(MouseButton::Left),
                (row, column),
                target.modifiers,
            ),
        );
        let action = handle_mouse(
            &mut session,
            mouse_at(
                MouseEventKind::Up(MouseButton::Left),
                beside,
                target.modifiers,
            ),
        );
        assert!(
            !(target.acted)(&session, &action),
            "{}: a press on a panel let go beside it acted ({action:?})",
            target.name
        );

        // Pressed off the panel, carried over it and let go on it, which is a drag and not a click
        // on the target.
        for (kind, cell) in [
            (MouseEventKind::Down(MouseButton::Left), elsewhere),
            (MouseEventKind::Drag(MouseButton::Left), (row, column)),
        ] {
            handle_mouse(&mut session, mouse_at(kind, cell, target.modifiers));
        }
        let action = handle_mouse(
            &mut session,
            mouse_at(
                MouseEventKind::Up(MouseButton::Left),
                (row, column),
                target.modifiers,
            ),
        );
        assert!(
            !(target.acted)(&session, &action),
            "{}: a press elsewhere let go on a panel acted ({action:?})",
            target.name
        );
    }
}

/// MOUSE-45 and MOUSE-37. Rejects: a press on a panel drawn over the transcript beginning a
/// selection of the text under it, which a drag held below the transcript would then scroll the view
/// for and extend, and a release that ends it handing that text to the clipboard. The fixture
/// without a panel does all three, which shows a drag held below the transcript scrolls it here.
#[test]
fn a_press_on_a_panel_over_the_transcript_starts_no_selection_and_no_edge_scroll() {
    for under_a_panel in [false, true] {
        let mut session = a_long_reply(80);
        drawn_at(&mut session, 120, 24);
        for _ in 0..12 {
            wheel_up(&mut session);
        }
        let buffer = drawn_at(&mut session, 120, 24);
        let pane = the_transcript_pane(&session);
        let cell = find_cell(&buffer, "row 40");
        if under_a_panel {
            put_a_panel_over(&mut session, cell);
        }
        handle_mouse(
            &mut session,
            drag(MouseEventKind::Down(MouseButton::Left), cell.0, cell.1),
        );
        handle_mouse(
            &mut session,
            drag(MouseEventKind::Drag(MouseButton::Left), pane.bottom(), 8),
        );
        let scrolled_from = session.top_row();

        assert_eq!(
            session.pulling_at_an_edge(),
            !under_a_panel,
            "under a panel: {under_a_panel}"
        );
        let step = session.scroll_toward_the_pointer(std::time::Instant::now());
        assert_eq!(step, !under_a_panel, "under a panel: {under_a_panel}");
        assert_eq!(
            session.top_row() != scrolled_from,
            !under_a_panel,
            "under a panel: {under_a_panel}: the view moved"
        );
        assert_eq!(
            session.selection.is_some(),
            !under_a_panel,
            "under a panel: {under_a_panel}"
        );

        let handed = let_go(&mut session, (120, 24), pane.bottom(), 8);
        assert_eq!(
            handed.is_empty(),
            under_a_panel,
            "under a panel: {under_a_panel}: {handed:?}"
        );
    }
}

/// Draw the session's frame on a terminal this size and record it, as a redraw does.
fn draw_at(session: &mut Session, columns: u16, rows: u16) -> ratatui::buffer::Buffer {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let mut terminal = Terminal::new(TestBackend::new(columns, rows)).expect("terminal");
    let mut laid = crate::state::Laid::default();
    terminal
        .draw(|frame| laid = render::draw(frame, session))
        .expect("draw");
    session.note_layout(laid);
    terminal.backend().buffer().clone()
}

/// A frame with the info panel, a model and a context, drawn 100 columns by 14 rows, and the cell of
/// the context reading in it.
fn a_small_frame_with_a_context() -> (Session, (u16, u16)) {
    let (mut session, _) = panel_with_a_context();
    let buffer = draw_at(&mut session, 100, 14);
    let cell = find_on(&buffer, &render::context_reading(&session));
    (session, cell)
}

/// MOUSE-45. Rejects: the picker a turn in flight opens leaving the targets under it to answer a
/// press, which is the case a person meets: the context reading of the info panel covered by the
/// panel of an open `/effort`.
#[test]
fn a_press_on_the_picker_over_the_info_panel_leaves_the_context_unread() {
    let (mut session, (row, column)) = a_small_frame_with_a_context();
    session.open_picking(crate::picking::Picking::Effort(
        crate::effort_prompt::Picker::new(None),
    ));
    draw_at(&mut session, 100, 14);
    let popup = session
        .laid
        .popup(crate::state::Panel::Picker)
        .expect("the picker is recorded");
    assert!(
        popup
            .area
            .contains(ratatui::layout::Position::new(column, row)),
        "the picker {:?} is not over the context reading at ({row}, {column})",
        popup.area
    );

    assert_ne!(click(&mut session, row, column), Action::Copy);
    assert!(
        context_rows(&session).is_empty(),
        "the context was reported through the picker"
    );
    assert!(session.picking().is_some(), "the press closed the picker");
    assert!(session.selection.is_none());
}

/// MOUSE-45. Rejects: a turn of the wheel while a picker is open scrolling the transcript beneath
/// it, and a drag across it selecting what the picker covers.
#[test]
fn an_open_picker_takes_the_wheel_and_the_drag_from_the_transcript() {
    let (mut session, (row, column)) = a_small_frame_with_a_context();
    for n in 0..40 {
        session.transcript.push(crate::state::Entry::user(format!(
            "a line to scroll back through {n}"
        )));
    }
    draw_at(&mut session, 100, 14);
    session.open_picking(crate::picking::Picking::Effort(
        crate::effort_prompt::Picker::new(None),
    ));
    draw_at(&mut session, 100, 14);
    let scrolled = session.scroll;

    for kind in [MouseEventKind::ScrollUp, MouseEventKind::ScrollDown] {
        handle_mouse(&mut session, drag(kind, 1, 1));
        assert_eq!(session.scroll, scrolled, "{kind:?} moved the transcript");
    }
    swept(&mut session, (row, column), (row + 1, column + 5));
    handle_mouse(
        &mut session,
        drag(MouseEventKind::Up(MouseButton::Left), row + 1, column + 5),
    );
    assert!(session.selection.is_none(), "a drag began a selection");
}

/// MOUSE-45. Rejects: the scroller's key list leaving the call beneath it to answer a press that
/// lands on the list, which the list's own guard alone does not say for a press on its border.
#[test]
fn a_press_on_the_scrollers_key_list_expands_no_call_beneath_it() {
    let mut session = scroller_over_one_call();
    let (row, column) = find_on_text(&session, "out 0");
    session.toggle_scroller_help();
    draw_at(&mut session, 80, 11);
    let popup = session
        .laid
        .popup(crate::state::Panel::KeyList)
        .expect("the key list is recorded");
    let on_the_list = (
        popup.area.y + popup.area.height / 2,
        popup.area.x + popup.area.width / 2,
    );

    click(&mut session, on_the_list.0, on_the_list.1);
    click(&mut session, row, column);
    assert!(!render::as_text(&session).contains("out 29"));
}

/// MOUSE-46, MOUSE-47. Rejects: a left press outside the scroller's key list leaving it up or
/// doing anything else, a press on the list or of another button putting it away, and the wheel
/// scrolling the transcript behind it. Any key puts the list away, so a press outside does too.
#[test]
fn a_press_outside_the_scrollers_key_list_puts_it_away_and_nothing_else() {
    let mut session = scroller_over_one_call();
    let (row, column) = find_on_text(&session, "out 0");
    session.toggle_scroller_help();
    draw_at(&mut session, 80, 11);
    let list = session
        .laid
        .popup(crate::state::Panel::KeyList)
        .expect("the key list is recorded")
        .area;
    assert!(
        !list.contains(ratatui::layout::Position::new(column, row)),
        "the list covers the call, so a press on it is not outside"
    );
    let scrolled = session.scroll;

    // On the list, with another button, and the wheel anywhere: the list stays and nothing moves.
    click(&mut session, list.y + 1, list.x + 3);
    handle_mouse(
        &mut session,
        drag(MouseEventKind::Down(MouseButton::Right), row, column),
    );
    handle_mouse(&mut session, drag(MouseEventKind::ScrollDown, row, column));
    handle_mouse(&mut session, drag(MouseEventKind::ScrollUp, row, column));
    assert!(
        session.scroller().is_some_and(|scroller| scroller.help),
        "the key list was put away"
    );
    assert_eq!(
        session.scroll, scrolled,
        "the wheel moved the view behind the list"
    );

    // A left press outside is the press that puts it away, and it expands nothing.
    let action = click(&mut session, row, column);
    assert!(
        session.scroller().is_some_and(|scroller| !scroller.help),
        "the press outside left the list up"
    );
    assert_ne!(action, Action::Copy);
    assert!(!render::as_text(&session).contains("out 29"));
    assert!(session.selection.is_none());
}
