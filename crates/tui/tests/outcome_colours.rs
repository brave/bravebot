use bravebot_tui::{render, state::Session, theme};
use ratatui::{Terminal, backend::TestBackend};

#[test]
fn regression_success_cost_uses_neutral_colour() {
    let mut session = Session::new("test");
    session.paste("work");
    session.submit().unwrap();
    session.complete("answer", Vec::new(), 42);
    let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
    terminal
        .draw(|frame| {
            render::draw(frame, &session);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    for row in buffer.content().chunks(100) {
        let text: String = row.iter().map(|cell| cell.symbol()).collect();
        if text.contains("turn 1 done") {
            let at = row.iter().position(|cell| cell.symbol() == "(").unwrap();
            assert_eq!(
                row[at].fg,
                theme::muted(),
                "successful cost shown in failure colour; failure colour is {:?}",
                theme::fail()
            );
            return;
        }
    }
    panic!("completed status row not drawn");
}

/// Failure labels use the failure colour.
#[test]
fn failure_label_uses_failure_colour() {
    let mut session = Session::new("test");
    session.paste("work");
    session.submit().unwrap();
    let reason = bravebot_agent::Diagnosis::of(bravebot_agent::Category::Transport);
    session.fail("request failed", bravebot_agent::Ending::Failed(reason));
    let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
    terminal
        .draw(|frame| {
            render::draw(frame, &session);
        })
        .unwrap();
    for row in terminal.backend().buffer().content().chunks(100) {
        let text: String = row.iter().map(|cell| cell.symbol()).collect();
        if text.contains("turn 1 failed") {
            let label = row.iter().position(|cell| cell.symbol() == "t").unwrap();
            assert_eq!(row[label].fg, theme::fail());
            return;
        }
    }
    panic!("failure status row not drawn");
}
