//! BOT-6, BOT-7, BOT-8. A binary of its own so `BRAVEBOT_HOME` can point at a scratch
//! directory. The library tests share a process with every other test and must not move that
//! variable.

use bravebot_agent::bots::{NewBot, Refused, Row};
use bravebot_tui::Session;
use bravebot_tui::app::{handle_key, handle_paste};
use bravebot_tui::bots_pane::State;
use bravebot_tui::layout::Layout;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

static ENVIRONMENT: Mutex<()> = Mutex::new(());

struct Scratch {
    root: PathBuf,
    saved: Vec<(&'static str, Option<OsString>)>,
    _lock: MutexGuard<'static, ()>,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let lock = ENVIRONMENT.lock().unwrap_or_else(|held| held.into_inner());
        let root =
            std::env::temp_dir().join(format!("bravebot-bots-form-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("state")).expect("state");
        std::fs::create_dir_all(root.join("config")).expect("config");
        std::fs::create_dir_all(root.join("home")).expect("home");
        let saved = ["BRAVEBOT_HOME", "XDG_CONFIG_HOME", "HOME"]
            .into_iter()
            .map(|name| (name, std::env::var_os(name)))
            .collect();
        // SAFETY: the mutex is held until this value drops, and every test in this binary takes it
        // before it reads the variables, so no other test observes a mix of the two.
        unsafe {
            std::env::set_var("BRAVEBOT_HOME", root.join("state"));
            std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
            std::env::set_var("HOME", root.join("home"));
        }
        Self {
            root,
            saved,
            _lock: lock,
        }
    }

    fn state(&self) -> PathBuf {
        self.root.join("state")
    }

    fn data(&self) -> PathBuf {
        bravebot_agent::bots::data_folder(bravebot_agent::bots::System::current(), |name| {
            std::env::var_os(name)
        })
        .expect("a data folder")
    }

    fn bots_file(&self) -> PathBuf {
        self.state().join("bots.json")
    }

    fn bots_bytes(&self) -> Vec<u8> {
        std::fs::read(self.bots_file()).unwrap_or_default()
    }

    fn definition(&self, slug: &str) -> String {
        std::fs::read_to_string(self.state().join("agents").join(format!("{slug}.md")))
            .unwrap_or_default()
    }

    fn rows(&self) -> Vec<Row> {
        bravebot_agent::bots::list(&self.data()).rows
    }

    fn make(&self, name: &str, purpose: &str) {
        bravebot_agent::bots::make(
            &self.data(),
            NewBot {
                name: name.to_string(),
                purpose: purpose.to_string(),
                model: None,
                avatar: None,
            },
        )
        .unwrap_or_else(|refused| panic!("make {name}: {refused}"));
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(self.state(), std::fs::Permissions::from_mode(0o700));
            let _ = std::fs::set_permissions(
                self.state().join("agents"),
                std::fs::Permissions::from_mode(0o700),
            );
        }
        unsafe {
            for (name, value) in &self.saved {
                match value {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
            }
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn type_text(session: &mut Session, text: &str) {
    for character in text.chars() {
        handle_key(session, key(KeyCode::Char(character)));
    }
}

fn session() -> Session {
    let mut session = Session::new("none")
        .in_layout(Layout::Bots)
        .with_bots(State::read())
        .with_no_session_begun();
    session.start_in_the_list_where_it_is_drawn(120);
    session.laid.columns = 120;
    session
}

fn save_new(session: &mut Session, name: &str, purpose: &str) {
    handle_key(session, key(KeyCode::Char('n')));
    type_text(session, name);
    handle_key(session, key(KeyCode::Tab));
    type_text(session, purpose);
    handle_key(session, key(KeyCode::Enter));
}

fn body(definition: &str) -> &str {
    definition
        .split_once("---\n\n")
        .map(|(_, body)| body)
        .unwrap_or(definition)
}

fn screen(session: &mut Session) -> String {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 30)).expect("terminal");
    let mut laid = bravebot_tui::state::Laid::default();
    terminal
        .draw(|frame| laid = bravebot_tui::render::draw(frame, session))
        .expect("draw");
    session.note_layout(laid);
    let buffer = terminal.backend().buffer().clone();
    (0..buffer.area.height)
        .map(|row| {
            (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// BOT-7.
#[test]
fn a_make_writes_the_definition_body_and_the_reread_list_holds_the_bot() {
    let scratch = Scratch::new("make");
    let mut session = session();
    save_new(&mut session, "Ada", "Keeps notes.");

    assert!(!session.bots().form_open(), "a saved form stayed open");
    assert_eq!(session.bots().row_count(), 1);
    let rows = scratch.rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "Ada");
    assert_eq!(rows[0].slug, "ada");
    assert_eq!(rows[0].purpose, "Keeps notes.");
    assert_eq!(rows[0].model, None);
    assert_eq!(body(&scratch.definition("ada")), "Keeps notes.\n");
    assert!(screen(&mut session).contains("Ada"));
}

/// BOT-17.
#[test]
fn a_make_selects_the_new_bot() {
    let scratch = Scratch::new("select");
    scratch.make("Zed", "Already here.");
    let mut session = session();
    save_new(&mut session, "Ada", "The new one.");

    let rows = scratch.rows();
    let selected = session.bots().selected();
    assert_eq!(rows[selected].slug, "ada", "{rows:?} selected {selected}");
}

/// BOT-7.
#[test]
fn an_edit_rewrites_the_definition_body() {
    let scratch = Scratch::new("edit");
    scratch.make("Ada", "Keeps notes.");
    let mut session = session();
    handle_key(&mut session, key(KeyCode::Char('e')));
    handle_key(&mut session, key(KeyCode::Tab));
    for _ in 0.."Keeps notes.".len() {
        handle_key(&mut session, key(KeyCode::Backspace));
    }
    type_text(&mut session, "Writes the record.");
    handle_key(&mut session, key(KeyCode::Enter));

    assert!(!session.bots().form_open());
    let rows = scratch.rows();
    assert_eq!(rows[0].purpose, "Writes the record.");
    assert_eq!(body(&scratch.definition("ada")), "Writes the record.\n");
    assert!(!scratch.definition("ada").contains("Keeps notes."));
}

/// BOT-7.
#[test]
fn a_second_bot_with_the_same_name_takes_the_slug_with_a_number() {
    let scratch = Scratch::new("slug");
    let mut session = session();
    save_new(&mut session, "Web Dev", "Builds pages.");
    save_new(&mut session, "Web Dev", "Builds the next one.");

    let rows = scratch.rows();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].slug, "web-dev");
    assert_eq!(rows[1].slug, "web-dev-2");
    assert_ne!(
        scratch.definition("web-dev"),
        scratch.definition("web-dev-2")
    );
    assert!(body(&scratch.definition("web-dev-2")).contains("Builds the next one."));
}

/// BOT-7.
#[test]
fn the_hundred_and_first_bot_is_refused_and_every_row_is_kept() {
    let scratch = Scratch::new("ceiling");
    for number in 0..100 {
        scratch.make(&format!("Bot {number}"), "Keeps a seat.");
    }
    let before = scratch.bots_bytes();
    let mut session = session();
    assert_eq!(session.bots().row_count(), 100);
    save_new(&mut session, "Overflow", "Does not fit.");

    assert!(session.bots().form_open(), "a refused make closed the form");
    assert_eq!(
        session.bots().message(),
        Some(Refused::Ceiling.to_string().as_str())
    );
    assert_eq!(scratch.rows().len(), 100);
    assert_eq!(session.bots().row_count(), 100);
    assert_eq!(scratch.bots_bytes(), before);
    assert!(scratch.definition("overflow").is_empty());
}

/// BOT-7.
#[test]
fn a_name_or_a_purpose_with_a_null_is_refused_and_the_file_is_unchanged() {
    let scratch = Scratch::new("null");
    scratch.make("Ada", "Keeps notes.");
    let before = scratch.bots_bytes();
    let definition = scratch.definition("ada");
    let mut session = session();

    handle_key(&mut session, key(KeyCode::Char('n')));
    handle_paste(&mut session, "Bad\0name");
    handle_key(&mut session, key(KeyCode::Tab));
    type_text(&mut session, "A purpose.");
    handle_key(&mut session, key(KeyCode::Enter));
    assert!(session.bots().form_open());
    assert!(
        session
            .bots()
            .message()
            .is_some_and(|message| message.contains("null")),
        "{:?}",
        session.bots().message()
    );
    assert_eq!(scratch.bots_bytes(), before);

    handle_key(&mut session, key(KeyCode::Esc));
    handle_key(&mut session, key(KeyCode::Char('n')));
    type_text(&mut session, "Beryl");
    handle_key(&mut session, key(KeyCode::Tab));
    handle_paste(&mut session, "A\0purpose");
    handle_key(&mut session, key(KeyCode::Enter));
    assert!(session.bots().form_open());
    assert!(
        session
            .bots()
            .message()
            .is_some_and(|message| message.contains("null")),
        "{:?}",
        session.bots().message()
    );
    assert_eq!(scratch.bots_bytes(), before);
    assert_eq!(scratch.definition("ada"), definition);
    assert!(scratch.rows().iter().all(|row| row.slug != "beryl"));
}

/// BOT-7.
#[test]
fn a_model_over_the_bound_or_with_a_newline_is_refused_and_the_stored_model_is_unchanged() {
    let scratch = Scratch::new("model-refused");
    scratch.make("Ada", "Keeps notes.");
    let mut session = session();
    handle_key(&mut session, key(KeyCode::Char('e')));
    handle_key(&mut session, key(KeyCode::Tab));
    handle_key(&mut session, key(KeyCode::Tab));
    type_text(&mut session, "claude-sonnet");
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(scratch.rows()[0].model.as_deref(), Some("claude-sonnet"));

    let before = scratch.definition("ada");
    handle_key(&mut session, key(KeyCode::Char('e')));
    handle_key(&mut session, key(KeyCode::Tab));
    handle_key(&mut session, key(KeyCode::Tab));
    handle_paste(&mut session, "\nextra");
    handle_key(&mut session, key(KeyCode::Enter));
    assert!(session.bots().form_open());
    assert_eq!(scratch.rows()[0].model.as_deref(), Some("claude-sonnet"));
    assert_eq!(scratch.definition("ada"), before);

    handle_key(&mut session, key(KeyCode::Esc));
    handle_key(&mut session, key(KeyCode::Char('e')));
    handle_key(&mut session, key(KeyCode::Tab));
    handle_key(&mut session, key(KeyCode::Tab));
    handle_paste(&mut session, &"m".repeat(513));
    handle_key(&mut session, key(KeyCode::Enter));
    assert!(session.bots().form_open());
    assert_eq!(scratch.rows()[0].model.as_deref(), Some("claude-sonnet"));
    assert_eq!(scratch.definition("ada"), before);
}

/// BOT-7. An empty model field clears the stored model. Omitting the field would leave it.
#[test]
fn an_empty_model_field_clears_the_model_and_any_other_text_is_stored() {
    let scratch = Scratch::new("model-clear");
    scratch.make("Ada", "Keeps notes.");
    let mut session = session();
    handle_key(&mut session, key(KeyCode::Char('e')));
    handle_key(&mut session, key(KeyCode::Tab));
    handle_key(&mut session, key(KeyCode::Tab));
    type_text(&mut session, "claude-sonnet");
    handle_key(&mut session, key(KeyCode::Enter));

    let stored = scratch.definition("ada");
    assert!(stored.contains("model: 'claude-sonnet'"), "{stored}");
    assert_eq!(scratch.rows()[0].model.as_deref(), Some("claude-sonnet"));

    handle_key(&mut session, key(KeyCode::Char('e')));
    handle_key(&mut session, key(KeyCode::Tab));
    handle_key(&mut session, key(KeyCode::Tab));
    for _ in 0.."claude-sonnet".len() {
        handle_key(&mut session, key(KeyCode::Backspace));
    }
    assert_eq!(session.bots().form_model(), Some(""));
    handle_key(&mut session, key(KeyCode::Enter));

    assert!(!session.bots().form_open());
    assert_eq!(scratch.rows()[0].model, None);
    let cleared = scratch.definition("ada");
    assert!(
        !cleared.contains("model:"),
        "the empty field left the model:\n{cleared}"
    );
}

/// BOT-7.
#[test]
fn a_failed_write_names_the_path_and_keeps_the_list() {
    let scratch = Scratch::new("unwritable");
    scratch.make("Ada", "Keeps notes.");
    let before = scratch.bots_bytes();
    let definition = scratch.state().join("agents").join("ada.md");
    let definition_before = std::fs::read(&definition).unwrap();
    let mut session = session();
    handle_key(&mut session, key(KeyCode::Char('e')));
    handle_key(&mut session, key(KeyCode::Tab));
    type_text(&mut session, " more");
    // The row is replaced through the state directory, and the module sets that directory back to
    // mode 0700 before the write, so a mode on the directory does not refuse the save. The
    // definition is opened in place: mode 0400 can still be read, and the write is refused.
    if !deny_writing(&definition) {
        return;
    }
    handle_key(&mut session, key(KeyCode::Enter));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&definition, std::fs::Permissions::from_mode(0o600)).unwrap();
    }

    let message = session.bots().message().unwrap_or("").to_string();
    assert!(
        message.contains(&definition.display().to_string()),
        "message={message:?} open={} purpose={:?}",
        session.bots().form_open(),
        scratch.rows().first().map(|row| row.purpose.clone())
    );
    assert_eq!(std::fs::read(&definition).unwrap(), definition_before);
    assert_eq!(session.bots().row_count(), 1);
    assert_eq!(scratch.bots_bytes(), before);
    assert_eq!(scratch.rows()[0].purpose, "Keeps notes.");
    assert!(session.bots().form_open());
}

#[cfg(unix)]
fn deny_writing(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o400)).unwrap();
    if std::fs::OpenOptions::new().write(true).open(path).is_ok() {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
        eprintln!("SKIPPED (permissions do not bind this user)");
        return false;
    }
    true
}

