//! MOUSE-51: the target the pointer is over, and when a frame is wanted for it.

use super::popups::targets;
use super::*;
use crate::state::Hit;

fn moved(row: u16, column: u16) -> MouseEvent {
    drag(MouseEventKind::Moved, row, column)
}

/// What a person could tell of a session that moving the pointer must leave as it was, set out in one
/// string: the box and its caret, the view, what runs, what is selected and copied, which mode is
/// open, the permission mode, what waits and what the transcript says.
fn observed(session: &Session) -> String {
    let words: Vec<&str> = session
        .transcript
        .iter()
        .map(|entry| entry.text.as_str())
        .collect();
    format!(
        "{:?} {:?}",
        (
            session.input(),
            session.caret(),
            session.scroll,
            session.status,
            session.selection,
            session.copied,
            session.permission_mode(),
            session.queued.len(),
            session.plan_detail().len(),
            session.last_click.is_some(),
        ),
        (
            session.scrolling(),
            session.watching().is_some(),
            session.searching_history(),
            session.picking().is_some(),
            session.files_open(),
            session.panel_open(),
            words,
        )
    )
}

/// How many of the moves asked for a frame.
fn frames_asked(session: &mut Session, moves: impl IntoIterator<Item = (u16, u16)>) -> usize {
    moves
        .into_iter()
        .filter(|&(row, column)| handle_mouse(session, moved(row, column)) == Action::Redraw)
        .count()
}

/// MOUSE-51. Rejects: a frame asked for by each report of the pointer, which a pointer crossing a
/// row sends for every cell, and a frame not asked for when the pointer reaches the next target, whose
/// marking and line then stay on the one it left. A thousand moves across the cells of one row of the
/// info panel ask for one frame, a move to the next row for one more, and a move off every target for
/// the one that takes the marking away.
#[test]
fn a_thousand_moves_within_one_target_ask_for_one_frame_and_a_move_to_the_next_asks_for_one_more() {
    let (mut session, buffer) = panel_with_a_model();
    let (model, column) = find_on(&buffer, "claude-sonnet");
    let (effort, _) = find_on(&buffer, "effort");
    assert_ne!(model, effort);

    let along = |row: u16| (0..1000u16).map(move |step| (row, column + step % 8));
    assert_eq!(frames_asked(&mut session, along(model)), 1);
    assert_eq!(
        session.hovered().map(|hovered| hovered.hit),
        Some(Hit::Panel(crate::panel::Target::Model))
    );

    assert_eq!(frames_asked(&mut session, along(effort)), 1);
    assert_eq!(
        session.hovered().map(|hovered| hovered.hit),
        Some(Hit::Panel(crate::panel::Target::Effort))
    );

    // Back and forth between the two asks for a frame each time the target changes and for none while
    // it does not.
    let there_and_back = [
        (model, column),
        (model, column + 1),
        (effort, column),
        (effort, column + 1),
    ];
    assert_eq!(frames_asked(&mut session, there_and_back), 2);

    // The transcript's blank rows hold no target, so the move off the panel's rows asks for the frame
    // that takes the marking away, and the rest of the moves over them for none.
    let off = (0..500u16).map(|step| (1, 60 + step % 8));
    assert_eq!(frames_asked(&mut session, off), 1);
    assert!(session.hovered().is_none());
}

/// MOUSE-51. Rejects: two targets of one kind taken for one, so that a move from the row of one
/// server to the row of the next asks for no frame and the marking and the line stay on the first.
#[test]
fn a_move_from_the_row_of_one_server_to_the_row_of_another_asks_for_a_frame() {
    let (mut session, _) = panel_of_command_rows(false);
    session.servers.started = vec!["notes-server".to_string(), "docs-server".to_string()];
    let buffer = drawn(&mut session);
    let (notes, column) = find_on(&buffer, "notes-server");
    let (docs, _) = find_on(&buffer, "docs-server");
    assert_ne!(notes, docs);

    assert_eq!(frames_asked(&mut session, [(notes, column)]), 1);
    let first = session.hovered().expect("the first server's row");
    assert_eq!(frames_asked(&mut session, [(docs, column)]), 1);
    let second = session.hovered().expect("the second server's row");

    assert_eq!(first.hit, second.hit, "the rows are not of one kind");
    assert_ne!(first.area, second.area, "the marking did not move");
    assert_eq!(frames_asked(&mut session, [(docs, column + 1)]), 0);
}

