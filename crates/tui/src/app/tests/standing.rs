//! MOUSE-50: in which states each kind of target of the frame stands.
//!
//! The frame is drawn once and the state of the session moves on before the next, as when a key opens
//! the scroller, a picker or a delegate's view. Each row of the table is a target drawn in a frame,
//! and each column a state the session is put in afterwards with no frame drawn, so the record is the
//! one the frame before wrote. A cell of the table says whether the target is still found there.

use super::popups::targets;
use super::*;
use crate::state::{Hit, Panel, Popup, Status};

/// The states the session is put in after the frame is drawn.
#[derive(Debug, Clone, Copy)]
enum State {
    /// As the fixture built it.
    Rest,
    /// A turn is running.
    Turn,
    /// A command the person typed is running.
    Command,
    /// The scroller is open.
    Scroller,
    /// The scroller's list of keys is up.
    KeyList,
    /// A picker is open over the frame.
    Picker,
    /// The search over the prompts already sent is open.
    Search,
    /// The files state of the info panel is open.
    Files,
    /// The view of a delegate is open.
    DelegateView,
    /// The Sessions screen is open.
    SessionsScreen,
}

const STATES: [State; 10] = [
    State::Rest,
    State::Turn,
    State::Command,
    State::Scroller,
    State::KeyList,
    State::Picker,
    State::Search,
    State::Files,
    State::DelegateView,
    State::SessionsScreen,
];

/// Put the session in `state`, from whatever state it is in, without drawing a frame.
fn enter(session: &mut Session, state: State) {
    match state {
        State::Rest => {}
        State::Turn => session.status = Status::Working,
        State::Command => session.status = Status::Running,
        State::Scroller => {
            if !session.scrolling() {
                session.open_scroller();
            }
        }
        State::KeyList => {
            enter(session, State::Scroller);
            if session.scroller().is_some_and(|scroller| !scroller.help) {
                session.toggle_scroller_help();
            }
        }
        State::Picker => {
            if session.picking().is_none() {
                session.open_picking(crate::picking::Picking::Effort(
                    crate::effort_prompt::Picker::new(None),
                ));
            }
        }
        State::Search => {
            if !session.searching_history() {
                session.history = crate::history::History::from_entries(vec![
                    bravebot_session::store::Entry::sent("an earlier prompt", None),
                ]);
                assert!(session.open_history_search(), "no search opened");
            }
        }
        State::Files => enter_files(session),
        State::SessionsScreen => {
            if !session.showing_sessions() {
                session.open_sessions_screen(crate::sessions_screen::SessionsScreen::new(
                    bravebot_session::listing::Grouping::Together,
                    Vec::new(),
                    std::collections::HashMap::new(),
                    None,
                ));
            }
        }
        State::DelegateView => {
            use bravebot_agent::report::{DelegateId, Delegation};
            if session.watching_a_delegate() {
                return;
            }
            if session.delegates().is_empty() {
                session.delegate_started(Delegation {
                    stop: Default::default(),
                    id: DelegateId::nth(1),
                    kind: "reader".to_string(),
                    task: "find the parser".to_string(),
                });
            }
            assert!(session.watch(), "nothing to watch");
            // With several delegates the key opens the list of them, and Enter opens the first.
            if session.listing_delegates() {
                handle_key(session, key(KeyCode::Enter));
            }
            assert!(session.watching_a_delegate(), "no delegate's view opened");
        }
    }
}

/// Open the files state of the info panel over a session, after a write it lists.
pub(super) fn enter_files(session: &mut Session) {
    if session.files_open() {
        return;
    }
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
    // The state opens only where the panel fits, which the width of the frame says.
    let columns = session.laid.columns;
    session.laid.columns = columns.max(120);
    session.toggle_files();
    session.laid.columns = columns;
    assert!(session.files_open(), "the files state did not open");
}

/// The kinds of target, one for each of the ways a click is found, as the table names them. The
/// match is exhaustive, so a kind added to the frame fails to compile here until it is given a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Back,
    Reply,
    Block,
    Words,
    Queued,
    Footer,
    Call,
    Delegate,
    Link,
    Panel,
    FilesState,
    Wheel,
    Text,
    Item,
    Scrollbar,
    Popup,
    PopupRows,
    Match,
    SessionsScreen,
}

