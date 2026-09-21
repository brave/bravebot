//! What the terminal actually draws, and what an export of it holds.
//!
//! A failed turn is the case these cover. The reason it failed is the one line the person needs,
//! and it has to survive an expanded audit, a narrow window, a resize, and being written out to a
//! file. Every assertion here reads the frame or the export rather than the session, because the
//! session holding a reason nobody can see is the failure being tested for.

use bravebot_session::audit::TrailLine;
use bravebot_tui::render;
use bravebot_tui::state::Session;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

#[test]
fn a_default_terminal_size_renders_content() {
    let session = Session::new("kernel-enforced");
    let text = drawn(&session, 80, 24);
    assert!(
        text.contains("bravebot"),
        "status bar missing: {:?}",
        &text[..200.min(text.len())]
    );
    assert!(text.contains("Ask a question"), "hint missing");
}

/// The whole frame as text, one line per row.
///
/// Row by row rather than as one run of cells, so an assertion about a line being on the screen is
/// not satisfied by two halves of it meeting at a row boundary.
fn drawn(session: &Session, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|f| {
            render::draw(f, session);
        })
        .expect("draw");
    let cells: Vec<String> = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol().to_string())
        .collect();
    cells
        .chunks(width as usize)
        .map(|row| row.concat())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The same, with the rows joined by nothing, for asking whether a word is anywhere on the screen
/// at all.
fn anywhere(session: &Session, width: u16, height: u16) -> String {
    drawn(session, width, height).replace('\n', " ")
}

/// A turn that ran and then stopped, with `events` lines of audit under it.
///
/// The trail is put on the entry the way the interface puts it there: the failure is recorded
/// first and the trail lands on it afterwards.
fn a_failed_turn(reason: &str, events: usize) -> Session {
    let mut session = Session::new("none");
    session.paste("check the tests pass");
    session.submit().expect("the line was taken");
    stop_the_turn(&mut session, reason);
    if let Some(last) = session.transcript.last_mut() {
        last.trail = (0..events)
            .map(|n| TrailLine {
                text: format!("ok      file_read.path [routing] (T,pub) round {n}"),
                blocked: false,
            })
            .collect();
    }
    session
}

/// Record a failure on the turn in flight.
///
/// One place, so that what a failed turn is told is stated once for every test here. The ending is
/// a service that answered 503 three times, which is what the reasons here describe: the words are
/// the caller's so that a long one can be drawn, and the ending is what a record would keep.
fn stop_the_turn(session: &mut Session, reason: &str) {
    session.fail(
        reason,
        bravebot_agent::Ending::Failed(
            bravebot_agent::Diagnosis::of(bravebot_agent::Category::Unavailable)
                .with_status(503)
                .after(3),
        ),
    );
}

/// The size a person actually runs at, with the audit up. 923 events is what one real turn left
/// behind, and it is enough to fill the transcript several times over: the reason has to be
/// somewhere that does not scroll away under them.
#[test]
fn a_failed_turn_says_why_with_the_audit_expanded() {
    let mut session = a_failed_turn("the service answered HTTP 503", 923);
    session.show_trail = true;

    for (width, height) in [(150u16, 84u16), (150, 48), (90, 24), (46, 14)] {
        let text = anywhere(&session, width, height);
        assert!(
            text.contains("503"),
            "at {width}x{height} the reason was nowhere on the screen:\n{}",
            drawn(&session, width, height)
        );
    }
}

/// The same with the audit down, which is the default. Nothing about showing a reason may depend
/// on a mode the person has not turned on.
#[test]
fn a_failed_turn_says_why_with_the_audit_collapsed() {
    let session = a_failed_turn("the service answered HTTP 503", 923);
    assert!(!session.show_trail, "the audit is down by default");

    for (width, height) in [(150u16, 84u16), (150, 48), (90, 24), (46, 14)] {
        let text = anywhere(&session, width, height);
        assert!(
            text.contains("503"),
            "at {width}x{height} the reason was nowhere on the screen:\n{}",
            drawn(&session, width, height)
        );
    }
}

/// A reason longer than the window is wrapped or shortened, never dropped. The first words are the
/// ones that say what happened, so they are the ones that have to be there.
#[test]
fn a_long_failure_reason_is_not_lost() {
    let long = format!(
        "the service answered HTTP 503 {}",
        "and would not say more ".repeat(20)
    );
    let session = a_failed_turn(&long, 923);

    for (width, height) in [(150u16, 48u16), (90, 24), (46, 14)] {
        let text = anywhere(&session, width, height);
        assert!(
            text.contains("503"),
            "at {width}x{height} a long reason was dropped entirely:\n{}",
            drawn(&session, width, height)
        );
    }
}

