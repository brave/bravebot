//! MOUSE-44: no mouse event answers a question the interface puts to a person.
//!
//! Every question that waits for an answer is run twice over the same screen. The first run is
//! given only the keys a person presses at it. The second is given the same keys with a script of
//! pointer events at every cell of the screen between them. A question the pointer left alone reads
//! the whole script and ends on the last key, with the answer the first run gave.

use super::*;
use crate::testutil::Counting;
use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use std::path::Path;

/// The size of the screen each question is drawn on. Small enough that every cell can be pressed
/// in a test that runs in seconds, and large enough that the keys of a question are on it.
const COLUMNS: u16 = 44;
const ROWS: u16 = 16;

fn at(kind: MouseEventKind, column: u16, row: u16, modifiers: KeyModifiers) -> TermEvent {
    TermEvent::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers,
    })
}

/// A press, a double press, a drag, a release, the wheel, a middle and a right click, and a press
/// with each modifier held, on every cell of the screen.
fn pointer_over_every_cell() -> Vec<TermEvent> {
    use MouseButton::{Left, Middle, Right};
    use MouseEventKind::{Down, Drag, Moved, ScrollDown, ScrollUp, Up};
    let plain = KeyModifiers::NONE;
    let mut events = Vec::new();
    for row in 0..ROWS {
        for column in 0..COLUMNS {
            for (kind, modifiers) in [
                (Down(Left), plain),
                (Up(Left), plain),
                (Down(Left), plain),
                (Up(Left), plain),
                (Drag(Left), plain),
                (Down(Middle), plain),
                (Up(Middle), plain),
                (Down(Right), plain),
                (Up(Right), plain),
                (Down(Left), KeyModifiers::SHIFT),
                (Down(Left), KeyModifiers::CONTROL),
                (Down(Left), KeyModifiers::ALT),
                (ScrollUp, plain),
                (ScrollDown, plain),
                (Moved, plain),
            ] {
                events.push(at(kind, column, row, modifiers));
            }
        }
    }
    events
}

fn key(code: KeyCode) -> TermEvent {
    TermEvent::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn interrupt() -> TermEvent {
    TermEvent::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))
}

/// Asks one question over a terminal and says what came of it, as the text of the answer.
type Asks = Box<dyn Fn(&mut Terminal<Counting>) -> String + Send + Sync>;

/// One question, with the keys a person presses at it and the way to ask it.
struct Row {
    name: String,
    /// Keys before the pointer, which put the question in a state it has.
    before: Vec<TermEvent>,
    /// Keys after it, the last of which refuses the question.
    after: Vec<TermEvent>,
    ask: Asks,
}

fn row(
    name: &str,
    after: Vec<TermEvent>,
    ask: impl Fn(&mut Terminal<Counting>) -> String + Send + Sync + 'static,
) -> Row {
    Row {
        name: name.to_string(),
        before: Vec::new(),
        after,
        ask: Box::new(ask),
    }
}

/// The question with a line for a note open over it. The pointer is read there and then again over
/// the question once Escape has closed the line.
fn with_a_note_open(row: Row) -> Row {
    let mut after = vec![key(KeyCode::Esc)];
    after.extend(row.after);
    Row {
        name: format!("{} with the line for a note open", row.name),
        before: vec![key(KeyCode::Tab)],
        after,
        ask: row.ask,
    }
}

/// What the question answered when it was read `keys`, how many events it left unread, and whether
/// it read past the end.
fn asked(row: &Row, keys: Vec<TermEvent>) -> (String, usize, bool, usize) {
    let _script = crate::input::script::hold(keys);
    let mut terminal = Terminal::new(Counting::new(COLUMNS, ROWS)).expect("terminal");
    let answer = (row.ask)(&mut terminal);
    (
        answer,
        crate::input::script::unread(),
        crate::input::script::ran_dry(),
        terminal.backend().draws,
    )
}

