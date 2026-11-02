//! BOT-17, BOT-18.

use super::bots_screen::{self, Step};
use super::phase::{Carrying, Phase, Screen};
use super::*;
use crate::bots_pane::{self, Focus};
use crate::layout::Layout;
use crate::select::{Pane, Selection};
use crate::state::Laid;
use bravebot_agent::bots::{Dropped, Listing, Problem, Reason, Row};
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use std::path::PathBuf;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

fn alt(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::ALT)
}

fn type_text(session: &mut Session, text: &str) {
    for c in text.chars() {
        handle_key(session, key(KeyCode::Char(c)));
    }
}

fn row(slug: &str, name: &str, purpose: &str, avatar: &str) -> Row {
    Row {
        slug: slug.to_string(),
        name: name.to_string(),
        purpose: purpose.to_string(),
        avatar: avatar.to_string(),
        model: None,
        home: "/home/bots".to_string(),
        definition: None,
        session: None,
        conversations: Vec::new(),
        archived: 0,
        retired: 0.0,
        created: 1.0,
        updated: 1.0,
    }
}

fn three_bots() -> Vec<Row> {
    vec![
        row("zeta", "Zeta", "answers questions", "seed-zeta"),
        row("alpha", "Alpha", "reviews diffs", "seed-alpha"),
        row("mid", "Middle", "writes tests", "seed-mid"),
    ]
}

fn listing(rows: Vec<Row>) -> Listing {
    Listing {
        rows,
        ..Listing::default()
    }
}

fn state_of(listing: Listing) -> bots_pane::State {
    bots_pane::State::listing(listing, Some(PathBuf::from("/state/bots.json")))
}

fn without_a_session(rows: Vec<Row>, columns: u16) -> Session {
    let mut session = Session::new("none")
        .in_layout(Layout::Bots)
        .with_bots(state_of(listing(rows)))
        .with_no_session_begun();
    session.start_in_the_list_where_it_is_drawn(columns);
    session
}

fn in_a_session(rows: Vec<Row>, columns: u16) -> Session {
    let mut session = Session::new("none")
        .in_layout(Layout::Bots)
        .with_bots(state_of(listing(rows)));
    session.laid.columns = columns;
    session
}

fn drawn(session: &mut Session, width: u16, height: u16) -> Buffer {
    let mut terminal = ratatui::Terminal::new(TestBackend::new(width, height)).expect("terminal");
    let mut laid = Laid::default();
    terminal
        .draw(|frame| laid = render::draw(frame, session))
        .expect("draw");
    session.note_layout(laid);
    terminal.backend().buffer().clone()
}

fn row_text(buffer: &Buffer, row: u16) -> String {
    (0..buffer.area.width)
        .map(|column| buffer[(column, row)].symbol())
        .collect()
}

fn screen_text(buffer: &Buffer) -> String {
    (0..buffer.area.height)
        .map(|row| row_text(buffer, row))
        .collect::<Vec<_>>()
        .join("\n")
}

fn row_naming(buffer: &Buffer, needle: &str) -> u16 {
    (0..buffer.area.height)
        .find(|row| row_text(buffer, *row).contains(needle))
        .unwrap_or_else(|| panic!("{needle} was not drawn:\n{}", screen_text(buffer)))
}

fn the_laid_pane(session: &Session, wanted: Pane) -> Option<ratatui::layout::Rect> {
    session
        .laid
        .panes
        .iter()
        .find(|(pane, _)| *pane == wanted)
        .map(|(_, rect)| *rect)
}

fn selected(session: &Session) -> usize {
    session.bots().selected()
}

fn mouse(kind: MouseEventKind, row: u16, column: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

fn swept(session: &mut Session, from: (u16, u16), to: (u16, u16)) {
    handle_mouse(
        session,
        mouse(MouseEventKind::Down(MouseButton::Left), from.0, from.1),
    );
    handle_mouse(
        session,
        mouse(MouseEventKind::Drag(MouseButton::Left), to.0, to.1),
    );
}

mod drawing {
    use super::*;

    fn expected_face(seed: &str) -> Vec<Vec<ratatui::text::Span<'static>>> {
        crate::avatar::rows(
            seed,
            crate::avatar::Look::default(),
            &crate::theme::mark(),
            crate::blocks::needs_painting(),
        )
    }

    fn assert_face_drawn_at(buffer: &Buffer, seed: &str, top: u16, left: u16) {
        let face = expected_face(seed);
        assert_eq!(face.len(), crate::avatar::ROWS);
        for (line, cells) in face.iter().enumerate() {
            assert_eq!(cells.len(), crate::avatar::WIDTH);
            for (offset, span) in cells.iter().enumerate() {
                let cell = &buffer[(left + offset as u16, top + line as u16)];
                assert_eq!(
                    cell.symbol(),
                    span.content.as_ref(),
                    "face row {line} column {offset} of {seed}"
                );
                if let Some(colour) = span.style.fg {
                    assert_eq!(cell.fg, colour, "face row {line} column {offset} of {seed}");
                }
            }
        }
    }

    /// BOT-18.
    #[test]
    fn each_bot_is_drawn_with_the_face_its_seed_describes_and_is_as_tall_as_the_face() {
        let mut session = without_a_session(three_bots(), 120);
        let buffer = drawn(&mut session, 120, 40);

        let first = row_naming(&buffer, "Zeta");
        let second = row_naming(&buffer, "Alpha");
        let third = row_naming(&buffer, "Middle");
        assert_eq!(second - first, crate::avatar::ROWS as u16);
        assert_eq!(third - second, crate::avatar::ROWS as u16);
        for (seed, top) in [
            ("seed-zeta", first),
            ("seed-alpha", second),
            ("seed-mid", third),
        ] {
            assert_face_drawn_at(&buffer, seed, top, 2);
        }
        assert_ne!(
            expected_face("seed-zeta")
                .iter()
                .flatten()
                .map(|span| span.content.to_string())
                .collect::<Vec<_>>(),
            expected_face("seed-alpha")
                .iter()
                .flatten()
                .map(|span| span.content.to_string())
                .collect::<Vec<_>>(),
            "the fixture's two seeds draw one face"
        );
        let purpose = row_text(&buffer, first + 1);
        assert!(purpose.contains("answers questions"), "{purpose}");
    }

    /// BOT-3.
    #[test]
    fn the_bots_are_drawn_in_the_order_the_module_returned_them() {
        let mut session = without_a_session(three_bots(), 120);
        let buffer = drawn(&mut session, 120, 40);

        let order: Vec<u16> = ["Zeta", "Alpha", "Middle"]
            .iter()
            .map(|name| row_naming(&buffer, name))
            .collect();
        assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{order:?}");
    }

    /// BOT-17.
    #[test]
    fn control_characters_are_replaced_and_a_seed_is_never_drawn_as_text() {
        let rows = vec![row(
            "esc",
            "Ra\u{1b}[2Jw",
            "does\u{7}things\u{85}",
            "SEEDTEXT\u{1b}[31m",
        )];
        let mut session = without_a_session(rows, 120);
        let buffer = drawn(&mut session, 120, 20);

        let screen = screen_text(&buffer);
        assert!(screen.contains("Ra\u{241b}[2Jw"), "{screen}");
        assert!(screen.contains("does\u{2407}things"), "{screen}");
        assert!(
            screen.chars().all(|c| !c.is_control() || c == '\n'),
            "a control character reached a cell: {screen:?}"
        );
        assert!(!screen.contains("SEEDTEXT"), "{screen}");
        let top = row_naming(&buffer, "Ra");
        assert_face_drawn_at(&buffer, "SEEDTEXT\u{1b}[31m", top, 2);
        let drawn_cells: usize = (top..top + 5)
            .map(|line| {
                (2..12)
                    .filter(|column| !buffer[(*column, line)].symbol().trim().is_empty())
                    .count()
            })
            .sum();
        assert!(drawn_cells > 0, "no face was drawn:\n{screen}");
    }

    /// BOT-17.
    #[test]
    fn no_bots_draws_one_line_saying_how_to_make_one() {
        let mut session = without_a_session(Vec::new(), 120);
        let buffer = drawn(&mut session, 120, 20);
        assert!(
            screen_text(&buffer).contains("No bots yet. Make one in the desktop app"),
            "{}",
            screen_text(&buffer)
        );
    }

    /// BOT-4.
    #[test]
    fn a_file_that_does_not_parse_shows_no_rows_and_names_its_path() {
        let unreadable = Listing {
            unreadable: Some(Problem {
                path: PathBuf::from("/state/bots.json"),
                reason: "expected value at line 1 column 1".to_string(),
            }),
            ..Listing::default()
        };
        let mut session = Session::new("none")
            .in_layout(Layout::Bots)
            .with_bots(state_of(unreadable))
            .with_no_session_begun();
        let buffer = drawn(&mut session, 120, 20);
        let screen = screen_text(&buffer);
        assert!(screen.contains("/state/bots.json"), "{screen}");
        assert!(
            screen.contains("expected value at line 1 column 1"),
            "{screen}"
        );
        assert!(!screen.contains('\u{2580}'), "a face was drawn: {screen}");
    }

    /// BOT-3.
    #[test]
    fn a_row_reads_a_bounded_prefix_of_a_purpose_with_no_length_bound() {
        let huge = "x".repeat(8 * 1024 * 1024);
        let room = 40;
        assert!(
            bots_pane::read_for_a_row(&huge, room).chars().count() <= 2 * room + 64,
            "the whole purpose was read for one row"
        );
        assert_eq!(bots_pane::read_for_a_row("short", room), "short");

        let longest_made_printable = std::cell::Cell::new(0);
        bots_pane::shown_with(&huge, room, |text| {
            longest_made_printable.set(longest_made_printable.get().max(text.chars().count()));
            crate::render::printable(text)
        });
        assert!(
            longest_made_printable.get() <= 2 * room + 64,
            "the whole purpose was made printable for one row"
        );

        let mut with_control = String::from("a\u{1b}[31mb");
        with_control.push_str(&"y".repeat(1024 * 1024));
        let drawn_whole = crate::status::cut(&crate::render::printable(&with_control), room);
        assert_eq!(bots_pane::shown(&with_control, room), drawn_whole);
        assert!(drawn_whole.ends_with('\u{2026}'));
        assert!(drawn_whole.starts_with("a\u{241b}[31mb"));
    }

    /// BOT-3.
    #[test]
    fn a_row_with_a_multi_megabyte_purpose_draws_the_same_row_as_its_first_screenful() {
        let mut long = row("zeta", "Zeta", "", "seed-zeta");
        long.purpose = (0..400_000).map(|i| format!("w{i} ")).collect();
        let mut cut = long.clone();
        cut.purpose.truncate(200);
        let draw = |row: Row| {
            let mut session = Session::new("none")
                .in_layout(Layout::Bots)
                .with_bots(state_of(listing(vec![row])))
                .with_no_session_begun();
            screen_text(&drawn(&mut session, 120, 20))
        };
        assert_eq!(draw(long), draw(cut));
    }

    /// BOT-4.
    #[test]
    fn rows_still_in_the_desktops_file_are_not_called_no_bots() {
        let old_file = PathBuf::from("/data/bravebot-ui.json");
        let waiting = Listing {
            old_rows_waiting: true,
            ..Listing::default()
        };
        for rows in [Vec::new(), three_bots()] {
            let mut session = Session::new("none")
                .in_layout(Layout::Bots)
                .with_bots(
                    state_of(Listing {
                        rows: rows.clone(),
                        ..waiting.clone()
                    })
                    .with_old_file(old_file.clone()),
                )
                .with_no_session_begun();
            let screen = screen_text(&drawn(&mut session, 140, 30));
            assert!(
                !screen.contains("No bots yet"),
                "{} rows: {screen}",
                rows.len()
            );
            assert!(
                screen.contains("earlier version") && screen.contains("/data/bravebot-ui.json"),
                "{} rows: {screen}",
                rows.len()
            );
        }

        let mut settled = Session::new("none")
            .in_layout(Layout::Bots)
            .with_bots(state_of(Listing::default()).with_old_file(old_file))
            .with_no_session_begun();
        let screen = screen_text(&drawn(&mut settled, 140, 30));
        assert!(screen.contains("No bots yet"), "{screen}");
        assert!(!screen.contains("earlier version"), "{screen}");
    }

    /// BOT-4.
    #[test]
    fn an_unreadable_file_is_named_and_the_old_rows_line_is_not_drawn_beside_it() {
        let both = Listing {
            old_rows_waiting: true,
            unreadable: Some(Problem {
                path: PathBuf::from("/state/bots.json"),
                reason: "expected value at line 1 column 1".to_string(),
            }),
            ..Listing::default()
        };
        let mut session = Session::new("none")
            .in_layout(Layout::Bots)
            .with_bots(state_of(both).with_old_file(PathBuf::from("/data/bravebot-ui.json")))
            .with_no_session_begun();
        let screen = screen_text(&drawn(&mut session, 140, 30));
        assert!(screen.contains("/state/bots.json"), "{screen}");
        assert!(!screen.contains("earlier version"), "{screen}");
        assert!(!screen.contains("/data/bravebot-ui.json"), "{screen}");
    }

    /// BOT-3.
    #[test]
    fn dropped_entries_are_a_line_naming_the_file() {
        let dropped = Listing {
            rows: three_bots(),
            dropped: vec![Dropped {
                index: 1,
                slug: Some("bad".to_string()),
                reason: Reason::BadSlug,
            }],
            ..Listing::default()
        };
        let mut session = Session::new("none")
            .in_layout(Layout::Bots)
            .with_bots(state_of(dropped))
            .with_no_session_begun();
        let buffer = drawn(&mut session, 120, 40);
        assert!(
            screen_text(&buffer).contains("entries in /state/bots.json"),
            "{}",
            screen_text(&buffer)
        );

        let mut clean = without_a_session(three_bots(), 120);
        let buffer = drawn(&mut clean, 120, 40);
        assert!(!screen_text(&buffer).contains("/state/bots.json"));
    }

    /// BOT-17.
    #[test]
    fn the_panes_need_100_columns_and_the_prompt_box_is_drawn_at_any_width() {
        for (width, wide) in [(80, false), (99, false), (100, true), (140, true)] {
            let mut session = without_a_session(three_bots(), width);
            let buffer = drawn(&mut session, width, 30);
            let screen = screen_text(&buffer);
            assert_eq!(screen.contains("Zeta"), wide, "at {width}: {screen}");
            assert_eq!(
                screen.contains("The panes need a wider terminal"),
                !wide,
                "at {width}: {screen}"
            );
            assert!(screen.contains("Ask Brave Bot"), "no prompt box at {width}");
            assert_eq!(
                the_laid_pane(&session, Pane::Bots).is_some(),
                wide,
                "at {width}"
            );
            assert_eq!(
                session.bots().focus(),
                if wide { Focus::Panes } else { Focus::Prompt },
                "the phase starts in the list only where it is drawn, at {width}"
            );
        }
    }

    /// BOT-17.
    #[test]
    fn a_session_in_the_bots_layout_draws_the_panes_and_no_info_panel() {
        let mut session = in_a_session(three_bots(), 140);
        let buffer = drawn(&mut session, 140, 30);
        assert!(the_laid_pane(&session, Pane::Bots).is_some());
        assert!(the_laid_pane(&session, Pane::Panel).is_none());
        assert!(screen_text(&buffer).contains("Zeta"));
        let hint = row_text(&buffer, 29);
        assert!(!hint.contains("info"), "the hint offers the panel: {hint}");

        let mut narrow = in_a_session(three_bots(), 99);
        drawn(&mut narrow, 99, 30);
        assert!(the_laid_pane(&narrow, Pane::Bots).is_none());

        let mut default = Session::new("none");
        default.laid.columns = 140;
        let buffer = drawn(&mut default, 140, 30);
        assert!(
            row_text(&buffer, 29).contains("ctrl-x info"),
            "the default layout stopped offering the panel: {}",
            row_text(&buffer, 29)
        );
    }
}