#[cfg(not(unix))]
fn deny_writing(_path: &std::path::Path) -> bool {
    false
}

/// BOT-7, BOT-8.
#[test]
fn archive_and_delete_wait_for_y_and_restore_does_not() {
    let scratch = Scratch::new("confirm");
    scratch.make("Ada", "Keeps notes.");
    let mut session = session();

    handle_key(&mut session, key(KeyCode::Char('a')));
    assert!(session.bots().confirmation_open());
    assert_eq!(scratch.rows()[0].retired, 0.0);
    handle_key(&mut session, key(KeyCode::Char('n')));
    assert!(!session.bots().confirmation_open());
    assert_eq!(scratch.rows()[0].retired, 0.0);

    handle_key(&mut session, key(KeyCode::Char('a')));
    handle_key(&mut session, key(KeyCode::Esc));
    assert_eq!(scratch.rows()[0].retired, 0.0);

    handle_key(&mut session, key(KeyCode::Char('a')));
    handle_key(&mut session, key(KeyCode::Char('q')));
    assert_eq!(scratch.rows()[0].retired, 0.0);

    handle_key(&mut session, key(KeyCode::Char('a')));
    handle_key(&mut session, key(KeyCode::Char('Y')));
    assert!(!session.bots().confirmation_open());
    assert!(scratch.rows()[0].retired != 0.0);
    let order_after_archive: Vec<_> = scratch.rows().into_iter().map(|row| row.slug).collect();
    assert_eq!(order_after_archive, ["ada"]);
    let drawn = screen(&mut session);
    assert!(drawn.contains("Ada"), "{drawn}");
    assert!(!drawn.contains("archived"), "{drawn}");

    handle_key(&mut session, key(KeyCode::Char('r')));
    assert!(!session.bots().confirmation_open());
    assert_eq!(scratch.rows()[0].retired, 0.0);

    handle_key(&mut session, key(KeyCode::Char('a')));
    handle_key(&mut session, key(KeyCode::Char('y')));
    assert!(scratch.rows()[0].retired != 0.0);
    handle_key(&mut session, key(KeyCode::Char('d')));
    assert!(session.bots().confirmation_open());
    handle_key(&mut session, key(KeyCode::Char('x')));
    assert_eq!(scratch.rows().len(), 1);

    handle_key(&mut session, key(KeyCode::Char('d')));
    handle_key(&mut session, key(KeyCode::Char('y')));
    assert!(scratch.rows().is_empty());
    assert_eq!(session.bots().row_count(), 0);
    assert!(scratch.definition("ada").contains("Keeps notes."));
}

