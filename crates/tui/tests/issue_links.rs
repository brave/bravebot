//! `#<number>` in an assistant's words, drawn as a link to the issue or pull request on the
//! repository the checkout's `origin` names (VIEW-32).
//!
//! Asserted against the drawn buffer and not the lines the renderer returns: where a number lands
//! is decided by the widget that wraps the lines, so the cells it was drawn in are the only place
//! to ask.

use bravebot_agent::report::{Reach, Shown};
use bravebot_session::sessions::Aside;
use bravebot_tui::hyperlink;
use bravebot_tui::issue_link::{MARK, Repository};
use bravebot_tui::render;
use bravebot_tui::select::{self, Selection};
use bravebot_tui::state::{Entry, Session};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

const ISSUES: &str = "https://github.com/brave/bravebot/issues";

fn brave() -> Option<Repository> {
    Repository::from_remote("git@github.com:brave/bravebot.git")
}

fn session_in_brave() -> Session {
    let mut session = Session::new("kernel-enforced");
    session.adopt_repository(brave());
    session
}

fn drawn(session: &Session, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|frame| {
            render::draw(frame, session);
        })
        .expect("draw");
    terminal.backend().buffer().clone()
}

/// The address a cell is a link to, where it is one.
fn address_of(buffer: &Buffer, x: u16, y: u16) -> Option<String> {
    let symbol = buffer[(x, y)].symbol();
    let rest = symbol.strip_prefix("\x1b]8;;")?;
    rest.split_once("\x1b\\")
        .map(|(address, _)| address.to_string())
}

/// Each run of linked cells on a row as the address it opens and the text it shows, with the row
/// it is on.
fn links(buffer: &Buffer) -> Vec<(u16, String, String)> {
    let area = buffer.area;
    let mut found: Vec<(u16, String, String)> = Vec::new();
    for y in 0..area.height {
        let mut previous: Option<u16> = None;
        for x in 0..area.width {
            let Some(address) = address_of(buffer, x, y) else {
                continue;
            };
            let shown = hyperlink::shown(buffer[(x, y)].symbol());
            match (found.last_mut(), previous) {
                (Some((row, held, text)), Some(before))
                    if *row == y && *held == address && before + 1 == x =>
                {
                    text.push_str(shown);
                }
                _ => found.push((y, address, shown.to_string())),
            }
            previous = Some(x);
        }
    }
    found
}

fn issue(number: &str) -> String {
    format!("{ISSUES}/{number}")
}

fn rows(buffer: &Buffer) -> Vec<String> {
    let area = buffer.area;
    (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| hyperlink::shown(buffer[(x, y)].symbol()))
                .collect()
        })
        .collect()
}

fn underlined_cells(buffer: &Buffer) -> usize {
    buffer
        .content()
        .iter()
        .filter(|cell| cell.style().add_modifier.contains(MARK))
        .count()
}

/// The sentence a reply is drawn from, long enough that a narrow screen wraps it.
const REPLY: &str = "That change was made to fix the long standing problem described in #2879 \
                     and it also settles #12 for the people who reported it.";

#[test]
fn a_number_in_a_reply_links_to_the_issue_on_the_origin_repository() {
    let mut session = session_in_brave();
    session.narrate("fixed in #2879");
    let buffer = drawn(&session, 80, 24);

    let found = links(&buffer);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(
        (found[0].1.as_str(), found[0].2.as_str()),
        (issue("2879").as_str(), "#2879")
    );
    assert!(
        rows(&buffer)
            .iter()
            .any(|row| row.contains("fixed in #2879")),
        "linking changed what is drawn"
    );
}

/// A number the widget wrapped onto another row is linked where it was drawn. Counting columns
/// along the unwrapped line, which is what the text knows, puts the link on the wrong cells.
#[test]
fn a_number_on_a_wrapped_row_is_linked_where_it_is_drawn_whichever_way_the_transcript_is_drawn() {
    for scrolling in [false, true] {
        let mut session = session_in_brave();
        session.narrate(REPLY);
        if scrolling {
            session.open_scroller();
        }
        let buffer = drawn(&session, 30, 24);

        let found = links(&buffer);
        let shown: Vec<(&str, &str)> = found
            .iter()
            .map(|(_, address, text)| (address.as_str(), text.as_str()))
            .collect();
        assert_eq!(
            shown,
            [
                (issue("2879").as_str(), "#2879"),
                (issue("12").as_str(), "#12")
            ],
            "scrolling: {scrolling}\n{}",
            rows(&buffer).join("\n")
        );
        let drawn_rows = rows(&buffer);
        for (row, _, text) in &found {
            assert!(
                drawn_rows[*row as usize].contains(text.as_str()),
                "the link is not on the row its number was drawn on: {drawn_rows:#?}"
            );
        }
        assert!(
            found.iter().any(|(row, ..)| *row > 1),
            "the number did not wrap, so the case is not exercised:\n{}",
            drawn_rows.join("\n")
        );
    }
}

#[test]
fn a_number_in_a_reply_still_arriving_is_linked() {
    let mut session = session_in_brave();
    session.streaming("so far it looks like #2879");
    let found = links(&drawn(&session, 80, 24));
    assert_eq!(
        found
            .iter()
            .map(|(_, a, t)| (a.as_str(), t.as_str()))
            .collect::<Vec<_>>(),
        [(issue("2879").as_str(), "#2879")]
    );
}