mod selection {
    use super::*;

    fn two_panes() -> (Session, Buffer) {
        let mut session = in_a_session(vec![row("p", "PANEMARK", "purpose", "s")], 120);
        session
            .transcript
            .push(crate::state::Entry::user("TRANSCRIPTMARK one two"));
        drawn(&mut session, 120, 30);
        let buffer = drawn(&mut session, 120, 30);
        (session, buffer)
    }

    /// MOUSE-1.
    #[test]
    fn a_drag_in_one_pane_never_copies_the_other() {
        let (mut session, buffer) = two_panes();
        let transcript = row_naming(&buffer, "TRANSCRIPTMARK");
        swept(&mut session, (transcript, 0), (transcript, 119));
        let selection = session.selection.expect("selection");
        let text = crate::select::text(&buffer, &selection);
        assert!(text.contains("TRANSCRIPTMARK"), "{text:?}");
        assert!(!text.contains("PANEMARK"), "{text:?}");

        let pane = the_laid_pane(&session, Pane::Bots).expect("the pane is recorded");
        let listed = row_naming(&buffer, "PANEMARK");
        swept(&mut session, (listed, pane.right() - 1), (listed, 0));
        let selection = session.selection.expect("selection");
        assert_eq!(selection.pane(), Pane::Bots);
        let text = crate::select::text(&buffer, &selection);
        assert!(text.contains("PANEMARK"), "{text:?}");
        assert!(!text.contains("TRANSCRIPTMARK"), "{text:?}");
    }

    /// MOUSE-3.
    #[test]
    fn a_selection_in_the_pane_is_dropped_when_the_pane_stops_being_drawn() {
        let (mut session, _) = two_panes();
        let pane = the_laid_pane(&session, Pane::Bots).expect("the pane is recorded");
        session.selection = Some(Selection::in_pane(
            pane.top(),
            pane.left(),
            Pane::Bots,
            pane,
        ));
        let mut laid = session.laid.clone();

        session.note_layout(laid.clone());
        assert!(
            session.selection.is_some(),
            "a drawn pane lost its selection"
        );

        laid.panes.retain(|(kind, _)| *kind != Pane::Bots);
        session.note_layout(laid);
        assert!(
            session.selection.is_none(),
            "the selection outlived its pane"
        );
    }
}

mod keys {
    use super::*;

    fn in_the_panes() -> Session {
        let mut session = in_a_session(three_bots(), 120);
        drawn(&mut session, 120, 30);
        assert_eq!(handle_key(&mut session, alt('p')), Action::Redraw);
        assert!(
            session.panes_have_the_keys(),
            "the chord did not focus the panes"
        );
        session
    }

    /// BOT-17.
    #[test]
    fn the_arrows_and_j_and_k_move_through_the_list_and_stop_at_its_ends() {
        let mut session = in_the_panes();
        for (code, expected) in [
            (KeyCode::Down, 1),
            (KeyCode::Char('j'), 2),
            (KeyCode::Down, 2),
            (KeyCode::Up, 1),
            (KeyCode::Char('k'), 0),
            (KeyCode::Char('k'), 0),
        ] {
            assert_eq!(handle_key(&mut session, key(code)), Action::Redraw);
            assert_eq!(selected(&session), expected, "after {code:?}");
        }
        assert_eq!(session.input(), "", "a key reached the box");
        assert!(session.panes_have_the_keys());
    }

    #[test]
    fn the_marker_is_drawn_on_the_selected_bot() {
        let mut session = in_the_panes();
        handle_key(&mut session, key(KeyCode::Down));
        let buffer = drawn(&mut session, 120, 30);
        let marked = (0..30)
            .find(|line| row_text(&buffer, *line).contains('\u{276f}'))
            .expect("a marker");
        assert_eq!(marked, row_naming(&buffer, "Alpha"));
    }

    #[test]
    fn enter_on_a_bot_says_so_and_begins_no_session() {
        let mut session = in_the_panes();
        handle_key(&mut session, key(KeyCode::Down));

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Redraw
        );

        assert_eq!(
            session.bots().message(),
            Some("Starting a session as a bot arrives in a later release.")
        );
        assert_eq!(selected(&session), 1);
        assert!(session.panes_have_the_keys());
        let buffer = drawn(&mut session, 120, 30);
        assert!(screen_text(&buffer).contains("Starting a session as a bot"));
        handle_key(&mut session, key(KeyCode::Down));
        assert_eq!(
            session.bots().message(),
            None,
            "the message outlived the next key"
        );
    }

    #[test]
    fn tab_with_one_pane_drawn_moves_nothing() {
        let mut session = in_the_panes();
        handle_key(&mut session, key(KeyCode::Down));
        assert_eq!(handle_key(&mut session, key(KeyCode::Tab)), Action::Redraw);
        assert!(session.panes_have_the_keys());
        assert_eq!(selected(&session), 1);
    }

    /// BOT-17.
    #[test]
    fn escape_returns_focus_to_the_prompt() {
        let mut session = in_the_panes();
        handle_key(&mut session, key(KeyCode::Esc));
        assert!(!session.panes_have_the_keys());
        assert_eq!(selected(&session), 0);

        type_text(&mut session, "abc");
        assert_eq!(session.input(), "abc");
        handle_key(&mut session, key(KeyCode::Esc));
        assert_eq!(
            session.input(),
            "",
            "Esc on a half-typed line did not discard it"
        );
        assert_eq!(handle_key(&mut session, key(KeyCode::Esc)), Action::Redraw);
        assert!(!session.is_quitting());
    }

    #[test]
    fn the_chord_moves_focus_both_ways_and_nothing_where_no_pane_is_drawn() {
        let mut session = in_the_panes();
        assert_eq!(handle_key(&mut session, alt('p')), Action::Redraw);
        assert!(!session.panes_have_the_keys());

        let mut narrow = in_a_session(three_bots(), 99);
        assert_eq!(handle_key(&mut narrow, alt('p')), Action::None);
        assert!(!narrow.panes_have_the_keys());
        assert_eq!(narrow.input(), "", "the chord was typed");
    }

    #[test]
    fn narrowing_the_terminal_returns_focus_to_the_prompt() {
        let mut session = in_the_panes();
        session.laid.columns = 80;
        handle_key(&mut session, key(KeyCode::Char('j')));
        assert_eq!(session.input(), "j");
        assert_eq!(selected(&session), 0);
    }

    /// INPUT-13, INPUT-32.
    #[test]
    fn the_shortcut_list_names_the_panes_chord_in_the_bots_layout() {
        let mut session = Session::new("none").in_layout(Layout::Bots);
        type_text(&mut session, "?");
        let buffer = drawn(&mut session, 160, 40);
        assert!(
            screen_text(&buffer).contains("alt-p"),
            "{}",
            screen_text(&buffer)
        );

        let mut moved = std::collections::BTreeMap::new();
        moved.insert("panes".to_string(), "alt-k".to_string());
        let mut session = Session::new("none").in_layout(Layout::Bots);
        session.adopt_keybindings(&moved);
        type_text(&mut session, "?");
        let screen = screen_text(&drawn(&mut session, 160, 40));
        assert!(
            screen.contains("alt-k") && !screen.contains("alt-p"),
            "{screen}"
        );
    }

    /// INPUT-13.
    #[test]
    fn the_shortcut_list_of_the_default_layout_does_not_name_the_panes_chord() {
        let mut session = Session::new("none");
        type_text(&mut session, "?");
        let screen = screen_text(&drawn(&mut session, 160, 40));
        assert!(
            !screen.contains("alt-p") && !screen.contains("prompt and panes"),
            "{screen}"
        );
    }

    #[test]
    fn in_the_default_layout_the_chord_types_its_letter_as_before() {
        let mut session = Session::new("none");
        assert_eq!(handle_key(&mut session, alt('p')), Action::Redraw);
        assert_eq!(session.input(), "p");
        assert!(session.bindings().claims(&alt('p')));
        assert_eq!(Layout::Default.claims_key(&mut session, alt('p')), None);
    }

    /// BOT-17, INPUT-4.
    #[test]
    fn ctrl_c_in_the_panes_follows_the_ladder() {
        let mut session = in_the_panes();
        assert_eq!(handle_key(&mut session, ctrl('c')), Action::Redraw);
        assert!(session.offered_to_leave);
        assert_eq!(handle_key(&mut session, ctrl('c')), Action::Quit);

        let mut withdrawn = in_the_panes();
        handle_key(&mut withdrawn, ctrl('c'));
        handle_key(&mut withdrawn, key(KeyCode::Down));
        assert!(
            !withdrawn.offered_to_leave,
            "a key in the list left the offer standing"
        );
        assert_ne!(handle_key(&mut withdrawn, ctrl('c')), Action::Quit);
        assert_eq!(handle_key(&mut withdrawn, ctrl('c')), Action::Quit);
    }

    #[test]
    fn the_panel_chord_does_nothing_in_the_bots_layout() {
        for focused in [false, true] {
            let mut session = in_a_session(three_bots(), 140);
            drawn(&mut session, 140, 30);
            if focused {
                handle_key(&mut session, alt('p'));
            }
            handle_key(&mut session, ctrl('c'));
            assert_eq!(handle_key(&mut session, ctrl('x')), Action::Redraw);
            assert!(!session.panel_open());
            assert!(!session.offered_to_leave);
            assert_eq!(session.input(), "");
        }
    }
}

