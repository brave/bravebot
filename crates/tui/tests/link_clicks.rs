//! A click with ctrl held on a web address in the assistant's words asks for the browser
//! (MOUSE-40).
//!
//! Asserted through the mouse handler against a frame that was drawn, as a click is: what a click
//! opens is decided by the cells the last frame put on the screen, so the tests draw first and
//! click what they find.

use bravebot_agent::report::{Activity, Reach, Shown};
use bravebot_tui::app::{Action, handle_key, handle_mouse};
use bravebot_tui::hyperlink;
use bravebot_tui::issue_link::Repository;
use bravebot_tui::render;
use bravebot_tui::state::{Entry, Hit, Session};
use bravebot_tui::text_links::{PathRef, Target};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

const GUIDE: &str = "https://example.test/guide?x=1&y=2";

/// The link the last frame recorded at a cell, as a click on the cell finds it: the target of the
/// registry the click is answered from that is drawn over the others and stands now.
fn link_at(session: &Session, row: u16, column: u16) -> Option<&Target> {
    match session.target_at(row, column)?.hit {
        Hit::Link(index) => session.laid.links.get(index),
        _ => None,
    }
}

/// The web address recorded at a cell, if the link there is one.
fn url_at(session: &Session, row: u16, column: u16) -> Option<&str> {
    match link_at(session, row, column)? {
        Target::Address(address) => Some(address),
        Target::Path(_) => None,
    }
}

/// The file path recorded at a cell, if the link there is one.
fn path_at(session: &Session, row: u16, column: u16) -> Option<&PathRef> {
    match link_at(session, row, column)? {
        Target::Path(path) => Some(path),
        Target::Address(_) => None,
    }
}

/// The address a click on a cell of the info panel opens, if the row there is the pull request or
/// the issue.
fn panel_link_at(session: &Session, row: u16, column: u16) -> Option<&str> {
    match session.target_at(row, column)?.hit {
        Hit::Panel(target) => session.panel_link(target),
        _ => None,
    }
}

/// A session whose transcript holds `entries`, drawn at `width` by `height`, with the frame's layout
/// handed back to it as the loop does after every draw.
fn drawn(entries: Vec<Entry>, width: u16, height: u16) -> (Session, Buffer) {
    let mut session = Session::new("kernel-enforced");
    for entry in entries {
        session.transcript.push(entry);
    }
    let buffer = draw(&mut session, width, height);
    (session, buffer)
}

fn draw(session: &mut Session, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    let mut laid = Default::default();
    terminal
        .draw(|frame| laid = render::draw(frame, session))
        .expect("draw");
    session.note_layout(laid);
    terminal.backend().buffer().clone()
}

fn reply(text: &str) -> Entry {
    Entry::assistant(text, Vec::new())
}

fn event(kind: MouseEventKind, row: u16, column: u16, modifiers: KeyModifiers) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers,
    }
}

fn click_with(session: &mut Session, row: u16, column: u16, modifiers: KeyModifiers) -> Action {
    handle_mouse(
        session,
        event(
            MouseEventKind::Down(MouseButton::Left),
            row,
            column,
            modifiers,
        ),
    );
    handle_mouse(
        session,
        event(
            MouseEventKind::Up(MouseButton::Left),
            row,
            column,
            modifiers,
        ),
    )
}

fn ctrl_click(session: &mut Session, row: u16, column: u16) -> Action {
    click_with(session, row, column, KeyModifiers::CONTROL)
}

/// The row and column where `needle` is drawn.
fn find(buffer: &Buffer, needle: &str) -> (u16, u16) {
    (0..buffer.area.height)
        .find_map(|row| {
            let line: String = (0..buffer.area.width)
                .map(|column| hyperlink::shown(buffer[(column, row)].symbol()).to_string())
                .collect();
            // Counted in cells: the lead of a reply is one cell and more than one byte.
            line.find(needle)
                .map(|at| (row, line[..at].chars().count() as u16))
        })
        .unwrap_or_else(|| panic!("{needle:?} is not on the screen"))
}

fn opens(address: &str) -> Action {
    Action::OpenUrl(address.to_string())
}

/// What a person sees: a ctrl-click on the address asks for the browser with that address and
/// nothing else, and what was swept is not left behind.
#[test]
fn a_ctrl_click_on_an_address_in_a_reply_asks_for_the_browser() {
    let (mut session, buffer) = drawn(
        vec![reply(&format!("The docs are at {GUIDE} and then more."))],
        100,
        12,
    );
    let (row, column) = find(&buffer, "https://");

    for offset in [0, 1, 17, GUIDE.len() as u16 - 1] {
        assert_eq!(
            ctrl_click(&mut session, row, column + offset),
            opens(GUIDE),
            "offset {offset}"
        );
        assert!(session.selection.is_none(), "offset {offset}");
    }
}

/// The address ends where the sentence resumes, so the full stop after it and the brackets around
/// it are not in what opens.
#[test]
fn what_opens_is_the_address_and_not_the_sentence_around_it() {
    let (mut session, buffer) = drawn(
        vec![reply(
            "See (https://example.test/a_(b)). Or https://example.test/c, then **https://example.test/d**.",
        )],
        100,
        12,
    );
    for (needle, expected) in [
        ("https://example.test/a_", "https://example.test/a_(b)"),
        ("https://example.test/c", "https://example.test/c"),
        ("https://example.test/d", "https://example.test/d"),
    ] {
        let (row, column) = find(&buffer, needle);
        assert_eq!(ctrl_click(&mut session, row, column + 2), opens(expected));
    }
    let (row, column) = find(&buffer, "https://example.test/c");
    let past = column + "https://example.test/c".len() as u16;
    assert_eq!(
        ctrl_click(&mut session, row, past),
        Action::Copy,
        "the comma after an address opened it"
    );
}

/// A click without ctrl is the selection start it always was, and so is a drag, a double click and
/// a click beside the address: only a single click that swept nothing opens it.
#[test]
fn a_click_without_ctrl_a_drag_or_a_double_click_opens_nothing() {
    let (mut session, buffer) = drawn(vec![reply(&format!("go to {GUIDE} now"))], 100, 12);
    let (row, column) = find(&buffer, "https://");

    assert_eq!(
        click_with(&mut session, row, column + 3, KeyModifiers::NONE),
        Action::Copy
    );
    assert_eq!(
        click_with(&mut session, row, column + 3, KeyModifiers::SHIFT),
        Action::Copy
    );

    // Swept: the press and the release are on different cells.
    handle_mouse(
        &mut session,
        event(
            MouseEventKind::Down(MouseButton::Left),
            row,
            column + 1,
            KeyModifiers::CONTROL,
        ),
    );
    handle_mouse(
        &mut session,
        event(
            MouseEventKind::Drag(MouseButton::Left),
            row,
            column + 6,
            KeyModifiers::CONTROL,
        ),
    );
    let released = handle_mouse(
        &mut session,
        event(
            MouseEventKind::Up(MouseButton::Left),
            row,
            column + 6,
            KeyModifiers::CONTROL,
        ),
    );
    assert_eq!(released, Action::Copy, "a sweep opened the address");

    // A release on another cell than the press began on, with nothing swept between.
    handle_mouse(
        &mut session,
        event(
            MouseEventKind::Down(MouseButton::Left),
            row,
            column + 2,
            KeyModifiers::CONTROL,
        ),
    );
    let released = handle_mouse(
        &mut session,
        event(
            MouseEventKind::Up(MouseButton::Left),
            row,
            column + 8,
            KeyModifiers::CONTROL,
        ),
    );
    assert_eq!(released, Action::Copy, "a press elsewhere opened it");

    // The second press of a double click made with ctrl held opened the address with the first and
    // does nothing (MOUSE-50).
    session.last_click = None;
    let first = ctrl_click(&mut session, row, column + 12);
    assert_eq!(first, opens(GUIDE));
    handle_mouse(
        &mut session,
        event(
            MouseEventKind::Down(MouseButton::Left),
            row,
            column + 12,
            KeyModifiers::CONTROL,
        ),
    );
    let second = handle_mouse(
        &mut session,
        event(
            MouseEventKind::Up(MouseButton::Left),
            row,
            column + 12,
            KeyModifiers::CONTROL,
        ),
    );
    assert_eq!(
        second,
        Action::None,
        "the second click of a double opened it"
    );

    // Beside it.
    session.last_click = None;
    assert_eq!(
        ctrl_click(&mut session, row, column - 1),
        Action::Copy,
        "the blank before the address opened it"
    );
    let past = column + GUIDE.len() as u16;
    assert_eq!(
        ctrl_click(&mut session, row, past),
        Action::Copy,
        "the blank after the address opened it"
    );
}

/// An address the widget wrapped is one address, and a click on any row of it opens the whole.
#[test]
fn an_address_wrapped_over_rows_opens_whole_from_any_of_them() {
    let long = "https://example.test/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/end";
    let (mut session, buffer) = drawn(vec![reply(&format!("read {long} please"))], 30, 12);
    let (first, _) = find(&buffer, "https://");
    let rows: Vec<u16> = (first..first + 4)
        .filter(|&row| (0..30).any(|column| url_at(&session, row, column).is_some()))
        .collect();
    assert!(rows.len() >= 3, "the address was not wrapped: {rows:?}");
    for row in rows {
        let column = (0..30)
            .find(|&column| url_at(&session, row, column).is_some())
            .expect("a cell of the address");
        assert_eq!(
            ctrl_click(&mut session, row, column),
            opens(long),
            "row {row}"
        );
    }
}

/// Only the assistant's words hold a link: the same address in what a person typed, in a note, in
/// a quarantined block, under a call and in a failure is text.
#[test]
fn an_address_outside_the_assistants_words_is_not_a_link() {
    let shown = Shown {
        origin: "notes.md".to_string(),
        reach: Reach::NotThePlanner,
        label: "(U,priv)".to_string(),
        preview: vec![format!("click {GUIDE} now")],
        lines: 1,
    };
    let mut quarantined = Entry::tool(Activity::running("Read", "notes.md").done("1 line"));
    quarantined.shown = Some(shown);
    let (mut session, buffer) = drawn(
        vec![
            Entry::user(format!("my own prompt {GUIDE} here")),
            Entry::system(format!("a note {GUIDE} here")),
            Entry::failure(format!("a failure {GUIDE} here")),
            Entry::output(format!("program output {GUIDE} here")),
            quarantined,
            Entry::tool(Activity::running("Fetch", GUIDE).done("200")),
        ],
        120,
        30,
    );
    let mut checked = 0;
    for row in 0..buffer.area.height {
        let line: String = (0..buffer.area.width)
            .map(|column| buffer[(column, row)].symbol().to_string())
            .collect();
        if let Some(at) = line.find("https://") {
            checked += 1;
            let column = line[..at].chars().count() as u16;
            assert_eq!(
                ctrl_click(&mut session, row, column + 2),
                Action::Copy,
                "{line}"
            );
        }
    }
    assert!(
        checked >= 5,
        "only {checked} of the addresses were on the screen"
    );
}

/// A fenced block is verbatim text for copying, so an address or a path in one is text and not a
/// link, while the same words outside it are.
#[test]
fn an_address_or_a_path_in_a_fenced_block_is_not_a_link() {
    use bravebot_tui::text_links::{PathRef, Target};
    let (session, _) = drawn(
        vec![reply(
            "run it:\n\n```sh\ncurl https://example.test/in src/in.rs\n```\n\nthen https://example.test/out and src/out.rs",
        )],
        100,
        16,
    );
    assert_eq!(
        session.laid.links,
        [
            Target::Address("https://example.test/out".to_string()),
            Target::Path(PathRef {
                path: "src/out.rs".to_string(),
                line: None,
                column: None
            })
        ]
    );
}