/// MOUSE-51. Rejects: a move of the pointer that does anything but change what is marked: selecting,
/// clicking, scrolling, opening a view, typing or copying. Every cell of the screen is moved to, in
/// each frame the table of targets draws, and the session is as it was.
#[test]
fn moving_across_every_target_changes_no_session_state_but_the_hover() {
    for fixture in targets() {
        let (mut session, (row, column)) = (fixture.build)();
        let before = observed(&session);

        assert!(
            frames_asked(&mut session, [(row, column)]) <= 1,
            "{}",
            fixture.name
        );
        // The text of the box is where the caret goes wherever the pointer is over it, and says
        // nothing of a click.
        let text_of_the_box = fixture.name == "a place in the text of the box";
        assert_eq!(
            session.hovered().is_some(),
            !text_of_the_box,
            "{}: a target was hovered or not where its cell is",
            fixture.name
        );
        let rows = session.laid.lines.max(40);
        let columns = session.laid.columns.max(120);
        for row in 0..rows {
            for column in 0..columns {
                handle_mouse(&mut session, moved(row, column));
            }
        }
        assert_eq!(
            observed(&session),
            before,
            "{}: moving the pointer changed the session",
            fixture.name
        );
    }
}

/// MOUSE-51. Rejects: a target still marked after a key, after the terminal was left or resized, and
/// after a button went down, each of which is something a person looks at the screen for the result
/// of, with the marking left on what the pointer was over before it. Each also asks for the frame that
/// takes the marking away.
#[test]
fn what_is_marked_ends_with_a_key_a_focus_loss_a_resize_and_a_press() {
    use ratatui::crossterm::event::Event as TermEvent;
    type Ending = fn(&mut Session);
    let endings: [(&str, Ending); 4] = [
        ("a key", |session| {
            crate::app::keys::took_input(session, &TermEvent::Key(key(KeyCode::Char('a'))));
        }),
        ("the terminal being left", |session| {
            crate::app::keys::took_input(session, &TermEvent::FocusLost);
        }),
        ("a resize", |session| {
            crate::app::keys::took_input(session, &TermEvent::Resize(100, 30));
        }),
        ("a press", |session| {
            handle_mouse(
                session,
                drag(MouseEventKind::Down(MouseButton::Left), 1, 60),
            );
        }),
    ];
    for (name, ending) in endings {
        let (mut session, buffer) = panel_with_a_model();
        let (row, column) = find_on(&buffer, "claude-sonnet");
        handle_mouse(&mut session, moved(row, column));
        assert!(session.hovered().is_some(), "{name}: nothing was hovered");
        assert!(!session.take_the_hover_ended());

        ending(&mut session);
        assert!(session.hovered().is_none(), "{name}: still hovered");
        assert!(!session.is_hovering(), "{name}: still hovering");
        assert!(
            session.take_the_hover_ended(),
            "{name}: no frame was asked for to take the marking away"
        );
    }

    // A release of a key is the tail of a press that has already ended it, and a turn of the wheel
    // moves what is under the pointer without ending that the pointer is over something.
    let (mut session, buffer) = panel_with_a_model();
    let (row, column) = find_on(&buffer, "claude-sonnet");
    handle_mouse(&mut session, moved(row, column));
    handle_mouse(&mut session, drag(MouseEventKind::ScrollDown, row, column));
    assert!(session.is_hovering(), "the wheel ended the hover");
}