/// The question read to the end of a script that ends on a key that refuses it, with an answer
/// that is the one that key gave when nothing else had been pressed.
fn assert_the_pointer_answered_nothing(row: &Row) {
    let keys: Vec<TermEvent> = row.before.iter().chain(&row.after).cloned().collect();
    let (control, unread, ran_dry, _) = asked(row, keys);
    assert!(
        !ran_dry && unread == 0,
        "{}: its own keys do not end it ({control})",
        row.name
    );

    let mut keys = row.before.clone();
    keys.extend(pointer_over_every_cell());
    keys.extend(row.after.clone());
    let (answer, unread, ran_dry, _) = asked(row, keys);
    assert!(
        !ran_dry,
        "{}: the question read past the end of the script and was answered with {answer}",
        row.name
    );
    assert_eq!(
        unread, 0,
        "{}: the pointer answered it with {answer} before the script ended",
        row.name
    );
    assert_eq!(
        answer, control,
        "{}: the pointer changed the answer",
        row.name
    );
}

/// The question of each kind a turn puts to a person, asked as the interface asks it. Matched with
/// no catch-all, so a kind added to the replies is not built until it has a row here.
fn questions_of(kind: Kind) -> Vec<Row> {
    match kind {
        Kind::Write => {
            let plain = || {
                row("write", vec![interrupt()], |terminal| {
                    format!("{:?}", ask(terminal, &request("fn main() {}\n", None)))
                })
            };
            let editing = row("write that may be edited", vec![interrupt()], |terminal| {
                let edited = ask_editing(terminal, &an_editable_write(), |_, _, _| {
                    panic!("a pointer event opened the editor")
                });
                format!("{edited:?}")
            });
            // Long enough that the body is more than the screen holds and the keys scroll it.
            let long = row("write of a long file", vec![interrupt()], |terminal| {
                let content = "let x = 1;\n".repeat(60);
                format!("{:?}", ask(terminal, &request(&content, None)))
            });
            vec![with_a_note_open(plain()), plain(), long, editing]
        }
        Kind::Run => {
            let plain = || {
                row("run", vec![interrupt()], |terminal| {
                    format!("{:?}", ask_run(terminal, &a_run(false)))
                })
            };
            vec![with_a_note_open(plain()), plain()]
        }
        Kind::ReadOutput => vec![row("read output", vec![interrupt()], |terminal| {
            format!("{:?}", ask_output(terminal, &an_output("some output\n")))
        })],
        Kind::Vet => vec![row("vet", vec![interrupt()], |terminal| {
            let request = a_vetting(Verdict::Unsafe, Some("it asks the reader to act"), "text");
            format!("{:?}", ask_vet(terminal, &request))
        })],
        Kind::Fetch => vec![row("fetch", vec![interrupt()], |terminal| {
            let request = a_metadata_fetch_behind_a_long_userinfo();
            format!("{:?}", ask_fetch(terminal, &request))
        })],
        Kind::Vouch => vec![row("vouch", vec![interrupt()], |terminal| {
            let request = a_vouch("notes.md", "a note", false);
            format!("{:?}", ask_vouch(terminal, &request))
        })],
        Kind::Exposure => vec![row("exposure", vec![interrupt()], |terminal| {
            let request = ExposureRequest {
                path: ".env".into(),
                credentials: vec!["an AWS access key id at .env:1".into()],
            };
            format!("{:?}", ask_exposure(terminal, &request))
        })],
        Kind::Server => vec![row("server", vec![interrupt()], |terminal| {
            let request = ServerRequest {
                language: "Rust".into(),
                program: "/usr/bin/rust-analyzer".into(),
                args: Vec::new(),
                workspace: "/work".into(),
                runs_build_tooling: true,
                declared: false,
            };
            format!("{:?}", ask_server(terminal, &request))
        })],
        Kind::Manifest => vec![row("manifest", vec![interrupt()], |terminal| {
            format!("{:?}", ask_manifest(terminal, &a_plan(&["move a note"])))
        })],
        Kind::ToolList => vec![row("tool list", vec![interrupt()], |terminal| {
            format!("{:?}", ask_tool_list(terminal, &tool_list()))
        })],
        Kind::McpCall => {
            let plain = || {
                row("mcp call", vec![interrupt()], |terminal| {
                    format!("{:?}", ask_mcp_call(terminal, &call(true, None)))
                })
            };
            vec![with_a_note_open(plain()), plain()]
        }
        Kind::Move => vec![row("move", vec![interrupt()], |terminal| {
            format!("{:?}", ask_move(terminal, &a_move(false)))
        })],
        Kind::Path => vec![row("path", vec![interrupt()], |terminal| {
            format!("{:?}", ask_path(terminal, &a_path_request(false)))
        })],
        Kind::Host => vec![row("host", vec![interrupt()], |terminal| {
            let request = HostRequest {
                hosts: vec!["example.com".into()],
            };
            format!("{:?}", ask_host(terminal, &request))
        })],
        // A short header, so the brief has more rows on the screen than a notch of the wheel moves.
        Kind::Consult => vec![row("consult", vec![interrupt()], |terminal| {
            let request = bravebot_agent::confirm::ConsultRequest {
                bot: "r".into(),
                definition: "r.md".into(),
                model: "m".into(),
                rounds: 8,
                budget: None,
                brief: "look at the parser\n".repeat(20),
                label: bravebot_core::label::Label::trusted_public(),
            };
            format!("{:?}", ask_consult(terminal, &request))
        })],
        Kind::Ask => vec![
            row(
                "a question the planner puts",
                vec![key(KeyCode::Esc)],
                |terminal| format!("{:?}", crate::ask::ask(terminal, &a_question_with_rows())),
            ),
            row(
                "a question the driver puts",
                vec![key(KeyCode::Esc)],
                |terminal| {
                    let asking = a_question_with_rows();
                    format!("{:?}", crate::ask::ask_as(terminal, &asking, "sandbox"))
                },
            ),
        ],
    }
}