const KINDS: [Kind; 19] = [
    Kind::Back,
    Kind::Reply,
    Kind::Block,
    Kind::Words,
    Kind::Queued,
    Kind::Footer,
    Kind::Call,
    Kind::Delegate,
    Kind::Link,
    Kind::Panel,
    Kind::FilesState,
    Kind::Wheel,
    Kind::Text,
    Kind::Item,
    Kind::Scrollbar,
    Kind::Popup,
    Kind::PopupRows,
    Kind::Match,
    Kind::SessionsScreen,
];

fn kind_of(hit: Hit) -> Kind {
    match hit {
        Hit::Back => Kind::Back,
        Hit::Reply(_) => Kind::Reply,
        Hit::Block(..) => Kind::Block,
        Hit::Words(_) => Kind::Words,
        Hit::Queued(_) => Kind::Queued,
        Hit::Footer(_) => Kind::Footer,
        Hit::Call(_) => Kind::Call,
        Hit::Delegate(_) => Kind::Delegate,
        Hit::Link(_) => Kind::Link,
        Hit::Panel(target) if target.is_in_the_files_state() => Kind::FilesState,
        Hit::Panel(_) => Kind::Panel,
        Hit::Wheel(_) => Kind::Wheel,
        Hit::Text { .. } => Kind::Text,
        Hit::Item(_) => Kind::Item,
        Hit::Scrollbar => Kind::Scrollbar,
        Hit::Popup(_) => Kind::Popup,
        Hit::PopupRows(_) => Kind::PopupRows,
        Hit::Match(_) => Kind::Match,
        Hit::SessionsScreen(_) => Kind::SessionsScreen,
    }
}

/// Whether a target stands in each state, in the order of [`STATES`]: as built, a turn running, a
/// command running, the scroller open, its list of keys up, a picker open, the search open, the files
/// state open, and a delegate's view open.
type Stands = [bool; 10];

/// What is drawn beside the transcript in the plain frame: it stands at rest and while work runs.
/// The scroller and the views of a delegate replace the frame, a panel that holds the input is
/// answered by itself, and the files state draws other rows in place of these.
const A_ROW_OF_THE_PLAIN_FRAME: Stands = [
    true, true, true, false, false, false, false, false, false, false,
];

/// A control of the files state, and a pane of it the wheel scrolls, are drawn by the files state,
/// over the frame the scroller and the views of a delegate replace, and stand in no state the
/// ordinary rows of the panel do.
const A_CONTROL_OF_THE_FILES_STATE: Stands = [
    true, true, true, false, false, false, false, true, false, false,
];

/// A hint is read by no key while a command the person typed is running.
const A_HINT: Stands = [
    true, true, false, false, false, false, false, false, false, false,
];

/// The way out of shell mode is a key of the box at rest, which a turn does not read.
const THE_WAY_OUT_OF_SHELL: Stands = [
    true, false, false, false, false, false, false, false, false, false,
];

/// The transcript's own targets stand in the scroller as well as in the plain frame.
const A_TARGET_OF_THE_TRANSCRIPT: Stands = [
    true, true, true, true, false, false, false, false, false, false,
];

/// A link in the assistant's words is drawn in the transcript wherever that is, so an address stands
/// in the scroller, in the files state and in a delegate's view, and under no panel that holds the
/// input.
const AN_ADDRESS: Stands = [
    true, true, true, true, false, false, false, true, true, false,
];

/// A path is shown and opened where the person is at the plain prompt or in the scroller, which is
/// where nothing else is drawing the screen.
const A_PATH: Stands = [
    true, false, false, true, false, false, false, true, false, false,
];

/// The way back stands in a delegate's view, over the scroller or not.
const THE_WAY_BACK: Stands = [
    true, true, true, true, false, false, false, false, true, false,
];

/// The list under the box is read by no view that replaces the frame.
const THE_LIST_UNDER_THE_BOX: Stands = [
    true, true, true, false, false, false, false, true, false, false,
];

/// The scrollbar is kept beside the transcript by the files state.
const THE_SCROLLBAR: Stands = [
    true, true, true, false, false, false, false, true, false, false,
];

/// A panel that holds the input, and the rows of its list, stand while they are drawn.
const A_PANEL_THAT_HOLDS_THE_INPUT: Stands = [true; 10];

/// A row or a tab of the Sessions screen is the frame while it is open, so it stands for as long
/// as the Sessions screen does, and no part of the plain frame stands under it.
const THE_CENTER: Stands = [true; 10];

