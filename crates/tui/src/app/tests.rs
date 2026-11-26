use super::*;

use crate::state::Status;

use bravebot_config::limit::Limit;

use ratatui::crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// The private modes a run of bytes leaves turned on, in the order the terminal would read
/// them: a mode asked for and then given up again is not left on.
fn modes_left_on(written: &str) -> std::collections::BTreeSet<String> {
    let mut on = std::collections::BTreeSet::new();
    for request in written.split("\x1b[?").skip(1) {
        let mode: String = request.chars().take_while(char::is_ascii_digit).collect();
        match request[mode.len()..].chars().next() {
            Some('h') => {
                on.insert(mode);
            }
            Some('l') => {
                on.remove(&mode);
            }
            _ => {}
        }
    }
    on
}

fn asked_for(enhanced: bool) -> String {
    let mut written = Vec::new();
    ask_for_modes(&mut written, enhanced, true).expect("ask");
    String::from_utf8(written).expect("utf8")
}

fn given_back(enhanced: bool) -> String {
    let mut written = Vec::new();
    give_back_modes(&mut written, enhanced).expect("give back");
    String::from_utf8(written).expect("utf8")
}

fn listed(key: &str, window: Option<u64>) -> bravebot_aichat::models::Model {
    bravebot_aichat::models::Model {
        key: key.to_string(),
        display_name: key.to_string(),
        premium: false,
        provider: None,
        conversation_tokens: window,
        reads_effort: true,
        advertised: bravebot_aichat::models::Advertised::default(),
    }
}

fn bedrock_configured(pairs: &[(&str, &str)]) -> bravebot_config::bedrock::Bedrock {
    let owned: Vec<(String, String)> = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    bravebot_config::bedrock::Bedrock::from_lookup(|name| {
        owned
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    })
    .expect("configured")
}

/// A gateway configured by a `provider` block, for the picker tests below.
fn gateway(models: &str) -> bravebot_config::provider::Provider {
    let text = format!(
        r#"{{"provider": {{"openrouter": {{
                "options": {{"baseURL": "https://openrouter.example.invalid/api/v1"}},
                "models": {models}
            }}}}}}"#
    );
    let serde_json::Value::Object(root) = serde_json::from_str(&text).expect("json") else {
        panic!("not an object");
    };
    bravebot_config::provider::Provider::all(&root)
        .pop()
        .expect("one provider")
}

/// A loopback server answering every request with `body`, and the path of each request it
/// answered. A path is sent before its answer is written, so a caller whose request has returned
/// finds it already there.
fn a_listing_server(body: &'static str) -> (String, std::sync::mpsc::Receiver<String>) {
    use std::io::{BufRead, BufReader, Write};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let address = format!("http://{}", listener.local_addr().expect("an address"));
    let (sender, asked) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(10)));
            let mut reader = BufReader::new(stream);
            let mut request = String::new();
            if reader.read_line(&mut request).is_err() {
                continue;
            }
            let mut header = String::new();
            while matches!(reader.read_line(&mut header), Ok(read) if read > 2) {
                header.clear();
            }
            let Some(path) = request.split(' ').nth(1) else {
                continue;
            };
            if sender.send(path.to_string()).is_err() {
                return;
            }
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = reader.into_inner().write_all(response.as_bytes());
        }
    });
    (address, asked)
}

/// A configuration whose roster is what a settings file named and whose Brave credentials are
/// blank, so the listing costs no request and the window under test is the one in the file.
fn a_config_with_a_named_roster() -> Config {
    use bravebot_config::env_var;

    Config::from_lookup(|key| match key {
        env_var::USE_BEDROCK => Some("1".into()),
        env_var::AWS_REGION => Some("us-west-2".into()),
        env_var::BEDROCK_OPUS_MODEL => Some("opus-arn".into()),
        _ => None,
    })
    .expect("an account named on its own is a working configuration")
}

/// The rows the xAI account's tests script: two models of the account, with windows a budget can
/// be told apart from the default by, filed under the provider the listing files them under.
fn the_xai_accounts_listing() -> Vec<bravebot_aichat::models::Model> {
    ["xai/grok-4.5", "xai/grok-build-0.1"]
        .into_iter()
        .map(|key| bravebot_aichat::models::Model {
            provider: Some("xAI".to_string()),
            reads_effort: false,
            ..listed(key, Some(131_072))
        })
        .collect()
}

/// The row the picker's cursor opens on and the rows it marks as current, for a session as it stands.
fn picker_opened(session: &mut Session, config: &Config) -> (Option<String>, Vec<String>) {
    open_model_picker(session, config, crate::model_prompt::Scope::EverySession);
    session.settle_picking();
    let Some(crate::picking::Picking::Model(picker)) = session.picking() else {
        panic!("no model picker is open");
    };
    let marked = picker
        .matching()
        .into_iter()
        .filter(|model| picker.is_current(model))
        .map(|model| model.key.clone())
        .collect();
    (picker.chosen().map(|model| model.key.clone()), marked)
}

fn write_question(path: &str) -> crate::remote_confirm::ToMain {
    crate::remote_confirm::ToMain::Write(bravebot_agent::confirm::WriteRequest {
        written_since_checkout: false,
        path: path.into(),
        contents: "body\n".into(),
        existing: None,
        diff: bravebot_agent::diff::Diff::compute("", "body\n"),
        intent: bravebot_agent::confirm::Intent::Create,
        untrusted: false,
        remark: None,
        protected: false,
        credentials: Vec::new(),
        may_always: false,
        record: None,
        may_edit: false,
    })
}

fn fetch_question() -> crate::remote_confirm::ToMain {
    crate::remote_confirm::ToMain::Fetch(bravebot_agent::confirm::FetchRequest {
        url: "https://example.test/".into(),
        host: "example.test".into(),
        redirected_from: None,
        classified: Default::default(),
    })
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

fn ctrl_key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::CONTROL)
}

/// The same chord on the way up. A terminal asked for disambiguated keys sends these as well as
/// presses, so every loop that reads a key has to tell them apart.
fn released(key: KeyEvent) -> KeyEvent {
    KeyEvent {
        kind: KeyEventKind::Release,
        ..key
    }
}

fn shift(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::SHIFT)
}

/// A session with `text` typed into the box, one key at a time.
fn typed_into(text: &str) -> Session {
    let mut session = Session::new("none");
    for c in text.chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    session
}