/// MOUSE-51. Rejects: a target the pointer is over staying hovered once the state says a click on it
/// does nothing: the hint under the box while a command the person typed is running, which no key of
/// the row is read for, and a target under a picker, which holds the input.
#[test]
fn a_target_that_stops_standing_is_no_longer_hovered() {
    let (mut session, (row, column)) = hints_located(hints::with_the_panel_closed);
    assert_eq!(frames_asked(&mut session, [(row, column)]), 1);
    assert!(session.hovered().is_some());

    session.status = crate::state::Status::Running;
    assert_eq!(
        frames_asked(&mut session, [(row, column)]),
        1,
        "the hint was still hovered while its key is refused"
    );
    assert!(session.hovered().is_none());

    let (mut session, buffer) = panel_with_a_model();
    let (row, column) = find_on(&buffer, "claude-sonnet");
    session.open_picking(crate::picking::Picking::Effort(
        crate::effort_prompt::Picker::new(None),
    ));
    handle_mouse(&mut session, moved(row, column));
    assert!(session.hovered().is_none(), "a target under a picker");
}

/// MOUSE-51. Rejects: a marking that stays where the target was once the frame is drawn without it,
/// as when the panel is closed under a pointer that has not moved, since no report of the pointer
/// follows the frame that changed what is under it.
#[test]
fn a_frame_that_moves_what_is_under_the_pointer_moves_the_hover() {
    let (mut session, buffer) = panel_with_a_model();
    let (row, column) = find_on(&buffer, "claude-sonnet");
    handle_mouse(&mut session, moved(row, column));
    assert!(session.hovered().is_some());

    session.toggle_panel();
    drawn(&mut session);
    assert!(
        session.hovered().is_none(),
        "the panel is closed and the row is still hovered"
    );
    assert!(session.is_hovering(), "the pointer did not stop hovering");
}

fn hints_located(build: fn() -> (Session, String)) -> (Session, (u16, u16)) {
    super::hints::located(build)
}

use super::hints;

/// The session's frame drawn again at the size it was last drawn at, which the cell of a fixture was
/// found in. A layout that was made without a frame has no size of its own and takes the one it was
/// made for, with a row for the footer.
fn drawn_again(session: &mut Session) -> ratatui::buffer::Buffer {
    let (columns, lines) = match session.laid.columns {
        0 => (session.laid.width, session.laid.height + 1),
        columns => (columns, session.laid.lines),
    };
    drawn_at(session, columns, lines)
}

/// MOUSE-51. Rejects: a target the pointer is over drawn as it was, a mark on cells that are not it, a
/// mark across the blanks of its row, a mark that changes what a cell says, and a frame in which a
/// target that cannot be underlined, the scrollbar's track, is. Each fixture of the table of targets is
/// drawn with and without the pointer over it, and the cells that differ are the words of its first row
/// and are underlined and bold.
#[test]
fn the_target_under_the_pointer_is_drawn_underlined_and_bold_and_nothing_else_changes() {
    use ratatui::style::Modifier;
    let mark = Modifier::UNDERLINED | Modifier::BOLD;
    for fixture in targets() {
        let (mut session, (row, column)) = (fixture.build)();
        session.end_hover();
        let plain = drawn_again(&mut session);
        handle_mouse(&mut session, moved(row, column));
        let hovered = session.hovered();
        let marked = drawn_again(&mut session);
        let Some(hovered) = hovered else {
            assert_eq!(
                fixture.name, "a place in the text of the box",
                "{}: no target was hovered",
                fixture.name
            );
            continue;
        };
        assert_eq!(plain.area, marked.area, "{}", fixture.name);

        // The one row where text is added is the row of hints, in cells that were blank.
        let hint_row = session.laid.hint_row.map(|row| row.area.y);
        let mut changed = Vec::new();
        for y in 0..plain.area.height {
            for x in 0..plain.area.width {
                let (before, after) = (&plain[(x, y)], &marked[(x, y)]);
                if before.symbol() != after.symbol() {
                    assert!(
                        Some(y) == hint_row && before.symbol().trim().is_empty(),
                        "{}: the marking changed what ({x}, {y}) says",
                        fixture.name
                    );
                    continue;
                }
                if before.modifier != after.modifier {
                    changed.push((x, y));
                    assert!(
                        after.modifier.contains(mark),
                        "{}: ({x}, {y}) is marked as {:?}",
                        fixture.name,
                        after.modifier
                    );
                }
            }
        }
        // The blanks between the words of a row are marked with them and the blanks before and after
        // them are not, since a target is as wide as its row.
        for &(x, y) in &changed {
            let at_an_end =
                !changed.contains(&(x.wrapping_sub(1), y)) || !changed.contains(&(x + 1, y));
            assert!(
                !at_an_end || !marked[(x, y)].symbol().trim().is_empty(),
                "{}: a blank at the end of the mark at ({x}, {y})",
                fixture.name
            );
        }
        if matches!(hovered.hit, Hit::Scrollbar) {
            assert!(changed.is_empty(), "{}: the track was marked", fixture.name);
            continue;
        }
        assert!(
            !changed.is_empty(),
            "{}: the target was not marked",
            fixture.name
        );
        let words: Vec<u16> = (hovered.area.x..hovered.area.right())
            .filter(|x| !marked[(*x, hovered.area.y)].symbol().trim().is_empty())
            .collect();
        let (first, last) = (words[0], words[words.len() - 1]);
        for x in first..=last {
            assert!(
                changed.contains(&(x, hovered.area.y)),
                "{}: ({x}, {}) is a word of the target and is not marked",
                fixture.name,
                hovered.area.y
            );
        }
        if !matches!(hovered.hit, Hit::Link(_)) {
            assert!(
                changed.iter().all(|&(_, y)| y == hovered.area.y),
                "{}: cells outside the first row were marked: {changed:?}",
                fixture.name
            );
        }
    }
}