/// What each fixture of the table of targets a panel may cover is, and where it stands.
fn expected(name: &str) -> (Kind, Stands) {
    match name {
        "the model row of the info panel"
        | "the effort row of the info panel"
        | "the context reading of the info panel"
        | "a started server's row of the info panel"
        | "the session's name in the info panel"
        | "a language server's row of the info panel"
        | "a task of the plan in the info panel"
        | "the mark of a finished task in the info panel"
        | "the pull request row of the info panel"
        | "the issue row of the info panel"
        | "the hide row of the info panel"
        | "the Files section of the info panel" => (Kind::Panel, A_ROW_OF_THE_PLAIN_FRAME),
        "a file's row of the files state"
        | "the close control of the files state"
        | "the control for the next edit in the files state"
        | "the control for the previous edit in the files state" => {
            (Kind::FilesState, A_CONTROL_OF_THE_FILES_STATE)
        }
        "the show trail hint"
        | "the shortcuts hint"
        | "the info panel hint"
        | "the hint that opens the delegate view"
        | "the hint that opens the view of what commands printed"
        | "the hint that moves a command to the background"
        | "the hint that returns a held view to its end"
        | "the hint that pastes a picture"
        | "the permission mode under the box" => (Kind::Footer, A_HINT),
        "the hint that leaves shell mode" => (Kind::Footer, THE_WAY_OUT_OF_SHELL),
        "a call in the scroller" | "a call at rest" => (Kind::Call, A_TARGET_OF_THE_TRANSCRIPT),
        "a delegate in the scroller" | "a delegate's block at rest" => {
            (Kind::Delegate, A_TARGET_OF_THE_TRANSCRIPT)
        }
        "the scrollbar beside the transcript" => (Kind::Scrollbar, THE_SCROLLBAR),
        "a place in the text of the box" => (Kind::Text, A_ROW_OF_THE_PLAIN_FRAME),
        "the marker that leads a reply" => (Kind::Reply, A_TARGET_OF_THE_TRANSCRIPT),
        "the word that copies a fenced block" => (Kind::Block, A_TARGET_OF_THE_TRANSCRIPT),
        "a prompt waiting behind a turn" => (Kind::Queued, A_ROW_OF_THE_PLAIN_FRAME),
        "the way back of a delegate's view" | "the sentence that closes a delegate's view" => {
            (Kind::Back, THE_WAY_BACK)
        }
        "a web address in a reply" => (Kind::Link, AN_ADDRESS),
        "a file path in a reply" | "a file path in a reply, to the editor" => (Kind::Link, A_PATH),
        other => panic!("no row of the table says where {other:?} stands"),
    }
}

/// The cell of a word in the words of a reply, which is nothing a left click does.
fn a_row_of_the_words_of_a_reply() -> (Session, (u16, u16)) {
    let (session, buffer) =
        super::popups::a_reply_of("The docs say nothing of it, and neither does this answer.");
    let cell = super::popups::shown_at(&buffer, "docs");
    (session, cell)
}

/// The cell of a row of the list of commands under the box, with the list offered.
fn a_row_of_the_list_under_the_box() -> (Session, (u16, u16)) {
    let mut session = typed_into("/c");
    let buffer = drawn(&mut session);
    let (row, column) = find_cell(&buffer, "/clear");
    (session, (row, column))
}

/// The cell of a pane of the files state the wheel scrolls where nothing else is drawn: the first
/// row of the panel for the list, and its last row, which names the keys, for the edits.
fn a_pane_of_the_files_state(edits: bool) -> (Session, (u16, u16)) {
    let session = super::popups::in_the_files_state();
    let panel = session
        .laid
        .panes
        .iter()
        .find(|(pane, _)| *pane == crate::select::Pane::Panel)
        .expect("the panel is recorded")
        .1;
    let row = if edits { panel.bottom() - 1 } else { panel.y };
    (session, (row, panel.x + 3))
}

/// The cell of a row of the Sessions screen, and of one of its tabs, with it open.
fn the_sessions_screen_with(tab: bool) -> (Session, (u16, u16)) {
    use bravebot_session::listing::{Grouping, Page, Status as TaskStatus, Task};
    let mut session = Session::new("kernel-enforced");
    let mut sessions_screen = crate::sessions_screen::SessionsScreen::new(
        Grouping::Together,
        Vec::new(),
        std::collections::HashMap::new(),
        None,
    );
    sessions_screen.replace(Page {
        tasks: vec![Task {
            id: "a".to_string(),
            directory: "/work/one".to_string(),
            title: "a task of the Sessions screen".to_string(),
            status: TaskStatus::Saved,
            held: None,
            updated: 1,
            started: 0,
            model: None,
            branch: None,
            prompt: None,
            turns: 0,
            tokens: 0,
            read: 0,
            written: 0,
            mode: None,
            background: false,
            live: false,
            current: false,
            manifest: false,
            agent: None,
            bytes: 0,
            issue: None,
            pull_request: None,
            handed_off_from: None,
        }],
        more: false,
    });
    session.open_sessions_screen(sessions_screen);
    let buffer = drawn(&mut session);
    let cell = find_cell(
        &buffer,
        if tab {
            "Needs you"
        } else {
            // The start of it, since a long name is cut in the row and whole in the details.
            "a task of the"
        },
    );
    (session, cell)
}