/// A table is cut into rows of its own, so an address in a cell is text.
#[test]
fn an_address_in_a_table_cell_is_not_a_link() {
    let (mut session, buffer) = drawn(
        vec![reply(&format!(
            "| name | where |\n| --- | --- |\n| guide | {GUIDE} |\n"
        ))],
        100,
        12,
    );
    let (row, column) = find(&buffer, "https://");
    assert_eq!(ctrl_click(&mut session, row, column + 2), Action::Copy);
}

/// A run that is not a plain web address does not open, whatever it looks like: an address naming a
/// user, one with a character that is not printable ASCII in it, and a scheme that is not a page.
#[test]
fn an_address_the_opener_would_refuse_is_not_a_link() {
    for text in [
        "https://github.com@example.test/login",
        "https://user:secret@example.test/",
        "https://exampl\u{e9}.test/",
        "https://example.test/\u{202e}fdp.exe",
        "https://example.test/\u{200b}",
        "https://evil.test\\.github.com/",
        "https://example.test/a\\b",
        "file:///etc/passwd",
        "javascript:alert(1)",
        "ftp://example.test/",
        "https://",
    ] {
        let (mut session, buffer) = drawn(vec![reply(&format!("open {text} now"))], 100, 12);
        let (row, column) = find(&buffer, text.split("://").next().unwrap());
        for offset in 0..text.chars().count().min(20) as u16 {
            assert_eq!(
                ctrl_click(&mut session, row, column + offset),
                Action::Copy,
                "{text:?} at {offset}"
            );
        }
    }
}

/// The cells of a link cannot be told from the other cells of the line except by being underlined,
/// and the mark the matcher left on them is taken off before the terminal is sent anything.
#[test]
fn an_address_is_underlined_and_no_mark_reaches_the_terminal() {
    use ratatui::style::Modifier;
    let (_, buffer) = drawn(
        vec![reply(&format!("see {GUIDE} and `{GUIDE}` ok"))],
        100,
        12,
    );
    let (row, column) = find(&buffer, "https://");
    for offset in 0..GUIDE.len() as u16 {
        assert!(
            buffer[(column + offset, row)]
                .modifier
                .contains(Modifier::UNDERLINED),
            "offset {offset}"
        );
    }
    assert!(
        !buffer[(column - 1, row)]
            .modifier
            .contains(Modifier::UNDERLINED)
    );
    assert!(
        buffer.content.iter().all(|cell| !cell
            .modifier
            .intersects(Modifier::SLOW_BLINK | Modifier::RAPID_BLINK)),
        "a mark was left on a cell the terminal is sent"
    );
}

/// What a click opens is what the screen shows underlined, and nothing else: for every cell of the
/// screen, a ctrl-click opens an address exactly where the cell is drawn underlined, and the address
/// it opens holds the characters of the underlined run the cell is in. What is expected is read off
/// the frame that was drawn and not out of the record the click is answered from, so a record that
/// disagrees with the screen fails.
#[test]
fn what_a_click_opens_is_what_the_screen_shows_underlined_everywhere() {
    use ratatui::style::Modifier;
    let (mut session, buffer) = drawn(
        vec![
            Entry::user("a prompt"),
            reply(&format!(
                "one {GUIDE} two https://other.test/x.\nthree (https://third.test/y_(z)) four"
            )),
        ],
        60,
        14,
    );
    let underlined = |row: u16, column: u16| {
        buffer[(column, row)]
            .modifier
            .contains(Modifier::UNDERLINED)
    };
    // The characters of the run of underlined cells that `column` is in, on its row.
    let run_at = |row: u16, column: u16| -> String {
        let mut first = column;
        while first > 0 && underlined(row, first - 1) {
            first -= 1;
        }
        let mut last = column;
        while last + 1 < buffer.area.width && underlined(row, last + 1) {
            last += 1;
        }
        (first..=last)
            .map(|at| buffer[(at, row)].symbol().to_string())
            .collect()
    };
    let mut cells_with_an_address = 0;
    for row in 0..buffer.area.height {
        for column in 0..buffer.area.width {
            let shown = underlined(row, column).then(|| run_at(row, column));
            session.last_click = None;
            let acted = ctrl_click(&mut session, row, column);
            match shown {
                Some(run) => {
                    cells_with_an_address += 1;
                    let Action::OpenUrl(address) = acted else {
                        panic!("{row},{column}: an underlined cell opened nothing ({acted:?})");
                    };
                    assert!(
                        address.contains(&run),
                        "{row},{column}: {address:?} is not the address drawn as {run:?}"
                    );
                }
                None => assert!(
                    !matches!(acted, Action::OpenUrl(_)),
                    "{row},{column}: a cell that is not underlined opened an address"
                ),
            }
        }
    }
    assert!(cells_with_an_address > 40, "{cells_with_an_address}");
}

/// A link drawn under a picker or under the scroller's list of keys is hidden, so a click there is
/// the picker's or the list's and opens nothing. A history search has the screen's clicks.
#[test]
fn a_link_that_is_covered_or_whose_screen_is_something_else_opens_nothing() {
    use bravebot_tui::effort_prompt::Picker;
    use bravebot_tui::picking::Picking;

    let text = vec![reply(&format!("go to {GUIDE} now"))];
    let (mut session, buffer) = drawn(text.clone(), 100, 12);
    let (row, column) = find(&buffer, "https://");
    assert_eq!(ctrl_click(&mut session, row, column + 3), opens(GUIDE));

    session.last_click = None;
    session.open_picking(Picking::Effort(Picker::new(None)));
    assert_eq!(url_at(&session, row, column + 3), None);
    assert_ne!(ctrl_click(&mut session, row, column + 3), opens(GUIDE));

    let (mut session, buffer) = drawn(text.clone(), 100, 12);
    let (row, column) = find(&buffer, "https://");
    session.open_scroller();
    let buffer = draw(&mut session, 100, 12);
    let (row, column) = {
        let _ = (row, column);
        find(&buffer, "https://")
    };
    session.last_click = None;
    assert_eq!(ctrl_click(&mut session, row, column + 3), opens(GUIDE));
    handle_key(
        &mut session,
        KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE),
    );
    assert!(session.scroller().is_some_and(|scroller| scroller.help));
    session.last_click = None;
    assert_eq!(url_at(&session, row, column + 3), None);
    assert_ne!(ctrl_click(&mut session, row, column + 3), opens(GUIDE));

    let (mut session, buffer) = drawn(text, 100, 12);
    let (row, column) = find(&buffer, "https://");
    session.history =
        bravebot_tui::history::History::from_entries(vec![bravebot_session::store::Entry::sent(
            "an earlier prompt",
            None,
        )]);
    assert!(session.open_history_search());
    session.last_click = None;
    assert_ne!(ctrl_click(&mut session, row, column + 3), opens(GUIDE));
}

/// In the scroller the transcript is read as it is at rest, so an address opens the same way.
#[test]
fn an_address_in_the_scroller_opens_as_it_does_at_rest() {
    let (mut session, _) = drawn(vec![reply(&format!("go to {GUIDE} now"))], 100, 12);
    session.open_scroller();
    let buffer = draw(&mut session, 100, 12);
    let (row, column) = find(&buffer, "https://");
    assert_eq!(ctrl_click(&mut session, row, column + 5), opens(GUIDE));
}

/// An issue number is linked by an escape sequence whose address is not the text it covers, and a
/// link like that is not read: the address drawn is the only one that opens.
#[test]
fn a_number_linked_to_an_issue_is_not_an_address_to_open() {
    let mut session = Session::new("kernel-enforced");
    session.adopt_repository(Repository::from_remote("git@github.com:brave/bravebot.git"));
    session
        .transcript
        .push(reply("fixed in #2879 and https://example.test/x"));
    let buffer = draw(&mut session, 100, 12);
    let (row, column) = find(&buffer, "#2879");
    assert!(
        buffer[(column, row)].symbol().contains("github.com"),
        "the number was not linked: {:?}",
        buffer[(column, row)].symbol()
    );
    assert_eq!(ctrl_click(&mut session, row, column + 2), Action::Copy);
    assert_eq!(url_at(&session, row, column + 2), None);
    let (row, column) = find(&buffer, "https://example.test/x");
    assert_eq!(
        ctrl_click(&mut session, row, column + 2),
        opens("https://example.test/x")
    );
}

/// The reply still arriving is drawn through the same lines, so its addresses open too.
#[test]
fn an_address_in_a_reply_still_arriving_opens() {
    let mut session = Session::new("kernel-enforced");
    session.streaming(&format!("looking at {GUIDE} for you"));
    let buffer = draw(&mut session, 100, 12);
    let (row, column) = find(&buffer, "https://");
    assert_eq!(ctrl_click(&mut session, row, column + 4), opens(GUIDE));
}

const PULL_REQUEST: &str = "https://github.com/brave/bravebot/pull/3200";
const ISSUE: &str = "https://github.com/brave/bravebot/issues/2685";

/// A session with the info panel open, naming the issue and the pull request it was given.
fn with_a_panel(issue: Option<&str>, pull_request: Option<&str>) -> (Session, Buffer) {
    let mut session = Session::new("kernel-enforced");
    // What the last frame measured, which is what decides whether the panel fits when it is asked
    // for.
    session.laid.columns = 120;
    session.toggle_panel();
    session.identify("a session", "/work/dir".to_string(), Some("main"));
    session.link(issue, pull_request);
    session
        .transcript
        .push(reply(&format!("and the transcript has {GUIDE} too")));
    let buffer = draw(&mut session, 120, 24);
    (session, buffer)
}

/// A click on the pull request row or the issue row opens what the person gave with `/pr` and
/// `/issue`, on the word or on the link, with or without ctrl, and leaves nothing selected.
#[test]
fn a_click_on_the_panels_pull_request_or_issue_row_opens_what_the_person_gave() {
    let (mut session, buffer) = with_a_panel(Some(ISSUE), Some(PULL_REQUEST));
    for modifiers in [KeyModifiers::NONE, KeyModifiers::CONTROL] {
        for (needle, offset, address) in [
            ("Pull request", 2, PULL_REQUEST),
            ("brave/bravebot#3200", 5, PULL_REQUEST),
            ("Issue", 1, ISSUE),
            ("brave/bravebot#2685", 12, ISSUE),
        ] {
            let (row, column) = find(&buffer, needle);
            session.last_click = None;
            assert_eq!(
                click_with(&mut session, row, column + offset, modifiers),
                opens(address),
                "{needle} with {modifiers:?}"
            );
            assert!(session.selection.is_none(), "{needle}");
            assert_eq!(panel_link_at(&session, row, column + offset), Some(address));
        }
    }
}

/// An address of another shape is drawn as written and opens as written, and a row too long for
/// the panel is drawn cut and opens whole, since what opens is what was set and not the text.
#[test]
fn a_link_row_opens_the_address_as_it_was_set_and_not_the_text_drawn_for_it() {
    let long = "https://example.test/a/very/long/path/to/a/ticket/that/is/longer/than/the/row?id=7";
    let (mut session, buffer) = with_a_panel(Some(long), None);
    let (row, column) = find(&buffer, "Issue");
    assert_eq!(
        click_with(&mut session, row, column + 1, KeyModifiers::NONE),
        opens(long)
    );
    let drawn: String = (0..120)
        .map(|c| hyperlink::shown(buffer[(c, row)].symbol()).to_string())
        .collect();
    assert!(!drawn.contains(long), "the row was not cut: {drawn}");
}