/// MOUSE-51. Rejects: an address wrapped over two rows marked on the row the pointer is on and not on
/// the rest, which reads as two addresses where there is one.
#[test]
fn an_address_wrapped_over_rows_is_marked_on_every_row_of_it() {
    use ratatui::style::Modifier;
    let mut session = Session::new("kernel-enforced");
    session.transcript.push(crate::state::Entry::assistant(
        "See https://example.test/a/very/long/path/that/does/not/fit/on/one/row/of/the/screen now",
        Vec::new(),
    ));
    drawn_at(&mut session, 40, 12);
    let runs: Vec<_> = session
        .laid
        .targets
        .iter()
        .filter(|target| matches!(target.hit, Hit::Link(_)))
        .map(|target| target.area)
        .collect();
    assert!(runs.len() >= 2, "the address was not wrapped: {runs:?}");

    handle_mouse(&mut session, moved(runs[0].y, runs[0].x + 1));
    let buffer = drawn_at(&mut session, 40, 12);
    for run in runs {
        for x in run.x..run.right() {
            assert!(
                buffer[(x, run.y)]
                    .modifier
                    .contains(Modifier::UNDERLINED | Modifier::BOLD),
                "({x}, {}) of the address is not marked",
                run.y
            );
        }
    }
}

/// What a click on the fixture's target is said to do, as the catalog words it, or none where the
/// target says nothing.
fn said_of(name: &str) -> Option<&'static str> {
    Some(match name {
        "the model row of the info panel" => t!(hover_panel_model),
        "the effort row of the info panel" => t!(hover_panel_effort),
        "the context reading of the info panel" => t!(hover_panel_context),
        "a started server's row of the info panel" => t!(hover_panel_servers),
        "the permission mode under the box" => t!(hover_footer_mode),
        "a call in the scroller" => t!(hover_call_toggle),
        "a delegate in the scroller" | "a delegate's block at rest" => t!(hover_delegate),
        "the show trail hint" => t!(hover_footer_trail),
        "the shortcuts hint" => t!(hover_footer_shortcuts),
        "the info panel hint" => t!(hover_footer_info),
        "the hint that opens the delegate view" => t!(hover_footer_watch),
        "the hint that opens the view of what commands printed" => t!(hover_footer_outputs),
        "the hint that moves a command to the background" => t!(hover_footer_background),
        "the hint that returns a held view to its end" => t!(hover_footer_held),
        "the hint that leaves shell mode" => t!(hover_footer_cancel_shell),
        "the hint that pastes a picture" => t!(hover_footer_paste),
        "the scrollbar beside the transcript" => t!(hover_scrollbar),
        "a place in the text of the box" => return None,
        "a call at rest" => t!(hover_call_open),
        "the marker that leads a reply" => t!(hover_reply),
        "the word that copies a fenced block" => t!(hover_block),
        "a prompt waiting behind a turn" => t!(hover_queued),
        "the way back of a delegate's view" | "the sentence that closes a delegate's view" => {
            t!(hover_back)
        }
        "the session's name in the info panel" => t!(hover_panel_name),
        "the hide row of the info panel" => t!(hover_panel_hide),
        "the Files section of the info panel" => t!(hover_panel_files),
        "a file's row of the files state" => t!(hover_panel_file),
        "the close control of the files state" => t!(hover_panel_close_files),
        "the control for the next edit in the files state" => t!(hover_panel_edit_next),
        "the control for the previous edit in the files state" => t!(hover_panel_edit_previous),
        "a language server's row of the info panel" => t!(hover_panel_languages),
        "a task of the plan in the info panel" => t!(hover_panel_task),
        "the mark of a finished task in the info panel" => t!(hover_panel_detail),
        "a web address in a reply" => t!(hover_address),
        "a file path in a reply" | "a file path in a reply, to the editor" => t!(hover_path),
        "the pull request row of the info panel" | "the issue row of the info panel" => {
            t!(hover_panel_link)
        }
        other => panic!("no sentence is expected for {other:?}"),
    })
}