mod no_session {
    use super::*;

    /// INPUT-4.
    #[test]
    fn the_prompt_box_follows_the_ladder_of_the_input_spec() {
        let mut session = without_a_session(three_bots(), 80);
        type_text(&mut session, "half a thought");
        handle_key(&mut session, key(KeyCode::Esc));
        assert_eq!(session.input(), "");
        assert_eq!(handle_key(&mut session, key(KeyCode::Esc)), Action::Redraw);
        assert!(!session.is_quitting());

        type_text(&mut session, "again");
        assert_eq!(handle_key(&mut session, ctrl('c')), Action::Redraw);
        assert_eq!(session.input(), "");
        assert!(session.offered_to_leave);
        assert!(!session.is_quitting());
        assert_eq!(handle_key(&mut session, ctrl('c')), Action::Quit);
        assert!(session.is_quitting());
    }

    /// BOT-17.
    #[test]
    fn a_line_other_than_a_layout_help_or_quitting_is_refused() {
        for line in [
            "fix the build",
            "/status",
            "/clear",
            "/help how",
            "/panel",
            "/files",
            "!ls",
        ] {
            let mut session = without_a_session(three_bots(), 80);
            type_text(&mut session, line);
            let action = handle_key(&mut session, key(KeyCode::Enter));
            assert_eq!(action, Action::Redraw, "{line}");
            assert_eq!(session.status, Status::Idle, "{line}");
            assert_eq!(session.input(), "", "{line}");
            assert!(
                session
                    .bots()
                    .message()
                    .is_some_and(|said| said.contains("Choose a bot first")),
                "{line}: {:?}",
                session.bots().message()
            );
            assert!(session.queued.is_empty(), "{line}");
            assert_eq!(
                session.transcript.len(),
                0,
                "{line} left a transcript entry"
            );
            assert!(!session.panel_open(), "{line}");
        }
    }

    #[test]
    fn layout_quitting_and_the_key_list_are_taken_at_any_width() {
        for width in [80, 120] {
            let mut session = without_a_session(three_bots(), width);
            drawn(&mut session, width, 30);
            session.bots_mut().set_focus(Focus::Prompt);
            type_text(&mut session, "/layout default");
            assert_eq!(
                handle_key(&mut session, key(KeyCode::Enter)),
                Action::Layout(Layout::Default),
                "at {width}"
            );

            let mut leaving = without_a_session(three_bots(), width);
            leaving.bots_mut().set_focus(Focus::Prompt);
            type_text(&mut leaving, "/exit");
            assert_eq!(handle_key(&mut leaving, key(KeyCode::Enter)), Action::Quit);

            let mut help = without_a_session(three_bots(), width);
            help.bots_mut().set_focus(Focus::Prompt);
            type_text(&mut help, "?");
            assert!(help.shortcuts, "the key list did not come up at {width}");
        }
    }

    #[test]
    fn layout_names_the_layouts_that_exist_when_it_is_refused() {
        let mut session = without_a_session(three_bots(), 80);
        type_text(&mut session, "/layout bots");
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Redraw
        );
        assert!(
            session
                .bots()
                .message()
                .is_some_and(|said| said.contains("already open"))
        );

        type_text(&mut session, "/layout nowhere");
        handle_key(&mut session, key(KeyCode::Enter));
        let said = session.bots().message().expect("a refusal");
        assert!(
            said.contains("nowhere") && said.contains("default, bots"),
            "{said}"
        );