/// Nothing else on the panel opens a link: the heading above the rows, the rows around them, the
/// transcript beside it, a drag, a double click and the panel's border, which is its resize handle.
#[test]
fn a_click_off_the_link_rows_a_drag_or_a_double_click_opens_nothing() {
    let (mut session, buffer) = with_a_panel(Some(ISSUE), Some(PULL_REQUEST));
    let (row, column) = find(&buffer, "Pull request");
    for needle in ["Links", "a session", "Context", "context"] {
        let (other, at) = find(&buffer, needle);
        session.last_click = None;
        let acted = click_with(&mut session, other, at, KeyModifiers::NONE);
        assert!(!matches!(acted, Action::OpenUrl(_)), "{needle}: {acted:?}");
    }

    // Swept, and a press let go on another cell.
    handle_mouse(
        &mut session,
        event(
            MouseEventKind::Down(MouseButton::Left),
            row,
            column,
            KeyModifiers::NONE,
        ),
    );
    handle_mouse(
        &mut session,
        event(
            MouseEventKind::Drag(MouseButton::Left),
            row,
            column + 6,
            KeyModifiers::NONE,
        ),
    );
    let released = handle_mouse(
        &mut session,
        event(
            MouseEventKind::Up(MouseButton::Left),
            row,
            column + 6,
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(released, Action::Copy, "a sweep opened the link");

    // The second press of a double click opened the link with the first and does nothing
    // (MOUSE-50).
    session.last_click = None;
    assert_eq!(
        click_with(&mut session, row, column + 1, KeyModifiers::NONE),
        opens(PULL_REQUEST)
    );
    let second = click_with(&mut session, row, column + 1, KeyModifiers::NONE);
    assert_eq!(
        second,
        Action::None,
        "the second click of a double opened it"
    );

    // The border column.
    let border = (0..120)
        .find(|&c| buffer[(c, row)].symbol() == "\u{2502}")
        .expect("the panel's border");
    session.last_click = None;
    let acted = click_with(&mut session, row, border, KeyModifiers::NONE);
    assert!(!matches!(acted, Action::OpenUrl(_)), "{acted:?}");
}

/// The panel draws a link set with an address the opener refuses, and a click on its row is not a
/// click on a link: nothing is told to open it and the row is not a target.
#[test]
fn a_link_the_opener_would_refuse_is_drawn_and_not_a_target() {
    let named = "https://github.com@example.test/brave/bravebot/pull/1";
    let (mut session, buffer) = with_a_panel(None, Some(named));
    let (row, column) = find(&buffer, "Pull request");
    assert_eq!(
        click_with(&mut session, row, column + 1, KeyModifiers::NONE),
        Action::Copy
    );
    assert_eq!(panel_link_at(&session, row, column + 1), None);
}

/// A link that is cleared stops being a target the next frame, and a link set later is one.
#[test]
fn a_cleared_link_is_no_target_and_a_new_one_is() {
    let (mut session, buffer) = with_a_panel(Some(ISSUE), None);
    let (row, column) = find(&buffer, "Issue");
    session.link(None, None);
    let after = draw(&mut session, 120, 24);
    assert!(panel_link_at(&session, row, column + 1).is_none());
    let drawn: String = (0..120)
        .map(|c| hyperlink::shown(after[(c, row)].symbol()).to_string())
        .collect();
    assert!(!drawn.contains("Issue"), "the row is still drawn: {drawn}");
    session.link(None, Some(PULL_REQUEST));
    let again = draw(&mut session, 120, 24);
    let (row, column) = find(&again, "Pull request");
    assert_eq!(
        click_with(&mut session, row, column + 1, KeyModifiers::NONE),
        opens(PULL_REQUEST)
    );
}

/// A picker is drawn over the panel and has the clicks that land on it.
#[test]
fn a_link_row_under_a_picker_opens_nothing() {
    use bravebot_tui::effort_prompt::Picker;
    use bravebot_tui::picking::Picking;

    let (mut session, buffer) = with_a_panel(Some(ISSUE), Some(PULL_REQUEST));
    let (row, column) = find(&buffer, "Pull request");
    session.open_picking(Picking::Effort(Picker::new(None)));
    assert_eq!(panel_link_at(&session, row, column + 1), None);
    assert_ne!(
        click_with(&mut session, row, column + 1, KeyModifiers::NONE),
        opens(PULL_REQUEST)
    );
}

/// The click is the same wherever the panel's row is, so for every cell of the screen a click
/// opens an address exactly where one of the two questions a pointer asks says there is one.
#[test]
fn a_click_on_the_panel_and_a_click_on_the_transcript_each_open_their_own_address() {
    let (mut session, buffer) = with_a_panel(Some(ISSUE), Some(PULL_REQUEST));
    let mut opened = std::collections::BTreeSet::new();
    for row in 0..buffer.area.height {
        for column in 0..buffer.area.width {
            let expected = url_at(&session, row, column)
                .or_else(|| panel_link_at(&session, row, column))
                .map(str::to_string);
            session.last_click = None;
            let acted = click_with(&mut session, row, column, KeyModifiers::CONTROL);
            match expected {
                Some(address) => {
                    assert_eq!(acted, Action::OpenUrl(address.clone()), "{row},{column}");
                    opened.insert(address);
                }
                None => assert!(!matches!(acted, Action::OpenUrl(_)), "{row},{column}"),
            }
        }
    }
    assert_eq!(
        opened,
        [GUIDE, ISSUE, PULL_REQUEST]
            .into_iter()
            .map(str::to_string)
            .collect()
    );
}

/// A working directory holding a few files, and a session in it saying `reply` and drawn.
struct Project {
    // Held so the directory stays until the test ends.
    _directory: tempfile::TempDir,
    root: std::path::PathBuf,
    workspace: bravebot_agent::workspace::Workspace,
}

impl Project {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("a directory");
        let root = directory.path().canonicalize().expect("a path");
        std::fs::create_dir_all(root.join("src")).expect("a directory");
        std::fs::write(
            root.join("src/main.rs"),
            "fn main() {\n    let a = 1;\n    let b = 2;\n    run(a, b);\n}\n",
        )
        .expect("a file");
        std::fs::create_dir_all(root.join("docs.d")).expect("a directory");
        let workspace = bravebot_agent::workspace::Workspace::new(&root).expect("a workspace");
        Self {
            _directory: directory,
            root,
            workspace,
        }
    }

    fn show(
        &self,
        session: &mut Session,
        path: &str,
        line: Option<u32>,
    ) -> bravebot_session::audit::Trail {
        bravebot_tui::file_preview::show(
            session,
            &self.workspace,
            &bravebot_core::trust::TrustStore::new(&self.root),
            &bravebot_core::permissions::Permissions::new(),
            &bravebot_tui::text_links::PathRef {
                path: path.to_string(),
                line,
                column: None,
            },
        )
    }
}

fn path(name: &str, line: Option<u32>) -> bravebot_tui::text_links::PathRef {
    bravebot_tui::text_links::PathRef {
        path: name.to_string(),
        line,
        column: None,
    }
}

const WAIT: std::time::Duration = std::time::Duration::from_millis(400);

/// A click on a path in a reply is not carried out at once: it waits for the time a second press
/// would take, and then asks for the file at the line the path named.
#[test]
fn a_click_on_a_path_waits_for_a_second_press_and_then_asks_for_the_file() {
    let (mut session, buffer) = drawn(vec![reply("the fix is in src/main.rs:3 now")], 100, 12);
    let (row, column) = find(&buffer, "src/main.rs");
    let now = std::time::Instant::now();

    assert_eq!(
        click_with(&mut session, row, column + 4, KeyModifiers::NONE),
        Action::Redraw
    );
    assert!(session.selection.is_none());
    let due = session.the_click_due_in(now).expect("a click waits");
    assert!(due <= WAIT, "{due:?}");
    // Looked at once the click has been made, which is when a loop would look: it is not yet due.
    let at_once = std::time::Instant::now();
    assert_eq!(
        session.take_the_click_due(at_once),
        None,
        "carried out at once"
    );

    let asked = session.take_the_click_due(now + WAIT + std::time::Duration::from_millis(1));
    assert_eq!(asked, Some(path("src/main.rs", Some(3))));
    assert_eq!(session.take_the_click_due(now + WAIT * 2), None, "twice");
    assert_eq!(session.the_click_due_in(now), None);
}

/// Selecting a file name with a double click is how a person copies one, so the first press of a
/// double click shows nothing, and neither does a press anywhere else while the click waits.
#[test]
fn a_double_click_on_a_path_selects_a_word_and_shows_nothing() {
    let (mut session, buffer) = drawn(vec![reply("the fix is in src/main.rs now")], 100, 12);
    let (row, column) = find(&buffer, "src/main.rs");
    let later = std::time::Instant::now() + std::time::Duration::from_secs(1);

    click_with(&mut session, row, column + 6, KeyModifiers::NONE);
    assert!(
        session
            .the_click_due_in(std::time::Instant::now())
            .is_some()
    );
    let second = click_with(&mut session, row, column + 6, KeyModifiers::NONE);
    assert_eq!(second, Action::Copy, "the second click of a double");
    assert!(
        session
            .selection
            .is_some_and(|selection| !selection.is_empty())
    );
    assert_eq!(session.take_the_click_due(later), None);

    session.last_click = None;
    click_with(&mut session, row, column + 1, KeyModifiers::NONE);
    handle_mouse(
        &mut session,
        event(
            MouseEventKind::Down(MouseButton::Left),
            0,
            0,
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(session.take_the_click_due(later), None, "a press elsewhere");
}

/// A click that waited so long that something else must have happened since is dropped rather than
/// carried out late.
#[test]
fn a_click_that_waited_far_too_long_is_not_carried_out() {
    let (mut session, buffer) = drawn(vec![reply("see src/main.rs now")], 100, 12);
    let (row, column) = find(&buffer, "src/main.rs");
    click_with(&mut session, row, column + 2, KeyModifiers::NONE);
    let much_later = std::time::Instant::now() + std::time::Duration::from_secs(60);
    assert_eq!(session.take_the_click_due(much_later), None);
    assert_eq!(session.the_click_due_in(much_later), None, "it was kept");
}

/// A click on a path waits 400 milliseconds for a second press and is carried out up to two seconds
/// after it was made: before the first it is kept and not yet due, and after the second it is dropped
/// as one the person has left behind. The waits are well clear of the bounds, since the clock is the
/// test's own and the click reads the real one.
#[test]
fn a_click_on_a_path_is_due_after_400_milliseconds_and_dropped_after_two_seconds() {
    let ms = std::time::Duration::from_millis;
    for (waited, carried_out) in [
        (350, false),
        (450, true),
        (1_900, true),
        (2_100, false),
        (3_000, false),
        (60_000, false),
    ] {
        let (mut session, buffer) = drawn(vec![reply("see src/main.rs now")], 100, 12);
        let (row, column) = find(&buffer, "src/main.rs");
        click_with(&mut session, row, column + 2, KeyModifiers::NONE);
        let later = std::time::Instant::now() + ms(waited);

        let due = session.take_the_click_due(later);
        assert_eq!(
            due,
            carried_out.then(|| path("src/main.rs", None)),
            "{waited} ms"
        );
        // Before it is due the click is kept to be carried out, and after, taken or dropped, it is not.
        assert_eq!(
            session.the_click_due_in(later).is_some(),
            waited < 400,
            "{waited} ms"
        );
    }
}

/// Only a single click with nothing held that swept nothing on a path waits: a modifier, a sweep,
/// a click beside the path and a click on an address do not.
#[test]
fn a_click_with_a_modifier_a_sweep_or_beside_the_path_waits_for_nothing() {
    let (mut session, buffer) = drawn(
        vec![reply(&format!("see src/main.rs and {GUIDE} now"))],
        100,
        12,
    );
    let (row, column) = find(&buffer, "src/main.rs");
    let now = std::time::Instant::now();
    for modifiers in [
        KeyModifiers::CONTROL,
        KeyModifiers::SHIFT,
        KeyModifiers::ALT,
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ] {
        session.last_click = None;
        click_with(&mut session, row, column + 3, modifiers);
        assert_eq!(session.the_click_due_in(now), None, "{modifiers:?}");
    }

    session.last_click = None;
    handle_mouse(
        &mut session,
        event(
            MouseEventKind::Down(MouseButton::Left),
            row,
            column + 1,
            KeyModifiers::NONE,
        ),
    );
    handle_mouse(
        &mut session,
        event(
            MouseEventKind::Drag(MouseButton::Left),
            row,
            column + 6,
            KeyModifiers::NONE,
        ),
    );
    let released = handle_mouse(
        &mut session,
        event(
            MouseEventKind::Up(MouseButton::Left),
            row,
            column + 6,
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(released, Action::Copy);
    assert_eq!(session.the_click_due_in(now), None, "a sweep");

    handle_mouse(
        &mut session,
        event(
            MouseEventKind::Down(MouseButton::Left),
            row,
            column + 2,
            KeyModifiers::NONE,
        ),
    );
    handle_mouse(
        &mut session,
        event(
            MouseEventKind::Up(MouseButton::Left),
            row,
            column + 9,
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(session.the_click_due_in(now), None, "a press elsewhere");

    for beside in [column - 1, column + "src/main.rs".len() as u16] {
        session.last_click = None;
        assert_eq!(
            click_with(&mut session, row, beside, KeyModifiers::NONE),
            Action::Copy
        );
        assert_eq!(session.the_click_due_in(now), None, "beside the path");
    }

    let (row, column) = find(&buffer, "https://");
    session.last_click = None;
    assert_eq!(
        click_with(&mut session, row, column + 3, KeyModifiers::NONE),
        Action::Copy
    );
    assert_eq!(session.the_click_due_in(now), None, "an address");
}

/// A state a session can be put in, by name.
type Mode = (&'static str, fn(&mut Session));

/// The states where a click on a path is not answered: the scroller, a history search, a picker
/// over the transcript and a turn that is running, which is drawing at the end of the transcript.
/// Each is what a session is put in by the function.
fn where_a_click_on_a_path_is_not_answered() -> [Mode; 4] {
    use bravebot_tui::effort_prompt::Picker;
    use bravebot_tui::picking::Picking;
    [
        ("the scroller", |session| session.open_scroller()),
        ("a history search", |session| {
            session.history = bravebot_tui::history::History::from_entries(vec![
                bravebot_session::store::Entry::sent("an earlier prompt", None),
            ]);
            assert!(session.open_history_search());
        }),
        ("a picker", |session| {
            session.open_picking(Picking::Effort(Picker::new(None)));
        }),
        ("a turn", |session| {
            session.status = bravebot_tui::Status::Working;
        }),
    ]
}

/// A path is not answered where the file would not be seen or the screen is another's: the
/// scroller, a history search, a picker over it and a turn that is running, which is drawing at the
/// end of the transcript.
#[test]
fn a_click_on_a_path_where_the_file_would_not_be_seen_waits_for_nothing() {
    let text = vec![reply("see src/main.rs now")];
    let now = std::time::Instant::now();
    for (name, open) in where_a_click_on_a_path_is_not_answered() {
        let (mut session, buffer) = drawn(text.clone(), 100, 12);
        let (row, column) = find(&buffer, "src/main.rs");
        click_with(&mut session, row, column + 3, KeyModifiers::NONE);
        assert!(
            session.the_click_due_in(now).is_some(),
            "{name}: the fixture"
        );
        session.drop_the_click_put_off();
        session.last_click = None;

        open(&mut session);
        let (row, column) = if name == "the scroller" {
            find(&draw(&mut session, 100, 12), "src/main.rs")
        } else {
            (row, column)
        };
        click_with(&mut session, row, column + 3, KeyModifiers::NONE);
        assert_eq!(session.the_click_due_in(now), None, "{name}");
    }
}

/// A click that waits for a second press is answered where it falls due as it would be where it was
/// made. A key can open the scroller or a search, or a turn can begin, in the time it waits, and the
/// file is not then added to a transcript the person has gone from.
#[test]
fn a_click_on_a_path_is_not_carried_out_where_the_session_went_while_it_waited() {
    let text = vec![reply("see src/main.rs now")];
    let later = std::time::Instant::now() + WAIT * 2;
    for (name, open) in where_a_click_on_a_path_is_not_answered() {
        let (mut session, buffer) = drawn(text.clone(), 100, 12);
        let (row, column) = find(&buffer, "src/main.rs");
        click_with(&mut session, row, column + 3, KeyModifiers::NONE);
        open(&mut session);

        assert_eq!(session.take_the_click_due(later), None, "{name}");
        assert_eq!(session.the_click_due_in(later), None, "{name}: it was kept");
    }

    let (mut session, buffer) = drawn(text, 100, 12);
    let (row, column) = find(&buffer, "src/main.rs");
    click_with(&mut session, row, column + 3, KeyModifiers::NONE);
    assert_eq!(
        session.take_the_click_due(later),
        Some(path("src/main.rs", None)),
        "nothing happened while it waited, so it is carried out"
    );
}

/// The files state is a state of the info panel and the transcript is drawn beside it as at the
/// plain prompt, so a ctrl-click on an address opens it, a ctrl-click on a path asks for the editor
/// and a plain click on one shows the file.
#[test]
fn a_link_is_answered_with_the_files_state_open() {
    let diff = bravebot_agent::diff::Diff::compute("old\n", "new\n");
    let written = Entry::tool(
        Activity::running("Update", "a.rs")
            .of_tool("edit_file")
            .with_changes(diff.condensed(3))
            .with_written(Some(bravebot_agent::report::Written {
                landed: "a.rs".into(),
                added: diff.added(),
                removed: diff.removed(),
                numbers: diff.condensed_numbers(3),
            }))
            .done("done"),
    );
    let (mut session, _) = drawn(
        vec![
            reply(&format!("see src/main.rs:3 and {GUIDE} now")),
            written,
        ],
        120,
        30,
    );
    session.toggle_files();
    assert!(session.files_open(), "the files state did not open");
    let buffer = draw(&mut session, 120, 30);

    let (row, column) = find(&buffer, "https://");
    assert_eq!(ctrl_click(&mut session, row, column + 2), opens(GUIDE));
    let (row, column) = find(&buffer, "src/main.rs");
    session.last_click = None;
    assert_eq!(
        ctrl_click(&mut session, row, column + 2),
        Action::EditFile(path("src/main.rs", Some(3)))
    );
    session.last_click = None;
    click_with(&mut session, row, column + 2, KeyModifiers::NONE);
    assert_eq!(
        session.take_the_click_due(std::time::Instant::now() + WAIT * 2),
        Some(path("src/main.rs", Some(3)))
    );
}

/// The file's lines are added to the end of the transcript as a call with a quarantined block under
/// it: behind the bar in the margin, with the name and the label in the heading, the line saying no
/// model has it, the rows numbered from the line the path named, and the view at the end.
#[test]
fn the_file_is_shown_under_a_call_in_a_quarantined_block() {
    let project = Project::new();
    let (mut session, _) = drawn(vec![reply("see src/main.rs:3 now")], 100, 24);
    session.scroll = 7;

    let trail = project.show(&mut session, "src/main.rs", Some(3));
    assert!(!trail.events().is_empty(), "the release was not recorded");
    assert_eq!(session.scroll, 0, "the view did not go to the end");

    let entry = session.transcript.last().expect("an entry");
    let shown = entry.shown.as_ref().expect("a quarantined block");
    assert_eq!(shown.origin, "src/main.rs");
    assert_eq!(shown.reach, bravebot_agent::report::Reach::NoModel);
    assert_eq!(shown.label, "(U,priv)");
    assert_eq!(shown.lines, 5);
    assert_eq!(
        shown.preview,
        [
            "1  fn main() {",
            "2      let a = 1;",
            "3      let b = 2;",
            "4      run(a, b);",
            "5  }"
        ]
    );

    let buffer = draw(&mut session, 100, 24);
    let rows: Vec<String> = (0..buffer.area.height)
        .map(|row| {
            (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol().to_string())
                .collect()
        })
        .collect();
    let heading = rows
        .iter()
        .position(|row| row.contains("untrusted") && row.contains("src/main.rs"))
        .expect("the heading of the block");
    assert!(rows[heading].contains("(U,priv)"), "{}", rows[heading]);
    assert!(
        rows[heading].contains("in no model's context"),
        "{}",
        rows[heading]
    );
    // The call the block belongs to is above it, with the number of lines the file has.
    let call = rows
        .iter()
        .position(|row| row.contains("Preview(src/main.rs)"))
        .expect("the call");
    assert!(call < heading, "{call} {heading}");
    assert!(rows[call + 1].contains("5 lines"), "{}", rows[call + 1]);
    for expected in ["fn main() {", "let b = 2;", "run(a, b);"] {
        let row = rows
            .iter()
            .find(|row| row.contains(expected))
            .unwrap_or_else(|| panic!("{expected} is not on the screen"));
        assert!(row.trim_start().starts_with('\u{2503}'), "no bar: {row}");
    }
}

/// A file with an escape sequence in it cannot draw on the screen: each control character is a
/// visible glyph, so the sequence is shown and not carried out.
#[test]
fn a_file_with_escape_sequences_cannot_draw_on_the_screen() {
    let project = Project::new();
    std::fs::write(
        project.root.join("evil.txt"),
        "\u{1b}[2J\u{1b}]0;owned\u{7}\u{1b}]8;;https://evil.test/\u{1b}\\click\u{1b}]8;;\u{1b}\\\n",
    )
    .expect("a file");
    let (mut session, _) = drawn(vec![], 100, 24);
    project.show(&mut session, "evil.txt", None);
    let buffer = draw(&mut session, 100, 24);
    for cell in buffer.content.iter() {
        assert!(
            !cell.symbol().contains('\u{1b}') && !cell.symbol().contains('\u{7}'),
            "{:?}",
            cell.symbol()
        );
    }
    let text: String = buffer
        .content
        .iter()
        .map(|cell| cell.symbol().to_string())
        .collect();
    // Each is a visible glyph and not dropped, so what the file held is what the person reads.
    assert!(text.contains("click"), "{text}");
    assert!(
        text.contains('\u{241b}'),
        "the escape was not made visible: {text}"
    );
    assert!(
        text.contains('\u{2407}'),
        "the bell was not made visible: {text}"
    );
}

/// A name that is not a file in the directories the session reads is told so, in the interface's own
/// words and without the name, which is text the assistant wrote. Nothing is added to the transcript
/// but the note.
#[test]
fn a_path_that_names_no_file_in_reach_says_so_without_repeating_it() {
    let project = Project::new();
    let (mut session, _) = drawn(vec![reply("ok")], 100, 12);
    let said = bravebot_i18n::t!(session_path_not_a_file).to_string();
    for name in [
        "NOPE-missing-name.rs",
        "../outside-name.rs",
        "docs.d",
        "/etc/hosts",
        "src",
    ] {
        let before = session.transcript.len();
        project.show(&mut session, name, None);
        assert_eq!(session.transcript.len(), before + 1, "{name}");
        let note = session.transcript.last().expect("a note");
        assert_eq!(note.text, said, "{name}");
        assert!(note.shown.is_none(), "{name}");
        assert!(!note.text.contains(name), "{name}");
    }
}

/// A file the person's rules say is not read is not shown, and one too large for a preview says how
/// large it is.
#[test]
fn a_denied_file_and_a_file_too_large_are_refused_in_words() {
    let project = Project::new();
    std::fs::write(project.root.join(".env"), "TOKEN=1\n").expect("a file");
    std::fs::write(
        project.root.join("big.log"),
        vec![b'a'; (bravebot_agent::file_preview::MOST_BYTES + 1) as usize],
    )
    .expect("a file");
    let (mut session, _) = drawn(vec![], 100, 12);
    let (permissions, rejected) = bravebot_core::permissions::Permissions::parse(
        &["Read(.env)".to_string()],
        &[],
        &[],
        &bravebot_core::permissions::Anchors::none(),
    );
    assert!(rejected.is_empty());
    let target = |name: &str| bravebot_tui::text_links::PathRef {
        path: name.to_string(),
        line: None,
        column: None,
    };
    let trust = bravebot_core::trust::TrustStore::new(&project.root);

    bravebot_tui::file_preview::show(
        &mut session,
        &project.workspace,
        &trust,
        &permissions,
        &target(".env"),
    );
    assert_eq!(
        session.transcript.last().expect("a note").text,
        bravebot_i18n::t!(session_preview_denied)
    );

    bravebot_tui::file_preview::show(
        &mut session,
        &project.workspace,
        &trust,
        &permissions,
        &target("big.log"),
    );
    let note = &session.transcript.last().expect("a note").text;
    assert!(
        note.contains(&(bravebot_agent::file_preview::MOST_BYTES + 1).to_string()),
        "{note}"
    );
}

/// A click with ctrl held on a path asks for the editor at the line and column the path gave, and
/// leaves nothing selected and no preview waiting.
#[test]
fn a_ctrl_click_on_a_path_asks_for_the_editor_at_its_place() {
    let (mut session, buffer) = drawn(
        vec![reply(
            "see src/main.rs:3:5 and src/lib.rs and src/b.rs:9-12 now",
        )],
        100,
        12,
    );
    let now = std::time::Instant::now();
    for (needle, expected) in [
        (
            "src/main.rs",
            bravebot_tui::text_links::PathRef {
                path: "src/main.rs".to_string(),
                line: Some(3),
                column: Some(5),
            },
        ),
        ("src/lib.rs", path("src/lib.rs", None)),
        ("src/b.rs", path("src/b.rs", Some(9))),
    ] {
        let (row, column) = find(&buffer, needle);
        for offset in [0, 3, needle.len() as u16 - 1] {
            session.last_click = None;
            assert_eq!(
                ctrl_click(&mut session, row, column + offset),
                Action::EditFile(expected.clone()),
                "{needle} at {offset}"
            );
            assert!(session.selection.is_none());
            assert_eq!(session.the_click_due_in(now), None, "a preview waits");
        }
    }
}

/// Without ctrl it is the click that shows the file, and a sweep, a double click, shift and a click
/// beside the path ask for no editor.
#[test]
fn a_click_that_is_not_a_ctrl_click_on_a_path_asks_for_no_editor() {
    let (mut session, buffer) = drawn(vec![reply("see src/main.rs:3 now")], 100, 12);
    let (row, column) = find(&buffer, "src/main.rs");
    let opens_an_editor = |action: &Action| matches!(action, Action::EditFile(_));

    for modifiers in [KeyModifiers::NONE, KeyModifiers::SHIFT, KeyModifiers::ALT] {
        session.last_click = None;
        let acted = click_with(&mut session, row, column + 3, modifiers);
        assert!(!opens_an_editor(&acted), "{modifiers:?}");
    }

    session.last_click = None;
    handle_mouse(
        &mut session,
        event(
            MouseEventKind::Down(MouseButton::Left),
            row,
            column + 1,
            KeyModifiers::CONTROL,
        ),
    );
    handle_mouse(
        &mut session,
        event(
            MouseEventKind::Drag(MouseButton::Left),
            row,
            column + 6,
            KeyModifiers::CONTROL,
        ),
    );
    let released = handle_mouse(
        &mut session,
        event(
            MouseEventKind::Up(MouseButton::Left),
            row,
            column + 6,
            KeyModifiers::CONTROL,
        ),
    );
    assert_eq!(released, Action::Copy, "a sweep asked for the editor");

    session.last_click = None;
    assert!(opens_an_editor(&ctrl_click(&mut session, row, column + 4)));
    let second = ctrl_click(&mut session, row, column + 4);
    assert_eq!(second, Action::None, "the second click of a double");

    for beside in [
        column - 1,
        column + ":3 now".len() as u16 + "src/main.rs".len() as u16,
    ] {
        session.last_click = None;
        let acted = ctrl_click(&mut session, row, beside);
        assert!(!opens_an_editor(&acted), "beside the path");
    }
}

/// The editor takes the terminal, so it is not asked for while something else is drawing it: a
/// running turn, a command, a history search, and a picker over the transcript. The scroller is the
/// transcript, so it is asked for there.
#[test]
fn a_ctrl_click_on_a_path_asks_for_no_editor_while_the_screen_is_something_elses() {
    use bravebot_tui::effort_prompt::Picker;
    use bravebot_tui::picking::Picking;

    type Mode = (&'static str, bool, fn(&mut Session));
    let modes: [Mode; 6] = [
        ("a turn", false, |session| {
            session.status = bravebot_tui::Status::Working;
        }),
        ("a command", false, |session| {
            session.status = bravebot_tui::Status::Running;
        }),
        ("a history search", false, |session| {
            session.history = bravebot_tui::history::History::from_entries(vec![
                bravebot_session::store::Entry::sent("an earlier prompt", None),
            ]);
            assert!(session.open_history_search());
        }),
        ("a picker", false, |session| {
            session.open_picking(Picking::Effort(Picker::new(None)));
        }),
        ("the scroller", true, |session| session.open_scroller()),
        ("nothing", true, |_| {}),
    ];
    for (name, asks, open) in modes {
        let (mut session, buffer) = drawn(vec![reply("see src/main.rs:3 now")], 100, 12);
        open(&mut session);
        let buffer = if name == "the scroller" {
            draw(&mut session, 100, 12)
        } else {
            buffer
        };
        let (row, column) = find(&buffer, "src/main.rs");
        let acted = ctrl_click(&mut session, row, column + 3);
        assert_eq!(
            matches!(acted, Action::EditFile(_)),
            asks,
            "{name}: {acted:?}"
        );
    }
}

/// An address is still an address under ctrl, and a path is not read as one: each opens what it is.
#[test]
fn a_ctrl_click_opens_an_address_in_the_browser_and_a_path_in_the_editor() {
    let (mut session, buffer) = drawn(
        vec![reply(&format!(
            "{GUIDE} and src/main.rs and https://other.test/a.rs"
        ))],
        100,
        12,
    );
    let (row, column) = find(&buffer, "src/main.rs");
    assert_eq!(
        ctrl_click(&mut session, row, column + 2),
        Action::EditFile(path("src/main.rs", None))
    );
    let (row, column) = find(&buffer, "https://other.test/a.rs");
    assert_eq!(
        ctrl_click(&mut session, row, column + 2),
        opens("https://other.test/a.rs"),
        "a path inside an address opened in an editor"
    );
}

/// A path the widget wrapped is one path, and a click on any row of it asks for the whole name, the
/// plain click for the file and the ctrl-click for the editor.
#[test]
fn a_path_wrapped_over_rows_is_one_path_from_any_of_them() {
    let long = "crates/tui/src/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/lib.rs:7";
    let (mut session, _) = drawn(vec![reply(&format!("open {long} please"))], 30, 12);
    let rows: Vec<u16> = (0..12)
        .filter(|&row| (0..30).any(|column| path_at(&session, row, column).is_some()))
        .collect();
    assert!(rows.len() >= 3, "the path was not wrapped: {rows:?}");
    let whole = path(&long[..long.len() - 2], Some(7));
    for row in rows {
        let column = (0..30)
            .find(|&column| path_at(&session, row, column).is_some())
            .expect("a cell of the path");
        session.last_click = None;
        assert_eq!(
            ctrl_click(&mut session, row, column),
            Action::EditFile(whole.clone()),
            "row {row}"
        );
        session.last_click = None;
        click_with(&mut session, row, column, KeyModifiers::NONE);
        let asked = session.take_the_click_due(std::time::Instant::now() + WAIT * 2);
        assert_eq!(asked, Some(whole.clone()), "row {row}");
    }
}

/// A path with wide characters in it is one path wherever the wrap falls. Where a wide character does
/// not fit in the last column it starts the next row and leaves the row one cell short, which is not
/// where the name ends: a click on any row asks for the whole name, and a name the wrap cut short
/// is never what a click asks for.
#[test]
fn a_path_with_a_wide_character_is_one_path_wherever_the_wrap_falls() {
    use bravebot_tui::text_links::{PathRef, Target};
    let mut gaps = 0;
    for padding in 0..30 {
        let name = format!("{}.md\u{6587}\u{6863}.txt", "a".repeat(padding));
        let (session, _) = drawn(vec![reply(&format!("see {name} now"))], 30, 12);
        assert_eq!(
            session.laid.links,
            [Target::Path(PathRef {
                path: name.clone(),
                line: None,
                column: None
            })],
            "{name:?}"
        );
        // A row of the path that ends one cell short of the edge is where the gap is.
        let short = (0..12).any(|row| {
            let last = (0..30)
                .rev()
                .find(|&column| path_at(&session, row, column).is_some());
            last == Some(28) && path_at(&session, row, 29).is_none()
        });
        gaps += usize::from(short);
    }
    assert!(
        gaps > 0,
        "no padding left a row one cell short, so nothing was tried"
    );
}

/// Type `line` into the box and press Enter, as a person does, and say what the press asked for.
fn type_command(session: &mut Session, line: &str) -> Action {
    for c in line.chars() {
        handle_key(session, KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    handle_key(session, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
}

/// What was added to the transcript after its first `before` entries, one string for each.
fn said_since(session: &Session, before: usize) -> Vec<String> {
    session
        .transcript
        .iter()
        .skip(before)
        .map(|entry| entry.text.clone())
        .collect()
}

/// `/open` lists the links of the latest reply that has any: the newest reply that holds one, and
/// not an older reply's, a person's prompt's, a note's or a newer reply that holds none. They are
/// numbered from one, in the order they are drawn, each as it is drawn: a path with the place after
/// it as it was written, and an address without the sentence's punctuation around it.
#[test]
fn the_open_command_lists_the_links_of_the_latest_reply_that_has_any_numbered_from_one() {
    use bravebot_i18n::t;
    let (mut session, _) = drawn(
        vec![
            reply("old: https://old.test/a and src/old.rs"),
            Entry::user("a prompt with https://prompt.test/p"),
            reply("new: src/b.rs:9-12 then https://new.test/x, and (https://new.test/y)."),
            reply("no links in this one, only prose, Makefile and 1.5.0"),
            Entry::system("a note with https://note.test/n"),
        ],
        100,
        24,
    );
    let before = session.transcript.len();

    assert_eq!(type_command(&mut session, "/open"), Action::Redraw);

    assert_eq!(
        said_since(&session, before),
        vec![
            t!(open_listed, count = 3).to_string(),
            t!(open_item, number = 1, link = "src/b.rs:9-12").to_string(),
            t!(open_item, number = 2, link = "https://new.test/x").to_string(),
            t!(open_item, number = 3, link = "https://new.test/y").to_string(),
        ]
    );
    assert_eq!(
        session.listed_links(),
        [
            bravebot_tui::text_links::Target::Path(path("src/b.rs", Some(9))),
            bravebot_tui::text_links::Target::Address("https://new.test/x".to_string()),
            bravebot_tui::text_links::Target::Address("https://new.test/y".to_string()),
        ]
    );
}

/// A reply that holds one link is listed as one, in the words for one.
#[test]
fn the_open_command_says_one_link_in_the_singular() {
    use bravebot_i18n::t;
    let (mut session, _) = drawn(vec![reply("see https://example.test/only")], 100, 12);
    let before = session.transcript.len();
    type_command(&mut session, "/open");
    assert_eq!(
        said_since(&session, before),
        vec![
            t!(open_listed, count = 1).to_string(),
            t!(open_item, number = 1, link = "https://example.test/only").to_string(),
        ]
    );
}

/// A reply with more links than the command lists is listed up to the most, with a line saying how
/// many are left out, and a number past it names nothing: the person was not shown that link. A reply
/// with exactly the most has no such line.
#[test]
fn a_reply_with_more_links_than_the_most_is_listed_to_the_most_and_says_how_many_are_left() {
    use bravebot_i18n::t;
    use bravebot_tui::open_command::MOST_LISTED;
    let reply_of = |count: usize| {
        (1..=count)
            .map(|n| format!("see https://example.test/{n}\n\n"))
            .collect::<String>()
    };

    let (mut session, _) = drawn(vec![reply(&reply_of(MOST_LISTED))], 100, 12);
    let before = session.transcript.len();
    type_command(&mut session, "/open");
    let said = said_since(&session, before);
    assert_eq!(
        said.len(),
        1 + MOST_LISTED,
        "a line was added past the most"
    );
    assert_eq!(said[0], t!(open_listed, count = MOST_LISTED).to_string());

    let (mut session, _) = drawn(vec![reply(&reply_of(MOST_LISTED + 7))], 100, 12);
    let before = session.transcript.len();
    type_command(&mut session, "/open");
    let said = said_since(&session, before);
    assert_eq!(said.len(), 2 + MOST_LISTED);
    assert_eq!(
        said[0],
        t!(open_listed, count = MOST_LISTED + 7).to_string()
    );
    assert_eq!(
        said[MOST_LISTED],
        t!(
            open_item,
            number = MOST_LISTED,
            link = format!("https://example.test/{MOST_LISTED}")
        )
        .to_string()
    );
    assert_eq!(said[MOST_LISTED + 1], t!(open_more, count = 7).to_string());
    assert_eq!(session.listed_links().len(), MOST_LISTED);

    assert_eq!(
        type_command(&mut session, &format!("/open {MOST_LISTED}")),
        opens(&format!("https://example.test/{MOST_LISTED}"))
    );
    let before = session.transcript.len();
    let past = type_command(&mut session, &format!("/open {}", MOST_LISTED + 1));
    assert!(!matches!(past, Action::OpenUrl(_)), "{past:?}");
    assert_eq!(
        said_since(&session, before),
        vec![t!(open_no_such, number = MOST_LISTED + 1, links = MOST_LISTED).to_string()]
    );
}

/// Where no reply holds a link the command says so and leaves nothing to count from, including the
/// list an earlier `/open` made.
#[test]
fn the_open_command_says_so_where_no_reply_holds_a_link() {
    use bravebot_i18n::t;
    let (mut session, _) = drawn(
        vec![
            Entry::user("see https://example.test/mine and src/mine.rs"),
            reply("prose and nothing else, e.g. a word, 1.5.0 and Makefile"),
        ],
        100,
        12,
    );
    session
        .transcript
        .push(reply("then https://example.test/later"));
    type_command(&mut session, "/open");
    assert_eq!(session.listed_links().len(), 1);

    session.transcript.clear();
    session
        .transcript
        .push(reply("prose and nothing else, e.g. a word"));
    let before = session.transcript.len();
    type_command(&mut session, "/open");
    assert_eq!(
        said_since(&session, before),
        vec![t!(open_none).to_string()]
    );
    assert!(
        session.listed_links().is_empty(),
        "the list from before was kept"
    );
}

/// The reply still arriving is the latest reply, as a click reaches an address in it, and once it
/// holds no link the newest finished reply that does is.
#[test]
fn the_open_command_lists_the_reply_still_arriving_first() {
    use bravebot_i18n::t;
    let (mut session, _) = drawn(vec![reply("finished: https://done.test/a")], 100, 12);
    session.streaming("arriving: https://arriving.test/b and src/c.rs");
    let _ = draw(&mut session, 100, 12);
    let before = session.transcript.len();
    type_command(&mut session, "/open");
    assert_eq!(
        said_since(&session, before),
        vec![
            t!(open_listed, count = 2).to_string(),
            t!(open_item, number = 1, link = "https://arriving.test/b").to_string(),
            t!(open_item, number = 2, link = "src/c.rs").to_string(),
        ]
    );

    let (mut session, _) = drawn(vec![reply("finished: https://done.test/a")], 100, 12);
    session.streaming("arriving, with no link yet");
    let before = session.transcript.len();
    type_command(&mut session, "/open");
    assert_eq!(
        said_since(&session, before),
        vec![
            t!(open_listed, count = 1).to_string(),
            t!(open_item, number = 1, link = "https://done.test/a").to_string(),
        ]
    );
}

/// The shapes the click tests use, with what each is drawn as and, for a path, the line it starts
/// at. Each is the words of one reply.
fn the_shapes_the_link_tests_use() -> Vec<(String, Vec<&'static str>)> {
    let long_address =
        "https://example.test/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/end";
    let long_path =
        "crates/tui/src/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/lib.rs";
    vec![
        (
            format!("The docs are at {GUIDE} and then more."),
            vec![GUIDE],
        ),
        (
            "See (https://example.test/a_(b)). Or https://example.test/c, then **https://example.test/d**."
                .to_string(),
            vec![
                "https://example.test/a_(b)",
                "https://example.test/c",
                "https://example.test/d",
            ],
        ),
        (format!("read {long_address} please"), vec![long_address]),
        (
            "see src/main.rs:3:5 and src/lib.rs and src/b.rs:9-12 now".to_string(),
            vec!["src/main.rs:3:5", "src/lib.rs", "src/b.rs:9-12"],
        ),
        (
            format!("open {long_path}:7 please"),
            vec!["crates/tui/src/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/lib.rs:7"],
        ),
        (
            "refused: https://github.com@example.test/login, file:///etc/passwd, https://exampl\u{e9}.test/ and https://exampl.test/ok"
                .to_string(),
            vec!["https://exampl.test/ok"],
        ),
        (
            "code `https://example.test/code` and `src/code.rs` and `not a path` end".to_string(),
            vec!["https://example.test/code", "src/code.rs"],
        ),
        (
            format!("| name | where |\n| --- | --- |\n| guide | {GUIDE} |\n\nafter: src/after.rs"),
            vec!["src/after.rs"],
        ),
        (
            "words: 1.5.0, Makefile, and/or, e.g, me@example.com, https:// and example.test only"
                .to_string(),
            vec!["example.test"],
        ),
        (
            "HTTP://EXAMPLE.TEST/A and https://example.test/a#frag".to_string(),
            vec!["HTTP://EXAMPLE.TEST/A", "https://example.test/a#frag"],
        ),
        // The text of a markdown link ends where the target begins, so a link whose text is its
        // target is two links and the text is not run together with the target.
        (
            "see [https://example.test/a](https://example.test/a) now".to_string(),
            vec!["https://example.test/a", "https://example.test/a"],
        ),
        (
            "see [src/main.rs](src/main.rs) now".to_string(),
            vec!["src/main.rs", "src/main.rs"],
        ),
        (
            "see [main.rs](src/main.rs:12) now".to_string(),
            vec!["main.rs", "src/main.rs:12"],
        ),
        (
            "see [docs](https://example.test/docs) and [`src/code.rs`](src/code.rs) now".to_string(),
            vec!["https://example.test/docs", "src/code.rs", "src/code.rs"],
        ),
    ]
}

/// The command and a click cannot disagree about what a reply links to: for each shape the tests of
/// the click use, at three widths, the links the command lists are the links the frame recorded for
/// a click, in the same order, as many and none other, and each is listed as it is drawn.
#[test]
fn what_the_open_command_lists_is_what_a_click_reaches_in_every_shape_the_link_tests_use() {
    use bravebot_i18n::t;
    use bravebot_tui::text_links::Target;
    for (text, drawn_as) in the_shapes_the_link_tests_use() {
        for width in [30u16, 60, 100] {
            let (mut session, _) = drawn(
                vec![
                    Entry::user(format!("a prompt with {GUIDE} and src/prompt.rs")),
                    Entry::system(format!("a note with {GUIDE}")),
                    reply(&text),
                ],
                width,
                60,
            );
            let reached: Vec<Target> = session.laid.links.clone();
            let before = session.transcript.len();

            type_command(&mut session, "/open");

            assert_eq!(
                session.listed_links(),
                reached.as_slice(),
                "{text:?} at {width}"
            );
            let mut expected = vec![t!(open_listed, count = drawn_as.len()).to_string()];
            expected.extend(
                drawn_as
                    .iter()
                    .enumerate()
                    .map(|(at, link)| t!(open_item, number = at + 1, link = *link).to_string()),
            );
            if drawn_as.is_empty() {
                expected = vec![t!(open_none).to_string()];
            }
            assert_eq!(
                said_since(&session, before),
                expected,
                "{text:?} at {width}"
            );
        }
    }
}

/// A link that ends in the last column of its row is listed when more of its line follows it, as a
/// click reaches it, since nothing is drawn below it that it could run on into.
#[test]
fn a_link_ending_in_the_last_column_of_a_row_is_listed_as_a_click_reaches_it() {
    use bravebot_i18n::t;
    // The reply's first two cells are its marker, so 28 characters fill a row of 30.
    let address = "https://example.test/abcdefg";
    assert_eq!(address.len(), 28);
    for text in [address.to_string(), format!("{address} and more after it")] {
        let (mut session, _) = drawn(vec![reply(&text)], 30, 12);
        let reached = session.laid.links.clone();
        assert_eq!(reached.len(), 1, "{text:?}: the fixture is not a link");
        let before = session.transcript.len();
        type_command(&mut session, "/open");
        assert_eq!(session.listed_links(), reached.as_slice(), "{text:?}");
        assert_eq!(
            said_since(&session, before),
            vec![
                t!(open_listed, count = 1).to_string(),
                t!(open_item, number = 1, link = address).to_string(),
            ],
            "{text:?}"
        );
    }
}

/// A reply longer than the screen is listed whole, where a click reaches only the links in view:
/// the keyboard route does not stop at the edge of what the person has scrolled to, and every link
/// the click reaches is among the ones listed.
#[test]
fn a_reply_longer_than_the_screen_is_listed_whole_and_holds_every_link_a_click_reaches() {
    let mut text = String::from("start https://top.test/one\n\n");
    for line in 0..30 {
        text.push_str(&format!("line {line} of prose\n\n"));
    }
    text.push_str("end https://bottom.test/two and src/last.rs:4");
    let (mut session, _) = drawn(vec![reply(&text)], 60, 10);
    let reached = session.laid.links.clone();
    assert!(
        reached.len() < 3,
        "the whole reply is on the screen: {reached:?}"
    );

    type_command(&mut session, "/open");

    let listed = session.listed_links();
    assert_eq!(listed.len(), 3, "{listed:?}");
    assert!(reached.iter().all(|link| listed.contains(link)));
}

/// The first cell of the screen, row by row, that the last frame recorded `link` in.
fn first_cell_of(
    session: &Session,
    link: &bravebot_tui::text_links::Target,
    width: u16,
    height: u16,
) -> (u16, u16) {
    (0..height)
        .flat_map(|row| (0..width).map(move |column| (row, column)))
        .find(|&(row, column)| link_at(session, row, column) == Some(link))
        .unwrap_or_else(|| panic!("{link:?} is not on the screen"))
}

/// What a click on `link` asks for without a modifier: an address asks for the browser with ctrl held,
/// as it does for no other click, and a path asks to show the file once it is known not to be the
/// first of a double click.
fn what_a_click_on_the_link_asks_for(
    session: &mut Session,
    link: &bravebot_tui::text_links::Target,
    width: u16,
    height: u16,
) -> Action {
    use bravebot_tui::text_links::Target;
    let (row, column) = first_cell_of(session, link, width, height);
    session.last_click = None;
    match link {
        Target::Address(_) => ctrl_click(session, row, column),
        Target::Path(_) => {
            click_with(session, row, column, KeyModifiers::NONE);
            let due = session
                .take_the_click_due(std::time::Instant::now() + WAIT * 2)
                .expect("the click on a path waited for nothing");
            Action::Preview(due)
        }
    }
}

/// A number does what a click on that link does, in every shape the click tests use and at three
/// widths: an address asks for the browser with the address the click asks for, and a path asks to
/// show the file with the path, line and column the click asks for. The command and the click are
/// two ways to one action, so a second reading of the reply would show here as a different one.
#[test]
fn a_number_asks_for_what_a_click_on_that_link_asks_for_in_every_shape_the_link_tests_use() {
    let mut asked = 0;
    for (text, _) in the_shapes_the_link_tests_use() {
        for width in [30u16, 60, 100] {
            let (mut session, _) = drawn(
                vec![Entry::user(format!("a prompt with {GUIDE}")), reply(&text)],
                width,
                60,
            );
            let links = session.laid.links.clone();
            let by_click: Vec<Action> = links
                .iter()
                .map(|link| what_a_click_on_the_link_asks_for(&mut session, link, width, 60))
                .collect();

            type_command(&mut session, "/open");
            let by_number: Vec<Action> = (1..=links.len())
                .map(|number| type_command(&mut session, &format!("/open {number}")))
                .collect();

            assert_eq!(by_number, by_click, "{text:?} at {width}");
            asked += by_number.len();
        }
    }
    assert!(asked > 30, "{asked} links were asked for");
}

/// A number names the link it was listed beside, whatever the transcript holds by the time it is
/// typed: a reply that has since arrived does not move what it means.
#[test]
fn a_number_names_the_link_the_list_printed_beside_it_and_not_the_latest_reply_now() {
    let (mut session, _) = drawn(vec![reply("first: https://listed.test/one")], 100, 12);
    type_command(&mut session, "/open");
    session
        .transcript
        .push(reply("second: https://later.test/one"));
    let _ = draw(&mut session, 100, 12);
    assert_eq!(
        type_command(&mut session, "/open 1"),
        opens("https://listed.test/one")
    );
}

/// A number with no list to count from, one that is not in the list and words that are not a number
/// open nothing and say why, in the interface's own words and without repeating a link.
#[test]
fn a_number_that_names_no_listed_link_opens_nothing_and_says_why() {
    use bravebot_i18n::t;
    let (mut session, _) = drawn(
        vec![reply("see https://example.test/a and src/b.rs")],
        100,
        12,
    );
    let nothing = |action: &Action| {
        !matches!(
            action,
            Action::OpenUrl(_) | Action::Preview(_) | Action::EditFile(_) | Action::Submit(_)
        )
    };

    let before = session.transcript.len();
    let asked = type_command(&mut session, "/open 1");
    assert!(nothing(&asked), "{asked:?}");
    assert_eq!(
        said_since(&session, before),
        vec![t!(open_list_first).to_string()],
        "before any list"
    );

    type_command(&mut session, "/open");
    // The number as it was read: leading zeros are not part of it.
    for (typed, read) in [("0", 0), ("3", 3), ("99", 99), ("007", 7)] {
        let before = session.transcript.len();
        let asked = type_command(&mut session, &format!("/open {typed}"));
        assert!(nothing(&asked), "{typed}: {asked:?}");
        assert_eq!(
            said_since(&session, before),
            vec![format!(
                "there is no link {read}; the last list had 2 links"
            )],
            "{typed}"
        );
    }
    // `edit` with nothing to open is the one thing left that says what the command takes: any other
    // text is an address or a name, which is opened or refused as such.
    for words in ["edit", "edit   "] {
        let before = session.transcript.len();
        let asked = type_command(&mut session, &format!("/open {words}"));
        assert!(nothing(&asked), "{words}: {asked:?}");
        assert_eq!(
            said_since(&session, before),
            vec![t!(open_takes).to_string()],
            "{words}"
        );
    }
}

/// A list that held none says so by number as well: there is no link 1 where the last list had
/// none, and it is not told to list first, which it did.
#[test]
fn a_number_after_a_list_that_held_no_link_says_the_list_had_none() {
    let (mut session, _) = drawn(vec![reply("only prose here")], 100, 12);
    type_command(&mut session, "/open");
    let before = session.transcript.len();
    type_command(&mut session, "/open 1");
    assert_eq!(
        said_since(&session, before),
        vec!["there is no link 1; the last list had none".to_string()]
    );
}

/// A list ends with the conversation it was a list of: `/clear` takes it, so a number typed in the
/// new conversation is not read against the old one's links.
#[test]
fn a_clear_drops_the_list() {
    use bravebot_i18n::t;
    let (mut session, _) = drawn(vec![reply("see https://example.test/a")], 100, 12);
    type_command(&mut session, "/open");
    session.clear();
    let before = session.transcript.len();
    let asked = type_command(&mut session, "/open 1");
    assert!(!matches!(asked, Action::OpenUrl(_)), "{asked:?}");
    assert_eq!(
        said_since(&session, before),
        vec![t!(open_list_first).to_string()]
    );
}

/// Type `line` into the box while a turn runs and press Enter, as the turn's own loop is given it.
fn type_while_working(session: &mut Session, line: &str) -> Action {
    for c in line.chars() {
        bravebot_tui::app::handle_key_while_working(
            session,
            KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE),
        );
    }
    bravebot_tui::app::handle_key_while_working(
        session,
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
    )
}

/// A file is shown where a click on a path acts and nowhere else. At the plain prompt the click waits
/// for a second press and the command asks to show the file. While a turn runs the path is no target
/// and the command says that a file is shown when nothing is running, and the address beside it
/// opens by number as it opens by click. While a command runs the path is no target, and while
/// something else that is not a turn is working the command waits, as every command does there.
#[test]
fn a_file_is_shown_where_a_click_on_a_path_acts_and_nowhere_else() {
    use bravebot_i18n::t;
    let text = "the fix is in src/main.rs:3 and the guide is at https://example.test/guide";
    let cell = |buffer: &Buffer, needle: &str| find(buffer, needle);

    // At the plain prompt.
    let (mut session, buffer) = drawn(vec![reply(text)], 100, 12);
    let (row, column) = cell(&buffer, "src/main.rs");
    assert!(path_at(&session, row, column).is_some());
    type_command(&mut session, "/open");
    assert_eq!(
        type_command(&mut session, "/open 1"),
        Action::Preview(path("src/main.rs", Some(3)))
    );

    // While a turn runs.
    let (mut session, buffer) = drawn(vec![reply(text)], 100, 12);
    let (row, column) = cell(&buffer, "src/main.rs");
    assert!(matches!(
        type_command(&mut session, "go"),
        Action::Submit(_)
    ));
    assert_eq!(session.status, bravebot_tui::Status::Working);
    assert!(
        path_at(&session, row, column).is_none(),
        "the click acts on a path while a turn runs"
    );
    type_while_working(&mut session, "/open");
    let before = session.transcript.len();
    let asked = type_while_working(&mut session, "/open 1");
    assert!(
        !matches!(asked, Action::Preview(_)),
        "a file was shown while a turn runs: {asked:?}"
    );
    assert_eq!(
        session
            .said_while_working()
            .last()
            .map(|said| said.text.clone()),
        Some(t!(open_not_while_working).to_string())
    );
    assert!(session.queued.is_empty(), "the refusal waited");
    assert_eq!(session.transcript.len(), before + 1);
    let (row, column) = cell(&buffer, "https://example.test/guide");
    assert!(url_at(&session, row, column).is_some());
    assert_eq!(
        type_while_working(&mut session, "/open 2"),
        opens("https://example.test/guide")
    );

    // While a command runs nothing is typed, and the click acts on no path.
    let (mut session, buffer) = drawn(vec![reply(text)], 100, 12);
    let (row, column) = cell(&buffer, "src/main.rs");
    session.status = bravebot_tui::Status::Running;
    assert!(path_at(&session, row, column).is_none());

    // While a summary or an aside is being written, which is working and no turn, every command
    // waits, and the click acts on no path.
    let (mut session, buffer) = drawn(vec![reply(text)], 100, 12);
    let (row, column) = cell(&buffer, "src/main.rs");
    type_command(&mut session, "/open");
    session.status = bravebot_tui::Status::Working;
    assert!(path_at(&session, row, column).is_none());
    let asked = type_while_working(&mut session, "/open 1");
    assert!(!matches!(asked, Action::Preview(_)), "{asked:?}");
    assert_eq!(session.queued.len(), 1, "the command did not wait");
}

/// `edit` and a number do what a click with ctrl held on that path does, in every shape the click
/// tests use and at three widths: the editor is asked for with the path, line and column the click
/// asks for. An address is not a file, and is refused with a note in place of being opened in either
/// program.
#[test]
fn edit_and_a_number_ask_for_what_a_ctrl_click_on_that_path_asks_for_in_every_shape() {
    use bravebot_i18n::t;
    use bravebot_tui::text_links::Target;
    let mut paths = 0;
    let mut addresses = 0;
    for (text, _) in the_shapes_the_link_tests_use() {
        for width in [30u16, 60, 100] {
            let (mut session, _) = drawn(
                vec![Entry::user(format!("a prompt with {GUIDE}")), reply(&text)],
                width,
                60,
            );
            let links = session.laid.links.clone();
            type_command(&mut session, "/open");
            for (at, link) in links.iter().enumerate() {
                let (row, column) = first_cell_of(&session, link, width, 60);
                session.last_click = None;
                let by_number = type_command(&mut session, &format!("/open edit {}", at + 1));
                match link {
                    Target::Path(_) => {
                        paths += 1;
                        assert_eq!(
                            by_number,
                            ctrl_click(&mut session, row, column),
                            "{link:?} in {text:?} at {width}"
                        );
                        assert!(matches!(by_number, Action::EditFile(_)));
                    }
                    Target::Address(_) => {
                        addresses += 1;
                        let before = session.transcript.len();
                        let asked = type_command(&mut session, &format!("/open edit {}", at + 1));
                        assert!(
                            !matches!(
                                asked,
                                Action::OpenUrl(_) | Action::EditFile(_) | Action::Preview(_)
                            ),
                            "{link:?} in {text:?} at {width}: {asked:?}"
                        );
                        assert_eq!(
                            said_since(&session, before),
                            vec![t!(open_edit_an_address).to_string()]
                        );
                    }
                }
            }
        }
    }
    assert!(
        paths > 10 && addresses > 10,
        "{paths} paths, {addresses} addresses"
    );
}

/// The editor takes the terminal, so it is asked for where a click on a path acts: not while a turn
/// is drawing on it, where the command says so and nothing waits, and not while something that is no
/// turn is working, where the line waits as every command does.
#[test]
fn edit_asks_for_no_editor_while_a_turn_is_drawing_on_the_terminal() {
    use bravebot_i18n::t;
    let text = "the fix is in src/main.rs:3 and the guide is at https://example.test/guide";

    let (mut session, _) = drawn(vec![reply(text)], 100, 12);
    assert_eq!(type_command(&mut session, "/open"), Action::Redraw);
    assert_eq!(
        type_command(&mut session, "/open edit 1"),
        Action::EditFile(path("src/main.rs", Some(3)))
    );

    let (mut session, _) = drawn(vec![reply(text)], 100, 12);
    assert!(matches!(
        type_command(&mut session, "go"),
        Action::Submit(_)
    ));
    type_while_working(&mut session, "/open");
    let asked = type_while_working(&mut session, "/open edit 1");
    assert!(
        !matches!(asked, Action::EditFile(_)),
        "an editor was asked for while a turn runs: {asked:?}"
    );
    assert_eq!(
        session
            .said_while_working()
            .last()
            .map(|said| said.text.clone()),
        Some(t!(open_edit_not_while_working).to_string())
    );
    assert!(session.queued.is_empty(), "the refusal waited");

    let (mut session, _) = drawn(vec![reply(text)], 100, 12);
    type_command(&mut session, "/open");
    session.status = bravebot_tui::Status::Working;
    let asked = type_while_working(&mut session, "/open edit 1");
    assert!(!matches!(asked, Action::EditFile(_)), "{asked:?}");
    assert_eq!(session.queued.len(), 1, "the command did not wait");
}

/// What `edit` takes is a number, as a number alone does: with no list and past the last link the
/// command opens nothing and says why, and with nothing after it says what the command takes. Text
/// that is no number is a name to open, as it is without `edit`.
#[test]
fn edit_with_no_listed_link_or_nothing_to_open_opens_nothing_and_says_why() {
    use bravebot_i18n::t;
    let (mut session, _) = drawn(
        vec![reply("see src/a.rs and https://example.test/b")],
        100,
        12,
    );
    let nothing = |action: &Action| {
        !matches!(
            action,
            Action::OpenUrl(_) | Action::Preview(_) | Action::EditFile(_) | Action::Submit(_)
        )
    };

    let before = session.transcript.len();
    let asked = type_command(&mut session, "/open edit 1");
    assert!(nothing(&asked), "{asked:?}");
    assert_eq!(
        said_since(&session, before),
        vec![t!(open_list_first).to_string()]
    );

    type_command(&mut session, "/open");
    for (typed, read) in [("0", 0), ("3", 3), ("012", 12)] {
        let before = session.transcript.len();
        let asked = type_command(&mut session, &format!("/open edit {typed}"));
        assert!(nothing(&asked), "{typed}: {asked:?}");
        assert_eq!(
            said_since(&session, before),
            vec![format!(
                "there is no link {read}; the last list had 2 links"
            )]
        );
    }
    // `edit` with nothing after it says what the command takes. Anything else after it is a name, and
    // a word that is not `edit` in lower case is a name as a whole.
    let before = session.transcript.len();
    let asked = type_command(&mut session, "/open edit");
    assert!(nothing(&asked), "{asked:?}");
    assert_eq!(
        said_since(&session, before),
        vec![t!(open_takes).to_string()]
    );
    for (typed, edited, opened) in [
        ("edit one", Some("one"), None),
        ("edit 1 2", Some("1 2"), None),
        ("edit -1", Some("-1"), None),
        ("edit 1x", Some("1x"), None),
        ("Edit 1", None, Some("Edit 1")),
        ("editx 1", None, Some("editx 1")),
        (
            "edit 99999999999999999999999",
            Some("99999999999999999999999"),
            None,
        ),
    ] {
        let asked = type_command(&mut session, &format!("/open {typed}"));
        match (edited, opened) {
            (Some(name), None) => assert_eq!(asked, Action::EditFile(path(name, None)), "{typed}"),
            (None, Some(name)) => assert_eq!(asked, Action::Preview(path(name, None)), "{typed}"),
            _ => unreachable!(),
        }
    }
    // Spaces between the word and the number are not part of either.
    assert_eq!(
        type_command(&mut session, "/open   edit    1  "),
        Action::EditFile(path("src/a.rs", None))
    );
}

/// The text of a link typed after `/open` asks for what a click on that link asks for, in every shape
/// the click tests use and at three widths: an address for the browser, a path, with the place after
/// it, for the file to be shown, and with `edit` for the editor. The person typing the exact field is
/// the endorsement, so the text is read as the click reads a path and checked as the click checks an
/// address, and nothing is done with it that a click would not do.
#[test]
fn what_is_typed_asks_for_what_a_click_on_that_link_asks_for_in_every_shape_the_link_tests_use() {
    use bravebot_i18n::t;
    use bravebot_tui::text_links::Target;
    let mut typed = 0;
    for (text, drawn_as) in the_shapes_the_link_tests_use() {
        for width in [30u16, 60, 100] {
            let (mut session, _) = drawn(
                vec![Entry::user(format!("a prompt with {GUIDE}")), reply(&text)],
                width,
                60,
            );
            let links = session.laid.links.clone();
            assert_eq!(links.len(), drawn_as.len(), "{text:?} at {width}");
            for (link, drawn) in links.iter().zip(&drawn_as) {
                let by_click = what_a_click_on_the_link_asks_for(&mut session, link, width, 60);
                assert_eq!(
                    type_command(&mut session, &format!("/open {drawn}")),
                    by_click,
                    "{drawn} in {text:?} at {width}"
                );
                typed += 1;

                let (row, column) = first_cell_of(&session, link, width, 60);
                session.last_click = None;
                match link {
                    Target::Path(_) => assert_eq!(
                        type_command(&mut session, &format!("/open edit {drawn}")),
                        ctrl_click(&mut session, row, column),
                        "edit {drawn} in {text:?} at {width}"
                    ),
                    Target::Address(_) => {
                        let before = session.transcript.len();
                        let asked = type_command(&mut session, &format!("/open edit {drawn}"));
                        assert!(matches!(asked, Action::Redraw), "{drawn}: {asked:?}");
                        assert_eq!(
                            said_since(&session, before),
                            vec![t!(open_edit_an_address).to_string()],
                            "{drawn}"
                        );
                    }
                }
            }
        }
    }
    assert!(typed > 30, "{typed} links were typed");
}

/// What is typed is what is opened: the blanks around it are not part of it, and the sentence
/// punctuation that a click leaves off an address is, since the person typed it.
#[test]
fn what_is_typed_is_opened_as_it_was_typed() {
    let (mut session, _) = drawn(vec![reply("nothing here")], 100, 12);
    assert_eq!(
        type_command(&mut session, "/open   https://example.test/a   "),
        opens("https://example.test/a")
    );
    assert_eq!(
        type_command(&mut session, "/open https://example.test/a."),
        opens("https://example.test/a.")
    );
    assert_eq!(
        type_command(&mut session, "/open HTTP://EXAMPLE.TEST/A"),
        opens("HTTP://EXAMPLE.TEST/A")
    );
    assert_eq!(
        type_command(&mut session, "/open src/main.rs:3:5"),
        Action::Preview(bravebot_tui::text_links::PathRef {
            path: "src/main.rs".to_string(),
            line: Some(3),
            column: Some(5),
        })
    );
    // Digits alone are a number, and a name made of digits is typed with its directory.
    assert_eq!(
        type_command(&mut session, "/open ./12"),
        Action::Preview(path("./12", None))
    );
}

/// An address the opener refuses is refused as typed as it is as clicked: a user in front of the host,
/// a character outside printable ASCII, a scheme that is not a page, a blank, and no host. Each says
/// so in the interface's own words and starts nothing, and a refusal says the same of the address
/// that `edit` was given.
#[test]
fn a_typed_address_the_opener_would_refuse_opens_nothing_and_says_so() {
    use bravebot_i18n::t;
    let (mut session, _) = drawn(vec![reply("nothing here")], 100, 12);
    for text in [
        "https://github.com@example.test/login",
        "https://user:secret@example.test/",
        "https://exampl\u{e9}.test/",
        "https://example.test/\u{202e}fdp.exe",
        "https://example.test/\u{200b}",
        "https://example.test/a b",
        "https://evil.test\\.github.com/",
        "file:///etc/passwd",
        "ftp://example.test/",
        "https://",
        "-https://example.test/",
    ] {
        for edit in ["", "edit "] {
            let before = session.transcript.len();
            let asked = type_command(&mut session, &format!("/open {edit}{text}"));
            assert_eq!(asked, Action::Redraw, "{edit}{text:?}: {asked:?}");
            assert_eq!(
                said_since(&session, before),
                vec![t!(open_not_an_address).to_string()],
                "{edit}{text:?}"
            );
        }
    }
}

/// A name typed after `/open` reaches the action a click's path does, unchanged, and that action
/// refuses it in the click's words: a name that leads out, names no file, names a directory or a
/// file outside every directory the session works in is the one answer that the path names no file
/// there, without the name. A file with no extension, or a blank in its name, which could not be
/// clicked, is shown when it is there.
#[test]
fn a_typed_name_is_shown_or_refused_in_the_words_the_click_on_a_path_gets() {
    let project = Project::new();
    std::fs::write(project.root.join("Makefile"), "all:\n\ttrue\n").expect("a file");
    std::fs::write(project.root.join("my notes.txt"), "one\ntwo\n").expect("a file");
    let (mut session, _) = drawn(vec![reply("nothing here")], 100, 12);
    let said = bravebot_i18n::t!(session_path_not_a_file).to_string();

    for name in [
        "NOPE-missing-name.rs",
        "../outside-name.rs",
        "docs.d",
        "/etc/hosts",
        "src",
        "no such file.txt",
        "-1",
        "javascript:alert(1)",
    ] {
        let asked = type_command(&mut session, &format!("/open {name}"));
        let Action::Preview(target) = asked else {
            panic!("{name}: {asked:?}");
        };
        assert_eq!(target.path, name);
        let before = session.transcript.len();
        project.show(&mut session, &target.path, target.line);
        let note = &session.transcript[session.transcript.len() - 1];
        assert_eq!(session.transcript.len(), before + 1, "{name}");
        assert_eq!(note.text, said, "{name}");
        assert!(!note.text.contains(name), "{name}");
    }

    for (typed, lines) in [("Makefile", 2), ("my notes.txt", 2), ("src/main.rs:3", 5)] {
        let asked = type_command(&mut session, &format!("/open {typed}"));
        let Action::Preview(target) = asked else {
            panic!("{typed}: {asked:?}");
        };
        project.show(&mut session, &target.path, target.line);
        let shown = session
            .transcript
            .last()
            .and_then(|entry| entry.shown.as_ref())
            .unwrap_or_else(|| panic!("{typed} showed nothing"));
        assert_eq!(shown.lines, lines, "{typed}");
    }
}

/// While a turn runs an address typed opens, as a click on one does, and a name typed, with or without
/// `edit`, is refused with a note and nothing waits: a file is shown and an editor started where a
/// click on a path acts.
#[test]
fn a_typed_address_opens_while_a_turn_runs_and_a_typed_name_is_refused() {
    use bravebot_i18n::t;
    let (mut session, _) = drawn(vec![reply("nothing here")], 100, 12);
    assert!(matches!(
        type_command(&mut session, "go"),
        Action::Submit(_)
    ));
    assert_eq!(
        type_while_working(&mut session, "/open https://example.test/a"),
        opens("https://example.test/a")
    );
    for (line, said) in [
        ("/open src/main.rs", t!(open_not_while_working).to_string()),
        (
            "/open edit src/main.rs",
            t!(open_edit_not_while_working).to_string(),
        ),
    ] {
        let asked = type_while_working(&mut session, line);
        assert!(
            !matches!(asked, Action::Preview(_) | Action::EditFile(_)),
            "{line}: {asked:?}"
        );
        assert_eq!(
            session
                .said_while_working()
                .last()
                .map(|said| said.text.clone()),
            Some(said),
            "{line}"
        );
    }
    assert!(session.queued.is_empty(), "something waited");
}

/// The address `/pr` and `/issue` set is opened from the keyboard by typing it after `/open`, and it
/// asks for what a click on the panel's row for it asks for.
#[test]
fn the_address_a_panel_row_opens_is_opened_by_typing_it_after_the_open_command() {
    let (mut session, buffer) = with_a_panel(Some(ISSUE), Some(PULL_REQUEST));
    for (needle, address) in [("Pull request", PULL_REQUEST), ("Issue", ISSUE)] {
        let (row, column) = find(&buffer, needle);
        session.last_click = None;
        let by_click = click_with(&mut session, row, column + 1, KeyModifiers::NONE);
        assert_eq!(by_click, opens(address), "{needle}");
        assert_eq!(
            type_command(&mut session, &format!("/open {address}")),
            by_click,
            "{needle}"
        );
    }
}