/// The cells of the row of hints of the last frame, as one string, and as the cells of it.
fn the_hint_row(session: &Session, buffer: &ratatui::buffer::Buffer) -> (String, Vec<String>) {
    let row = session
        .laid
        .hint_row
        .expect("the frame drew no hint row")
        .area;
    let cells: Vec<String> = (row.x..row.right())
        .map(|x| buffer[(x, row.y)].symbol().to_string())
        .collect();
    (cells.concat(), cells)
}

/// MOUSE-51. Rejects: a target that does not say what a click on it does, a line that says it where
/// the row has no room, which is half a sentence or a hint written over, and a hint that is moved or
/// covered to make room. Each fixture of the table of targets is drawn with and without the pointer
/// over it: the cells of the row are all as they were, and the sentence is added in the room after
/// them where the whole of it fits there, and nowhere else.
#[test]
fn the_row_of_hints_says_what_a_click_on_the_target_under_the_pointer_does_in_the_room_it_leaves() {
    let mut said_in_full = 0;
    for fixture in targets() {
        let (mut session, (row, column)) = (fixture.build)();
        session.end_hover();
        let plain = drawn_again(&mut session);
        let (before, before_cells) = the_hint_row(&session, &plain);
        handle_mouse(&mut session, moved(row, column));
        let hovered = drawn_again(&mut session);
        let (after, after_cells) = the_hint_row(&session, &hovered);

        for (index, cell) in before_cells.iter().enumerate() {
            if !cell.trim().is_empty() {
                assert_eq!(
                    cell, &after_cells[index],
                    "{}: the cell {index} of the row was changed",
                    fixture.name
                );
            }
        }
        let Some(said) = said_of(fixture.name) else {
            assert_eq!(after, before, "{}: a line was said", fixture.name);
            continue;
        };
        let width = crate::wrap::display_width(said);
        let drawn_to = before_cells
            .iter()
            .rposition(|cell| !cell.trim().is_empty())
            .map_or(0, |last| last + 1);
        // After two blanks, to the edge.
        let room = after_cells.len().saturating_sub(drawn_to + 2);
        let fits = room >= width;
        said_in_full += usize::from(fits);
        assert_eq!(
            after.contains(said),
            fits,
            "{}: the sentence {said:?} {} fit in {room} columns after {before:?} and is {}",
            fixture.name,
            if fits { "does" } else { "does not" },
            if fits {
                "missing"
            } else {
                "there in part or whole"
            }
        );
        // A link is told by where it goes where the sentence has no room, which
        // `the_line_for_a_link_carries_the_address_or_path_cut_to_the_room_left` holds to account.
        let says_where_it_goes = [
            "a web address in a reply",
            "a file path in a reply",
            "a file path in a reply, to the editor",
            "the pull request row of the info panel",
            "the issue row of the info panel",
        ]
        .contains(&fixture.name);
        if !fits && !says_where_it_goes {
            assert_eq!(after, before, "{}: the row was written to", fixture.name);
        }
    }
    // Most of the table has the room, so a line that is never drawn does not pass for one that is
    // drawn where it fits.
    assert!(
        said_in_full >= 20,
        "only {said_in_full} fixtures had the sentence said"
    );
}