/// The cell of a match in the search over the prompts already sent, with the search open.
fn a_match_of_the_search() -> (Session, (u16, u16)) {
    let mut session = Session::new("none");
    session.history =
        crate::history::History::from_entries(vec![bravebot_session::store::Entry::sent(
            "an earlier prompt",
            None,
        )]);
    assert!(session.open_history_search(), "no search opened");
    let buffer = drawn(&mut session);
    let cell = find_cell(&buffer, "an earlier prompt");
    (session, cell)
}

/// The cell inside a picker a turn in flight opened, over its rows, and the one on its border.
fn a_picker_and_its_rows() -> (Session, (u16, u16), (u16, u16)) {
    let (mut session, _) = panel_with_a_context();
    session.open_picking(crate::picking::Picking::Effort(
        crate::effort_prompt::Picker::new(None),
    ));
    drawn(&mut session);
    let popup = session
        .laid
        .popup(Panel::Picker)
        .expect("the picker is recorded");
    let list = popup.list.expect("the picker lists rows");
    (
        session,
        (list.y + 1, list.x + 2),
        (popup.area.y, popup.area.x + 2),
    )
}

/// The cells of the menu a right click opened, over its first item and on its border.
fn a_menu_and_its_cells() -> (Session, (u16, u16), (u16, u16)) {
    let (mut session, buffer) = super::popups::a_reply_of("The docs say nothing of it.");
    let (row, column) = super::popups::shown_at(&buffer, "docs");
    handle_mouse(
        &mut session,
        drag(MouseEventKind::Down(MouseButton::Right), row, column),
    );
    assert!(session.menu_open(), "no menu opened");
    drawn_again(&mut session);
    let popup = session
        .laid
        .popup(Panel::Menu)
        .expect("the menu is recorded");
    let list = popup.list.expect("the menu lists items");
    (
        session,
        (list.y, list.x + 3),
        (popup.area.y, popup.area.x + 2),
    )
}

/// The session's frame drawn again at the size it was last drawn at.
fn drawn_again(session: &mut Session) -> ratatui::buffer::Buffer {
    let (columns, lines) = (session.laid.columns, session.laid.lines);
    drawn_at(session, columns, lines)
}

/// The cell inside the scroller's list of keys.
fn the_list_of_keys() -> (Session, (u16, u16)) {
    let mut session = scroller_over_one_call();
    session.toggle_scroller_help();
    drawn_at(&mut session, 80, 11);
    let popup = session
        .laid
        .popup(Panel::KeyList)
        .expect("the list of keys is recorded");
    (
        session,
        (
            popup.area.y + popup.area.height / 2,
            popup.area.x + popup.area.width / 2,
        ),
    )
}

/// The first line of the table that is wrong, for the message of the test: a row and a column.
fn wrong_cells(
    name: &str,
    cell: impl Fn() -> (Session, (u16, u16)),
    kind: Kind,
    stands: Stands,
) -> Vec<String> {
    let mut wrong = Vec::new();
    for (state, wanted) in STATES.into_iter().zip(stands) {
        let (mut session, (row, column)) = cell();
        let at_rest = session
            .target_at(row, column)
            .map(|target| kind_of(target.hit));
        if at_rest != Some(kind) {
            wrong.push(format!(
                "{name}: as built the cell is {at_rest:?}, not {kind:?}"
            ));
            continue;
        }
        enter(&mut session, state);
        let found = session
            .target_at(row, column)
            .map(|target| kind_of(target.hit));
        let stands_now = found == Some(kind);
        if stands_now != wanted {
            wrong.push(format!(
                "{name} in {state:?}: found {found:?}, and the table says it {} stand",
                if wanted { "should" } else { "should not" }
            ));
        }
    }
    wrong
}