fn a_question_with_rows() -> bravebot_core::ask::Asking {
    use bravebot_core::ask::{Asking, Prompt, Row};
    Asking {
        prompts: vec![Prompt {
            header: "Sandbox".into(),
            question: "Turn the sandbox off?".into(),
            rows: vec![
                Row {
                    index: 0,
                    label: "No".into(),
                    detail: None,
                },
                Row {
                    index: 1,
                    label: "Yes".into(),
                    detail: Some("Applies from the next turn.".into()),
                },
            ],
            multiple: false,
            key: "sandbox".into(),
        }],
        ..Default::default()
    }
}

fn a_session(id: &str, title: &str) -> bravebot_session::sessions::Summary {
    bravebot_session::sessions::Summary {
        id: id.into(),
        title: title.into(),
        directory: "/work".into(),
        front: Some("terminal".into()),
        branch: None,
        issue: None,
        pull_request: None,
        handed_off_from: None,
        updated: 100,
        bytes: 1024,
        manifest: false,
    }
}

/// The questions that are not a reply to a turn: put by a command, at startup, at a credential
/// read, or beside a list of sessions.
fn other_questions() -> Vec<Row> {
    let start = row("start a server", vec![interrupt()], |terminal| {
        let declaration = bravebot_config::mcp::Declaration::http("https://example.com/mcp".into())
            .expect("a declaration");
        let question = bravebot_agent::servers::Question {
            alias: "files",
            file: ".mcp.json",
            declaration: &declaration,
            program: None,
            changed: false,
        };
        format!("{:?}", ask_to_start(terminal, &question))
    });
    let remove = row("remove a checkout", vec![interrupt()], |terminal| {
        let request = a_checkout_forty_files_were_written_in();
        format!("{:?}", ask_remove_checkout(terminal, &request))
    });
    let trust = row(
        "trust the directory",
        vec![key(KeyCode::Char('n'))],
        |terminal| {
            let mut carried = String::new();
            let asked = crate::trust_prompt::ask(terminal, Path::new("/work"), None, &mut carried);
            format!("{:?}", asked.map(|(_, answer)| answer))
        },
    );
    let named = row(
        "trust a directory a settings file named",
        vec![key(KeyCode::Char('n'))],
        |terminal| {
            let mut carried = String::new();
            let directories = ["../other".to_string()];
            let asked = crate::trust_prompt::ask_named(terminal, &directories, &mut carried);
            format!("{asked:?}")
        },
    );
    let granted = row(
        "grant the rules a checkout proposed",
        vec![key(KeyCode::Char('n'))],
        |terminal| {
            let mut carried = String::new();
            let rules = [bravebot_agent::granted::Proposed::new(
                Path::new("/work/.bravebot/settings.json"),
                "run(git status)",
            )];
            let asked = crate::trust_prompt::ask_granted(terminal, &rules, &mut carried);
            format!("{asked:?}")
        },
    );
    let credential = row(
        "read a credential",
        vec![key(KeyCode::Char('k')), key(KeyCode::Enter)],
        |_| {
            let read = crate::hidden::answer_from(crate::input::read);
            format!(
                "{:?}",
                read.map(|secret| secret.map(|key| key.expose().to_string()))
            )
        },
    );
    let delete = Row {
        before: vec![key(KeyCode::Delete)],
        ..row(
            "delete a session's record",
            vec![key(KeyCode::Char('y')), key(KeyCode::Esc)],
            |terminal| {
                let picker =
                    crate::resume::Picker::new(vec![a_session("abcdef12", "notes")], "/work");
                let deleted = std::cell::RefCell::new(Vec::new());
                let choice =
                    crate::resume::converse(terminal, Path::new("/work"), picker, 3, |_, id| {
                        deleted
                            .borrow_mut()
                            .push((id.to_string(), crate::input::script::unread()));
                        Ok(())
                    });
                format!("{choice:?} {:?}", deleted.into_inner())
            },
        )
    };
    vec![start, remove, trust, named, granted, credential, delete]
}