/// BOT-8.
#[test]
fn delete_is_not_offered_for_a_bot_that_is_not_archived_and_removes_nothing() {
    let scratch = Scratch::new("delete-in-use");
    scratch.make("Ada", "Keeps notes.");
    let mut session = session();
    handle_key(&mut session, key(KeyCode::Char('d')));
    assert!(!session.bots().confirmation_open());
    assert_eq!(scratch.rows().len(), 1);
    handle_key(&mut session, key(KeyCode::Char('y')));
    assert_eq!(scratch.rows().len(), 1);
}

/// BOT-6.
#[test]
fn a_long_paste_is_stored_whole_and_a_control_character_is_saved() {
    let scratch = Scratch::new("paste");
    let pasted = "one\ntwo\nthree\nfour";
    let mut session = session();
    handle_key(&mut session, key(KeyCode::Char('n')));
    type_text(&mut session, "Ada");
    handle_key(&mut session, key(KeyCode::Tab));
    assert_eq!(
        handle_paste(&mut session, pasted),
        bravebot_tui::app::Action::Redraw
    );
    assert_eq!(session.bots().form_purpose(), Some(pasted));
    let drawn = screen(&mut session);
    assert!(drawn.contains("one") && drawn.contains("four"), "{drawn}");
    assert!(!drawn.contains("[Pasted text"), "{drawn}");
    handle_key(&mut session, key(KeyCode::Enter));

    assert_eq!(scratch.rows()[0].purpose, pasted);
    assert_eq!(body(&scratch.definition("ada")), "one\ntwo\nthree\nfour\n");

    let control = "see\u{7}me";
    handle_key(&mut session, key(KeyCode::Char('e')));
    handle_key(&mut session, key(KeyCode::Tab));
    for _ in 0..pasted.len() {
        handle_key(&mut session, key(KeyCode::Backspace));
    }
    handle_paste(&mut session, control);
    let drawn = screen(&mut session);
    assert!(drawn.contains("see\u{2407}me"), "{drawn}");
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(scratch.rows()[0].purpose, control);
    assert!(scratch.definition("ada").contains(control));
}
