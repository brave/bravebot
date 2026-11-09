//! BOT-7, BOT-17. A binary of its own: engaging incognito lasts for the life of the process, so
//! this file never asserts a write that an ordinary run would keep.

use bravebot_tui::Session;
use bravebot_tui::app::handle_key;
use bravebot_tui::bots_pane::State;
use bravebot_tui::layout::Layout;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::ffi::OsString;

/// BOT-7.
#[test]
fn an_incognito_make_says_so_and_writes_nothing() {
    let root = std::env::temp_dir().join(format!(
        "bravebot-bots-form-incognito-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("state")).expect("state");
    std::fs::create_dir_all(root.join("config")).expect("config");
    let saved = ["BRAVEBOT_HOME", "XDG_CONFIG_HOME"]
        .into_iter()
        .map(|name| (name, std::env::var_os(name)))
        .collect::<Vec<_>>();
    // SAFETY: this binary has one test, so nothing else reads the variables while they point here.
    unsafe {
        std::env::set_var("BRAVEBOT_HOME", root.join("state"));
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
    }

    bravebot_core::incognito::engage();
    let mut session = Session::new("none")
        .in_layout(Layout::Bots)
        .with_bots(State::read())
        .with_no_session_begun();
    session.start_in_the_list_where_it_is_drawn(120);
    session.laid.columns = 120;
    handle_key(&mut session, key(KeyCode::Char('n')));
    type_text(&mut session, "Ada");
    handle_key(&mut session, key(KeyCode::Tab));
    type_text(&mut session, "Keeps notes.");
    handle_key(&mut session, key(KeyCode::Enter));

    let message = session.bots().message().unwrap_or("");
    assert!(
        message.contains("incognito"),
        "the refusal did not say why: {message}"
    );
    assert!(session.bots().form_open(), "a refused make closed the form");
    assert!(!root.join("state").join("bots.json").exists());
    let agents = root.join("state").join("agents");
    let wrote_a_definition = agents.exists()
        && std::fs::read_dir(&agents)
            .map(|entries| {
                entries
                    .flatten()
                    .any(|entry| entry.path().extension().is_some())
            })
            .unwrap_or(false);
    assert!(!wrote_a_definition, "an incognito make wrote a definition");

    restore(&saved);
    let _ = std::fs::remove_dir_all(&root);
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn type_text(session: &mut Session, text: &str) {
    for character in text.chars() {
        handle_key(session, key(KeyCode::Char(character)));
    }
}

fn restore(saved: &[(&str, Option<OsString>)]) {
    unsafe {
        for (name, value) in saved {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}
