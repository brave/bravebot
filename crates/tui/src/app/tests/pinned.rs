//! VIEW-44 and MOUSE-54: the prompt drawn above the view once it has scrolled off, and a click on it.

use super::*;
use crate::select::Pane;
use crate::state::Entry;

/// A reply of forty paragraphs, each naming the turn it answers, so that a row of it says which.
fn long_reply(turn: &str) -> String {
    (1..=40)
        .map(|n| format!("{turn} answer {n}"))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Two finished turns, each a prompt and a reply longer than the pane at 120 by 24, drawn once so the
/// view is at the tail of the second.
fn two_long_turns() -> Session {
    let mut session = Session::new("none");
    for turn in ["first", "second"] {
        session
            .transcript
            .push(Entry::user(format!("the {turn} question")));
        session
            .transcript
            .push(Entry::assistant(long_reply(turn), Vec::new()));
    }
    drawn(&mut session);
    session
}

/// The text of row `row` of `buffer`, without the blanks at its end or the thumb of a scrollbar.
fn row_text(buffer: &ratatui::buffer::Buffer, row: u16) -> String {
    (0..buffer.area.width)
        .map(|column| buffer[(column, row)].symbol())
        .collect::<String>()
        .trim_end_matches(['\u{2590}', ' '])
        .to_string()
}

/// Every row of `buffer`, for the message of a failed assertion.
fn screen(buffer: &ratatui::buffer::Buffer) -> String {
    (0..buffer.area.height)
        .map(|row| row_text(buffer, row))
        .collect::<Vec<_>>()
        .join("\n")
}

/// How many rows of `buffer` hold `needle`.
fn rows_holding(buffer: &ratatui::buffer::Buffer, needle: &str) -> usize {
    (0..buffer.area.height)
        .filter(|&row| row_text(buffer, row).contains(needle))
        .count()
}

/// The cell of the marker of the prompt drawn above the view, which a click on it finds.
pub(super) fn located_on_the_pinned_prompt() -> (Session, (u16, u16)) {
    let session = two_long_turns();
    let row = session
        .laid
        .pinned
        .expect("no prompt was drawn above the view");
    (session, (row.y, row.x + 2))
}

/// Whether a click on the prompt drawn above the view did what it does: the view is held, with the
/// prompt's row at the top of it.
pub(super) fn went_to_the_prompt(session: &Session, _: &Action) -> bool {
    session.scroll > 0
}

/// VIEW-44. Rejects: no prompt drawn above a view whose prompt has scrolled off, the latest prompt
/// drawn above a view whose first row is in an earlier turn, a prompt drawn above the view while the
/// prompt itself is the first row, and the row taken from the transcript instead of given to the
/// prompt. The view is read at the tail of the second turn, scrolled back to the end of the first
/// with the second prompt on the screen below it, and at the first row.
#[test]
fn the_prompt_of_the_turn_at_the_top_of_the_view_is_drawn_above_it() {
    let mut session = two_long_turns();
    let tail = drawn(&mut session);
    assert_eq!(
        row_text(&tail, 0),
        "> the second question",
        "{}",
        screen(&tail)
    );
    assert_eq!(rows_holding(&tail, "the second question"), 1);
    let window = session.laid.window.expect("a transcript was drawn");
    assert_eq!(
        window.area.y, 1,
        "the transcript did not begin under the prompt"
    );
    assert!(
        rows_holding(&tail, "second answer") > 0,
        "{}",
        screen(&tail)
    );

    session.scroll_up(70);
    let earlier = drawn(&mut session);
    assert_eq!(
        row_text(&earlier, 0),
        "> the first question",
        "{}",
        screen(&earlier)
    );
    assert!(row_text(&earlier, 1).is_empty() || row_text(&earlier, 1).contains("first answer"));
    // The later prompt is on the screen below the first turn's last rows, and is not the one above.
    assert_eq!(
        rows_holding(&earlier, "the second question"),
        1,
        "{}",
        screen(&earlier)
    );

    session.scroller_to_first_row();
    let first = drawn(&mut session);
    assert_eq!(session.laid.pinned, None);
    assert_eq!(rows_holding(&first, "the first question"), 1);
    assert_eq!(session.laid.window.map(|window| window.area.y), Some(0));
}

/// VIEW-44. Rejects: what is drawn below the entries from the session's live state counted as rows
/// of the last entry, which leaves a long reply still arriving with no prompt above it. The turn's
/// prompt is the last entry, and the reply is still being written under it.
#[test]
fn a_reply_still_arriving_has_the_prompt_that_asked_for_it_drawn_above_it() {
    let mut session = Session::new("none");
    session.transcript.push(Entry::user("the running question"));
    session.streaming(&long_reply("running"));
    drawn(&mut session);
    let buffer = drawn(&mut session);
    assert_eq!(
        row_text(&buffer, 0),
        "> the running question",
        "{}",
        screen(&buffer)
    );
    assert_eq!(rows_holding(&buffer, "the running question"), 1);
    assert!(
        rows_holding(&buffer, "running answer") > 0,
        "{}",
        screen(&buffer)
    );
}

/// VIEW-44. Rejects: the prompt before a shell command drawn above what the command printed, which
/// reads as though the prompt asked for it.
#[test]
fn what_a_shell_command_printed_has_no_prompt_drawn_above_it() {
    let mut session = Session::new("none");
    session.transcript.push(Entry::user("an earlier question"));
    session
        .transcript
        .push(Entry::assistant("a short answer", Vec::new()));
    session.transcript.push(Entry::shell("seq 1 60"));
    session.transcript.push(Entry::output(
        (1..=60)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join("\n"),
    ));
    let buffer = drawn(&mut session);
    assert_eq!(session.laid.pinned, None);
    assert_eq!(rows_holding(&buffer, "an earlier question"), 0);
}

/// MOUSE-54. Rejects: a click on the prompt drawn above the view that does nothing, that goes to
/// another row than the prompt's, and that leaves the prompt drawn twice. After the click the prompt
/// is the first row of the view, and no prompt is drawn above it.
#[test]
fn a_click_on_the_prompt_drawn_above_the_view_puts_that_prompt_at_the_top() {
    let mut session = two_long_turns();
    session.scroll_up(70);
    drawn(&mut session);
    let row = session
        .laid
        .pinned
        .expect("no prompt was drawn above the view");
    let action = super::popups::click_held(&mut session, (row.y, row.x + 4), KeyModifiers::NONE);
    assert_eq!(action, Action::Redraw);
    let after = drawn(&mut session);
    assert_eq!(session.laid.pinned, None);
    assert_eq!(row_text(&after, 0), "> the first question");
    assert_eq!(rows_holding(&after, "the first question"), 1);
}

/// VIEW-44. Rejects: the row of the prompt drawn above the view counted as a row of the transcript's
/// pane, where a press begins a selection of the screen's cells and a copy takes the prompt's words
/// as though they were the row of the transcript under it. A drag along the row selects nothing, and
/// a drag from the first row under it copies that row's words and nothing of the prompt.
#[test]
fn a_selection_never_begins_on_the_prompt_drawn_above_the_view_or_takes_its_words() {
    let mut session = two_long_turns();
    let buffer = drawn(&mut session);
    let row = session
        .laid
        .pinned
        .expect("no prompt was drawn above the view");
    let pane = session
        .laid
        .panes
        .iter()
        .find(|(pane, _)| *pane == Pane::Transcript)
        .expect("the transcript's pane is recorded")
        .1;
    assert_eq!(pane.y, row.bottom());

    swept(&mut session, (row.y, 2), (row.y, 20));
    assert!(
        session.selection.is_none(),
        "a drag on the prompt's row selected {:?}",
        session.selection
    );
    assert_eq!(
        let_go(&mut session, (120, 24), row.y, 20),
        Vec::<String>::new()
    );

    // The first row under the prompt that has words on it.
    let under = (row.bottom()..pane.bottom())
        .find(|&at| !row_text(&buffer, at).is_empty())
        .expect("a row of words under the prompt");
    let end = pane.right() - 1;
    swept(&mut session, (under, 0), (under, end));
    let handed = let_go(&mut session, (120, 24), under, end);
    let [copied] = handed.as_slice() else {
        panic!(
            "the clipboard was handed {handed:?} for {:?} at {:?}\n{}",
            session.selection,
            session.laid.window,
            screen(&buffer)
        );
    };
    assert_eq!(copied.trim(), row_text(&buffer, under).trim());
    assert!(!copied.contains("question"), "{copied}");
}

/// VIEW-44 and VIEW-39. Rejects: a track that runs beside the prompt drawn above the view, which
/// stands for a row the transcript does not scroll in, and a thumb that does not reach the end of the
/// track while the view is at the tail.
#[test]
fn the_scrollbar_runs_beside_the_rows_under_the_prompt_and_reaches_the_end_at_the_tail() {
    let mut session = two_long_turns();
    let buffer = drawn(&mut session);
    let row = session
        .laid
        .pinned
        .expect("no prompt was drawn above the view");
    let bar = session.laid.bar.expect("no scrollbar was drawn");
    let window = session.laid.window.expect("a transcript was drawn");
    assert_eq!(
        (bar.track.y, bar.track.height),
        (window.area.y, window.area.height)
    );
    assert_eq!(bar.track.y, row.bottom());
    assert_eq!(buffer[(bar.track.x, row.y)].symbol(), " ");
    let (first, length) = bar.thumb();
    assert_eq!(first + length, bar.track.height);
}

/// VIEW-44 and SCROLL-1. Rejects: the scroller drawing no prompt above its view, which moves the words
/// at the top of the view up a row when the scroller opens over a view that had one, and a prompt
/// still drawn above the view once `{` has put that prompt at the top of it.
#[test]
fn the_scroller_draws_the_prompt_above_its_view_and_opening_it_moves_no_words() {
    let mut session = two_long_turns();
    session.scroll_up(70);
    let at_rest = drawn(&mut session);
    let under = row_text(&at_rest, 1);
    assert_eq!(row_text(&at_rest, 0), "> the first question");

    handle_key(
        &mut session,
        KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
    );
    assert!(session.scrolling(), "the scroller did not open");
    let scroller = drawn(&mut session);
    assert_eq!(row_text(&scroller, 0), "> the first question");
    assert_eq!(row_text(&scroller, 1), under);

    handle_key(&mut session, key(KeyCode::Char('{')));
    let at_the_prompt = drawn(&mut session);
    assert_eq!(session.laid.pinned, None);
    assert_eq!(row_text(&at_the_prompt, 0), "> the first question");
    assert_eq!(rows_holding(&at_the_prompt, "the first question"), 1);
}

/// VIEW-44. Rejects: a prompt drawn above the view in another ink than the prompt's own row, which
/// reads as a line of the interface and not as the prompt. The marker and the first word are read
/// where the prompt is drawn in the transcript and where it is drawn above the view.
#[test]
fn the_prompt_above_the_view_is_drawn_in_the_ink_of_the_prompt() {
    let _held = crate::theme::exclusive();
    crate::theme::apply_brave();
    let mut session = two_long_turns();
    let pinned = drawn(&mut session);
    let row = session
        .laid
        .pinned
        .expect("no prompt was drawn above the view");
    session.scroller_to_first_row();
    let own = drawn(&mut session);
    let (at, column) = find_cell(&own, "> the first question");
    for offset in [0, 2, 6] {
        assert_eq!(
            own[(column + offset, at)].style(),
            pinned[(row.x + offset, row.y)].style(),
            "column {offset}"
        );
    }
}

/// VIEW-44 and SCROLL-3. Rejects: the scroller counting the row of the prompt drawn above its view as
/// a row of the view, which leaves the transcript's last row under the bottom of the screen at `G` and
/// has a page move a row further than it showed, so that a row is never on the screen. The view is
/// paged from the first row to the last, a prompt drawn above it on the way, and each page begins no
/// further on than the row after the last one shown. At the last row the session holds the view the
/// frame drew, and not the one a row higher that the frame before it implied.
#[test]
fn the_scroller_pages_by_the_rows_under_the_prompt_and_reaches_the_last_row() {
    let mut session = two_long_turns();
    handle_key(
        &mut session,
        KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
    );
    handle_key(&mut session, key(KeyCode::Char('g')));
    drawn(&mut session);
    let mut pinned = 0;
    loop {
        let before = session.laid.window.expect("a transcript was drawn");
        if before.top + before.area.height >= session.laid.rows {
            break;
        }
        handle_key(&mut session, key(KeyCode::Char('f')));
        drawn(&mut session);
        let after = session.laid.window.expect("a transcript was drawn");
        pinned += usize::from(session.laid.pinned.is_some());
        assert!(after.top > before.top, "the page did not move");
        assert!(
            after.top <= before.top + before.area.height,
            "rows {} to {} were never shown",
            before.top + before.area.height,
            after.top
        );
    }
    assert!(pinned > 0, "no page had a prompt drawn above it");

    handle_key(&mut session, key(KeyCode::Char('g')));
    drawn(&mut session);
    handle_key(&mut session, key(KeyCode::Char('G')));
    drawn(&mut session);
    assert!(
        session.laid.pinned.is_some(),
        "the last page drew no prompt"
    );
    let window = session.laid.window.expect("a transcript was drawn");
    assert_eq!(window.top + window.area.height, session.laid.rows);
    // The view the session holds is the one drawn, so a press is placed on the rows on the screen.
    assert_eq!(session.rows_below(), 0);
    assert_eq!(session.top_row(), window.top);
}

/// Whether the frame just drawn put no prompt above the view: none recorded, the transcript's window
/// beginning at the first row of its pane, and no row of the screen holding `prompt`.
fn nothing_pinned(
    session: &Session,
    buffer: &ratatui::buffer::Buffer,
    pane_top: u16,
    prompt: &str,
) {
    let window = session.laid.window.expect("a transcript was drawn");
    assert!(
        session.laid.rows > window.area.height,
        "the view fits its pane, so nothing could be pinned:\n{}",
        screen(buffer)
    );
    assert_eq!(session.laid.pinned, None, "{}", screen(buffer));
    assert_eq!(window.area.y, pane_top, "{}", screen(buffer));
    assert_eq!(rows_holding(buffer, prompt), 0, "{}", screen(buffer));
}

/// VIEW-44. Rejects: a prompt drawn above a delegate's view, at rest or in the scroller, and above a
/// request `/request` opened, none of which holds a prompt the person typed. Each view is longer than
/// its pane. The session's transcript is a prompt and entries that are not, so a delegate's view read
/// against the session's entries finds that prompt above whatever row is at its top. A request's
/// lines are drawn with no entries, so its part holds that no entry is found above a row of one.
#[test]
fn no_prompt_is_drawn_above_a_delegates_view_or_a_request() {
    use bravebot_agent::report::{Activity, DelegateId, Delegation};
    let mut session = Session::new("none");
    session
        .transcript
        .push(Entry::user("the session's question"));
    for n in 0..80 {
        session
            .transcript
            .push(Entry::assistant(format!("a short answer {n}"), Vec::new()));
    }
    let id = DelegateId::nth(1);
    session.delegate_started(Delegation {
        stop: Default::default(),
        id,
        kind: "reader".to_string(),
        task: "find the parser".to_string(),
    });
    session.reporting_for(Some(id));
    for n in 0..40 {
        let file = format!("file{n}.rs");
        session.start_activity(Activity::running("Read", &file));
        session.finish_activity(Activity::running("Read", &file).done("1 line"));
    }
    session.reporting_for(None);
    assert!(session.watch(), "nothing to watch");
    assert!(session.watching_a_delegate(), "no delegate's view opened");

    // Under the two rows that name the delegate and what it was asked.
    drawn(&mut session);
    let at_rest = drawn(&mut session);
    nothing_pinned(&session, &at_rest, 2, "the session's question");

    session.open_scroller();
    drawn(&mut session);
    let scroller = drawn(&mut session);
    nothing_pinned(&session, &scroller, 0, "the session's question");

    let mut session = two_long_turns();
    session.set_last_request(bravebot_agent::request_view::RequestView {
        model: "m".to_string(),
        spans: (0..40)
            .map(|n| bravebot_agent::request_view::Span {
                role: "user",
                provenance: bravebot_agent::request_view::Provenance::Typed,
                text: format!("span {n} of the request"),
            })
            .collect(),
        tools: vec!["read_file".to_string()],
        tools_bytes: 0,
    });
    session.show_request();
    drawn(&mut session);
    let request = drawn(&mut session);
    nothing_pinned(&session, &request, 0, "question");
}