/// Every question there is, one row each.
fn every_question() -> Vec<Row> {
    let mut rows: Vec<Row> = KINDS.iter().flat_map(|kind| questions_of(*kind)).collect();
    rows.extend(other_questions());
    rows
}

/// MOUSE-44. Rejects: a press, a release, a wheel turn or a click of another button, on any cell of
/// the screen, answering a question that waits on a person. A reader of events with an arm that
/// approved, granted or refused for a pointer would end the question before the script did, or on
/// something other than the key that refuses it.
#[test]
fn no_pointer_event_answers_any_question() {
    let rows = every_question();
    let failures: Vec<String> = std::thread::scope(|scope| {
        let handles: Vec<_> = rows
            .iter()
            .map(|row| scope.spawn(move || assert_the_pointer_answered_nothing(row)))
            .collect();
        rows.iter()
            .zip(handles)
            .filter_map(|(row, handle)| {
                handle.join().err().map(|panic| {
                    panic
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| panic.downcast_ref::<&str>().map(|text| (*text).to_string()))
                        .unwrap_or_else(|| format!("{}: panicked", row.name))
                })
            })
            .collect()
    });
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The questions that are read in `ask.rs`, `trust_prompt.rs`, `hidden.rs` and `resume.rs` and not by
/// the loops of `confirm.rs`, by the start of the name of the row.
const READ_IN_THEIR_OWN_FILES: &[&str] = &[
    "a question the planner puts",
    "a question the driver puts",
    "trust the directory",
    "trust a directory",
    "grant the rules",
    "read a credential",
    "delete a session",
];