/// MOUSE-51. Rejects: the address or the path of a link left off the line, an address cut from the
/// left, which loses the host that tells it from another, a path cut from the right, which loses the
/// name, and a cut that leaves more than the row holds. The line is the sentence, two blanks with a
/// point between and the address as the cells show it, cut with an ellipsis to the room that is left.
/// Where the room has none to spare for the address beside the sentence, the line is the address, cut,
/// and not the sentence: the address is what the person cannot read elsewhere, since the cells show
/// only what the row of the transcript could hold of it. The widths are from one that holds both
/// to one that holds neither.
#[test]
fn the_line_for_a_link_carries_the_address_or_path_cut_to_the_room_left() {
    let address = "https://example.test/a/long/path/to/a/page/that/is/longer/than/the/row";
    let path = "crates/tui/src/a/long/directory/name/main.rs:42";
    let mut with_both = 0;
    let mut detail_alone = 0;
    for (words, hover_on, said, detail, keeps_the_end) in [
        (
            format!("The page is at {address} now"),
            "https://",
            t!(hover_address),
            address,
            false,
        ),
        (
            format!("The fix is in {path} now"),
            "crates/",
            t!(hover_path),
            path,
            true,
        ),
    ] {
        for columns in [200u16, 140, 120, 90, 80, 60, 40] {
            let mut session = Session::new("kernel-enforced");
            session
                .transcript
                .push(crate::state::Entry::assistant(&words, Vec::new()));
            let buffer = drawn_at(&mut session, columns, 12);
            let cell = super::popups::shown_at(&buffer, hover_on);
            handle_mouse(&mut session, moved(cell.0, cell.1));
            let (plain_row, _) = the_hint_row(&session, &buffer);
            let buffer = drawn_at(&mut session, columns, 12);
            let (row, _) = the_hint_row(&session, &buffer);
            // What was added to the row of hints, at the right of what it said, in the room that
            // was left after two blanks.
            let room = usize::from(columns)
                .saturating_sub(crate::wrap::display_width(plain_row.trim_end()) + 2);
            let width_of_the_sentence = crate::wrap::display_width(said);
            let separator = "  \u{b7}  ";
            let beside = room.saturating_sub(width_of_the_sentence + separator.chars().count());
            let line = row[plain_row.trim_end().len()..].trim();
            assert!(
                crate::wrap::display_width(line) <= room,
                "{columns}: the line is wider than the room left"
            );
            let (shown, whole) = if width_of_the_sentence <= room && beside >= 4 {
                with_both += 1;
                assert!(
                    line.starts_with(said),
                    "{columns}: {line:?} is not the sentence"
                );
                let after = line[said.len()..].trim_start();
                (after.trim_start_matches('\u{b7}').trim(), beside)
            } else if room >= 4 {
                detail_alone += 1;
                assert!(
                    !line.contains(said),
                    "{columns}: {line:?} is the sentence with no room for the address beside it"
                );
                (line, room)
            } else {
                assert_eq!(row, plain_row, "{columns}: written where nothing fits");
                continue;
            };
            if detail.chars().count() <= whole {
                assert_eq!(shown, detail, "{columns}");
            } else {
                assert!(shown.contains('\u{2026}'), "{columns}: not cut: {shown:?}");
                let kept = shown.replace('\u{2026}', "");
                if keeps_the_end {
                    assert!(detail.ends_with(&kept), "{columns}: {shown:?} lost the end");
                } else {
                    assert!(
                        detail.starts_with(&kept),
                        "{columns}: {shown:?} lost the start"
                    );
                }
            }
        }
    }
    assert!(
        with_both >= 2 && detail_alone >= 4,
        "{with_both} {detail_alone}"
    );
}