        type_text(&mut session, "/layout");
        handle_key(&mut session, key(KeyCode::Enter));
        assert!(
            session
                .bots()
                .message()
                .is_some_and(|said| said.contains("default, bots"))
        );
    }

    type Handler = fn(&mut Session, KeyEvent) -> Action;
    type Prepare = fn(&mut Session);
    type Acted = fn(&Session, &Action) -> bool;

    fn nothing_to_prepare(_: &mut Session) {}

    fn a_delegate_to_watch(session: &mut Session) {
        use bravebot_agent::report::{DelegateId, Delegation};
        let id = DelegateId::nth(1);
        session.delegate_started(Delegation {
            stop: Default::default(),
            id,
            kind: "checker".to_string(),
            task: "run the build".to_string(),
        });
        session.reporting_for(Some(id));
    }

    fn a_command_to_move(session: &mut Session) {
        session.status = Status::Working;
        session.movable(bravebot_core::cancel::Handoff::new());
    }

    fn a_line_to_enhance(session: &mut Session) {
        type_text(session, "write a poem about the sea");
    }

    /// BOT-17.
    #[test]
    fn the_chords_of_actions_that_need_a_session_do_nothing() {
        let shift_tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT);
        let back_tab = KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE);
        let chords: [(&str, KeyEvent, Handler, Prepare, Acted); 8] = [
            (
                "scroller",
                ctrl('o'),
                handle_key,
                nothing_to_prepare,
                |session, _| session.scrolling(),
            ),
            (
                "watch",
                ctrl('l'),
                handle_key,
                a_delegate_to_watch,
                |session, _| session.watching().is_some(),
            ),
            (
                "trail",
                ctrl('t'),
                handle_key,
                nothing_to_prepare,
                |_, action| *action != Action::None,
            ),
            (
                "background",
                ctrl('b'),
                handle_key_while_working,
                a_command_to_move,
                |session, _| !session.can_move_to_background(),
            ),
            (
                "editor",
                ctrl('g'),
                handle_key,
                nothing_to_prepare,
                |_, action| *action == Action::Edit,
            ),
            (
                "enhance",
                ctrl('p'),
                handle_key,
                a_line_to_enhance,
                |_, action| matches!(action, Action::Enhance(..)),
            ),
            (
                "shift-tab",
                shift_tab,
                handle_key,
                nothing_to_prepare,
                |_, action| *action != Action::None,
            ),
            (
                "back-tab",
                back_tab,
                handle_key,
                nothing_to_prepare,
                |_, action| *action != Action::None,
            ),
        ];
        for (name, chord, handler, prepare, acted) in chords {
            let mut with_a_session = in_a_session(three_bots(), 80);
            drawn(&mut with_a_session, 80, 30);
            prepare(&mut with_a_session);
            let action = handler(&mut with_a_session, chord);
            assert!(
                acted(&with_a_session, &action),
                "{name}: the fixture does not make the chord act in a session, so it shows nothing"
            );

            let mut session = without_a_session(three_bots(), 80);
            drawn(&mut session, 80, 30);
            prepare(&mut session);
            let before = session.input().to_string();
            let action = handler(&mut session, chord);
            assert_eq!(action, Action::None, "{name}");
            assert!(!acted(&session, &action), "{name}");
            assert_eq!(session.input(), before, "{name} typed into the box");
        }
    }

    fn keys_that_could_act() -> Vec<KeyEvent> {
        let bindings = Session::new("none").bindings().clone();
        let bound = [
            bindings.background_name(),
            bindings.stash_name(),
            bindings.scroller_name(),
            bindings.editor_name(),
            bindings.history_name(),
            bindings.trail_name(),
            bindings.watch_name(),
            bindings.paste_name(),
            bindings.panel_name(),
            bindings.scope_name(),
            bindings.panes_name(),
            bindings.undo_name(),
            bindings.suspend_name(),
            bindings.enhance_name(),
        ]
        .iter()
        .map(|name| {
            let chord = crate::keybindings::KeyChord::parse(name)
                .unwrap_or_else(|| panic!("{name} is not a chord"));
            KeyEvent::new(chord.code, chord.modifiers)
        })
        .collect::<Vec<_>>();
        let letters = ('a'..='z').flat_map(|letter| {
            [
                KeyModifiers::CONTROL,
                KeyModifiers::ALT,
                KeyModifiers::CONTROL | KeyModifiers::ALT,
            ]
            .map(|modifiers| KeyEvent::new(KeyCode::Char(letter), modifiers))
        });
        let others = [
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT),
            KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('_'), KeyModifiers::CONTROL),
            KeyEvent::new(KeyCode::Char('/'), KeyModifiers::CONTROL),
            KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE),
            KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('!'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('@'), KeyModifiers::NONE),
        ];
        let enters = [
            KeyModifiers::NONE,
            KeyModifiers::SHIFT,
            KeyModifiers::CONTROL,
            KeyModifiers::ALT,
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        ]
        .map(|modifiers| KeyEvent::new(KeyCode::Enter, modifiers));
        bound
            .into_iter()
            .chain(letters)
            .chain(others)
            .chain(enters)
            .collect()
    }

    fn is_an_action_the_screen_can_take(step: &Step) -> bool {
        match step {
            Step::Quit | Step::Layout(_) => true,
            Step::Stay(action) => {
                #[cfg(unix)]
                if matches!(action, Action::Suspend) {
                    return true;
                }
                matches!(
                    action,
                    Action::None | Action::Redraw | Action::Copy | Action::Paste
                )
            }
        }
    }

    /// BOT-17.
    #[test]
    fn no_key_and_no_enter_hands_the_loop_an_action_the_screen_does_not_carry_out() {
        let lines = [
            "",
            "fix the build",
            "!ls",
            "/status",
            "@src",
            "/layout default",
        ];
        let mut tried = 0;
        for width in [80, 120] {
            for line in lines {
                for pressed in keys_that_could_act() {
                    for alone in [true, false] {
                        let mut session = without_a_session(three_bots(), width);
                        drawn(&mut session, width, 30);
                        type_text(&mut session, line);
                        session.key_arrived_alone = alone;
                        let step = bots_screen::step(&mut session, TermEvent::Key(pressed));
                        assert!(
                            is_an_action_the_screen_can_take(&step),
                            "{pressed:?} after {line:?} at {width} columns gave {step:?}"
                        );
                        tried += 1;
                    }
                }
            }
        }
        assert!(tried > 2000, "only {tried} presses were tried");
    }

    /// BOT-17.
    #[test]
    fn a_frame_is_drawn_after_an_event_that_changes_the_screen_and_not_after_a_timeout() {
        assert!(!bots_screen::owes_a_frame(None));

        let mut session = without_a_session(three_bots(), 120);
        drawn(&mut session, 120, 30);
        let mut owed = |event: TermEvent| {
            let step = bots_screen::step(&mut session, event);
            bots_screen::owes_a_frame(Some(&step))
        };
        assert!(owed(typed_key_event(KeyCode::Down, KeyModifiers::NONE)));
        assert!(owed(TermEvent::Resize(100, 30)));
        assert!(!owed(typed_key_event(
            KeyCode::Char('o'),
            KeyModifiers::CONTROL
        )));
        assert!(owed(typed_key_event(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL
        )));

        for action in [Action::Redraw, Action::Copy, Action::Paste] {
            assert!(
                bots_screen::owes_a_frame(Some(&Step::Stay(action))),
                "a copy or a paste changes what is drawn"
            );
        }
    }

    fn without_whitespace(text: &str) -> String {
        text.chars().filter(|c| !c.is_whitespace()).collect()
    }

    fn assert_the_width_line_then_the_message(buffer: &Buffer, message: &str, at: &str) {
        let wanted = without_whitespace(&format!("{}{}", bots_pane::line_about_width(), message));
        let screen = without_whitespace(&screen_text(buffer));
        assert!(
            screen.contains(&wanted),
            "the line about width and the message are not both drawn, in that order, {at}:\n{}",
            screen_text(buffer)
        );
    }

    /// BOT-17.
    #[test]
    fn a_refusal_below_100_columns_is_drawn_under_the_line_about_width_and_the_next_key_clears_it()
    {
        for columns in [80, 55, 40] {
            let mut session = without_a_session(three_bots(), columns);
            drawn(&mut session, columns, 30);
            type_text(&mut session, "/layout nope");
            handle_key(&mut session, key(KeyCode::Enter));
            let at = format!("at {columns} columns");
            let buffer = drawn(&mut session, columns, 30);
            assert_the_width_line_then_the_message(
                &buffer,
                "There is no layout named nope. The layouts are default, bots.",
                &at,
            );

            handle_key(&mut session, key(KeyCode::Char('x')));
            let after = screen_text(&drawn(&mut session, columns, 30));
            assert!(!after.contains("no layout named"), "{at}:\n{after}");
            assert!(
                without_whitespace(&after)
                    .contains(&without_whitespace(&bots_pane::line_about_width())),
                "{at}:\n{after}"
            );
        }
    }

    /// BOT-17.
    #[test]
    fn the_message_of_a_refused_line_goes_with_the_next_key_wherever_the_focus_is() {
        let mut session = without_a_session(three_bots(), 120);
        drawn(&mut session, 120, 30);
        handle_key(&mut session, alt('p'));
        assert_eq!(session.bots().focus(), Focus::Prompt);
        type_text(&mut session, "fix the build");
        handle_key(&mut session, key(KeyCode::Enter));
        let said = screen_text(&drawn(&mut session, 120, 30));
        assert!(said.contains("Choose a bot first"), "{said}");

        handle_key(&mut session, key(KeyCode::Char('a')));
        let after = screen_text(&drawn(&mut session, 120, 30));
        assert!(!after.contains("Choose a bot first"), "{after}");
    }

    /// BOT-17.
    #[test]
    fn a_key_that_takes_a_message_away_in_the_loop_owes_the_frame_that_removes_it() {
        let mut session = without_a_session(three_bots(), 120);
        drawn(&mut session, 120, 30);
        handle_key(&mut session, alt('p'));
        type_text(&mut session, "fix the build");
        handle_key(&mut session, key(KeyCode::Enter));
        assert!(session.bots().message().is_some());
        assert_eq!(
            stepped(&mut session, KeyCode::Char('o'), KeyModifiers::CONTROL),
            Step::Stay(Action::Redraw)
        );
        assert!(session.bots().message().is_none());
    }

    fn a_file_to_drop(name: &str) -> String {
        let directory = crate::testutil::scratch_dir("layout-drops");
        std::fs::create_dir_all(&directory).expect("a directory");
        let file = directory.join(name);
        std::fs::write(&file, "text").expect("a file");
        file.display().to_string()
    }

    /// BOT-17.
    #[test]
    fn a_paste_or_a_drop_while_the_list_has_the_keys_reaches_no_box() {
        let dropped = a_file_to_drop("dropped.txt");
        for text in ["/layout default", dropped.as_str()] {
            let mut session = without_a_session(three_bots(), 120);
            drawn(&mut session, 120, 30);
            assert!(session.panes_have_the_keys());
            assert_eq!(handle_paste(&mut session, text), Action::None);
            assert_eq!(session.input(), "", "{text}");
            assert!(session.attached().is_empty(), "{text}");

            handle_key(&mut session, alt('p'));
            assert!(!session.panes_have_the_keys());
            assert_eq!(handle_paste(&mut session, text), Action::Redraw);
            assert!(
                !session.input().is_empty() || !session.attached().is_empty(),
                "a paste with the prompt focused went nowhere: {text}"
            );
        }
    }

    /// BOT-17.
    #[test]
    fn a_picture_pasted_while_the_list_has_the_keys_names_no_chord() {
        let mut session = without_a_session(three_bots(), 120);
        drawn(&mut session, 120, 30);
        assert_eq!(handle_paste(&mut session, ""), Action::None);
        assert!(session.bots().message().is_none());
        take_from_clipboard(
            &mut session,
            crate::clipboard::Pasted::Text("/layout default".into()),
        );
        assert_eq!(session.input(), "");

        handle_key(&mut session, alt('p'));
        take_from_clipboard(
            &mut session,
            crate::clipboard::Pasted::Text("/layout default".into()),
        );
        assert_eq!(session.input(), "/layout default");
    }

    /// BOT-17, INPUT-4.
    #[test]
    fn ctrl_c_and_ctrl_d_reach_the_ladder_from_the_list() {
        for chord in [ctrl('c'), ctrl('d')] {
            let mut session = without_a_session(three_bots(), 120);
            drawn(&mut session, 120, 30);
            assert!(session.panes_have_the_keys());
            assert_eq!(handle_key(&mut session, chord), Action::Redraw, "{chord:?}");
            assert!(session.offered_to_leave, "{chord:?}");
            assert_eq!(handle_key(&mut session, chord), Action::Quit, "{chord:?}");
        }
    }

    /// BOT-17, INPUT-47.
    #[cfg(unix)]
    #[test]
    fn the_suspend_chord_reaches_its_handler_from_the_list() {
        let mut session = without_a_session(three_bots(), 120);
        drawn(&mut session, 120, 30);
        assert!(session.panes_have_the_keys());
        assert!(session.bindings().is_suspend(&ctrl('z')));
        assert_eq!(handle_key(&mut session, ctrl('z')), Action::Suspend);

        session.key_arrived_alone = false;
        assert_eq!(handle_key(&mut session, ctrl('z')), Action::Redraw);
        assert!(session.bots().message().is_some());

        assert_eq!(handle_key(&mut session, ctrl('r')), Action::Redraw);
        assert!(session.input().is_empty());
    }

    fn ctrl_alt(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL | KeyModifiers::ALT)
    }

    fn with_history_on(chord: &str, focus: Focus) -> Session {
        let mut moved = std::collections::BTreeMap::new();
        moved.insert("history".to_string(), chord.to_string());
        let mut session = without_a_session(three_bots(), 120);
        session.adopt_keybindings(&moved);
        session.history =
            crate::history::History::from_entries(vec![bravebot_session::store::Entry::sent(
                "an earlier prompt",
                None,
            )]);
        drawn(&mut session, 120, 30);
        session.bots_mut().set_focus(focus);
        session
    }

    /// BOT-17, INPUT-32.
    #[test]
    fn a_chord_moved_onto_ctrl_alt_c_or_d_does_not_reach_the_prompt_from_the_list() {
        for (chord, name) in [(ctrl_alt('c'), "ctrl-alt-c"), (ctrl_alt('d'), "ctrl-alt-d")] {
            let mut in_the_prompt = with_history_on(name, Focus::Prompt);
            handle_key(&mut in_the_prompt, chord);
            assert!(
                in_the_prompt.searching_history(),
                "{name}: the fixture does not make the chord act on the prompt"
            );

            let mut in_the_list = with_history_on(name, Focus::Panes);
            assert_eq!(
                handle_key(&mut in_the_list, chord),
                Action::Redraw,
                "{name}"
            );
            assert!(
                !in_the_list.searching_history(),
                "{name} reached the prompt from the list"
            );
            assert!(
                !in_the_list.offered_to_leave,
                "{name} was taken for the ladder"
            );
        }
    }

    fn list_and_prompt_box() -> (Session, Buffer) {
        let mut session = without_a_session(vec![row("p", "LISTMARK", "purpose", "s")], 120);
        session.bots_mut().set_focus(Focus::Prompt);
        type_text(&mut session, "BOXMARK words");
        drawn(&mut session, 120, 30);
        let buffer = drawn(&mut session, 120, 30);
        (session, buffer)
    }

    /// MOUSE-1.
    #[test]
    fn a_drag_in_the_list_never_copies_the_prompt_box_and_the_reverse() {
        let (mut session, buffer) = list_and_prompt_box();
        let listed = row_naming(&buffer, "LISTMARK");
        let boxed = row_naming(&buffer, "BOXMARK");

        swept(&mut session, (listed, 0), (boxed, 119));
        let selection = session.selection.expect("a selection");
        assert_eq!(selection.pane(), Pane::Bots);
        let text = crate::select::text(&buffer, &selection);
        assert!(text.contains("LISTMARK"), "{text:?}");
        assert!(!text.contains("BOXMARK"), "{text:?}");

        swept(&mut session, (boxed, 119), (listed, 0));
        let selection = session.selection.expect("a selection");
        assert_eq!(selection.pane(), Pane::Input);
        let text = crate::select::text(&buffer, &selection);
        assert!(text.contains("BOXMARK"), "{text:?}");
        assert!(!text.contains("LISTMARK"), "{text:?}");
    }

    /// MOUSE-3.
    #[test]
    fn a_selection_in_the_list_is_dropped_when_a_resize_below_100_columns_stops_drawing_it() {
        let (mut session, buffer) = list_and_prompt_box();
        let listed = row_naming(&buffer, "LISTMARK");
        swept(&mut session, (listed, 3), (listed, 20));
        assert!(session.selection.is_some(), "no selection was made");

        drawn(&mut session, 120, 30);
        assert!(
            session.selection.is_some(),
            "a list that is still drawn lost its selection"
        );

        drawn(&mut session, 99, 30);
        assert!(
            the_laid_pane(&session, Pane::Bots).is_none(),
            "the list is still recorded below 100 columns"
        );
        assert!(
            session.selection.is_none(),
            "the selection outlived the list"
        );
    }

    const OFFER: &str = "ctrl-c again to exit  ";

    fn last_row(buffer: &Buffer) -> String {
        row_text(buffer, buffer.area.height - 1)
    }

    /// INPUT-4.
    #[test]
    fn the_first_ctrl_c_says_the_next_one_leaves_at_every_width() {
        for columns in [40, 60, 80, 99, 100, 120, 140] {
            for typed in ["", "half a thought"] {
                let mut session = without_a_session(three_bots(), columns);
                type_text(&mut session, typed);
                let before = drawn(&mut session, columns, 30);
                assert!(
                    !screen_text(&before).contains("ctrl-c again"),
                    "offer drawn before any press at {columns} columns:\n{}",
                    screen_text(&before)
                );

                assert_eq!(handle_key(&mut session, ctrl('c')), Action::Redraw);
                let after = drawn(&mut session, columns, 30);
                assert!(
                    last_row(&after).ends_with(OFFER),
                    "no offer at the right of the hint row at {columns} columns with {typed:?} typed:\n{}",
                    screen_text(&after)
                );
            }
        }
    }

    /// INPUT-4.
    #[test]
    fn a_key_that_takes_the_offer_back_takes_its_line_off_the_screen() {
        let mut session = without_a_session(three_bots(), 100);
        drawn(&mut session, 100, 30);
        assert!(session.panes_have_the_keys());
        handle_key(&mut session, ctrl('c'));
        assert!(session.offered_to_leave);
        handle_key(&mut session, key(KeyCode::Down));
        assert_eq!(session.bots().selected(), 1);
        let buffer = drawn(&mut session, 100, 30);
        assert!(
            !screen_text(&buffer).contains("ctrl-c again"),
            "{}",
            screen_text(&buffer)
        );
    }

    fn offered_then(session: &mut Session, columns: u16, typed: &str) {
        drawn(session, columns, 30);
        if !typed.is_empty() {
            handle_key(session, key(KeyCode::Esc));
        }
        type_text(session, typed);
        assert_eq!(session.input(), typed);
        assert_eq!(handle_key(session, ctrl('c')), Action::Redraw);
        assert!(last_row(&drawn(session, columns, 30)).ends_with(OFFER));
    }

    fn assert_withdrawn_and_offered_again(session: &mut Session, columns: u16, what: &str) {
        let screen = screen_text(&drawn(session, columns, 30));
        assert!(
            !screen.contains("ctrl-c again"),
            "the offer is still drawn after {what} at {columns} columns:\n{screen}"
        );
        assert_eq!(
            handle_key(session, ctrl('c')),
            Action::Redraw,
            "the next Ctrl-C left after {what} at {columns} columns"
        );
        assert!(!session.is_quitting());
        assert!(last_row(&drawn(session, columns, 30)).ends_with(OFFER));
    }

    /// INPUT-4.
    #[test]
    fn the_panel_chord_and_the_chords_that_need_a_session_take_the_offer_back_and_say_so() {
        for columns in [80, 100, 140] {
            for typed in ["", "half a thought"] {
                for chord in [ctrl('x'), ctrl('o')] {
                    let mut session = without_a_session(three_bots(), columns);
                    offered_then(&mut session, columns, typed);
                    assert_eq!(
                        handle_key(&mut session, chord),
                        Action::Redraw,
                        "{chord:?} withdrew the offer and owes a frame"
                    );
                    assert_withdrawn_and_offered_again(
                        &mut session,
                        columns,
                        &format!("{chord:?}"),
                    );
                }
            }
        }
    }

    /// INPUT-4.
    #[test]
    fn the_panes_chord_takes_the_offer_back_and_says_so_where_no_list_is_drawn() {
        for columns in [80, 99] {
            for typed in ["", "half a thought"] {
                let mut session = without_a_session(three_bots(), columns);
                offered_then(&mut session, columns, typed);
                assert_eq!(handle_key(&mut session, alt('p')), Action::Redraw);
                assert_withdrawn_and_offered_again(&mut session, columns, "the panes chord");
            }
        }
    }

    /// INPUT-4.
    #[test]
    fn a_chord_takes_back_the_note_of_a_cleared_line_that_offered_no_exit() {
        for chord in [ctrl('x'), alt('p')] {
            let mut session = without_a_session(three_bots(), 80);
            drawn(&mut session, 80, 30);
            handle_key(&mut session, key(KeyCode::Esc));
            type_text(&mut session, "half a thought");
            session.key_arrived_alone = false;
            assert_eq!(handle_key(&mut session, ctrl('c')), Action::Redraw);
            assert!(session.cleared_by_interrupt && !session.offered_to_leave);
            assert!(last_row(&drawn(&mut session, 80, 30)).ends_with(OFFER));
            assert_eq!(handle_key(&mut session, chord), Action::Redraw, "{chord:?}");
            assert!(
                !last_row(&drawn(&mut session, 80, 30)).contains("ctrl-c again"),
                "{chord:?}"
            );
        }
    }

    /// INPUT-4.
    #[test]
    fn the_panes_chord_takes_the_offer_back_and_says_so_where_the_list_is_drawn() {
        for typed in ["", "half a thought"] {
            let mut session = without_a_session(three_bots(), 120);
            offered_then(&mut session, 120, typed);
            assert_eq!(handle_key(&mut session, alt('p')), Action::Redraw);
            assert_withdrawn_and_offered_again(&mut session, 120, "the panes chord");
        }
    }

    /// INPUT-4.
    #[test]
    fn a_chord_with_no_offer_to_take_back_owes_no_frame() {
        for (columns, chord) in [
            (80, ctrl('x')),
            (80, ctrl('o')),
            (80, alt('p')),
            (120, ctrl('x')),
        ] {
            let mut session = without_a_session(three_bots(), columns);
            drawn(&mut session, columns, 30);
            assert_eq!(handle_key(&mut session, chord), Action::None, "{chord:?}");
        }
    }

    fn stepped(session: &mut Session, code: KeyCode, modifiers: KeyModifiers) -> Step {
        bots_screen::step(session, typed_key_event(code, modifiers))
    }

    /// INPUT-4.
    #[test]
    fn a_chord_that_takes_the_offer_back_in_the_loop_owes_the_frame_that_removes_it() {
        let ctrl_c = (KeyCode::Char('c'), KeyModifiers::CONTROL);
        let chords = [
            (140, (KeyCode::Char('x'), KeyModifiers::CONTROL)),
            (140, (KeyCode::Char('o'), KeyModifiers::CONTROL)),
            (140, (KeyCode::Char('p'), KeyModifiers::ALT)),
            (80, (KeyCode::Char('x'), KeyModifiers::CONTROL)),
            (80, (KeyCode::Char('p'), KeyModifiers::ALT)),
        ];
        for (columns, chord) in chords {
            for typed in ["", "half a thought"] {
                let mut session = without_a_session(three_bots(), columns);
                drawn(&mut session, columns, 30);
                if !typed.is_empty() {
                    handle_key(&mut session, key(KeyCode::Esc));
                    type_text(&mut session, typed);
                }
                let first = stepped(&mut session, ctrl_c.0, ctrl_c.1);
                assert_eq!(first, Step::Stay(Action::Redraw));
                assert!(last_row(&drawn(&mut session, columns, 30)).ends_with(OFFER));

                assert_eq!(
                    stepped(&mut session, chord.0, chord.1),
                    Step::Stay(Action::Redraw),
                    "{chord:?} at {columns} columns with {typed:?} typed took the offer back and owes a frame"
                );
                assert_withdrawn_and_offered_again(&mut session, columns, &format!("{chord:?}"));
            }
        }
    }

    /// INPUT-4.
    #[test]
    fn a_chord_with_no_offer_standing_owes_no_frame_in_the_loop() {
        for (columns, code, modifiers) in [
            (140, KeyCode::Char('x'), KeyModifiers::CONTROL),
            (80, KeyCode::Char('o'), KeyModifiers::CONTROL),
            (80, KeyCode::Char('p'), KeyModifiers::ALT),
        ] {
            let mut session = without_a_session(three_bots(), columns);
            drawn(&mut session, columns, 30);
            assert_eq!(
                stepped(&mut session, code, modifiers),
                Step::Stay(Action::None),
                "{code:?} at {columns} columns"
            );
        }
    }

    /// BOT-17.
    #[test]
    fn the_hint_names_the_panes_chord_only_where_the_panes_are_drawn() {
        for columns in [40, 60, 80, 99] {
            let mut session = without_a_session(three_bots(), columns);
            let hint = last_row(&drawn(&mut session, columns, 30));
            assert!(
                !hint.contains("alt-p") && !hint.contains("moves between"),
                "the hint names a chord that does nothing at {columns} columns: {hint:?}"
            );
            if columns >= 60 {
                assert!(
                    hint.contains("/layout default for a session"),
                    "{columns}: {hint:?}"
                );
            }
        }
        for columns in [100, 140] {
            let mut session = without_a_session(three_bots(), columns);
            let hint = last_row(&drawn(&mut session, columns, 30));
            assert!(
                hint.contains("alt-p moves between the list and the prompt")
                    && hint.contains("/layout default for a session"),
                "{columns}: {hint:?}"
            );
        }
    }

    fn typed_key_event(code: KeyCode, modifiers: KeyModifiers) -> TermEvent {
        TermEvent::Key(KeyEvent::new(code, modifiers))
    }

    #[test]
    fn a_step_quits_changes_layout_or_stays() {
        let mut session = without_a_session(three_bots(), 120);
        drawn(&mut session, 120, 30);
        assert_eq!(
            bots_screen::step(
                &mut session,
                typed_key_event(KeyCode::Down, KeyModifiers::NONE)
            ),
            Step::Stay(Action::Redraw)
        );
        assert_eq!(selected(&session), 1);

        bots_screen::step(
            &mut session,
            typed_key_event(KeyCode::Esc, KeyModifiers::NONE),
        );
        for c in "/layout default".chars() {
            bots_screen::step(
                &mut session,
                typed_key_event(KeyCode::Char(c), KeyModifiers::NONE),
            );
        }
        assert_eq!(
            bots_screen::step(
                &mut session,
                typed_key_event(KeyCode::Enter, KeyModifiers::NONE)
            ),
            Step::Layout(Layout::Default)
        );

        let mut leaving = without_a_session(three_bots(), 120);
        drawn(&mut leaving, 120, 30);
        let first = bots_screen::step(
            &mut leaving,
            typed_key_event(KeyCode::Char('c'), KeyModifiers::CONTROL),
        );
        assert_ne!(first, Step::Quit);
        assert_eq!(
            bots_screen::step(
                &mut leaving,
                typed_key_event(KeyCode::Char('c'), KeyModifiers::CONTROL)
            ),
            Step::Quit
        );
    }
}