/// MOUSE-44. A pointer crossing the window reports a position for each cell it crosses, and a
/// question that is drawn again for each lays its body out again for each, which for a long write is
/// a draw that takes longer than the pointer takes to cross a cell. Rejects: a question that draws
/// again for a position of the pointer, or for any report of the mouse that is not the wheel where it
/// is read by the loops of `confirm.rs`, which read nothing from the mouse but the wheel. Each
/// question is read a script of its own keys, and then the same keys with a position at every cell of
/// the screen between them, and draws the same number of frames.
#[test]
fn a_pointer_crossing_the_screen_asks_for_no_frame_of_any_question() {
    use MouseEventKind::{Down, Drag, Moved, Up};
    let positions: Vec<TermEvent> = (0..ROWS)
        .flat_map(|row| (0..COLUMNS).map(move |column| (row, column)))
        .map(|(row, column)| at(Moved, column, row, KeyModifiers::NONE))
        .collect();
    let buttons: Vec<TermEvent> = (0..ROWS)
        .flat_map(|row| (0..COLUMNS).map(move |column| (row, column)))
        .flat_map(|(row, column)| {
            [
                at(Down(MouseButton::Left), column, row, KeyModifiers::NONE),
                at(Drag(MouseButton::Left), column, row, KeyModifiers::NONE),
                at(Up(MouseButton::Left), column, row, KeyModifiers::NONE),
            ]
        })
        .collect();
    let mut asked_with_buttons = 0;
    for row in every_question() {
        let calm: Vec<TermEvent> = row.before.iter().chain(&row.after).cloned().collect();
        let (_, _, _, frames) = asked(&row, calm);
        let reads_the_mouse_itself = READ_IN_THEIR_OWN_FILES
            .iter()
            .any(|name| row.name.starts_with(name));
        let crossings = [
            Some(&positions),
            (!reads_the_mouse_itself).then_some(&buttons),
        ];
        for crossing in crossings.into_iter().flatten() {
            let mut keys = row.before.clone();
            keys.extend(crossing.iter().cloned());
            keys.extend(row.after.clone());
            let (answer, unread, ran_dry, drawn) = asked(&row, keys);
            assert!(
                !ran_dry && unread == 0,
                "{}: the script was not read to its end ({answer})",
                row.name
            );
            assert_eq!(
                drawn,
                frames,
                "{}: the pointer asked for {} frames more",
                row.name,
                drawn.saturating_sub(frames)
            );
            asked_with_buttons += usize::from(crossing.len() == buttons.len());
        }
    }
    assert!(
        asked_with_buttons >= 10,
        "the loops of `confirm.rs` were not tried with the buttons"
    );
}