/// MOUSE-50. Rejects: a target of the frame that stands in a state its click does nothing in, such as
/// a row of the info panel found after the scroller replaced the frame, a hint found while a command
/// runs or a path found while a turn draws the screen, and a target that stops standing where its
/// click works, such as an address in the scroller. Each fixture of the table of targets is asked in
/// each state, and the fixtures of the kinds that table has none for are asked here.
#[test]
fn every_kind_of_target_stands_in_the_states_its_click_works_in_and_in_no_other() {
    let mut wrong = Vec::new();
    let mut kinds_asked = Vec::new();
    for fixture in targets() {
        let (kind, stands) = expected(fixture.name);
        kinds_asked.push(kind);
        wrong.extend(wrong_cells(
            fixture.name,
            || (fixture.build)(),
            kind,
            stands,
        ));
    }

    wrong.extend(wrong_cells(
        "a row of the words of a reply",
        a_row_of_the_words_of_a_reply,
        Kind::Words,
        A_TARGET_OF_THE_TRANSCRIPT,
    ));
    kinds_asked.push(Kind::Words);
    wrong.extend(wrong_cells(
        "a row of the list under the box",
        a_row_of_the_list_under_the_box,
        Kind::Item,
        THE_LIST_UNDER_THE_BOX,
    ));
    kinds_asked.push(Kind::Item);
    for (name, edits) in [
        ("the list pane of the files state", false),
        ("the edits pane of the files state", true),
    ] {
        wrong.extend(wrong_cells(
            name,
            || a_pane_of_the_files_state(edits),
            Kind::Wheel,
            A_CONTROL_OF_THE_FILES_STATE,
        ));
    }
    kinds_asked.push(Kind::Wheel);
    for (name, tab) in [
        ("a row of the Sessions screen", false),
        ("a tab of the Sessions screen", true),
    ] {
        wrong.extend(wrong_cells(
            name,
            || the_sessions_screen_with(tab),
            Kind::SessionsScreen,
            THE_CENTER,
        ));
    }
    kinds_asked.push(Kind::SessionsScreen);
    wrong.extend(wrong_cells(
        "a match of the search over earlier prompts",
        a_match_of_the_search,
        Kind::Match,
        A_PANEL_THAT_HOLDS_THE_INPUT,
    ));
    kinds_asked.push(Kind::Match);
    wrong.extend(wrong_cells(
        "the rows of a picker",
        || {
            let (session, rows, _) = a_picker_and_its_rows();
            (session, rows)
        },
        Kind::PopupRows,
        A_PANEL_THAT_HOLDS_THE_INPUT,
    ));
    kinds_asked.push(Kind::PopupRows);
    wrong.extend(wrong_cells(
        "the border of a picker",
        || {
            let (session, _, border) = a_picker_and_its_rows();
            (session, border)
        },
        Kind::Popup,
        A_PANEL_THAT_HOLDS_THE_INPUT,
    ));
    wrong.extend(wrong_cells(
        "the items of the menu a right click opened",
        || {
            let (session, items, _) = a_menu_and_its_cells();
            (session, items)
        },
        Kind::PopupRows,
        A_PANEL_THAT_HOLDS_THE_INPUT,
    ));
    wrong.extend(wrong_cells(
        "the border of the menu a right click opened",
        || {
            let (session, _, border) = a_menu_and_its_cells();
            (session, border)
        },
        Kind::Popup,
        A_PANEL_THAT_HOLDS_THE_INPUT,
    ));
    wrong.extend(wrong_cells(
        "the scroller's list of keys",
        the_list_of_keys,
        Kind::Popup,
        A_PANEL_THAT_HOLDS_THE_INPUT,
    ));
    kinds_asked.push(Kind::Popup);

    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    for kind in KINDS {
        assert!(
            kinds_asked.contains(&kind),
            "no fixture asks where {kind:?} stands"
        );
    }
}