mod what_the_phase_offers {
    use super::*;
    use crate::state::Offered;

    const NOT_TAKEN_WITHOUT_A_SESSION: [&str; 20] = [
        "run a shell command",
        "name a file",
        "send",
        "new line",
        "take what is offered",
        "what to ask before acting",
        "clear the line",
        "earlier prompts",
        "scroll the transcript",
        "$EDITOR",
        "watch a delegate",
        "scroller",
        "stash",
        "show what a turn did",
        "paste, pictures too",
        "background a running command",
        "info panel",
        "undo the last edit",
        "more specific",
        "select, copy on release",
    ];

    fn at_the_prompt(columns: u16) -> Session {
        let mut session = without_a_session(three_bots(), columns);
        session.bots_mut().set_focus(Focus::Prompt);
        session
    }

    fn beneath_the_box(session: &mut Session, columns: u16) -> String {
        let buffer = drawn(session, columns, 60);
        let area = the_laid_pane(session, Pane::Offered).expect("an offer is drawn");
        (area.y..area.y + area.height)
            .map(|row| row_text(&buffer, row))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn offered_commands(session: &Session) -> Vec<&'static str> {
        match session.offered() {
            Offered::Slash { commands, skills } => {
                assert!(skills.is_empty(), "skills were offered: {skills:?}");
                let mut names: Vec<_> = commands.iter().map(|command| command.name).collect();
                names.sort_unstable();
                names
            }
            _ => Vec::new(),
        }
    }

    /// BOT-17.
    #[test]
    fn the_key_list_holds_only_what_the_phase_takes() {
        for columns in [60, 80, 99, 100, 140] {
            let mut session = at_the_prompt(columns);
            type_text(&mut session, "?");
            let list = beneath_the_box(&mut session, columns);
            for wanted in ["this list", "ctrl-c", "ctrl-d"] {
                assert!(list.contains(wanted), "{wanted} at {columns}:\n{list}");
            }
            for refused in NOT_TAKEN_WITHOUT_A_SESSION {
                assert!(
                    !list.contains(refused),
                    "{refused:?} is listed at {columns}:\n{list}"
                );
            }
            let chord = session.bindings().panes_name();
            assert_eq!(
                list.contains(&chord),
                bots_pane::drawn(columns),
                "the panes chord at {columns}:\n{list}"
            );
        }
    }

    /// BOT-17.
    #[cfg(unix)]
    #[test]
    fn the_key_list_names_the_suspend_chord_the_phase_takes() {
        for columns in [60, 99, 140] {
            let mut session = at_the_prompt(columns);
            type_text(&mut session, "?");
            let list = beneath_the_box(&mut session, columns);
            let chord = session.bindings().suspend_name();
            assert!(list.contains(&chord), "{chord} at {columns}:\n{list}");
            assert!(
                list.contains("suspend to the shell"),
                "the meaning at {columns}:\n{list}"
            );
        }
    }

    /// BOT-17.
    #[test]
    fn the_slash_menu_holds_only_what_the_phase_takes() {
        for columns in [60, 80, 99, 100, 140] {
            let mut session = at_the_prompt(columns);
            type_text(&mut session, "/");
            session.settle_skills(|| {
                vec![crate::skills::Skill {
                    name: "lint".to_string(),
                    description: "what lint is for".to_string(),
                    argument_hint: None,
                    source: bravebot_agent::skills::Source::Home.into(),
                    person_only: false,
                }]
            });
            assert_eq!(
                offered_commands(&session),
                ["/exit", "/layout"],
                "at {columns}"
            );
            let menu = beneath_the_box(&mut session, columns);
            assert!(
                menu.contains("/layout") && menu.contains("/exit"),
                "{columns}:\n{menu}"
            );
            for refused in ["/panel", "/files", "/status", "/help", "/lint"] {
                assert!(
                    !menu.contains(refused),
                    "{refused} is offered at {columns}:\n{menu}"
                );
            }

            type_text(&mut session, "l");
            assert_eq!(offered_commands(&session), ["/layout"], "at {columns}");
            let mut session = at_the_prompt(columns);
            type_text(&mut session, "/p");
            assert!(
                matches!(session.offered(), Offered::Nothing),
                "{:?}",
                session.offered()
            );
        }
    }

    #[test]
    fn a_session_in_the_default_layout_still_offers_everything() {
        let mut session = Session::new("none");
        type_text(&mut session, "?");
        let list = beneath_the_box_of_a_session(&mut session);
        for listed in NOT_TAKEN_WITHOUT_A_SESSION {
            assert!(list.contains(listed), "{listed:?} is gone:\n{list}");
        }

        let mut session = Session::new("none");
        type_text(&mut session, "/");
        assert_eq!(
            offered_commands(&session).len(),
            crate::command_table::commands().len()
        );
    }

    fn beneath_the_box_of_a_session(session: &mut Session) -> String {
        screen_text(&drawn(session, 200, 60))
    }
}

mod the_title {
    use super::*;

    /// BOT-17.
    #[test]
    fn the_screen_takes_the_name_off_the_title_of_the_session_it_follows() {
        let mut out = Vec::new();
        crate::title::show(&mut out, "y/rename zzfoo").expect("title of the session");
        assert!(String::from_utf8_lossy(&out).contains("bravebot · y/rename zzfoo"));

        let mut out = Vec::new();
        bots_screen::drop_the_ended_sessions_title(&mut out).expect("title of the screen");
        let written = String::from_utf8(out).expect("utf-8");
        assert_eq!(written, "\x1b]0;bravebot\x07");
        assert!(!written.contains("zzfoo"));
    }
}

mod commands_in_the_bots_layout {
    use super::*;

    fn typed_in(session: &mut Session, line: &str) -> Action {
        type_text(session, line);
        handle_key(session, key(KeyCode::Enter))
    }