/// What the question had on the screen when the last of `events` had been read, between the keys the
/// question is put with and the key that refuses it.
fn shown_after(row: &Row, events: Vec<TermEvent>) -> String {
    let mut keys = row.before.clone();
    keys.extend(events);
    keys.extend(row.after.clone());
    let _script = crate::input::script::hold(keys);
    let mut terminal = Terminal::new(Counting::new(COLUMNS, ROWS)).expect("terminal");
    let answer = (row.ask)(&mut terminal);
    assert!(
        !crate::input::script::ran_dry() && crate::input::script::unread() == 0,
        "{}: not ended by its own keys ({answer})",
        row.name
    );
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

/// MOUSE-46. Rejects: a turn of the wheel that does not move the body of a question that keys move,
/// that moves it another way or another distance than a notch of the wheel is, that moves it past
/// where the keys stop, or that answers: each question that scrolls under the keys shows the same
/// screen after one notch down as after the keys of the same number of rows, and after the notch
/// back up as after the keys that go back.
#[test]
fn a_turn_of_the_wheel_moves_the_body_of_a_question_as_the_keys_do() {
    use MouseEventKind::{ScrollDown, ScrollUp};
    let step = usize::from(crate::state::wheel_rows_named(None));
    let wheel = |kind| at(kind, COLUMNS / 2, ROWS / 2, KeyModifiers::NONE);
    let mut scrolled = Vec::new();
    for row in every_question() {
        let at_rest = shown_after(&row, Vec::new());
        // These read the terminal in files of their own and move a cursor among answers or edit a
        // line with the keys, which a body that scrolls does not. The wheel is not read in them.
        if READ_IN_THEIR_OWN_FILES
            .iter()
            .any(|name| row.name.starts_with(name))
        {
            assert_eq!(
                shown_after(&row, vec![wheel(ScrollDown), wheel(ScrollUp)]),
                at_rest,
                "{}: the wheel did something",
                row.name
            );
            continue;
        }
        let by_keys = shown_after(&row, (0..step).map(|_| key(KeyCode::Down)).collect());
        let by_wheel = shown_after(&row, vec![wheel(ScrollDown)]);
        assert_eq!(
            by_wheel, by_keys,
            "{}: a notch of the wheel is not {step} presses of Down",
            row.name
        );
        if by_keys == at_rest {
            continue;
        }
        scrolled.push(row.name.clone());
        // Back up by a notch is back up by the same keys. Not the screen it began with, since a
        // question that says how many rows have not been on the screen counts the ones that were.
        let down_and_up = (0..step)
            .map(|_| key(KeyCode::Down))
            .chain((0..step).map(|_| key(KeyCode::Up)))
            .collect();
        assert_eq!(
            shown_after(&row, vec![wheel(ScrollDown), wheel(ScrollUp)]),
            shown_after(&row, down_and_up),
            "{}: a notch back up is not {step} presses of Up",
            row.name
        );
        // As far as the keys go and no further. Walked through the same rows, since a question that
        // counts the rows not yet on the screen would count a jump to the end differently.
        let far = (0..200).map(|_| wheel(ScrollDown)).collect();
        let by_the_same_keys = (0..200 * step).map(|_| key(KeyCode::Down)).collect();
        assert_eq!(
            shown_after(&row, far),
            shown_after(&row, by_the_same_keys),
            "{}: the wheel went past where the keys stop",
            row.name
        );
    }
    assert!(
        scrolled.len() >= 5,
        "only {scrolled:?} scroll at {COLUMNS} by {ROWS}, so the table proves little"
    );
}

/// Where the crate reads events from the terminal, how many calls each file makes in the code that
/// ships, and what reads them. A file that is not here, or a count that has moved, means a new
/// reader of events: one that puts a question to a person is a row of [`every_question`], and
/// one that does not is added here with the reason it is not a question.
const READERS: &[(&str, usize, &str)] = &[
    ("input.rs", 5, "the reader every file below goes through"),
    (
        "confirm.rs",
        1,
        "the questions a turn puts, which are the rows of Kind, and the two a command puts: a \
         server to start and a checkout to remove",
    ),
    ("confirm/vet.rs", 1, "the vet question, a row of Kind"),
    ("ask.rs", 2, "the questions the planner and the driver put"),
    (
        "trust_prompt.rs",
        1,
        "the three questions asked before a session",
    ),
    ("hidden.rs", 1, "the credential read"),
    (
        "resume.rs",
        1,
        "a list of sessions, and the question of deleting one",
    ),
    ("list_picker.rs", 1, "a list to pick from, not a question"),
    ("theme_prompt.rs", 1, "a list to pick from, not a question"),
    ("sessions_view.rs", 2, "a list to read, not a question"),
    ("app.rs", 7, "the loops of a session, which ask nothing"),
];

/// The calls to read or poll the terminal in the code of `source` that ships: every line before a
/// module of tests, leaving out comments. A call is a path (`input::read`, `event::poll`) or a name
/// imported by a `use` of the `input` module or crossterm's `event` module, since a file that
/// imports `poll` and `read` calls them with no path to count.
fn reads_of_the_terminal(source: &str) -> usize {
    let (mut uses, mut code) = (String::new(), String::new());
    let mut inside_a_use = false;
    let mut lines = source.lines().peekable();
    while let Some(line) = lines.next() {
        if line.trim() == "#[cfg(test)]"
            && lines
                .peek()
                .is_some_and(|next| next.trim_start().starts_with("mod "))
        {
            break;
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        inside_a_use |= trimmed.starts_with("use ")
            || trimmed.starts_with("pub use ")
            || (trimmed.starts_with("pub(") && trimmed.contains(" use "));
        let into = if inside_a_use { &mut uses } else { &mut code };
        into.push_str(line);
        into.push('\n');
        if line.contains(';') {
            inside_a_use = false;
        }
    }
    let paths = ["input::read", "input::poll", "event::read", "event::poll"]
        .iter()
        .map(|reader| code.matches(reader).count())
        .sum::<usize>();
    paths + names_imported_as_readers(&uses)
}

/// How many readers the `use` statements in `uses` import by name or glob from the `input` module
/// or crossterm's `event` module: each `read` or `poll` in the names after the module, and one for
/// a `*`. Another crate's `event` module is not the terminal's.
fn names_imported_as_readers(uses: &str) -> usize {
    uses.split(';')
        .flat_map(|statement| {
            ["input::", "crossterm::event::"]
                .into_iter()
                .filter_map(move |module| {
                    statement
                        .find(module)
                        .map(|from| &statement[from + module.len()..])
                })
        })
        .map(|names| {
            names
                .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .filter(|word| matches!(*word, "read" | "poll"))
                .count()
                + usize::from(names.contains('*'))
        })
        .sum()
}

/// MOUSE-44. Rejects: a file that reads events from the terminal by names it imported, which has no
/// `input::read` to count. The paths are counted, a comment and what follows the module of
/// tests are not, and what is imported from the module of events or of the reader is, by name, in
/// a list, over several lines, renamed and by glob.
#[test]
fn a_reader_imported_by_name_is_counted_as_the_paths_to_one_are() {
    for (source, readers) in [
        ("fn f() { input::read(); event::poll(d); }", 2),
        (
            "use crate::input::{poll, read};\nfn f() { poll(d); read(); }",
            2,
        ),
        ("use ratatui::crossterm::event::read;\nfn f() { read() }", 1),
        (
            "use ratatui::crossterm::event::{\n    self, poll,\n    read as next,\n};\n",
            2,
        ),
        ("pub(crate) use crate::input::*;\n", 1),
        ("use ratatui::crossterm::event::*;\n", 1),
        ("use rustix::event::{PollFd, poll};\nfn f() { poll(); }", 0),
        (
            "use crate::input::{self, key_of};\nuse ratatui::crossterm::event::{self, Event};",
            0,
        ),
        ("use std::io::Read;\nfn f() { let spread = 1; }", 0),
        ("// use crate::input::{poll, read};\nfn f() {}", 0),
        (
            "fn f() {}\n#[cfg(test)]\nmod tests {\n    use crate::input::read;\n}\n",
            0,
        ),
    ] {
        assert_eq!(reads_of_the_terminal(source), readers, "{source:?}");
    }
}

fn sources_under(directory: &Path, found: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("a directory of the crate") {
        let path = entry.expect("an entry").path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if path.is_dir() {
            if name != "tests" {
                sources_under(&path, found);
            }
        } else if name.ends_with(".rs") && !name.ends_with("_tests.rs") && name != "testutil.rs" {
            found.push(path);
        }
    }
}

/// MOUSE-44. Rejects: a question added to the interface that reads the terminal itself and is not
/// among the rows of [`no_pointer_event_answers_any_question`], which the reply kinds alone would
/// not catch for a question that is not a reply to a turn.
#[test]
fn every_reader_of_events_is_one_the_table_of_questions_knows() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources_under(&source, &mut files);
    let mut found: Vec<(String, usize)> = files
        .iter()
        .filter_map(|path| {
            let text = std::fs::read_to_string(path).expect("a source file");
            let reads = reads_of_the_terminal(&text);
            let name = path.strip_prefix(&source).expect("under src");
            (reads > 0).then(|| (name.to_string_lossy().replace('\\', "/"), reads))
        })
        .collect();
    found.sort();
    let mut known: Vec<(String, usize)> = READERS
        .iter()
        .map(|(file, count, _)| ((*file).to_string(), *count))
        .collect();
    known.sort();
    assert_eq!(
        found, known,
        "a file reads events that READERS does not name, or reads another number of them. A reader \
         that puts a question to a person is a row of `every_question`, and the others are named \
         in READERS with why they are not questions"
    );
}
