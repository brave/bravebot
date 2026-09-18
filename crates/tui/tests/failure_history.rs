use bravebot_agent::{Category, Diagnosis, Ending};
use bravebot_tui::{render, state::Session};

fn first_visible_history_row(session: &mut Session) -> String {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 24)).unwrap();
    let mut laid = bravebot_tui::state::Laid::default();
    terminal
        .draw(|frame| {
            laid = render::draw(frame, session);
        })
        .unwrap();
    session.note_layout(laid);
    terminal
        .backend()
        .buffer()
        .content()
        .chunks(100)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .find(|row| row.contains("history row"))
        .unwrap()
}

#[test]
fn turn_endings_keep_the_visible_history_anchor() {
    for ending in [
        Ending::Done,
        Ending::Stopped { attempts: Some(1) },
        Ending::Failed(Diagnosis::of(Category::Transport)),
    ] {
        let mut session = Session::new("test");
        for index in 0..60 {
            session.narrate(format!("history row {index:03}"));
        }
        session.paste("work");
        let prompt = session.submit().unwrap();
        first_visible_history_row(&mut session);
        session.scroll_up(30);
        let before = first_visible_history_row(&mut session);
        match ending {
            Ending::Done => session.complete("done", vec![], 0),
            Ending::Stopped { attempts } => {
                session.stopped(attempts);
                session.restore(prompt);
            }
            Ending::Failed(_) => session.fail("the request failed", ending),
        }
        assert_eq!(before, first_visible_history_row(&mut session));
        assert_eq!(before, first_visible_history_row(&mut session));
    }
}