/// MOUSE-50. Rejects: a panel that holds the input being found only inside the cells it covers, so
/// that a pointer beside a picker is told it is over the frame beneath, and a press beside it then
/// answered by what is there. Every cell of the screen that is not the panel's finds nothing while a
/// picker, the scroller's list of keys or the search is open.
#[test]
fn while_a_panel_holds_the_input_no_cell_beside_it_is_a_target() {
    for state in [State::Picker, State::KeyList, State::Search] {
        let (mut session, _) = a_row_of_the_list_under_the_box();
        session.laid.targets.push(crate::state::Target::on_screen(
            ratatui::layout::Rect::new(0, 0, 120, 40),
            Hit::Queued(0),
        ));
        enter(&mut session, state);
        // The panel the state opened, since one that is not open is not found.
        let panel = match state {
            State::Picker => Panel::Picker,
            State::KeyList => Panel::KeyList,
            _ => Panel::HistorySearch,
        };
        session.laid.targets.extend(
            Popup {
                panel,
                area: ratatui::layout::Rect::new(40, 10, 10, 5),
                list: None,
            }
            .recorded(),
        );
        for row in 0..40 {
            for column in 0..120 {
                let inside = (10..15).contains(&row) && (40..50).contains(&column);
                let found = session.target_at(row, column).map(|target| target.hit);
                assert_eq!(
                    found.is_some_and(Hit::is_a_panel),
                    inside,
                    "{state:?}: ({row}, {column}) found {found:?}"
                );
                assert!(
                    inside || found.is_none(),
                    "{state:?}: ({row}, {column}) found {found:?} beside the panel"
                );
            }
        }
    }
}

/// MOUSE-50. A panel that holds the input is recorded as the cells it covers by the frame that draws
/// it, and stands for as long as it is open. A key closes it before the next frame is drawn, and the
/// cells it covered are not a panel's then. Rejects: a panel that stands because it was recorded,
/// which swallows a press on its old cells as the panel's until the next frame, when the press was
/// meant for what is there. Each panel is found where it is drawn, closed by the key that closes
/// it with no frame drawn, and is no longer found.
#[test]
fn a_panel_a_key_has_closed_is_no_target_before_the_next_frame_is_drawn() {
    let found_as_a_panel = |session: &Session, (row, column): (u16, u16)| {
        session
            .target_at(row, column)
            .is_some_and(|target| target.hit.is_a_panel())
    };

    let (mut session, rows, border) = a_picker_and_its_rows();
    for cell in [rows, border] {
        assert!(found_as_a_panel(&session, cell), "the picker at {cell:?}");
    }
    session.press_in_picking(KeyCode::Esc, KeyModifiers::NONE);
    for cell in [rows, border] {
        assert!(
            !found_as_a_panel(&session, cell),
            "a picker closed by Escape at {cell:?}"
        );
    }

    let (mut session, items, border) = a_menu_and_its_cells();
    for cell in [items, border] {
        assert!(found_as_a_panel(&session, cell), "the menu at {cell:?}");
    }
    session.close_the_menu();
    for cell in [items, border] {
        assert!(
            !found_as_a_panel(&session, cell),
            "a menu closed by Escape at {cell:?}"
        );
    }

    let (mut session, cell) = the_list_of_keys();
    assert!(found_as_a_panel(&session, cell), "the list of keys");
    session.toggle_scroller_help();
    assert!(!found_as_a_panel(&session, cell), "a list of keys put away");

    let (mut session, cell) = a_match_of_the_search();
    assert!(found_as_a_panel(&session, cell), "the search");
    session.close_history_search();
    assert!(
        !found_as_a_panel(&session, cell),
        "a search closed by Escape"
    );
}

/// MOUSE-50. The way back out of a delegate's view stands while a view of a delegate is open and in
/// no state that has none. The table above asks of each target the record the frame wrote while the
/// fixture is built, and the way back is built inside the view, so it never asks the states with no
/// view open. Rejects: the way back standing after the view has closed, which a click on the cells of
/// the label then answers by leaving a view that is not open: the record of the view is asked at rest,
/// while a turn runs, while a command runs and in the scroller, each entered with the view closed.
#[test]
fn the_way_back_stands_only_while_a_delegates_view_is_open() {
    let names = [
        "the way back of a delegate's view",
        "the sentence that closes a delegate's view",
    ];
    for fixture in targets()
        .into_iter()
        .filter(|fixture| names.contains(&fixture.name))
    {
        let (session, (row, column)) = (fixture.build)();
        assert!(
            session
                .target_at(row, column)
                .is_some_and(|target| target.hit == Hit::Back),
            "{}: not found with the view open",
            fixture.name
        );
        for state in [State::Rest, State::Turn, State::Command, State::Scroller] {
            let (mut session, (row, column)) = (fixture.build)();
            assert!(
                session.stop_watching(),
                "{}: no view was open",
                fixture.name
            );
            enter(&mut session, state);
            assert!(
                session
                    .target_at(row, column)
                    .is_none_or(|target| target.hit != Hit::Back),
                "{}: found in {state:?} with the view closed",
                fixture.name
            );
        }
    }
}