    fn said(session: &Session) -> String {
        session
            .viewed()
            .iter()
            .map(|entry| entry.text.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// BOT-17.
    #[test]
    fn branch_resume_and_manifest_are_refused() {
        for line in ["/branch", "/branch next", "/resume", "/resume abc"] {
            let mut session = in_a_session(three_bots(), 120);
            assert_eq!(typed_in(&mut session, line), Action::Redraw, "{line}");
            let said = said(&session);
            assert!(
                said.contains("--fork") && said.contains("--resume"),
                "{line}: {said}"
            );
        }
        let mut session = in_a_session(three_bots(), 120);
        assert_eq!(typed_in(&mut session, "/manifest do it"), Action::Redraw);
        assert!(said(&session).contains("/manifest is not available"));

        let mut default = Session::new("none");
        assert_eq!(
            typed_in(&mut default, "/branch next"),
            Action::Branch("next".to_string())
        );
        assert_eq!(
            typed_in(&mut default, "/resume"),
            Action::Resume(String::new())
        );
        assert!(matches!(
            typed_in(&mut default, "/manifest do it"),
            Action::Manifest(..)
        ));
    }

    fn isolated(body: impl FnOnce()) {
        if !crate::test_profile::in_isolated_profile() {
            return;
        }
        body();
    }

    /// BOT-17.
    #[test]
    fn the_panel_commands_and_chord_change_neither_the_flag_nor_the_saved_choice() {
        isolated(|| {
            let mut session = in_a_session(three_bots(), 140).with_stored_history();
            session
                .transcript
                .push(crate::state::Entry::user("something to list"));
            for line in ["/panel", "/files"] {
                assert_eq!(typed_in(&mut session, line), Action::Redraw, "{line}");
                assert!(said(&session).contains("draws no info panel"), "{line}");
            }
            handle_key(&mut session, ctrl('x'));
            assert!(!session.panel_open());
            assert!(!session.files_open());
            assert_eq!(bravebot_session::store::load_panel(), None);

            bravebot_session::store::save_panel(true);
            handle_key(&mut session, ctrl('x'));
            typed_in(&mut session, "/panel");
            assert_eq!(bravebot_session::store::load_panel(), Some(true));
            assert!(!session.panel_open(), "the flag moved");
        });
    }

    /// BOT-17.
    #[test]
    fn files_is_refused_with_its_own_line_where_a_file_was_written() {
        use bravebot_agent::diff::Diff;
        use bravebot_agent::report::{Activity, Written};
        let mut session = in_a_session(three_bots(), 140);
        session
            .transcript
            .push(crate::state::Entry::user("change things"));
        let diff = Diff::compute("one\ntwo\n", "one\nthree\n");
        session.transcript.push(crate::state::Entry::tool(
            Activity::running("Update", "a.rs")
                .of_tool("edit_file")
                .with_changes(diff.condensed(3))
                .with_written(Some(Written {
                    landed: "a.rs".into(),
                    added: diff.added(),
                    removed: diff.removed(),
                    numbers: diff.condensed_numbers(3),
                }))
                .done("done"),
        ));
        assert!(!session.written_files().is_empty());

        assert_eq!(typed_in(&mut session, "/files"), Action::Redraw);

        assert!(
            said(&session).contains("draws no info panel"),
            "{}",
            said(&session)
        );
        assert!(!session.files_open());
    }

    #[test]
    fn the_default_layout_given_the_same_keys_opens_the_panel_and_saves_the_choice() {
        isolated(|| {
            let mut session = Session::new("none").with_stored_history();
            session.laid.columns = 140;
            handle_key(&mut session, ctrl('x'));
            assert!(session.panel_open());
            assert_eq!(bravebot_session::store::load_panel(), Some(true));
        });
    }

    #[test]
    fn the_pane_reads_the_bots_file_through_the_module() {
        isolated(|| {
            let home = bravebot_agent::home::directory().expect("a state directory");
            std::fs::create_dir_all(&home).expect("the state directory");
            let entry = |slug: &str, name: &str| {
                serde_json::json!({
                    "slug": slug,
                    "name": name,
                    "purpose": "does a thing",
                    "avatar": slug,
                    "home": "/home/bots",
                    "created": 1,
                    "updated": 1,
                })
            };
            std::fs::write(
                bravebot_agent::bots::bots_file(&home),
                serde_json::json!({"bots": [entry("zeta", "Zeta"), entry("alpha", "Alpha"), {"slug": "Bad Slug"}]})
                    .to_string(),
            )
            .expect("the bots file");

            let mut session = Session::new("none")
                .in_layout(Layout::Bots)
                .with_bots(bots_pane::State::read())
                .with_no_session_begun();
            let buffer = drawn(&mut session, 120, 30);

            assert!(row_naming(&buffer, "Zeta") < row_naming(&buffer, "Alpha"));
            let unbroken: String = screen_text(&buffer).split_whitespace().collect();
            assert!(
                unbroken.contains("bots.json"),
                "no line about the dropped entry"
            );
        });
    }
}

mod the_layout_command {
    use super::*;

    fn typed_in(session: &mut Session, line: &str) -> Action {
        type_text(session, line);
        handle_key(session, key(KeyCode::Enter))
    }

    fn said(session: &Session) -> String {
        session
            .viewed()
            .iter()
            .map(|entry| entry.text.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn working() -> Session {
        let mut session = Session::new("none");
        assert!(matches!(typed_in(&mut session, "first"), Action::Submit(_)));
        assert_eq!(session.status, Status::Working);
        session
    }

    /// BOT-17.
    #[test]
    fn an_idle_session_ends_into_the_layout_named() {
        let mut session = Session::new("none");
        assert_eq!(
            typed_in(&mut session, "/layout bots"),
            Action::Layout(Layout::Bots)
        );
        assert_eq!(
            typed_in(&mut session, "/layout   bots  "),
            Action::Layout(Layout::Bots)
        );
    }

    /// BOT-17.
    #[test]
    fn it_is_refused_with_a_line_while_a_turn_runs_lines_wait_or_a_loop_goal_or_after_is_pending() {
        let mut running = working();
        assert_eq!(
            super::super::dispatch_command(&mut running, commanded("/layout bots")),
            Action::Redraw
        );
        assert!(
            said(&running).contains("while a turn is running"),
            "{}",
            said(&running)
        );

        let mut queued = working();
        type_text_while_working(&mut queued, "next");
        queued.complete("answered", Vec::new(), 0);
        assert!(!queued.queued.is_empty(), "the fixture queued nothing");
        assert_eq!(
            super::super::dispatch_command(&mut queued, commanded("/layout bots")),
            Action::Redraw
        );
        assert!(
            said(&queued).contains("lines are waiting"),
            "{}",
            said(&queued)
        );

        for (line, label) in [("/goal ship it", "goal"), ("/after 10m do it", "after")] {
            let mut session = Session::new("none");
            assert_eq!(typed_in(&mut session, line), Action::Redraw, "{label}");
            assert_eq!(
                typed_in(&mut session, "/layout bots"),
                Action::Redraw,
                "{label}"
            );
            assert!(
                said(&session).contains("is pending"),
                "{label}: {}",
                said(&session)
            );
        }
        let mut looping = Session::new("none");
        assert!(matches!(
            typed_in(&mut looping, "/loop 5m check the deploy"),
            Action::Submit(_)
        ));
        looping.complete("answered", Vec::new(), 0);
        assert_eq!(typed_in(&mut looping, "/layout bots"), Action::Redraw);
        assert!(said(&looping).contains("is pending"), "{}", said(&looping));

        let idle = Session::new("none");
        let refusal = layout_refusal(&idle, true).expect("incognito refuses");
        assert!(refusal.contains("incognito"), "{refusal}");
        assert_eq!(layout_refusal(&idle, false), None);
    }

    fn type_text_while_working(session: &mut Session, text: &str) {
        for c in text.chars() {
            handle_key_while_working(session, key(KeyCode::Char(c)));
        }
        handle_key_while_working(session, key(KeyCode::Enter));
    }

    /// BOT-17.
    #[test]
    fn typed_mid_turn_it_is_refused_at_once_and_never_waits() {
        let mut session = working();
        type_text_while_working(&mut session, "/layout bots");

        assert!(session.queued.is_empty(), "it waited");
        assert_eq!(session.interjections().take(), None);
        assert!(
            session
                .said_while_working()
                .iter()
                .any(|entry| entry.text.contains("while a turn is running")),
            "nothing said"
        );
        session.complete("answered", Vec::new(), 0);
        assert_eq!(session.take_queued_command(), None);
        assert_eq!(session.interface_layout(), Layout::Default);
    }

    #[test]
    fn the_layout_already_open_is_said_and_ends_nothing() {
        let mut session = Session::new("none");
        assert_eq!(typed_in(&mut session, "/layout default"), Action::Redraw);
        assert!(said(&session).contains("already open"));
    }

    /// BOT-17.
    #[test]
    fn a_session_working_under_an_agent_definition_is_refused_and_ends_nothing() {
        let mut session = Session::new("none");
        session.work_under(crate::state::Addressed {
            name: "rule-reviewer".to_string(),
            model: None,
        });
        let action = typed_in(&mut session, "/layout bots");

        assert_eq!(action, Action::Redraw);
        assert!(
            said(&session).contains("agent definition"),
            "{}",
            said(&session)
        );
        assert!(!session.is_quitting());
        assert_eq!(session.status, Status::Idle);
        assert_eq!(
            session
                .standing_definition()
                .map(|definition| definition.name.as_str()),
            Some("rule-reviewer")
        );
    }

    /// BOT-17.
    #[test]
    fn a_definition_the_agent_setting_chose_does_not_refuse_the_layout() {
        let mut session = Session::new("none");
        session.work_under(crate::state::Addressed {
            name: "rule-reviewer".to_string(),
            model: None,
        });
        session.standing_chosen_by_setting();

        assert_eq!(layout_refusal(&session, false), None);
        assert_eq!(
            typed_in(&mut session, "/layout bots"),
            Action::Layout(Layout::Bots)
        );
    }

    /// CMD-6.
    #[test]
    fn the_command_is_in_the_table_and_spells_the_layouts() {
        let command = commands()
            .into_iter()
            .find(|command| command.name == LAYOUT_COMMAND)
            .expect("/layout is in the table");
        assert_eq!(command.mid_turn, MidTurn::Runs);
        assert_eq!(command.typed_hint(), "[default | bots]");
        assert_eq!(
            completions("/lay")
                .iter()
                .map(|found| found.name)
                .collect::<Vec<_>>(),
            [LAYOUT_COMMAND]
        );
    }

    fn commanded(line: &str) -> crate::state::Commanded {
        crate::state::Commanded {
            line: line.to_string(),
            pasted: Vec::new(),
            attached: Vec::new(),
        }
    }
}

mod modes_over_the_layout {
    use super::*;
    use bravebot_agent::report::{DelegateId, Delegation};

    type Open = fn(&mut Session);

    fn open_picker(session: &mut Session) {
        open_effort_picker(session);
    }

    fn open_scroller(session: &mut Session) {
        session.open_scroller();
    }

    fn open_search(session: &mut Session) {
        session.history =
            crate::history::History::from_entries(vec![bravebot_session::store::Entry::sent(
                "an earlier prompt",
                None,
            )]);
        assert!(session.open_history_search(), "there was nothing to search");
    }

    fn open_delegates(session: &mut Session) {
        let id = DelegateId::nth(1);
        session.delegate_started(Delegation {
            stop: Default::default(),
            id,
            kind: "checker".to_string(),
            task: "run the build".to_string(),
        });
        session.reporting_for(Some(id));
        assert!(session.watch(), "the delegate view did not open");
    }

    fn mode_is_open(name: &str, session: &Session) -> bool {
        match name {
            "picker" => session.picking().is_some(),
            "scroller" => session.scrolling(),
            "search" => session.searching_history(),
            _ => session.watching().is_some(),
        }
    }

    type Handler = fn(&mut Session, KeyEvent) -> Action;

    #[test]
    fn an_open_mode_keeps_its_keys_through_both_handlers() {
        let modes: [(&str, Open); 4] = [
            ("picker", open_picker),
            ("scroller", open_scroller),
            ("search", open_search),
            ("delegates", open_delegates),
        ];
        let handlers: [(&str, Handler, Status); 2] = [
            ("handle_key", handle_key, Status::Idle),
            ("while working", handle_key_while_working, Status::Working),
        ];
        for (mode, open) in modes {
            for (handler_name, handler, status) in handlers {
                let label = format!("{mode} through {handler_name}");
                let mut session = in_a_session(three_bots(), 120);
                drawn(&mut session, 120, 30);
                handle_key(&mut session, alt('p'));
                handle_key(&mut session, key(KeyCode::Down));
                assert_eq!(selected(&session), 1, "{label}");
                session.status = status;
                open(&mut session);
                assert!(mode_is_open(mode, &session), "{label}: not open");

                handler(&mut session, key(KeyCode::Down));
                assert_eq!(selected(&session), 1, "{label}: the layout took Down");
                assert!(
                    mode_is_open(mode, &session),
                    "{label}: Down closed the mode"
                );

                handler(&mut session, key(KeyCode::Esc));
                assert!(
                    !mode_is_open(mode, &session),
                    "{label}: Esc left the mode open"
                );
                assert!(
                    session.panes_have_the_keys(),
                    "{label}: the layout took the Esc"
                );
                assert_eq!(selected(&session), 1, "{label}");

                handler(&mut session, key(KeyCode::Esc));
                assert!(
                    !session.panes_have_the_keys(),
                    "{label}: the pane did not take Esc"
                );
            }
        }
    }

    #[test]
    fn escape_in_the_panes_does_not_stop_a_turn_and_ctrl_c_does() {
        let mut session = in_a_session(three_bots(), 120);
        drawn(&mut session, 120, 30);
        session.status = Status::Working;
        assert!(stops_the_turn(&session, key(KeyCode::Esc)));

        handle_key_while_working(&mut session, alt('p'));
        assert!(session.panes_have_the_keys());
        assert!(!stops_the_turn(&session, key(KeyCode::Esc)));
        assert!(stops_the_turn(&session, ctrl('c')));
    }
}

mod phases {
    use super::*;
    use bravebot_session::sessions::Resumable;

    fn resumable(id: &str) -> Resumable {
        Resumable {
            id: id.to_string(),
            directory: PathBuf::from("/work"),
        }
    }

    fn carried(root: &std::path::Path) -> LayoutChange {
        LayoutChange {
            layout: Layout::Bots,
            workspace: Workspace::new(root).expect("a workspace"),
            sandbox: crate::sandbox_command::Sandboxed::typed(
                bravebot_sandbox::SandboxMode::ReadOnly,
            ),
        }
    }

    fn bots_screen_of(phase: &Phase) -> Option<(Option<&Resumable>, bool)> {
        match phase {
            Phase::NoSession(Screen::Bots { left, carrying }) => {
                Some((left.as_ref(), carrying.is_some()))
            }
            _ => None,
        }
    }

    #[test]
    fn the_bots_layout_starts_with_no_session_and_the_default_begins_one() {
        assert_eq!(
            bots_screen_of(&Phase::starting_with(Start::Layout(Layout::Bots))),
            Some((None, false))
        );
        assert!(matches!(
            Phase::starting_with(Start::Layout(Layout::Default)),
            Phase::Session {
                start: Start::Fresh,
                carrying: None
            }
        ));
    }

    #[test]
    fn a_session_that_chose_the_bots_layout_ends_into_its_screen_with_what_it_left() {
        let root = crate::testutil::scratch_dir("layout-phase");
        std::fs::create_dir_all(&root).expect("a directory");
        let phase = Phase::after_a_session_chose_a_layout(
            Ok(Ended::Left(Some(resumable("1-2")))),
            carried(&root),
        );
        let Phase::NoSession(Screen::Bots {
            left: Some(left),
            carrying: Some(carrying),
        }) = phase
        else {
            panic!("no screen with no session");
        };
        let Carrying { workspace, sandbox } = *carrying;
        assert_eq!(left.id, "1-2");
        assert_eq!(
            workspace.root(),
            Workspace::new(&root).expect("a workspace").root()
        );
        assert_eq!(
            sandbox,
            crate::sandbox_command::Sandboxed::typed(bravebot_sandbox::SandboxMode::ReadOnly)
        );

        let failed = Phase::after_a_session_chose_a_layout(
            Err(io::Error::other("the terminal went away")),
            carried(&root),
        );
        assert!(matches!(failed, Phase::Over(Err(_))));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn leaving_the_screen_begins_a_fresh_session_or_ends_the_interface() {
        assert!(matches!(
            Phase::after_the_bots_screen(
                bots_screen::Exit::Layout(Layout::Default),
                Some(resumable("3-4"))
            ),
            Phase::Session {
                start: Start::Fresh,
                carrying: None
            }
        ));
        let Phase::Over(Ok(Ended::Left(Some(left)))) =
            Phase::after_the_bots_screen(bots_screen::Exit::Quit, Some(resumable("3-4")))
        else {
            panic!("the interface did not end");
        };
        assert_eq!(left.id, "3-4");
        assert!(matches!(
            Phase::after_the_bots_screen(bots_screen::Exit::Quit, None),
            Phase::Over(Ok(Ended::Left(None)))
        ));
        assert!(matches!(
            Phase::after_the_bots_screen(bots_screen::Exit::Failed(io::Error::other("x")), None),
            Phase::Over(Err(_))
        ));
    }
}

mod hooks {
    use super::*;
    use bravebot_agent::hooks::SessionMoments;
    use serde_json::json;

    fn appending_to(moment: &str, log: &std::path::Path) -> serde_json::Value {
        json!({
            "on": moment,
            "run": ["/bin/sh", "-c", "echo ran >> \"$1\"", "sh", log.display().to_string()],
        })
    }

    fn fired(log: &std::path::Path) -> usize {
        std::fs::read_to_string(log).map_or(0, |text| text.lines().count())
    }

    /// HOOK-10.
    #[test]
    fn the_first_session_fires_the_moment_once_and_a_session_begun_by_layout_does_not_fire_it_again()
     {
        let root = crate::testutil::scratch_dir("layout-hooks");
        let _ = std::fs::remove_dir_all(&root);
        let state = root.join("state");
        std::fs::create_dir_all(&state).expect("a state directory");
        let started = root.join("started");
        let ended = root.join("ended");
        std::fs::write(
            state.join("hooks.json"),
            json!({"hooks": [appending_to("session-started", &started), appending_to("session-ended", &ended)]})
                .to_string(),
        )
        .expect("a hooks file");

        let mut moments = SessionMoments::read(Some(&state), &root);
        let mut session = Session::new("none");
        start_the_session(&mut session, &mut moments, |_| Ok(())).expect("the first session");
        assert_eq!(fired(&started), 1);

        start_the_session(&mut session, &mut moments, |_| Ok(())).expect("the fresh session");
        assert_eq!(
            fired(&started),
            1,
            "a session begun by /layout fired the moment again"
        );
        assert_eq!(fired(&ended), 0);
        assert!(moments.ended().is_empty());
        assert_eq!(fired(&ended), 1);
        let _ = std::fs::remove_dir_all(&root);
    }
}

mod leaving_for_a_layout {
    use super::*;
    use bravebot_session::sessions;

    fn isolated(body: impl FnOnce()) {
        if !crate::test_profile::in_isolated_profile() {
            return;
        }
        body();
    }

    fn opening_answers(
        session: &mut Session,
        state: &std::path::Path,
        root: &std::path::Path,
    ) -> Answers {
        let sources = RuleSources {
            state: Some(state.to_path_buf()),
            named: None,
            started: root.to_path_buf(),
            managed: bravebot_config::Managed::default(),
        };
        let settings = sources.settings(root);
        let permissions = rules_from(
            session,
            &settings,
            root,
            sources.grants(root).as_ref(),
            Whence::Asked,
            "the-first-session",
            |_, _| Some(true),
        )
        .expect("nobody left");
        let mut trust = TrustStore::new(root);
        trust.trust(".");
        Answers::opening(
            trust,
            TrustedPrograms::new(),
            Rules {
                sources,
                permissions,
            },
        )
    }

    fn hosts(names: &[&str]) -> Vec<String> {
        names.iter().map(|host| (*host).to_string()).collect()
    }

    fn the_sandbox() -> crate::sandbox_command::Sandboxed {
        crate::sandbox_command::Sandboxed::typed(bravebot_sandbox::SandboxMode::ReadOnly)
    }

    /// BOT-17.
    #[test]
    fn leaving_ends_what_the_session_was_granted_and_carries_what_resume_carries() {
        isolated(|| {
            let base = crate::test_profile::project("bravebot-layout-leave");
            let root = base.join("project");
            let opened = base.join("opened");
            std::fs::create_dir_all(&root).expect("a directory");
            std::fs::create_dir_all(&opened).expect("a directory");
            let mut workspace = Workspace::new(&root).expect("a workspace");
            let handed_on = workspace.clone();
            workspace
                .add_directory(opened.to_str().expect("a utf-8 path"))
                .expect("a directory to open");
            workspace.grant_path_reach(PathBuf::from("/data/in"), true, "reads in".to_string());
            workspace.grant_hosts(&hosts(&["yes.example"]));
            workspace.decline_hosts(&hosts(&["no.example"]));
            assert!(!handed_on.added_directories().is_empty());
            assert!(!handed_on.path_reach().is_empty());
            assert!(!handed_on.granted_hosts().is_empty());

            let mut session = Session::new("none").with_sandbox(the_sandbox());
            session.servers.started = vec!["docs".to_string()];
            let mut answers = opening_answers(&mut session, &root.join("state"), workspace.root());
            let mut stored =
                sessions::Handle::begin(&root, sessions::Front::Terminal, bravebot_stamp::BUILD);
            let mut conversation = Conversation::new();
            let mut scratch = None;
            let settings = bravebot_config::Settings::default();
            let run_settings = RunSettings::from_settings(&settings);
            let mut held = Held {
                session: &mut session,
                conversation: &mut conversation,
                stored: &mut stored,
                answers: &mut answers,
                workspace: &mut workspace,
                scratch: &mut scratch,
                settings: &settings,
                run_settings: &run_settings,
            };
            held.save("a first task");
            let id = held.stored.id().to_string();

            let mut servers = crate::state::Servers::default();
            let (change, ended) = held.leave_for_layout(&mut servers, Layout::Bots);

            for (what, workspace) in [
                ("the one handed on", &handed_on),
                ("the one the next layout carries", &change.workspace),
            ] {
                assert!(
                    workspace.added_directories().is_empty(),
                    "{what} kept a directory opened by name"
                );
                assert!(
                    workspace.path_reach().is_empty(),
                    "{what} kept a path programs were let reach"
                );
                assert!(
                    workspace.granted_hosts().is_empty(),
                    "{what} kept a host that was allowed"
                );
                assert_eq!(
                    workspace.unanswered_hosts(&hosts(&["yes.example", "no.example"])),
                    hosts(&["yes.example", "no.example"]),
                    "{what} kept a host answer"
                );
            }
            assert_eq!(change.layout, Layout::Bots);
            assert_eq!(change.sandbox, the_sandbox());
            assert_eq!(servers.started, vec!["docs".to_string()]);
            assert!(held.session.servers.started.is_empty());
            let Ended::Left(Some(left)) = ended else {
                panic!("the session left nothing to resume: {ended:?}");
            };
            assert_eq!(left.id, id);
        });
    }
}

mod scrolling {
    use super::*;

    const HEIGHT_OF_THREE: u16 = 24;

    fn eight_bots() -> Vec<Row> {
        (1..=8)
            .map(|n| row(&format!("bot{n}"), &format!("Bot{n}"), "a purpose", "seed"))
            .collect()
    }

    fn names_drawn(buffer: &Buffer) -> Vec<usize> {
        (1..=8)
            .filter(|n| screen_text(buffer).contains(&format!("Bot{n}")))
            .collect()
    }

    fn marked_row(buffer: &Buffer) -> u16 {
        (0..buffer.area.height)
            .find(|line| row_text(buffer, *line).contains('\u{276f}'))
            .unwrap_or_else(|| panic!("no marker:\n{}", screen_text(buffer)))
    }

    fn pressed(session: &mut Session, code: KeyCode, height: u16) -> Buffer {
        handle_key(session, key(code));
        drawn(session, 120, height)
    }

    fn at_the_top(height: u16) -> (Session, Buffer) {
        let mut session = without_a_session(eight_bots(), 120);
        let buffer = drawn(&mut session, 120, height);
        assert!(session.panes_have_the_keys());
        (session, buffer)
    }

    fn assert_marked(buffer: &Buffer, bot: usize, window: &[usize], after: &str) {
        assert_eq!(
            marked_row(buffer),
            row_naming(buffer, &format!("Bot{bot}")),
            "{after}: the marker is not on Bot{bot}\n{}",
            screen_text(buffer)
        );
        assert_eq!(
            names_drawn(buffer),
            window,
            "{after}\n{}",
            screen_text(buffer)
        );
    }

    /// BOT-17.
    #[test]
    fn the_window_moves_only_when_the_selection_leaves_it() {
        let (mut session, buffer) = at_the_top(HEIGHT_OF_THREE);
        assert_marked(&buffer, 1, &[1, 2, 3], "at the start");
        let first_row = marked_row(&buffer);

        for (n, window) in [(2, [1, 2, 3]), (3, [1, 2, 3]), (4, [2, 3, 4])] {
            let buffer = pressed(&mut session, KeyCode::Char('j'), HEIGHT_OF_THREE);
            assert_marked(&buffer, n, &window, &format!("down to Bot{n}"));
        }
        let at_the_bottom = marked_row(&pressed(&mut session, KeyCode::Char('k'), HEIGHT_OF_THREE));
        assert!(at_the_bottom > first_row);

        let buffer = drawn(&mut session, 120, HEIGHT_OF_THREE);
        assert_marked(&buffer, 3, &[2, 3, 4], "up one from the bottom");
        assert!(
            marked_row(&buffer) < row_naming(&buffer, "Bot4"),
            "the marker did not move up the screen"
        );

        let buffer = pressed(&mut session, KeyCode::Up, HEIGHT_OF_THREE);
        assert_marked(&buffer, 2, &[2, 3, 4], "up to the top of the window");

        let buffer = pressed(&mut session, KeyCode::Up, HEIGHT_OF_THREE);
        assert_marked(&buffer, 1, &[1, 2, 3], "up out of the window");
        assert_eq!(marked_row(&buffer), first_row);
    }

    /// BOT-17.
    #[test]
    fn the_last_page_keeps_the_marker_moving_up_the_screen() {
        let (mut session, _) = at_the_top(HEIGHT_OF_THREE);
        let mut buffer = drawn(&mut session, 120, HEIGHT_OF_THREE);
        for _ in 0..10 {
            buffer = pressed(&mut session, KeyCode::Char('j'), HEIGHT_OF_THREE);
        }
        assert_marked(&buffer, 8, &[6, 7, 8], "at the last bot");

        let taller = drawn(&mut session, 120, HEIGHT_OF_THREE + 5);
        assert_marked(
            &taller,
            8,
            &[5, 6, 7, 8],
            "a window one bot taller at the end",
        );
        let back = drawn(&mut session, 120, HEIGHT_OF_THREE);
        assert_marked(&back, 8, &[6, 7, 8], "the window back at three bots");

        let buffer = pressed(&mut session, KeyCode::Char('k'), HEIGHT_OF_THREE);
        assert_marked(&buffer, 7, &[6, 7, 8], "up one from the last bot");
        let buffer = pressed(&mut session, KeyCode::Char('k'), HEIGHT_OF_THREE);
        assert_marked(&buffer, 6, &[6, 7, 8], "up two from the last bot");
        let buffer = pressed(&mut session, KeyCode::Char('k'), HEIGHT_OF_THREE);
        assert_marked(&buffer, 5, &[5, 6, 7], "up out of the last page");
    }

    /// BOT-17.
    #[test]
    fn a_window_that_changes_height_keeps_the_selection_in_view() {
        let (mut session, _) = at_the_top(HEIGHT_OF_THREE);
        drawn(&mut session, 120, HEIGHT_OF_THREE);
        for _ in 0..3 {
            handle_key(&mut session, key(KeyCode::Char('j')));
        }
        let buffer = drawn(&mut session, 120, HEIGHT_OF_THREE);
        assert_marked(&buffer, 4, &[2, 3, 4], "before the resize");

        let taller = drawn(&mut session, 120, HEIGHT_OF_THREE + 5);
        assert_marked(&taller, 4, &[2, 3, 4, 5], "a window one bot taller");

        let shorter = drawn(&mut session, 120, HEIGHT_OF_THREE - 5);
        assert_marked(&shorter, 4, &[3, 4], "a window one bot shorter");

        let buffer = pressed(&mut session, KeyCode::Char('k'), HEIGHT_OF_THREE - 5);
        assert_marked(&buffer, 3, &[3, 4], "up after the window shrank");
    }
}

mod short_terminals {
    use super::*;

    fn with_two_notes() -> Session {
        let dropped = Listing {
            rows: vec![row("zeta", "Zeta", "answers questions", "seed-zeta")],
            dropped: vec![Dropped {
                index: 1,
                slug: Some("bad".to_string()),
                reason: Reason::BadSlug,
            }],
            ..Listing::default()
        };
        let mut session = Session::new("none")
            .in_layout(Layout::Bots)
            .with_bots(state_of(dropped))
            .with_no_session_begun();
        session.start_in_the_list_where_it_is_drawn(100);
        session
            .bots_mut()
            .say("A message of the screen".to_string());
        session
    }

    fn faces_rows(buffer: &Buffer, name_row: u16) -> usize {
        (name_row..buffer.area.height)
            .take_while(|line| {
                let text = row_text(buffer, *line);
                text.contains('\u{2580}') || text.contains('\u{2584}') || text.contains("Zeta")
            })
            .count()
    }

    /// BOT-17, BOT-18.
    #[test]
    fn the_list_is_drawn_before_the_notes_under_it_where_both_cannot_fit() {
        for height in [5, 9, 11] {
            let mut session = with_two_notes();
            let buffer = drawn(&mut session, 100, height);
            let screen = screen_text(&buffer);
            assert!(
                screen.contains("Zeta") && screen.contains('\u{276f}'),
                "the selected bot is not drawn at {height} rows:\n{screen}"
            );
            assert!(
                !screen.contains("A message of the screen") && !screen.contains("entries in"),
                "the notes took the rows the list needs at {height} rows:\n{screen}"
            );
        }

        let mut session = with_two_notes();
        let buffer = drawn(&mut session, 100, 9);
        assert_eq!(
            faces_rows(&buffer, row_naming(&buffer, "Zeta")),
            5,
            "{}",
            screen_text(&buffer)
        );

        let mut session = with_two_notes();
        let buffer = drawn(&mut session, 100, 12);
        let screen = screen_text(&buffer);
        assert!(screen.contains("A message of the screen"), "{screen}");
        assert!(screen.contains("entries in"), "{screen}");
        assert_eq!(
            faces_rows(&buffer, row_naming(&buffer, "Zeta")),
            5,
            "{screen}"
        );
    }
}

mod ladder_of_the_prompt_box {
    use super::*;
    use ratatui::crossterm::event::KeyEventKind;

    fn box_without_a_session() -> Session {
        let session = without_a_session(three_bots(), 80);
        assert!(!session.panes_have_the_keys());
        session
    }

    fn typed(line: &str) -> Session {
        let mut session = box_without_a_session();
        type_text(&mut session, line);
        session
    }

    /// INPUT-4.
    #[test]
    fn escape_clears_a_typed_line_without_quitting() {
        let mut session = typed("half a thought");
        assert_eq!(handle_key(&mut session, key(KeyCode::Esc)), Action::Redraw);
        assert_eq!(session.input(), "");
        assert!(!session.is_quitting());
    }

    /// INPUT-4.
    #[test]
    fn escape_on_an_empty_line_does_nothing_and_twice_clears_and_stays() {
        let mut session = box_without_a_session();
        assert_eq!(handle_key(&mut session, key(KeyCode::Esc)), Action::Redraw);
        assert!(!session.is_quitting());

        type_text(&mut session, "x");
        assert_eq!(handle_key(&mut session, key(KeyCode::Esc)), Action::Redraw);
        assert_eq!(handle_key(&mut session, key(KeyCode::Esc)), Action::Redraw);
        assert!(!session.is_quitting());
        assert!(!session.offered_to_leave);
    }

    /// INPUT-4.
    #[test]
    fn ctrl_c_on_a_line_takes_it_and_offers_and_the_second_press_leaves() {
        let mut session = typed("half a thought");
        assert_eq!(handle_key(&mut session, ctrl('c')), Action::Redraw);
        assert_eq!(session.input(), "");
        assert!(session.cleared_by_interrupt);
        assert!(session.offered_to_leave);
        assert!(!session.is_quitting());

        assert_eq!(handle_key(&mut session, ctrl('c')), Action::Quit);
        assert!(session.is_quitting());
    }

    /// INPUT-4.
    #[test]
    fn ctrl_c_on_an_empty_box_offers_without_claiming_a_line_went() {
        let mut session = box_without_a_session();
        assert_eq!(handle_key(&mut session, ctrl('c')), Action::Redraw);
        assert!(!session.is_quitting(), "one press ended the interface");
        assert!(session.offered_to_leave);
        assert!(!session.cleared_by_interrupt);
        assert_eq!(handle_key(&mut session, ctrl('c')), Action::Quit);
    }

    /// INPUT-4.
    #[test]
    fn a_key_between_the_two_presses_withdraws_the_offer() {
        let mut session = typed("x");
        handle_key(&mut session, ctrl('c'));
        assert!(session.cleared_by_interrupt);
        handle_key(&mut session, key(KeyCode::Char('y')));
        assert!(
            !session.cleared_by_interrupt,
            "the hint outstayed its press"
        );
        assert!(!session.offered_to_leave);
        assert_ne!(handle_key(&mut session, ctrl('c')), Action::Quit);
        assert_eq!(handle_key(&mut session, ctrl('c')), Action::Quit);
    }

    /// INPUT-4.
    #[test]
    fn input_that_is_not_a_key_withdraws_the_offer() {
        for taken in [
            TermEvent::Mouse(mouse(MouseEventKind::ScrollUp, 1, 1)),
            TermEvent::Mouse(mouse(MouseEventKind::Moved, 1, 1)),
            TermEvent::Resize(80, 24),
            TermEvent::FocusGained,
        ] {
            let mut session = box_without_a_session();
            handle_key(&mut session, ctrl('c'));
            assert!(session.offered_to_leave, "the offer was never made");

            bots_screen::step(&mut session, taken.clone());
            assert!(
                !session.offered_to_leave,
                "{taken:?} left the offer standing"
            );
        }
    }

    /// INPUT-4.
    #[test]
    fn two_interrupts_in_one_read_do_not_leave_and_arm_nothing() {
        for typed_first in [false, true] {
            let mut session = box_without_a_session();
            if typed_first {
                type_text(&mut session, "hello");
            }
            session.key_arrived_alone = false;
            assert_ne!(handle_key(&mut session, ctrl('c')), Action::Quit);
            session.key_arrived_alone = false;
            assert_ne!(handle_key(&mut session, ctrl('c')), Action::Quit);
            assert!(!session.is_quitting());
            assert!(!session.offered_to_leave, "a run armed the offer");
        }
    }

    /// INPUT-4.
    #[test]
    fn two_end_of_transmissions_in_one_read_do_not_leave() {
        let mut session = box_without_a_session();
        session.key_arrived_alone = false;
        assert_ne!(handle_key(&mut session, ctrl('d')), Action::Quit);
        assert_ne!(handle_key(&mut session, ctrl('d')), Action::Quit);
        assert!(!session.is_quitting());
    }

    /// INPUT-4.
    #[test]
    fn a_refused_way_out_says_so_in_the_message_of_the_screen() {
        for pressed in [ctrl('c'), ctrl('d')] {
            let mut session = box_without_a_session();
            session.key_arrived_alone = false;
            assert_eq!(handle_key(&mut session, pressed), Action::Redraw);
            assert!(
                session
                    .bots()
                    .message()
                    .is_some_and(|said| said.contains("press it again to leave")),
                "{pressed:?} was refused in silence: {:?}",
                session.bots().message()
            );
        }
    }

    /// INPUT-4.
    #[test]
    fn ctrl_d_leaves_only_from_an_empty_box_and_on_the_second_press() {
        let mut session = typed("x");
        assert_ne!(handle_key(&mut session, ctrl('d')), Action::Quit);
        assert!(!session.is_quitting());
        assert_eq!(session.input(), "x");

        handle_key(&mut session, key(KeyCode::Backspace));
        assert_eq!(handle_key(&mut session, ctrl('d')), Action::Redraw);
        assert!(!session.is_quitting());
        assert_eq!(handle_key(&mut session, ctrl('d')), Action::Quit);
    }

    /// INPUT-4.
    #[test]
    fn a_press_on_its_own_after_a_run_still_leaves() {
        let mut session = box_without_a_session();
        session.key_arrived_alone = false;
        handle_key(&mut session, ctrl('c'));
        session.key_arrived_alone = true;
        assert_eq!(handle_key(&mut session, ctrl('c')), Action::Redraw);
        assert_eq!(handle_key(&mut session, ctrl('c')), Action::Quit);
    }

    /// INPUT-4.
    #[test]
    fn letting_go_of_the_keys_in_either_order_still_leaves() {
        for released in [
            KeyEvent::new_with_kind(
                KeyCode::Char('c'),
                KeyModifiers::NONE,
                KeyEventKind::Release,
            ),
            KeyEvent::new_with_kind(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL,
                KeyEventKind::Release,
            ),
        ] {
            let mut session = box_without_a_session();
            bots_screen::step(&mut session, TermEvent::Key(ctrl('c')));
            assert!(session.offered_to_leave, "the offer was never made");

            bots_screen::step(&mut session, TermEvent::Key(released));
            assert!(session.offered_to_leave, "{released:?} withdrew the offer");

            assert_eq!(
                bots_screen::step(&mut session, TermEvent::Key(ctrl('c'))),
                Step::Quit,
                "the second press did not leave, after {released:?}"
            );
        }
    }
}