#[test]
fn a_number_in_a_table_cell_is_linked() {
    let mut session = session_in_brave();
    session.narrate("| issue | state |\n| --- | --- |\n| #2879 | open |\n| `#12` | closed |");
    let found = links(&drawn(&session, 80, 24));
    assert_eq!(
        found
            .iter()
            .map(|(_, a, t)| (a.as_str(), t.as_str()))
            .collect::<Vec<_>>(),
        [(issue("2879").as_str(), "#2879")]
    );
}

#[test]
fn a_number_in_an_asides_answer_is_linked() {
    let mut session = session_in_brave();
    session.asked_aside(Aside {
        question: "what was that about?".to_string(),
        answer: Some("it was filed as #2879".to_string()),
        kept: true,
    });
    let found = links(&drawn(&session, 80, 24));
    assert_eq!(
        found
            .iter()
            .map(|(_, a, t)| (a.as_str(), t.as_str()))
            .collect::<Vec<_>>(),
        [(issue("2879").as_str(), "#2879")]
    );
}

/// The same characters in code, in a word, or with no origin to name are text, and so is the
/// screen with no origin: not underlined either, since an underline nothing opens is a broken link.
#[test]
fn a_hash_that_is_not_a_reference_is_drawn_as_text() {
    let mut session = session_in_brave();
    session.narrate("run `#2879` for C#1 or page#12 or &#39; or #12abc or #0");
    let buffer = drawn(&session, 80, 24);
    assert_eq!(links(&buffer), []);
    assert_eq!(underlined_cells(&buffer), 0);
}

#[test]
fn without_a_github_origin_a_number_is_plain_text() {
    let mut session = Session::new("kernel-enforced");
    session.narrate("fixed in #2879");
    let buffer = drawn(&session, 80, 24);
    assert_eq!(links(&buffer), []);
    assert_eq!(underlined_cells(&buffer), 0, "a number was underlined");
    assert!(
        rows(&buffer)
            .iter()
            .any(|row| row.contains("fixed in #2879"))
    );

    // An origin that is not GitHub's reads the same as none.
    let mut elsewhere = Session::new("kernel-enforced");
    elsewhere.adopt_repository(Repository::from_remote(
        "https://gitlab.com/brave/bravebot.git",
    ));
    elsewhere.narrate("fixed in #2879");
    let buffer = drawn(&elsewhere, 80, 24);
    assert_eq!(links(&buffer), []);
    assert_eq!(underlined_cells(&buffer), 0);
}

/// A session that moves to a directory with no GitHub origin stops linking at the next frame,
/// whether the numbers are in the transcript or in the reply still arriving, and whether or not
/// the lines were kept from the frame before.
#[test]
fn a_session_that_leaves_its_repository_stops_linking() {
    let mut session = session_in_brave();
    session.narrate("fixed in #2879");
    session.streaming("and still arriving: #12");
    let found = links(&drawn(&session, 80, 24));
    assert_eq!(
        found.iter().map(|(_, a, _)| a.as_str()).collect::<Vec<_>>(),
        [issue("2879"), issue("12")]
    );

    session.adopt_repository(None);
    let buffer = drawn(&session, 80, 24);
    assert_eq!(links(&buffer), []);
    assert_eq!(underlined_cells(&buffer), 0, "the kept lines were reused");

    session.adopt_repository(Repository::from_remote(
        "https://github.com/netzenbot/fork.git",
    ));
    let found = links(&drawn(&session, 80, 24));
    assert_eq!(
        found.iter().map(|(_, a, _)| a.as_str()).collect::<Vec<_>>(),
        [
            "https://github.com/netzenbot/fork/issues/2879",
            "https://github.com/netzenbot/fork/issues/12"
        ]
    );
}

/// Content the planner may not read is drawn by its own path, which no link may reach: its
/// words are somebody else's, and a person is deciding what to make of them.
#[test]
fn a_number_in_quarantined_content_is_not_linked() {
    let mut session = session_in_brave();
    let mut entry = Entry::system("read notes.md");
    entry.shown = Some(Shown {
        origin: "notes.md #7".to_string(),
        reach: Reach::NotThePlanner,
        label: "(U,priv)".to_string(),
        preview: vec!["see #2879 and #12 for the story".to_string()],
        lines: 1,
    });
    session.transcript.push(entry);
    let buffer = drawn(&session, 80, 24);

    assert!(
        rows(&buffer)
            .iter()
            .any(|row| row.contains("see #2879 and #12")),
        "the content was not drawn"
    );
    assert_eq!(links(&buffer), []);
}

/// Only an assistant's words go through the matcher, so what a person typed and what a program
/// printed are left as they were.
#[test]
fn a_number_in_words_that_are_not_the_assistants_is_not_linked() {
    let mut session = session_in_brave();
    session.transcript.push(Entry::user("please look at #2879"));
    session.transcript.push(Entry::system("opened #12"));
    let buffer = drawn(&session, 80, 24);
    assert!(
        rows(&buffer)
            .iter()
            .any(|row| row.contains("look at #2879"))
    );
    assert_eq!(links(&buffer), []);
}

/// A linked cell holds the escape sequences around its character, and pasting those would put
/// invisible bytes into whatever the text is pasted into.
#[test]
fn a_selection_over_a_linked_number_copies_the_text() {
    let mut session = session_in_brave();
    session.narrate("fixed in #2879");
    let buffer = drawn(&session, 80, 24);
    let (row, ..) = links(&buffer).remove(0);
    let line = rows(&buffer)[row as usize].clone();

    let mut selection = Selection::started_at(row, 0);
    selection.extend_to(row, 79);
    let copied = select::text(&buffer, &selection);
    assert_eq!(copied, line.trim_end());
    assert!(copied.contains("fixed in #2879"), "{copied:?}");
    assert!(!copied.contains('\u{1b}'), "{copied:?}");
}