/// A reason is drawn as words. Whatever bytes reach it, nothing on the screen may be an escape
/// sequence the terminal acts on.
#[test]
fn a_failure_reason_carrying_terminal_controls_is_drawn_inert() {
    let session = a_failed_turn(
        "the service answered HTTP 503 \u{1b}[2J\u{7}\r\n\u{1b}]0;x\u{7}",
        4,
    );

    let text = drawn(&session, 150, 48);
    assert!(text.contains("503"), "the reason was dropped:\n{text}");
    for control in ['\u{1b}', '\u{7}', '\r'] {
        assert!(
            !text.contains(control),
            "a control character reached the screen: {:?}",
            control
        );
    }
}

/// Resizing redraws from the session, so the reason survives the window changing under it.
#[test]
fn a_failure_reason_survives_a_resize() {
    let mut session = a_failed_turn("the service answered HTTP 503", 923);
    session.show_trail = true;

    let mut terminal = Terminal::new(TestBackend::new(150, 48)).expect("terminal");
    terminal
        .draw(|f| {
            render::draw(f, &session);
        })
        .expect("draw");
    terminal.backend_mut().resize(90, 24);
    terminal
        .draw(|f| {
            render::draw(f, &session);
        })
        .expect("redraw");

    let text: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(
        text.contains("503"),
        "the reason went with the resize:\n{text}"
    );
}

/// Somebody reading back through what happened is not dragged to the end of the transcript by a
/// turn ending. The reason is still on the screen while they read, because it is drawn where the
/// transcript's offset does not reach.
#[test]
fn reading_older_scrollback_is_not_interrupted_by_a_failure() {
    let mut session = a_failed_turn("the service answered HTTP 503", 923);
    session.show_trail = true;
    session.scroll = 400;

    let text = anywhere(&session, 150, 48);
    assert_eq!(session.scroll, 400, "drawing moved the reader");
    assert!(
        text.contains("503"),
        "the reason was only visible at the bottom:\n{}",
        drawn(&session, 150, 48)
    );
}

/// A turn nobody stopped is not reported as one. A person who did not touch the keyboard reading
/// "stopped" has been told they did something they did not do, and the word is the only thing
/// distinguishing the two endings.
#[test]
fn a_failed_turn_is_called_failed_rather_than_stopped() {
    let failed = a_failed_turn("the service answered HTTP 503", 4);
    let words = anywhere(&failed, 150, 48);
    assert!(
        words.contains("failed"),
        "a failed turn was not called one:\n{}",
        drawn(&failed, 150, 48)
    );
    assert!(
        !words.contains("turn 1 stopped"),
        "a failure was reported as somebody stopping the turn:\n{}",
        drawn(&failed, 150, 48)
    );
}

/// The row reporting the turn, which is where its figures would be drawn.
///
/// One row rather than the whole screen: a cost drawn anywhere else is a different claim, and the
/// screen run together would not say which row a figure was on. Exactly one row reports the turn,
/// so a second would mean the row under test is not the one being read.
fn the_row_reporting_the_turn(session: &Session) -> String {
    let screen = drawn(session, 150, 48);
    let rows: Vec<&str> = screen
        .lines()
        .filter(|row| row.contains("turn 1"))
        .collect();
    assert_eq!(rows.len(), 1, "one row reports the turn:\n{screen}");
    rows[0].trim().to_string()
}

/// A turn in flight that has already spent tokens.
///
/// The spend is what makes the assertions mean anything: a turn that cost nothing would draw no
/// figure however the row was written.
fn a_turn_that_has_spent(session: &mut Session) {
    session.paste("check the tests pass");
    session.submit().expect("the line was taken");
    session.progressed(bravebot_agent::Spent {
        tokens: 4_200,
        ..Default::default()
    });
}

/// The figures on the row are the price of an answer, and a turn that failed produced none. A
/// number beside it reads as what the answer cost, which is a question about a turn that ended
/// without one.
#[test]
fn a_failed_turn_reports_no_cost_and_no_duration() {
    let mut session = Session::new("none");
    a_turn_that_has_spent(&mut session);
    stop_the_turn(&mut session, "the service answered HTTP 503");

    assert_eq!(
        session.finished.expect("a finished turn").tokens,
        4_200,
        "the fixture spent nothing, so a row carrying no figure would prove nothing"
    );
    assert_eq!(
        session.tokens, 4_200,
        "the session stopped counting what the abandoned turn spent"
    );
    let row = the_row_reporting_the_turn(&session);
    assert!(row.contains("failed"), "not the row that reports it: {row}");
    assert!(
        !row.contains("4.2k"),
        "what an abandoned turn spent was drawn as what it cost: {row}"
    );
    assert!(
        !row.contains('('),
        "a turn that produced no answer was priced: {row}"
    );
}