/// MOUSE-51. Rejects: the line written over a copy notice, over the banner that names a prompt an
/// outside link wrote, over the one line of the files state, or over a search being typed in the
/// scroller, each of which is a message the row is saying and which the line gives way to. The row of
/// hints is the same with the pointer over a target and without.
#[test]
fn the_line_gives_way_to_a_note_a_banner_the_files_line_and_a_search_being_typed() {
    fn with_and_without(session: &mut Session, cell: (u16, u16)) -> (String, String) {
        session.end_hover();
        let plain = drawn_again(session);
        let (without, _) = the_hint_row(session, &plain);
        handle_mouse(session, moved(cell.0, cell.1));
        let hovered = drawn_again(session);
        assert!(session.hovered().is_some(), "nothing was hovered");
        let (with, _) = the_hint_row(session, &hovered);
        (without, with)
    }

    // A copy notice at the right of the row.
    let (mut session, buffer) = panel_with_a_model();
    let cell = find_on(&buffer, "claude-sonnet");
    session.copied = Some(12);
    let (without, with) = with_and_without(&mut session, cell);
    assert!(without.contains("12 chars to clipboard"), "{without:?}");
    assert_eq!(with, without, "the line was written beside a notice");

    // The banner of a prompt an outside link wrote, which fills the row.
    let (mut session, buffer) = panel_with_a_model();
    let cell = find_on(&buffer, "claude-sonnet");
    session.prefill_from_link("fix the test");
    let (without, with) = with_and_without(&mut session, cell);
    assert!(without.contains("external link"), "{without:?}");
    assert_eq!(with, without, "the line was written beside a banner");

    // The files state's line of keys, which it has where the panel is not drawn, and which fills the
    // row. Beside the figures it has with the panel drawn there is room for the sentence.
    let mut session = Session::new("kernel-enforced");
    session.transcript.push(crate::state::Entry::assistant(
        "The docs are at https://example.test/guide now",
        Vec::new(),
    ));
    super::standing::enter_files(&mut session);
    let buffer = drawn_at(&mut session, 90, 12);
    let cell = super::popups::shown_at(&buffer, "https://");
    assert!(
        find_on(&buffer, "q back").0 > 0,
        "the files state drew no line of keys"
    );
    let (without, with) = with_and_without(&mut session, cell);
    assert_eq!(with, without, "the line was written beside the files line");

    // A search being typed in the scroller owns the line.
    let mut session = scroller_over_one_call();
    let cell = find_on_text(&session, "out 0");
    session.begin_search();
    let (without, with) = with_and_without(&mut session, cell);
    assert_eq!(with, without, "the line was written beside a search");
}

/// MOUSE-51. Rejects: the shorthand a row of the info panel draws for a pull request or an issue
/// being what the line shows, where the address that opens is what a person is endorsing by the click
/// ([MOUSE-41]).
#[test]
fn the_line_for_a_link_of_the_panel_shows_the_address_that_opens_and_not_the_shorthand() {
    let (mut session, buffer) = super::popups::with_a_pull_request_and_an_issue();
    let cell = super::popups::shown_at(&buffer, "Pull request");
    handle_mouse(&mut session, moved(cell.0, cell.1));
    let hovered = drawn_again(&mut session);
    let (row, _) = the_hint_row(&session, &hovered);
    let said = t!(hover_panel_link);
    let after = row
        .split(said)
        .nth(1)
        .unwrap_or_else(|| panic!("the sentence is not on the row: {row:?}"));
    assert!(
        after
            .trim()
            .starts_with("\u{b7}  https://github.com/brave/bravebot/"),
        "the address that opens is not on the row: {after:?}"
    );
}