fn drag(kind: MouseEventKind, row: u16, column: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

/// A frame with the transcript and the info panel side by side, drawn and recorded the way
/// a redraw does, with a marker word in each so a copy shows which one it took from.
fn two_panes() -> (Session, ratatui::buffer::Buffer) {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let mut session = Session::new("none");
    session.laid.columns = 120;
    session.toggle_panel();
    session.identify("PANELMARK", "/work/dir".to_string(), None);
    session
        .transcript
        .push(crate::state::Entry::user("TRANSCRIPTMARK one two three"));
    let mut terminal = Terminal::new(TestBackend::new(120, 24)).expect("terminal");
    let mut laid = crate::state::Laid::default();
    terminal
        .draw(|frame| laid = render::draw(frame, &session))
        .expect("draw");
    session.note_layout(laid);
    let mut buffer = ratatui::buffer::Buffer::empty(terminal.backend().buffer().area);
    terminal
        .draw(|frame| {
            render::draw(frame, &session);
        })
        .expect("draw");
    buffer.clone_from(terminal.backend().buffer());
    (session, buffer)
}

fn swept(session: &mut Session, from: (u16, u16), to: (u16, u16)) {
    handle_mouse(
        session,
        drag(MouseEventKind::Down(MouseButton::Left), from.0, from.1),
    );
    handle_mouse(
        session,
        drag(MouseEventKind::Drag(MouseButton::Left), to.0, to.1),
    );
}

/// The row and column where `needle` is drawn.
fn find_on(buffer: &ratatui::buffer::Buffer, needle: &str) -> (u16, u16) {
    (0..buffer.area.height)
        .find_map(|row| {
            let line: String = (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol().to_string())
                .collect();
            line.find(needle).map(|column| (row, column as u16))
        })
        .expect("text on the screen")
}

fn press(session: &mut Session, row: u16, column: u16, modifiers: KeyModifiers) {
    let mut event = drag(MouseEventKind::Down(MouseButton::Left), row, column);
    event.modifiers = modifiers;
    handle_mouse(session, event);
}

/// A scroller whose one call returned thirty lines and draws five, laid out at 80 by 10.
fn scroller_over_one_call() -> Session {
    use bravebot_agent::report::{Activity, Landing, Returned};
    let mut session = Session::new("kernel-enforced");
    session.finish_activity(Activity::running("Run", "make").done("ok"));
    session.landed(Landing::Context);
    let lines: Vec<String> = (0..30).map(|n| format!("out {n}")).collect();
    session.returned(Returned {
        lines: lines[..5].to_vec(),
        total: 30,
        from_the_end: false,
        whole: lines,
    });
    session.open_scroller();
    session.laid.width = 80;
    session.laid.height = 10;
    session.laid = render::as_last_drawn(&session);
    session
}

fn click(session: &mut Session, row: u16, column: u16) -> Action {
    handle_mouse(
        session,
        drag(MouseEventKind::Down(MouseButton::Left), row, column),
    );
    handle_mouse(
        session,
        drag(MouseEventKind::Up(MouseButton::Left), row, column),
    )
}

/// The scroller over one call and two delegates, so that a click has to pick the right one.
fn scroller_over_two_delegates() -> Session {
    use bravebot_agent::report::{DelegateId, Delegation};
    let mut session = scroller_over_one_call();
    for (n, task) in [(1, "find the parser"), (2, "write the docs")] {
        session.delegate_started(Delegation {
            stop: Default::default(),
            id: DelegateId::nth(n),
            kind: "reader".to_string(),
            task: task.to_string(),
        });
    }
    session.laid.height = 40;
    session.note_layout(render::as_last_drawn(&session));
    session
}

/// A session at rest with the info panel drawn beside a prompt and two delegates, and the
/// frame drawn so that the rows of each are the ones recorded.
fn at_rest_over_two_delegates() -> (Session, ratatui::buffer::Buffer) {
    use bravebot_agent::report::{DelegateId, Delegation};
    let (mut session, _) = two_panes();
    for (n, task) in [(1, "find the parser"), (2, "write the docs")] {
        session.delegate_started(Delegation {
            stop: Default::default(),
            id: DelegateId::nth(n),
            kind: "reader".to_string(),
            task: task.to_string(),
        });
    }
    let buffer = drawn(&mut session);
    (session, buffer)
}

/// A session at rest with a delegate in each of two turns, and the same number for both: the
/// driver numbers them from one in every turn. The frame is drawn so that the rows of each are
/// the ones recorded.
fn at_rest_over_a_delegate_in_each_of_two_turns() -> (Session, ratatui::buffer::Buffer) {
    use bravebot_agent::report::{DelegateId, Delegation};
    let (mut session, _) = two_panes();
    for (prompt, task) in [
        ("first request", "first turn task"),
        ("second request", "second turn task"),
    ] {
        session.transcript.push(crate::state::Entry::user(prompt));
        session.delegate_started(Delegation {
            stop: Default::default(),
            id: DelegateId::nth(1),
            kind: "reader".to_string(),
            task: task.to_string(),
        });
    }
    let buffer = drawn(&mut session);
    (session, buffer)
}

/// A delegate's view opened by a click, over a delegate that has answered, with the frame drawn
/// so the way back out and the sentence that closes the view are recorded.
fn a_finished_delegates_view() -> (Session, ratatui::buffer::Buffer) {
    use bravebot_agent::report::{DelegateId, Reported};
    let (mut session, buffer) = at_rest_over_two_delegates();
    session.delegate_finished(
        DelegateId::nth(2),
        "a reader delegate answered after 3 rounds".to_string(),
        false,
        Some(Reported::Said("it is in src/parser.rs".to_string())),
    );
    let (row, column) = find_on(&buffer, "write the docs");
    click(&mut session, row, column);
    assert!(session.watching_a_delegate(), "the view did not open");
    session.last_click = None;
    let buffer = drawn(&mut session);
    (session, buffer)
}

/// The buffer of the session's frame at a size of the caller's, with its layout recorded.
fn drawn_at(session: &mut Session, columns: u16, rows: u16) -> ratatui::buffer::Buffer {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let mut terminal = Terminal::new(TestBackend::new(columns, rows)).expect("terminal");
    let mut laid = crate::state::Laid::default();
    terminal
        .draw(|frame| laid = render::draw(frame, session))
        .expect("draw");
    session.note_layout(laid);
    terminal.backend().buffer().clone()
}

/// A session at rest with the info panel drawn, and in its transcript a run that returned fourteen
/// lines, a read that returned three and a write of twenty changes, the frame drawn so that the rows of
/// each are the ones recorded.
fn at_rest_over_calls() -> (Session, ratatui::buffer::Buffer) {
    use bravebot_agent::diff::Change;
    use bravebot_agent::report::{Activity, Landing, Returned};
    let (mut session, _) = two_panes();
    // Enough before the calls that the view is held below the top of the transcript, so a call
    // is drawn on a row that is not the first.
    for n in 0..4 {
        session
            .transcript
            .push(crate::state::Entry::user(format!("question {n}")));
    }
    session.finish_activity(Activity::running("Run", "make").done("ok"));
    session.landed(Landing::Context);
    let lines: Vec<String> = (0..14).map(|n| format!("out {n}")).collect();
    session.returned(Returned {
        lines: lines[..5].to_vec(),
        total: 14,
        from_the_end: false,
        whole: lines,
    });
    session.finish_activity(Activity::running("Read", "short.txt").done("3 lines"));
    session.landed(Landing::Context);
    session.returned(Returned {
        lines: vec!["tiny 0".into(), "tiny 1".into(), "tiny 2".into()],
        total: 3,
        from_the_end: false,
        whole: vec!["tiny 0".into(), "tiny 1".into(), "tiny 2".into()],
    });
    session.transcript.push(crate::state::Entry::tool(
        Activity::running("Write", "out.txt")
            .done("wrote 20 lines")
            .with_changes(
                (0..20)
                    .map(|n| Change::Added(format!("change {n}")))
                    .collect(),
            ),
    ));
    session.laid.columns = 120;
    let buffer = drawn_at(&mut session, 120, 40);
    (session, buffer)
}

/// The text of a drawn frame, one line to a row.
fn buffer_text(buffer: &ratatui::buffer::Buffer) -> String {
    (0..buffer.area.height)
        .map(|row| {
            (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The words of a reply that has a control character, a tab and a paragraph break in them, and
/// is wide enough to be drawn over more than one row beside the info panel.
const AN_EARLIER_REPLY: &str = "The first answer.\n\nA \u{1b}[31mmarked\u{1b}[0m paragraph with\ta tab in it, long enough that the screen has to wrap it over several rows of the transcript.";

/// A session at rest with the info panel drawn, a prompt between two replies, and the frame drawn
/// so that the marker of each reply is a place the layout recorded.
fn at_rest_over_two_replies() -> (Session, ratatui::buffer::Buffer) {
    let (mut session, _) = two_panes();
    session
        .transcript
        .push(crate::state::Entry::assistant(AN_EARLIER_REPLY, Vec::new()));
    // A reply of nothing but blanks draws no row, so what is drawn after it is not its.
    session
        .transcript
        .push(crate::state::Entry::assistant("  \n", Vec::new()));
    session
        .transcript
        .push(crate::state::Entry::user("and then"));
    session.transcript.push(crate::state::Entry::assistant(
        "The latest answer.",
        Vec::new(),
    ));
    session.transcript.push(
        crate::state::Entry::assistant("A named answer.", Vec::new())
            .answered_as(Some("reader".to_string())),
    );
    let buffer = drawn(&mut session);
    (session, buffer)
}

/// A reply that holds two fenced blocks. The first has a tab, markers a reader would take for
/// markdown, and a line wider than the transcript beside the info panel, which the screen wraps.
const A_REPLY_WITH_BLOCKS: &str = "Run this first:\n\n```sh\ncargo test -p bravebot-tui --lib\n\techo \"a *star* and a_b_c\" # and a comment long enough that the transcript has to wrap it over a second row\n```\n\nThen this one:\n\n~~~\nthe second block\n~~~\n\nDone.";

/// The row and column of every cell where `needle` begins in a drawn frame, top to bottom.
fn cells_of(buffer: &ratatui::buffer::Buffer, needle: &str) -> Vec<(u16, u16)> {
    let first = needle.chars().next().expect("a needle").to_string();
    let mut found = Vec::new();
    for row in 0..buffer.area.height {
        let cells: Vec<String> = (0..buffer.area.width)
            .map(|column| buffer[(column, row)].symbol().to_string())
            .collect();
        for column in 0..cells.len() {
            if cells[column] == first && cells[column..].concat().starts_with(needle) {
                found.push((row, column as u16));
            }
        }
    }
    found
}

/// A session at rest with the info panel drawn and a reply of two fenced blocks, the frame drawn
/// so that the word at the right of each block's header is a place the layout recorded.
fn at_rest_over_a_reply_with_blocks() -> (Session, ratatui::buffer::Buffer) {
    let (mut session, _) = two_panes();
    session.transcript.push(crate::state::Entry::assistant(
        A_REPLY_WITH_BLOCKS,
        Vec::new(),
    ));
    let buffer = drawn_at(&mut session, 120, 40);
    (session, buffer)
}

/// A session whose turn is running, with three prompts typed and sent behind it, and the frame
/// drawn so the rows each one waits on are the ones recorded.
fn working_with_three_waiting() -> (Session, ratatui::buffer::Buffer) {
    let (mut session, _) = two_panes();
    session.type_char('a');
    session.submit();
    for line in ["second question", "third question", "fourth question"] {
        for c in line.chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }
        handle_key_while_working(&mut session, key(KeyCode::Enter));
    }
    assert_eq!(session.queued.len(), 3);
    let buffer = drawn_at(&mut session, 120, 30);
    (session, buffer)
}

fn waiting(session: &Session) -> Vec<&str> {
    session
        .queued
        .iter()
        .map(|waiting| waiting.prompt.as_str())
        .collect()
}

/// A session with the info panel drawn, a model chosen and an effort in force, and the frame
/// drawn after both so the panel's rows are the ones recorded.
fn panel_with_a_model() -> (Session, ratatui::buffer::Buffer) {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let (mut session, _) = two_panes();
    let config = bravebot_config::Config::from_lookup(|key| match key {
        "SERVICES_KEY_AICHAT" => Some("a-signing-key".into()),
        "BRAVE_SERVICES_KEY_ID" => Some("a-key-id".into()),
        "BRAVE_AI_CHAT_ENDPOINT" => Some("https://ai-chat.bsg.brave.com".into()),
        _ => None,
    })
    .expect("config");
    session.choose_model("claude-sonnet".to_string(), &config);
    session.note_model_reads_effort(true);
    session.choose_effort(Some(bravebot_aichat::protocol::Effort::Xhigh));
    let mut terminal = Terminal::new(TestBackend::new(120, 24)).expect("terminal");
    let mut laid = crate::state::Laid::default();
    terminal
        .draw(|frame| laid = render::draw(frame, &session))
        .expect("draw");
    session.note_layout(laid);
    let buffer = terminal.backend().buffer().clone();
    (session, buffer)
}

/// A session with the panel drawn and a measured context, and the frame drawn after both.
fn panel_with_a_context() -> (Session, ratatui::buffer::Buffer) {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let (mut session, _) = two_panes();
    session.set_last_request(a_request_of_known_parts());
    session.measured(777, 10_000, false);
    let mut terminal = Terminal::new(TestBackend::new(120, 24)).expect("terminal");
    let mut laid = crate::state::Laid::default();
    terminal
        .draw(|frame| laid = render::draw(frame, &session))
        .expect("draw");
    session.note_layout(laid);
    let buffer = terminal.backend().buffer().clone();
    (session, buffer)
}

/// The cell of the first letter of the panel's hide row. `find_on` counts bytes, and the border
/// drawn before the text is more than one.
fn on_the_hide_row(buffer: &ratatui::buffer::Buffer) -> (u16, u16) {
    let (row, _) = find_on(buffer, "hide panel");
    let column = (0..buffer.area.width)
        .find(|column| buffer[(*column, row)].symbol() == "h")
        .expect("the hide row's text");
    (row, column)
}

/// A frame of the panel with a model and an effort chosen, a measured context and a started MCP
/// server, so that every row a click stands for a command on is drawn, with a turn running on
/// the session or not. The frame is drawn last so the rows recorded are the ones on the screen.
fn panel_of_command_rows(turn_running: bool) -> (Session, ratatui::buffer::Buffer) {
    let (mut session, _) = panel_with_a_context();
    let config = bravebot_config::Config::from_lookup(|key| match key {
        "SERVICES_KEY_AICHAT" => Some("a-signing-key".into()),
        "BRAVE_SERVICES_KEY_ID" => Some("a-key-id".into()),
        "BRAVE_AI_CHAT_ENDPOINT" => Some("https://ai-chat.bsg.brave.com".into()),
        _ => None,
    })
    .expect("config");
    session.choose_model("claude-sonnet".to_string(), &config);
    session.note_model_reads_effort(true);
    session.choose_effort(Some(bravebot_aichat::protocol::Effort::Xhigh));
    session.servers.started = vec!["notes-server".to_string()];
    let diff = bravebot_agent::diff::Diff::compute("old\n", "new\n");
    session.transcript.push(crate::state::Entry::tool(
        bravebot_agent::report::Activity::running("Update", "src/a.rs")
            .of_tool("edit_file")
            .with_changes(diff.condensed(3))
            .with_written(Some(bravebot_agent::report::Written {
                landed: "src/a.rs".into(),
                added: diff.added(),
                removed: diff.removed(),
                numbers: diff.condensed_numbers(3),
            }))
            .done("added 1 line, removed 1 line"),
    ));
    session.report_language_servers(bravebot_agent::lsp::Roster::of([
        bravebot_agent::lsp::Language::Rust,
    ]));
    if turn_running {
        type_line(&mut session, "first");
        handle_key(&mut session, key(KeyCode::Enter));
        assert!(session.a_turn_is_running());
    }
    let buffer = drawn(&mut session);
    (session, buffer)
}

/// What each row a click stands for a command on is drawn as, and the command it stands for.
fn command_rows(session: &Session) -> Vec<(String, &'static str)> {
    vec![
        ("claude-sonnet".to_string(), "/model"),
        (t!(panel_effort, level = "xhigh").to_string(), "/effort"),
        (render::context_reading(session), "/context"),
        ("notes-server".to_string(), "/mcp"),
        ("rust-analyzer".to_string(), "/lsp"),
        (
            t!(panel_files_total, count = session.written_totals().0).to_string(),
            "/diff",
        ),
    ]
}

/// Run `act` with what the turn's own loop holds beside the session.
fn beside_a_turn<R>(act: impl FnOnce(&mut Beside<'_>) -> R) -> R {
    let root = std::path::Path::new("/work");
    let mut stored = bravebot_session::sessions::Handle::begin(
        root,
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );
    let workspace = a_workspace();
    let managed = bravebot_config::Managed::default();
    let mut config = a_config_needing_no_sign_in();
    act(&mut Beside {
        managed: &managed,
        settings: &bravebot_config::Settings::default(),
        stored: &mut stored,
        root,
        home: None,
        workspace: &workspace,
        scratch: None,
        config: &mut config,
    })
}

thread_local! {
    /// The addresses a test's opener was handed, in order.
    static OPENED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// An opener that starts no browser and keeps the address it was handed, so a test can say what
/// would have been opened.
fn records_the_address(address: &str) -> bool {
    OPENED.with(|opened| opened.borrow_mut().push(address.to_string()));
    true
}

/// The addresses handed to the opener so far on this thread, and none after.
fn addresses_opened() -> Vec<String> {
    OPENED.with(|opened| std::mem::take(&mut *opened.borrow_mut()))
}

/// One cell for each different thing in the panel a click acts on, found by trying every cell.
fn the_panels_targets(session: &Session) -> Vec<(crate::panel::Target, u16, u16)> {
    let panel = session
        .laid
        .panes
        .iter()
        .find(|(pane, _)| *pane == crate::select::Pane::Panel)
        .expect("panel recorded")
        .1;
    let mut found: Vec<(crate::panel::Target, u16, u16)> = Vec::new();
    for row in panel.top()..panel.bottom() {
        for column in panel.left()..panel.right() {
            if let Some(target) = session.panel_target_at(row, column)
                && !found.iter().any(|(seen, ..)| *seen == target)
            {
                found.push((target, row, column));
            }
        }
    }
    found
}

/// The buffer of the session's frame, with its layout recorded.
fn drawn(session: &mut Session) -> ratatui::buffer::Buffer {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let mut terminal = Terminal::new(TestBackend::new(120, 24)).expect("terminal");
    let mut laid = crate::state::Laid::default();
    terminal
        .draw(|frame| laid = render::draw(frame, session))
        .expect("draw");
    session.note_layout(laid);
    terminal.backend().buffer().clone()
}

/// The row and column of the cell where `needle` is drawn, counting cells and not the bytes of
/// the border characters before it.
fn find_cell(buffer: &ratatui::buffer::Buffer, needle: &str) -> (u16, u16) {
    (0..buffer.area.height)
        .find_map(|row| {
            // What a cell shows, so that the cells of a terminal link are found by their text.
            let cells: Vec<String> = (0..buffer.area.width)
                .map(|column| crate::hyperlink::shown(buffer[(column, row)].symbol()).to_string())
                .collect();
            let first = needle.chars().next()?.to_string();
            (0..cells.len())
                .find(|&at| cells[at] == first && cells[at..].concat().starts_with(needle))
                .map(|column| (row, column as u16))
        })
        .expect("text on the screen")
}

/// A session with `text` in the box, drawn once so the box is recorded.
fn typing(text: &str) -> (Session, ratatui::buffer::Buffer) {
    let mut session = Session::new("none");
    session.paste(text);
    let buffer = drawn(&mut session);
    (session, buffer)
}

/// The row and column where `needle` is drawn in the session's own frame.
fn find_on_text(session: &Session, needle: &str) -> (u16, u16) {
    let text = render::as_text(session);
    text.lines()
        .enumerate()
        .find_map(|(row, line)| line.find(needle).map(|column| (row as u16, column as u16)))
        .expect("text on the screen")
}

/// Press at `from`, drag to `to` and let go through the function the loops answer a mouse report
/// with while a turn or a command runs, on a terminal of `size`, and say what the clipboard was
/// handed. The clipboard is a closure that records, so nothing reaches the real one.
fn dragged_and_released(
    session: &mut Session,
    size: (u16, u16),
    from: (u16, u16),
    to: (u16, u16),
) -> Vec<String> {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).expect("terminal");
    let mut handed = Vec::new();
    for event in [
        drag(MouseEventKind::Down(MouseButton::Left), from.0, from.1),
        drag(MouseEventKind::Drag(MouseButton::Left), to.0, to.1),
        drag(MouseEventKind::Up(MouseButton::Left), to.0, to.1),
    ] {
        answer_mouse(&mut terminal, session, event, |text| {
            handed.push(text.to_string());
            true
        })
        .expect("the report was answered");
    }
    handed
}

/// The same drag answered the way the loop at rest answers it: the event goes to the idle
/// loop's handler and a release it reports as a copy is carried out.
fn dragged_and_released_at_rest(
    session: &mut Session,
    size: (u16, u16),
    from: (u16, u16),
    to: (u16, u16),
) -> Vec<String> {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).expect("terminal");
    let mut handed = Vec::new();
    for event in [
        drag(MouseEventKind::Down(MouseButton::Left), from.0, from.1),
        drag(MouseEventKind::Drag(MouseButton::Left), to.0, to.1),
        drag(MouseEventKind::Up(MouseButton::Left), to.0, to.1),
    ] {
        if idle_action(session, TermEvent::Mouse(event)) == Action::Copy {
            copy_selection(&mut terminal, session, |text| {
                handed.push(text.to_string());
                true
            })
            .expect("the copy was made");
        }
    }
    handed
}

/// What a drawn frame has marked as selected, row by row, without the blanks after the text.
fn highlighted(buffer: &ratatui::buffer::Buffer) -> Vec<String> {
    let marked = crate::theme::picked_out().bg;
    (0..buffer.area.height)
        .filter_map(|row| {
            let text: String = (0..buffer.area.width)
                .filter(|&column| buffer[(column, row)].style().bg == marked)
                .map(|column| buffer[(column, row)].symbol().to_string())
                .collect();
            let text = text.trim_end().to_string();
            (!text.is_empty()).then_some(text)
        })
        .collect()
}

/// A session whose one reply is `lines` numbered lines, each on a row of its own.
fn a_long_reply(lines: usize) -> Session {
    let mut session = Session::new("none");
    let reply: Vec<String> = (0..lines)
        .map(|n| format!("row {n:02} of the reply"))
        .collect();
    session.complete(reply.join("\n"), Vec::new(), 0);
    session
}

fn wheel_up(session: &mut Session) {
    handle_mouse(session, drag(MouseEventKind::ScrollUp, 5, 5));
}

/// A transcript longer than the screen with a target of each kind beside it: a long reply to
/// select in, a last reply with its marker, the info panel with its rows, and a line in the box.
fn a_transcript_with_a_target_of_each_kind() -> (Session, ratatui::buffer::Buffer) {
    let (mut session, _) = panel_of_command_rows(false);
    let long: Vec<String> = (0..60)
        .map(|n| format!("row {n:02} of the reply"))
        .collect();
    session
        .transcript
        .push(crate::state::Entry::assistant(long.join("\n"), Vec::new()));
    session
        .transcript
        .push(crate::state::Entry::assistant(A_LAST_REPLY, Vec::new()));
    session.paste("draft text");
    let buffer = drawn_at(&mut session, 120, 24);
    (session, buffer)
}

/// The last reply of [`a_transcript_with_a_target_of_each_kind`].
const A_LAST_REPLY: &str = "Final answer.\nsecond\nthird\nfourth\nfifth\nsixth";

/// Release the button where the last event left it, through the loop's own function, and say
/// what the clipboard was handed.
fn let_go(session: &mut Session, size: (u16, u16), row: u16, column: u16) -> Vec<String> {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(size.0, size.1))
        .expect("terminal");
    let mut handed = Vec::new();
    answer_mouse(
        &mut terminal,
        session,
        drag(MouseEventKind::Up(MouseButton::Left), row, column),
        |text| {
            handed.push(text.to_string());
            true
        },
    )
    .expect("answered");
    handed
}

/// Where the transcript was last drawn.
fn the_transcript_pane(session: &Session) -> ratatui::layout::Rect {
    session.laid.window.expect("the transcript was drawn").area
}

fn at_a_step(start: Instant, steps: u32) -> Instant {
    start + select::EDGE_TICK * steps
}

/// A session with the info panel and a selection of the transcript held after a copy.
fn with_a_selection_held() -> Session {
    let (mut session, buffer) = two_panes();
    let (row, column) = find_cell(&buffer, "TRANSCRIPTMARK");
    dragged_and_released(&mut session, (120, 24), (row, column), (row, column + 10));
    assert!(
        session.holds_a_selection(),
        "the fixture holds no selection"
    );
    assert!(session.copied.is_some(), "the fixture copied nothing");
    session
}

/// A session editing vi's way. The choice is recorded nowhere, since a session that does not
/// persist reads no file and writes none.
fn editing_vis_way() -> Session {
    let mut session = Session::new("none");
    session.choose_editing(crate::vim::Editing::Vi);
    session
}

/// A press the way the turn's own loop takes it: read against [`stops_the_turn`] before any
/// ladder, and handed to the box otherwise.
fn pressed_during_a_turn(session: &mut Session, key: KeyEvent, cancel: &Cancel) {
    if stops_the_turn(session, key) {
        stop_what_is_running(session, cancel);
    } else {
        nothing_beside(|beside| turn_key(session, key, cancel, beside));
    }
}

/// A workspace for the turn's loop to lend `/status`, in tests of the other commands that read none of it.
fn a_workspace() -> Workspace {
    let directory = crate::testutil::scratch_dir("bravebot-app-beside");
    std::fs::create_dir_all(&directory).expect("scratch");
    Workspace::new(&directory).expect("workspace")
}

/// What a turn's loop holds beside the session, where a test reads none of it: a record that
/// is never written, and no home to keep an answer in. A `/rename` through it still makes the
/// store's directory for `/work`, so a test that renames runs in `in_isolated_profile`.
fn nothing_beside<R>(press: impl FnOnce(&mut Beside<'_>) -> R) -> R {
    beside_holding(&mut a_config_needing_no_sign_in(), press)
}

/// As [`nothing_beside`], with the configuration the turn was started with named.
fn beside_holding<R>(config: &mut Config, press: impl FnOnce(&mut Beside<'_>) -> R) -> R {
    let root = std::path::Path::new("/work");
    let mut stored = bravebot_session::sessions::Handle::begin(
        root,
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );
    press(&mut Beside {
        managed: &bravebot_config::Managed::default(),
        settings: &bravebot_config::Settings::default(),
        stored: &mut stored,
        root,
        home: None,
        workspace: &a_workspace(),
        scratch: None,
        config,
    })
}

/// Type a line, one key at a time, the way a user does.
fn type_line(session: &mut Session, line: &str) {
    for c in line.chars() {
        handle_key(session, key(KeyCode::Char(c)));
    }
}

/// A command line with nothing pasted into it, for a test about the word rather than the paste.
fn commanded(line: &str) -> crate::state::Commanded {
    crate::state::Commanded {
        attached: Vec::new(),
        line: line.to_string(),
        pasted: Vec::new(),
        typed_at: std::time::Instant::now(),
        names: crate::state::NamesFrom::nothing(),
    }
}

fn type_without_sending_while_working(session: &mut Session, line: &str) {
    for c in line.chars() {
        handle_key_while_working(session, key(KeyCode::Char(c)));
    }
}

/// A model picker open in `scope` over `session`, with the rows a listing of two models gave.
fn a_model_picker_open_in(
    session: &mut Session,
    config: &Config,
    scope: crate::model_prompt::Scope,
) {
    let (answer, listing) = std::sync::mpsc::channel();
    open_model_picker_awaiting(session, config, listing, scope);
    answer
        .send(Ok((
            vec![listed("opus-arn", None), listed("some-model", None)],
            Vec::new(),
        )))
        .unwrap();
    assert!(session.settle_picking());
    assert!(session.picking().is_some(), "the picker did not open");
}

fn the_scope_of_the_open_picker(session: &Session) -> crate::model_prompt::Scope {
    let Some(crate::picking::Picking::Model(picker)) = session.picking() else {
        panic!("no model picker is open");
    };
    picker.scope()
}

fn the_scope_of_the_open_effort_picker(session: &Session) -> crate::model_prompt::Scope {
    let Some(crate::picking::Picking::Effort(picker)) = session.picking() else {
        panic!("no effort picker is open");
    };
    picker.scope().expect("an effort picker always has a scope")
}

fn enter_line(session: &mut Session, line: &str) -> Action {
    for c in line.chars() {
        handle_key(session, key(KeyCode::Char(c)));
    }
    handle_key(session, key(KeyCode::Enter))
}

fn drafted(message: &str) -> bravebot_agent::commit_message::Drafted {
    bravebot_agent::commit_message::Drafted {
        message: Some(message.to_string()),
        partial: false,
        substituted: None,
        usage: bravebot_aichat::protocol::Usage::default(),
    }
}

/// A plan of four tasks, one in each status.
fn a_plan_of_four_tasks() -> Vec<bravebot_core::todo::Row> {
    use bravebot_core::todo::{Item, List, Status};
    bravebot_core::todo::rows(&List::new(vec![
        Item::new("read the spec", Status::Done),
        Item::new("drop the old test", Status::Cancelled),
        Item::new("write the test", Status::Active),
        Item::new("run the checks", Status::Pending),
    ]))
}

/// The name and bytes of every file in the isolated profile's state directory, in name order,
/// leaving out the files the box keeps the lines typed into it in. Those are written for every
/// line submitted, a command included, and say nothing about what the command recorded.
fn files_of_the_profile() -> Vec<(String, Vec<u8>)> {
    let Some(directory) = bravebot_session::store::directory() else {
        return Vec::new();
    };
    let mut files: Vec<(String, Vec<u8>)> = std::fs::read_dir(&directory)
        .map(|entries| {
            entries
                .map(|entry| {
                    let path = entry.expect("an entry").path();
                    (
                        path.file_name().unwrap().to_string_lossy().into_owned(),
                        std::fs::read(&path).unwrap_or_default(),
                    )
                })
                .filter(|(name, _)| !["history", "history.lock"].contains(&name.as_str()))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
}

/// An `/effort` line typed at rest and carried out the way the loop carries out what Enter on
/// it dispatches.
fn carry_out_effort_line(session: &mut Session, line: &str) {
    match enter_line(session, line) {
        Action::SetEffort(level, scope) => set_effort(session, &level, scope),
        Action::ChooseEffort(scope) => open_effort_picker(session, scope),
        other => panic!("{line:?} was not an effort command: {other:?}"),
    }
}

/// A turn running with the brief `/handoff` wrote still in the box, as a loop tick or a queued
/// prompt leaves it: the brief arrives at rest and the turn begins without anyone pressing.
fn a_turn_running_over_a_brief(brief: &str) -> Session {
    let mut session = a_turn_running_on("first");
    session.status = Status::Idle;
    session.propose_handoff(brief);
    session.status = Status::Working;
    assert!(session.handoff_pending() && session.a_turn_is_running());
    session
}

/// A scratch workspace holding `README.md`.
fn a_workspace_with_a_readme() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("a scratch directory");
    std::fs::write(root.path().join("README.md"), "# notes").expect("write");
    root
}

/// The task `run_turn_animated` builds for the turn the session has begun, through
/// `the_turn_task`, with the settings, permissions and session record a real turn passes.
fn a_task_built_for(session: &mut Session, root: &std::path::Path, prompt: &str) -> Task {
    let authority = bravebot_core::file_authority::FileAuthority::new(
        bravebot_agent::workspace::trust_store(root),
    );
    a_task_built_with(session, root, prompt, &authority)
}

/// [`a_task_built_for`] with the file authority the turn's decisions are recorded in, for a test
/// that reads the rules the turn left.
fn a_task_built_with(
    session: &mut Session,
    root: &std::path::Path,
    prompt: &str,
    authority: &bravebot_core::file_authority::FileAuthority,
) -> Task {
    let workspace = Workspace::new(root).expect("workspace");
    let stored = bravebot_session::sessions::Handle::begin(
        root,
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );
    let run = RunSettings::from_settings(&bravebot_config::Settings::default());
    let wrote = whose_line(session);
    the_turn_task(
        &run,
        session,
        &Permissions::new(),
        &workspace,
        &stored,
        &bravebot_core::credentials::Exposed::default(),
        authority,
        prompt,
        wrote,
        None,
        false,
    )
    .into_task()
}

/// The set a person's line is compared against, as a session with one definition of their
/// own resolves it.
fn a_resolved_set(model: Option<&str>) -> bravebot_core::delegate::Definitions {
    let mut definitions = bravebot_core::delegate::Definitions::default();
    let definition = bravebot_core::delegate::Definition::from_file(
        "rule-reviewer",
        "Checks a diff.",
        bravebot_core::delegate::Kind::Reader,
        None,
        "REVIEW",
        "~/.bravebot/agents",
    );
    definitions.insert(match model {
        Some(model) => definition.with_model(model),
        None => definition,
    });
    definitions
}

/// A configuration whose models need nothing signed in.
fn a_config_needing_no_sign_in() -> Config {
    Config::from_lookup(|key| match key {
        "SERVICES_KEY_AICHAT" => Some("test-key".into()),
        "BRAVE_SERVICES_KEY_ID" => Some("test-id".into()),
        "BRAVE_AI_CHAT_ENDPOINT" => Some("http://unused.invalid".into()),
        _ => None,
    })
    .expect("config")
}

/// Addresses whatever the session's line addressed, as the loop does once it has read the set.
fn addressing(session: &mut Session, argument: &str, model: Option<&str>) -> Action {
    address(
        session,
        &a_config_needing_no_sign_in(),
        &a_resolved_set(model),
        argument,
        Vec::new(),
        Vec::new(),
        crate::state::NamesFrom::nothing(),
    )
}

/// The definition a session started under `--agent` works under, as the command line leaves
/// it on the session.
fn standing() -> crate::state::Addressed {
    crate::state::Addressed {
        name: "rule-reviewer".to_string(),
        model: None,
    }
}

/// A home holding one definition of the person's own, for the tests of a resumed session.
fn a_home_with_a_definition(label: &str, model: Option<&str>) -> std::path::PathBuf {
    let home = crate::testutil::scratch_dir(label);
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join("agents")).expect("scratch");
    let model = model.map_or(String::new(), |model| format!("model: {model}\n"));
    std::fs::write(
        home.join("agents").join("rule-reviewer.md"),
        format!(
            "---\nname: rule-reviewer\ndescription: Checks a diff.\nkind: reader\n{model}---\n\nREVIEW\n"
        ),
    )
    .expect("write the definition");
    home
}

fn a_handle_for_test(workspace: &Workspace) -> bravebot_session::sessions::Handle {
    bravebot_session::sessions::Handle::begin(
        workspace.root(),
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    )
}

fn kept_checkout(worked_in: bool) -> bravebot_agent::workspace::SessionCheckout {
    bravebot_agent::workspace::SessionCheckout {
        applied: false,
        id: "c2".into(),
        path: "/state/checkouts/work/c2".into(),
        repository: "/work/.git".into(),
        commit: "0123456789abcdef0123456789abcdef01234567".into(),
        delegate: bravebot_core::delegate::DelegateId::nth(1),
        worked_in,
        candidates: Default::default(),
        size: None,
    }
}

fn last_note(session: &Session) -> &str {
    session
        .transcript
        .last()
        .map_or("", |entry| entry.text.as_str())
}

fn notes_of(session: &Session) -> Vec<&str> {
    session
        .transcript
        .iter()
        .map(|entry| entry.text.as_str())
        .collect()
}

/// Skills as a session resolving them would hold them. `mode` shares its letters with a
/// command, which is what lets a test tell a finished skill name from a half-typed command.
fn skills() -> Vec<crate::skills::Skill> {
    use bravebot_agent::skills::Source;
    [
        ("review-pr", Source::Home),
        ("release-notes", Source::Workspace),
        ("retro", Source::Home),
        ("mod", Source::Home),
    ]
    .into_iter()
    .map(|(name, source)| crate::skills::Skill {
        name: name.to_string(),
        description: format!("what {name} is for"),
        argument_hint: None,
        source: source.into(),
        person_only: false,
    })
    .collect()
}

/// Type a line the way the loop takes it: a key, then the skills settled before the next one.
fn type_with_skills(session: &mut Session, line: &str) {
    for c in line.chars() {
        handle_key(session, key(KeyCode::Char(c)));
        session.settle_skills(skills);
    }
}

/// A request whose bytes are known by role and origin, for `/context`.
fn a_request_of_known_parts() -> bravebot_agent::request_view::RequestView {
    use bravebot_agent::request_view::{Provenance, RequestView, Span};
    let span = |role, provenance, size: usize| Span {
        role,
        provenance,
        text: "x".repeat(size),
    };
    RequestView {
        model: "m".to_string(),
        spans: vec![
            span("system", Provenance::Driver, 300),
            span("system", Provenance::TrustedFile("AGENTS.md".into()), 100),
            span("system", Provenance::Trusted("skill list"), 50),
            span("user", Provenance::Typed, 80),
            span("assistant", Provenance::Planner, 120),
            // Words that read like another section's name; the origin decides, not the words.
            Span {
                role: "tool",
                provenance: Provenance::Reference("ref:1".into()),
                text: format!("Tool definitions{}", "y".repeat(234)),
            },
        ],
        tools: Vec::new(),
        tools_bytes: 100,
    }
}

/// What each row of `/context` says, as the number of tokens it names.
fn context_rows(session: &Session) -> Vec<(String, u64)> {
    said_in_the_transcript(session)
        .into_iter()
        .filter_map(|row| {
            let words: Vec<&str> = row.split_whitespace().collect();
            let at = words.iter().position(|word| *word == "tokens")?;
            let figure = words[at - 1].parse().ok()?;
            Some((words[..at - 1].join(" "), figure))
        })
        .collect()
}

/// A turn sent and answered with `reply`, as the loop records one.
fn answered(session: &mut Session, prompt: &str, reply: &str) {
    type_line(session, prompt);
    assert!(matches!(
        handle_key(session, key(KeyCode::Enter)),
        Action::Submit(_)
    ));
    session.complete(reply, Vec::new(), 0);
}

/// What a `/copy` line answered: the text it would copy, or what it said instead.
fn copied_by(session: &mut Session, line: &str) -> Result<String, String> {
    type_line(session, line);
    match handle_key(session, key(KeyCode::Enter)) {
        Action::CopyReply(text) => Ok(text),
        Action::Redraw => Err(said_in_the_transcript(session)
            .last()
            .cloned()
            .unwrap_or_default()),
        other => panic!("{line} answered {other:?}"),
    }
}

/// A session that has typed one prompt and has a turn in flight on it.
fn a_turn_running_on(prompt: &str) -> Session {
    let mut session = Session::new("none");
    type_line(&mut session, prompt);
    handle_key(&mut session, key(KeyCode::Enter));
    assert!(session.a_turn_is_running());
    session
}

fn type_while_working(session: &mut Session, line: &str) -> Action {
    type_while_working_without_enter(session, line);
    handle_key_while_working(session, key(KeyCode::Enter))
}

fn type_while_working_without_enter(session: &mut Session, line: &str) {
    for c in line.chars() {
        handle_key_while_working(session, key(KeyCode::Char(c)));
    }
}

/// A line typed during a turn and sent with `send`, each press taken the way the turn's own
/// loop takes it, with what that loop holds beside the session.
fn typed_during_a_turn(
    session: &mut Session,
    line: &str,
    send: KeyEvent,
    beside: &mut Beside<'_>,
) -> Cancel {
    let cancel = Cancel::new();
    for c in line.chars() {
        turn_key(session, key(KeyCode::Char(c)), &cancel, beside);
    }
    turn_key(session, send, &cancel, beside);
    cancel
}

fn said_under_the_turn(session: &Session) -> Vec<&str> {
    session
        .said_while_working()
        .iter()
        .map(|said| said.text.as_str())
        .collect()
}

/// What typing a `/btw` beside a turn says back, naming the key that opens the answer.
fn asked_note(session: &Session) -> String {
    t!(
        btw_asked_beside_a_turn,
        chord = session.bindings().watch_name()
    )
}

fn waiting_prompts(session: &Session) -> Vec<&str> {
    session
        .queued
        .iter()
        .map(|queued| queued.prompt.as_str())
        .collect()
}

/// A running turn that has sent its first request, so a `/btw` has an exchange to be asked over.
fn a_turn_that_has_sent_a_request() -> Session {
    let mut session = a_turn_running_on("first");
    session.set_fork(bravebot_agent::aside::Fork::of(&Conversation::new()));
    session.set_turn_files(Some(bravebot_core::file_authority::FileAuthority::new(
        bravebot_core::trust::TrustStore::new("/work"),
    )));
    session
}

/// A loopback endpoint that accepts a connection and never answers it, so a worker started
/// against it makes no request beyond this machine and waits until it is stopped.
fn a_model_that_never_answers() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let address = format!("http://{}", listener.local_addr().expect("an address"));
    std::thread::spawn(move || {
        let mut held = Vec::new();
        for stream in listener.incoming().flatten() {
            held.push(stream);
        }
    });
    address
}

/// `line` typed and sent during the turn, as the turn's loop takes it. A question it starts is
/// asked of a model that never answers, and stays out until it is stopped.
fn typed_beside_a_turn(session: &mut Session, line: &str) -> Cancel {
    let root = std::path::Path::new("/work");
    let mut stored = bravebot_session::sessions::Handle::begin(
        root,
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );
    let endpoint = a_model_that_never_answers();
    let mut config = Config::from_lookup(|key| match key {
        "SERVICES_KEY_AICHAT" => Some("test-key".into()),
        "BRAVE_SERVICES_KEY_ID" => Some("test-id".into()),
        "BRAVE_AI_CHAT_ENDPOINT" => Some(endpoint.clone()),
        _ => None,
    })
    .expect("config");
    typed_during_a_turn(
        session,
        line,
        key(KeyCode::Enter),
        &mut Beside {
            managed: &bravebot_config::Managed::default(),
            settings: &bravebot_config::Settings::default(),
            stored: &mut stored,
            root,
            home: None,
            workspace: &a_workspace(),
            scratch: None,
            config: &mut config,
        },
    )
}

/// A flight whose worker returns `done` once the test drops the sender it is given back, so the
/// worker is out until the test says it has ended.
fn a_question_out_beside_a_turn(
    session: &mut Session,
    done: crate::state::AsideDone,
) -> std::sync::mpsc::Sender<crate::remote_confirm::ToMain> {
    let (sender, from_worker) = std::sync::mpsc::channel();
    session.begin_aside_beside_a_turn(crate::state::AsideFlight {
        asked: "what is in parser.py".to_string(),
        cancel: Cancel::new(),
        from_worker,
        worker: std::thread::spawn(move || done),
    });
    sender
}

/// A scratch directory for a transcript to be written into, with a record begun under it.
fn exporting_into(name: &str) -> (std::path::PathBuf, bravebot_session::sessions::Handle) {
    let root = crate::testutil::scratch_dir(name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch");
    let stored = bravebot_session::sessions::Handle::begin(
        &root,
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );
    (root, stored)
}

/// `/issue` and `/pr` as the event loop carries them out, at rest.
fn linked(
    session: &mut Session,
    stored: &mut bravebot_session::sessions::Handle,
    line: &str,
) -> String {
    match dispatch_command(session, commanded(line)) {
        Action::Link(kind, argument) => link_session(session, stored, kind, &argument),
        other => panic!("{line} became {other:?}"),
    }
    last_note(session).to_string()
}

/// A session that persists, with a turn in flight on `prompt`. For a test that is about what a
/// command keeps, so it runs in `in_isolated_profile`.
fn a_persisting_turn_running_on(prompt: &str) -> Session {
    let mut session = Session::new("none").with_stored_history();
    type_line(&mut session, prompt);
    handle_key(&mut session, key(KeyCode::Enter));
    assert!(session.a_turn_is_running());
    session
}

/// A configuration whose roster is three named tiers and nothing fetched, so the picker is
/// offered a list without a request leaving the test.
fn a_config_offering_three_tiers() -> Config {
    use bravebot_config::env_var;
    Config::from_lookup(|name| match name {
        env_var::USE_BEDROCK => Some("1".into()),
        env_var::AWS_REGION => Some("us-west-2".into()),
        env_var::BEDROCK_OPUS_MODEL => Some("opus-arn".into()),
        env_var::BEDROCK_SONNET_MODEL => Some("sonnet-arn".into()),
        env_var::BEDROCK_HAIKU_MODEL => Some("haiku-arn".into()),
        _ => None,
    })
    .expect("a configuration")
}

/// Settings for a directory whose checkout requests `aliases`, as `/mcp` reads them.
fn settings_requesting_servers(name: &str, aliases: &[&str]) -> bravebot_config::Settings {
    let directory = tempfile::Builder::new()
        .prefix(name)
        .tempdir()
        .expect("directory")
        .keep();
    std::fs::create_dir_all(directory.join(".bravebot")).expect("settings directory");
    std::fs::write(
        directory.join(".bravebot/settings.json"),
        serde_json::json!({ "mcp": { "request": aliases } }).to_string(),
    )
    .expect("settings");
    bravebot_config::Settings::layered(None, Some(&directory), None)
}

/// `line` typed and sent during a turn, carried out beside settings that request `weather` and
/// `docs`, with `docs` turned off for the session.
fn mcp_typed_during_a_turn(session: &mut Session, line: &str) {
    let settings = settings_requesting_servers("bravebot-mcp-mid-turn", &["weather", "docs"]);
    mcp_typed_during_a_turn_beside(session, line, &settings, None);
}

/// `line` typed and sent during a turn, carried out beside `settings` and the state directory
/// `home`, which is where the installed plugins are read from.
fn mcp_typed_during_a_turn_beside(
    session: &mut Session,
    line: &str,
    settings: &bravebot_config::Settings,
    home: Option<&std::path::Path>,
) {
    let root = std::path::Path::new("/work");
    let mut stored = bravebot_session::sessions::Handle::begin(
        root,
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );
    typed_during_a_turn(
        session,
        line,
        key(KeyCode::Enter),
        &mut Beside {
            managed: &bravebot_config::Managed::default(),
            settings,
            stored: &mut stored,
            root,
            home,
            workspace: &a_workspace(),
            scratch: None,
            config: &mut a_config_needing_no_sign_in(),
        },
    );
}

/// What `/mcp` says for the settings of `mcp_typed_during_a_turn` while `docs` is turned off.
fn the_servers_listed() -> Vec<String> {
    vec![
        t!(mcp_command_not_running).to_string(),
        format!("  {}", t!(mcp_command_not_connected, alias = "weather")),
        format!("  {}", t!(mcp_command_off, alias = "docs")),
        t!(mcp_command_remove_hint).to_string(),
    ]
}

/// A line waiting for the turn to end, as `line` would have become had the turn been running
/// when it was typed behind others: a command line when it starts with `!`, a command that
/// waits whatever it is when it starts with a slash, and a prompt otherwise.
fn wait_as(session: &mut Session, line: &str) {
    if let Some(command_line) = line.strip_prefix('!') {
        session.shell = true;
        type_while_working(session, command_line);
        return;
    }
    if !line.starts_with('/') {
        type_while_working(session, line);
        return;
    }
    for c in line.chars() {
        handle_key_while_working(session, key(KeyCode::Char(c)));
    }
    assert!(session.queue_command(), "{line} was not queued");
}

thread_local! {
    /// What the fake `/schedule` seam was called with: the words and the session's directory.
    static SCHEDULE_CALLS: std::cell::RefCell<Vec<(Vec<String>, std::path::PathBuf)>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// The sender for the answer of the `run-now` the fake started.
    static SCHEDULE_RUN: std::cell::RefCell<Option<std::sync::mpsc::Sender<bravebot_schedule::Reply>>> =
        const { std::cell::RefCell::new(None) };
}

fn schedule_reply(
    outcome: bravebot_schedule::Outcome,
    out: &[&str],
    err: Option<&str>,
) -> bravebot_schedule::Reply {
    bravebot_schedule::Reply {
        outcome,
        out: out.iter().map(|line| (*line).to_string()).collect(),
        err: err.map(str::to_string),
        notice: None,
    }
}

/// A stand-in for the shared crate's session entry point that touches neither the state
/// directory nor the system scheduler. It records what it was called with and answers by the
/// first word.
fn fake_schedule(
    words: &[String],
    _typed: bravebot_schedule::TypedLine,
    project: &std::path::Path,
) -> bravebot_schedule::InSession {
    use bravebot_schedule::{InSession, Outcome};
    SCHEDULE_CALLS.with(|calls| {
        calls
            .borrow_mut()
            .push((words.to_vec(), project.to_path_buf()));
    });
    match words.first().map(String::as_str) {
        Some("remove") => {
            InSession::Ended(schedule_reply(Outcome::Argument, &[], Some("no entry 9")))
        }
        Some("run-now") => {
            let (sender, receiver) = std::sync::mpsc::channel();
            SCHEDULE_RUN.with(|run| *run.borrow_mut() = Some(sender));
            InSession::Running(receiver)
        }
        Some("hostile") => InSession::Ended(schedule_reply(
            Outcome::Done,
            &["a\u{1b}[2Jb\rc\u{7}d", "second"],
            None,
        )),
        Some("noticing") => InSession::Ended(bravebot_schedule::Reply {
            outcome: Outcome::Failed,
            out: vec!["first".to_string(), "second".to_string()],
            err: Some("the sentence".to_string()),
            notice: Some("the notice".to_string()),
        }),
        _ => InSession::Ended(schedule_reply(
            Outcome::Done,
            &["1  0 9 * * *  read  active"],
            None,
        )),
    }
}

fn schedule_calls() -> Vec<Vec<String>> {
    SCHEDULE_CALLS.with(|calls| {
        calls
            .borrow()
            .iter()
            .map(|(words, _)| words.clone())
            .collect()
    })
}

fn a_session_scheduling_through_the_fake() -> Session {
    SCHEDULE_CALLS.with(|calls| calls.borrow_mut().clear());
    let mut session = Session::new("none");
    session.schedule = fake_schedule;
    session
}

fn ctrl_enter() -> KeyEvent {
    KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL)
}

/// A session that has sent `prompts`, with nothing running.
fn having_sent(prompts: &[&str]) -> Session {
    let mut session = Session::new("none");
    for prompt in prompts {
        type_line(&mut session, prompt);
        handle_key(&mut session, key(KeyCode::Enter));
        session.complete("ok", Vec::new(), 0);
    }
    session
}

fn alt(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::ALT)
}

/// The directory those sessions ran in, which is the one their rules are read under.
fn here() -> &'static std::path::Path {
    std::path::Path::new("/tmp/x")
}

/// A session one finished turn in, with a goal and a loop running and a record written, the
/// state `/branch` is typed in.
fn a_session_to_branch(
    root: &std::path::Path,
) -> (
    Workspace,
    Session,
    bravebot_session::sessions::Handle,
    Conversation,
    Answers,
) {
    use bravebot_aichat::protocol::Message;
    use bravebot_session::sessions::{self, Standing};

    let root = root.to_path_buf();
    let workspace = Workspace::new(&root).expect("a workspace");
    let trust = TrustStore::new(&root);
    let programs = TrustedPrograms::new();
    let mut session = Session::new("none");
    let rules = starting_rules(&mut session, &root.join("state"), workspace.root());
    let answers = Answers::opening(trust, programs, rules);
    let mut stored =
        sessions::Handle::begin(&root, sessions::Front::Terminal, bravebot_stamp::BUILD);
    let mut conversation = Conversation::new();

    let prompt = "write a line saying hello into notes.txt";
    let start = conversation.recounted().len();
    type_line(&mut session, prompt);
    session.submit().expect("the prompt is sent");
    let point = rewind_point(
        &session,
        &conversation,
        &answers.trust,
        &answers.programs,
        &stored,
    );
    session.rewind.open(point, prompt.to_string());
    session.prompt_recorded(conversation.recounted().len());
    conversation.push(Message::user(prompt));
    conversation.push(Message::assistant("written"));
    session.complete("written", vec![], 10);
    session.record_turn(start, &conversation);
    stored.save(
        prompt,
        Standing {
            history: Some(session.turn_history()),
            conversation: &conversation.snapshot(),
            turns: session.turns,
            tokens: session.tokens,
            split: Default::default(),
            spend: session.spend_by_turn(),
            timing: session.timing_by_turn(),
            model: None,
            todos: &session.todos_by_turn(),
            asides: &[],
            trust: &answers.trust,
            programs: &answers.programs,
            directories: &[],
            manifest: None,
            rewind: session.rewind.points(),
            checkouts: &[],
            touched: None,
            answered_by_mode: false,
        },
    );
    session.start_goal("cargo test exits 0".to_string());
    session.start_loop(
        crate::loops::request("watch"),
        Vec::new(),
        Vec::new(),
        crate::state::NamesFrom::nothing(),
    );
    assert!(!session.rewind.points().is_empty());
    (workspace, session, stored, conversation, answers)
}

/// One pass of `settle_between_frames`, which the loop runs before each frame, over the state
/// `a_session_to_branch` builds, with no suggestion in flight and no watch. Whether the pass
/// asked for a draw.
fn settled(
    session: &mut Session,
    workspace: &Workspace,
    stored: &mut bravebot_session::sessions::Handle,
    conversation: &Conversation,
    answers: &Answers,
) -> bool {
    let mut config = a_config_needing_no_sign_in();
    let mut needs_draw = false;
    settle_between_frames(
        session,
        &mut config,
        workspace,
        stored,
        conversation,
        answers,
        &mut None,
        None,
        &mut needs_draw,
    );
    needs_draw
}

/// What a session holds once it has vouched for a program, drawn a run prompt for it, agreed a
/// file may be shown and built its language servers.
fn give_answers(answers: &mut Answers, root: &std::path::Path) {
    let make = bravebot_core::programs::Command::new("/usr/bin/make", vec!["check".into()], root);
    answers.programs.trust(make.clone());
    answers.asked_about.record(make);
    answers.exposed.allow(".env");
    answers.servers = Some(LanguageServers::new(root, None));
}

/// A terminal nothing is drawn on: a session opened or cleared bypassing every permission puts
/// no question to it.
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn a_fixed_viewport_terminal() -> ratatui::Terminal<ratatui::backend::CrosstermBackend<io::Stdout>>
{
    ratatui::Terminal::with_options(
        ratatui::backend::CrosstermBackend::new(io::stdout()),
        ratatui::TerminalOptions {
            viewport: ratatui::Viewport::Fixed(ratatui::layout::Rect::new(0, 0, 20, 5)),
        },
    )
    .expect("a terminal")
}

/// Begin the session a `/resume` hands the record to, as the loop's caller does.
#[expect(clippy::panic, reason = "a test fails by panicking")]
fn begun_from_a_switch(switch: Switch) -> Begun {
    let mut terminal = a_fixed_viewport_terminal();
    let mut config = a_config_needing_no_sign_in();
    let mut servers = crate::state::Servers::default();
    begin_session(
        &mut terminal,
        &mut config,
        &switch.workspace,
        &mut servers,
        Startup {
            confinement: String::new(),
            sockets_with_a_path_reachable: false,
            sandbox: switch.sandbox,
            start: Start::Resuming(switch.record),
            named: None,
            links: Links::default(),
            skip_permissions: true,
            asked_for_auto: false,
            prompts: bravebot_agent::turn::SystemPrompts::default(),
        },
    )
    .unwrap_or_else(|ended| panic!("the next session did not begin: {ended:?}"))
}

/// What a session left by `/resume` was standing on beside the conversation.
#[derive(Clone, Copy)]
enum Standing {
    Loop,
    Goal,
    Watch,
}

/// Put the session into bypass, which settles the questions a new session opens with.
#[expect(clippy::panic, reason = "a test fails by panicking")]
fn bypassing(session: &mut Session) {
    for _ in 0..8 {
        if session.permission_mode() == bravebot_agent::PermissionMode::Bypass {
            return;
        }
        session.cycle_permission_mode();
    }
    panic!("the fixture would put a question to the terminal");
}

/// Rewrite the stored record `id` of `root` after `edit`, so a session is one that a record
/// another program or a private conversation would have made, which `a_session_to_branch` does
/// not store.
fn change_stored_session(
    root: &std::path::Path,
    id: &str,
    edit: impl FnOnce(&mut bravebot_session::sessions::Record),
) {
    let mut record = bravebot_session::sessions::load(root, id).expect("the record is stored");
    edit(&mut record);
    let path = bravebot_session::sessions::project_directory(root)
        .expect("a sessions directory")
        .join(format!("{id}.json"));
    std::fs::write(
        path,
        serde_json::to_string(&record).expect("the record as json"),
    )
    .expect("write the record");
}

/// The state before some turn, for a test that only needs a point to exist.
fn a_point_before(turns: usize) -> bravebot_session::sessions::TurnSnapshot {
    bravebot_session::sessions::TurnSnapshot {
        conversation: Conversation::new().snapshot(),
        turns,
        tokens: 0,
        split: Default::default(),
        spend: std::collections::BTreeMap::new(),
        timing: std::collections::BTreeMap::new(),
        cached: None,
        cached_prompt_tokens: None,
        trust: TrustStore::new("/work"),
        programs: TrustedPrograms::new(),
        transcript_len: turns,
        title: "a session".to_string(),
        was_wrote: true,
        work: None,
        written: None,
    }
}

/// The rules a session opened in `root` holds once its person accepted whatever the checkout
/// proposed, with `state` standing in for `~/.bravebot` so nothing of the real one is read.
fn starting_rules(session: &mut Session, state: &std::path::Path, root: &std::path::Path) -> Rules {
    starting_rules_naming(session, state, root, None)
}

/// As [`starting_rules`], for a command line whose `--settings` named `named`.
fn starting_rules_naming(
    session: &mut Session,
    state: &std::path::Path,
    root: &std::path::Path,
    named: Option<&std::path::Path>,
) -> Rules {
    let sources = RuleSources {
        state: Some(state.to_path_buf()),
        named: named.map(std::path::Path::to_path_buf),
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
    Rules {
        sources,
        permissions,
    }
}

/// The answer for a move that must put no question.
fn never_asked(_: &[bravebot_agent::granted::Proposed], _: &mut String) -> Option<bool> {
    panic!("a question was put about a checkout's rules")
}

const CHECK: &str = r#"{"permissions": {"allow": ["Bash(bash scripts/check.sh)"]}}"#;

fn noted(session: &Session, text: &str) -> bool {
    session.transcript.iter().any(|entry| entry.text == text)
}

/// A workspace for the tests below, which need one only because a finished turn takes the
/// first look at anything it asked to have watched.
fn workspace_for_test() -> Workspace {
    let root = crate::testutil::scratch_dir("bravebot-fold-outcome-workspace");
    std::fs::create_dir_all(&root).expect("scratch");
    Workspace::new(&root).expect("workspace")
}

/// What the session put in front of the person, one entry per line.
fn said_in_the_transcript(session: &Session) -> Vec<String> {
    session
        .viewed()
        .iter()
        .map(|entry| entry.text.clone())
        .collect()
}

/// A record written by another build, on a branch this checkout is not on, that spent tokens
/// and had `/add-dir` open `extra`.
fn a_recorded_session_in(
    root: &std::path::Path,
    extra: &std::path::Path,
) -> bravebot_session::sessions::Record {
    serde_json::from_value(serde_json::json!({
        "id": "7-8",
        "directory": root.display().to_string(),
        "branch": "a-branch-this-checkout-is-not-on",
        "title": "an earlier session",
        "started": 1,
        "updated": 1,
        "tokens": 4321,
        "build": "an-older-build",
        "directories": [extra.display().to_string()],
        "conversation": {"messages": [], "context": "trusted"},
    }))
    .expect("a record")
}

/// Cancel the turn in flight the way the program does: through `finish_turn`, which is where a
/// stopped turn goes, and not through the fold a finished one goes to.
fn stop_through_finish_turn(session: &mut Session, text: &str) {
    finish_turn(
        session,
        &a_config_needing_no_sign_in(),
        &workspace_for_test(),
        Line {
            text,
            addressed: None,
            offered_a_later_look: false,
        },
        FinishedTurn {
            decisions: None,
            work: bravebot_agent::Work::default(),
            outcome: Err(turn::TurnError::Cancelled { attempts: None }),
            conversation: Conversation::new(),
            sink: Trail::new(),
            servers: None,
        },
        RetainedTurn {
            files: bravebot_core::file_authority::FileAuthority::new(TrustStore::new("/work")),
            programs: TrustedPrograms::new(),
            asked: AskedAbout::new(),
            exposed: bravebot_core::credentials::Exposed::new(),
            began: Conversation::new().mark(),
        },
    );
}

use bravebot_agent::{Ending, PermissionMode};

fn offered(text: &str) -> Session {
    let mut session = Session::new("none");
    session.adopt_suggestions(true);
    session.offer_suggestion(text.to_string());
    session
}

fn pressed(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

mod backends_accounts;
mod backends_effort;
mod backends_models;
mod box_text;
mod caching_note;
mod chrome;
mod clicks;
mod command_history;
mod commands_behind_a_waiting_line;
mod commands_effort;
mod commands_mid_turn;
mod commands_other;
mod commands_words;
mod completions;
mod dropping;
mod handover;
mod hangup;
#[cfg(unix)]
mod herdr;
mod hints;
mod history_search;
mod hover;
mod input_history_and_box;
mod input_keys;
mod input_terminal;
mod input_turn;
mod input_vi;
mod instructions;
mod interrupt;
mod jump;
mod loops_and_goals;
mod mentioned_sessions;
mod menu;
mod mouse_controls;
mod mouse_selection;
mod mouse_transcript_clicks;
mod naming;
mod panel;
mod pasting;
mod peeking;
mod permissions;
mod pickers;
mod pinned;
mod popups;
mod remainder;
mod scrollbar;
mod scroller;
mod selection;
mod sessions_records;
mod sessions_rewinding;
mod shell_mode;
mod standing;
mod startup_input;
mod summoning;
mod trust_paths;
mod view;
mod waiting_commands;
mod watching;
mod wide;
mod wrapped;