/// Cancelling buys no answer either, so the row says as little about cost as a failure's does.
/// Stated separately because nothing about the two endings is shared beyond the row they use.
#[test]
fn a_cancelled_turn_reports_no_cost_and_no_duration() {
    let mut session = Session::new("none");
    a_turn_that_has_spent(&mut session);
    session.stopped(Some(1));

    assert_eq!(
        session.finished.expect("a finished turn").tokens,
        4_200,
        "the fixture spent nothing, so a row carrying no figure would prove nothing"
    );
    assert_eq!(
        session.tokens, 4_200,
        "the session stopped counting what the abandoned turn spent"
    );
    let row = the_row_reporting_the_turn(&session);
    assert!(
        row.contains("cancelled"),
        "not the row that reports it: {row}"
    );
    assert!(
        !row.contains("4.2k"),
        "what an abandoned turn spent was drawn as what it cost: {row}"
    );
    assert!(
        !row.contains('('),
        "a turn that produced no answer was priced: {row}"
    );
}

/// A turn that succeeds after one failed reports its own outcome. The indicator describes the
/// last turn, not the worst one.
#[test]
fn a_turn_that_succeeds_after_a_failure_says_so() {
    let mut session = a_failed_turn("the service answered HTTP 503", 4);
    session.paste("try that again");
    session.submit().expect("the line was taken");
    session.complete("done", Vec::new(), 1_200);

    let words = anywhere(&session, 150, 48);
    assert!(
        words.contains("turn 2 done"),
        "the second turn was not reported as finished:\n{}",
        drawn(&session, 150, 48)
    );
    assert!(
        !words.contains("turn 2 failed"),
        "a turn that succeeded was reported as failed:\n{}",
        drawn(&session, 150, 48)
    );
}

/// An export is what a person keeps, so it holds what the screen held. A failure that is only on
/// the screen is one nobody can send to anybody.
#[test]
fn an_export_of_a_failed_turn_says_why_it_failed() {
    let session = a_failed_turn("the service answered HTTP 503", 4);

    let markdown = render::as_markdown(&session, "a session");
    assert!(
        markdown.contains("503"),
        "the export dropped the reason:\n{markdown}"
    );
    assert_eq!(
        markdown.matches("503").count(),
        1,
        "the reason was written more than once:\n{markdown}"
    );
    assert!(
        markdown.contains("check the tests pass"),
        "the prompt that failed was not in the export:\n{markdown}"
    );
}

/// A session where one turn failed between two that did not keeps each one's own ending, in
/// order, once each.
#[test]
fn an_export_attaches_each_reason_to_the_turn_that_had_it() {
    let mut session = Session::new("none");
    session.paste("first");
    session.submit().expect("taken");
    session.complete("the first answer", Vec::new(), 100);
    session.paste("second");
    session.submit().expect("taken");
    stop_the_turn(&mut session, "the service answered HTTP 503");
    session.paste("third");
    session.submit().expect("taken");
    session.complete("the third answer", Vec::new(), 100);

    let markdown = render::as_markdown(&session, "a session");
    let at_reason = markdown.find("503").expect("the reason was dropped");
    let at_second = markdown.find("second").expect("the prompt was dropped");
    let at_third = markdown.find("third").expect("the prompt was dropped");
    assert!(
        at_second < at_reason && at_reason < at_third,
        "the reason was not attached to the turn that failed:\n{markdown}"
    );
    assert_eq!(
        markdown.matches("503").count(),
        1,
        "the reason appeared more than once:\n{markdown}"
    );
}

/// The other ending an export has to hold. A turn stopped once it had said something keeps both
/// the prompt and the stop, and an export that dropped the stop is a prompt with no answer and
/// nothing saying why.
#[test]
fn an_export_of_a_cancelled_turn_says_it_was_cancelled() {
    let mut session = Session::new("none");
    session.paste("check the tests pass");
    let prompt = session.submit().expect("the line was taken");
    session.narrate("reading the tests first");
    session.stopped(Some(2));
    session.restore(&prompt);

    let markdown = render::as_markdown(&session, "a session");
    assert!(
        markdown.contains("Cancelled"),
        "the export does not say the turn was stopped:\n{markdown}"
    );
    assert!(
        !markdown.contains("Failed"),
        "a turn somebody stopped was exported as a failure:\n{markdown}"
    );
    assert!(
        markdown.contains("check the tests pass"),
        "the prompt that was stopped was not in the export:\n{markdown}"
    );
}

#[test]
fn cancellation_has_its_own_status_even_when_the_prompt_returns() {
    for attempts in [None, Some(0), Some(1)] {
        let mut session = Session::new("test");
        session.paste("work");
        let prompt = session.submit().unwrap();
        session.stopped(attempts);
        session.restore(&prompt);
        assert_eq!(session.input(), prompt);
        assert_eq!(
            session.finished.unwrap().ending,
            bravebot_agent::Ending::Stopped { attempts }
        );
        let text = drawn(&session, 80, 24);
        assert!(text.contains("turn 1 cancelled"), "{text}");
        assert!(!text.contains("turn 1 done"), "{text}");
        assert!(!text.contains("turn 1 failed"), "{text}");
    }
}
