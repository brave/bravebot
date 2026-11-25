//! INPUT-33 through the built binary, in a terminal: the terminal an editor is handed is out of raw
//! mode, and a session in lines takes none of the modes the interface that draws asks for.
//!
//! The terminal is `script`, whose output is a pipe read here, so nothing answers a query the binary
//! writes. These tests run on Linux and macOS, as `hangup.rs` does.

#![cfg(any(target_os = "linux", target_os = "macos"))]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

mod chatgpt_run;
#[path = "../../agent/test-support/chatgpt_stand_in.rs"]
mod chatgpt_stand_in;
mod terminal;

use chatgpt_run::{Scratch, environment};
use chatgpt_stand_in::StandIn;
use terminal::{Terminal, in_a_sized_terminal};

/// Entering the screen the interface draws its first frame on, which comes after raw mode.
const FIRST_FRAME: &str = "\x1b[?1049h";

/// The key that hands the prompt to an editor.
const CTRL_G: &str = "\x07";

/// A model that is configured but not reachable, so a session starts and asks nothing of a person.
fn configure_a_model(home: &Scratch) {
    std::fs::write(
        home.path.join(".bravebot/settings.json"),
        r#"{"model": "held/m", "provider": {"held": {"options": {"baseURL": "http://127.0.0.1:1/v1"}}}}"#,
    )
    .expect("write the settings");
}

/// Whether `path` exists before `bound` passes.
fn appeared(path: &Path, bound: Duration) -> bool {
    let stop = Instant::now() + bound;
    while Instant::now() < stop {
        if path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

/// An editor that writes what `stty -a` says about the terminal it was started on, and leaves the
/// file it was handed as it found it. Returns the editor and the file it writes.
///
/// The record is written beside the file it ends up at and moved into place, so that the file being
/// there means it is whole.
fn an_editor_that_records_its_terminal(home: &Scratch) -> (PathBuf, PathBuf) {
    let editor = home.path.join("record-the-terminal");
    let record = home.path.join("the-terminal-the-editor-had");
    let unfinished = home.path.join("the-terminal-the-editor-had.unfinished");
    std::fs::write(
        &editor,
        format!(
            "#!/bin/sh\n/bin/stty -a > '{unfinished}' 2>&1\n/bin/mv '{unfinished}' '{record}'\n",
            unfinished = unfinished.display(),
            record = record.display(),
        ),
    )
    .expect("write the editor");
    std::fs::set_permissions(&editor, std::fs::Permissions::from_mode(0o755))
        .expect("make the editor runnable");
    (editor, record)
}

/// INPUT-33: the terminal an editor is handed is the one the session found, so it is out of raw mode.
///
/// The session is started in a terminal with an editor in `$VISUAL` that records what `stty -a` says
/// about the terminal it was started on, and Ctrl-G hands the prompt to it. A terminal in raw mode
/// has line editing and echo off, and one handed back has both on. The regression it rejects is a
/// hand-back that passes the modes back and leaves raw mode on, which leaves the editor a terminal
/// that echoes nothing and does not hold a line.
#[test]
fn the_editor_a_prompt_is_written_in_is_started_on_a_terminal_that_is_out_of_raw_mode() {
    let home = Scratch::new("cli-terminal-modes-editor");
    configure_a_model(&home);
    let (editor, record) = an_editor_that_records_its_terminal(&home);
    let stand_in = StandIn::new(vec![]);
    let mut command = in_a_sized_terminal(&["--dangerously-skip-permissions"]);
    environment(&mut command, &home, &stand_in);
    command.env("VISUAL", &editor);
    let mut terminal = Terminal::start(command);

    let drawn = terminal.shows(FIRST_FRAME, Duration::from_secs(60));
    if drawn {
        // Raw mode is on by now, so the key waits in the terminal until the session reads it.
        terminal.press(CTRL_G);
    }
    let started = drawn && appeared(&record, Duration::from_secs(30));
    let ran = terminal.finish(Duration::from_millis(100));
    assert!(drawn, "the interface did not draw (shown: {:?})", ran.shown);
    assert!(
        started,
        "Ctrl-G did not start the editor (shown: {:?})",
        ran.shown
    );

    let had = std::fs::read_to_string(&record).expect("what the editor saw");
    let modes: Vec<&str> = had.split_whitespace().collect();
    assert!(
        modes.contains(&"icanon") && modes.contains(&"echo"),
        "the editor was started on a terminal still in raw mode (line editing and echo on are `icanon` and `echo`): {had}"
    );
}

/// What a terminal in cooked mode shows for Ctrl-A: the key as a caret and a letter. The interface that
/// draws reads the key as the start of the line and draws nothing for it, so these two characters can
/// only have come from the terminal echoing it.
const ECHOED_CTRL_A: &str = "^A";

/// The key itself.
const CTRL_A: &str = "\x01";

/// Typed after the key: one character the first frame does not contain, since the interface draws
/// each typed character behind a cursor move of its own.
const MARKER: &str = "Z";

/// INPUT-33: once the interface has drawn its first frame the terminal is in raw mode, so a key it
/// does not draw is not echoed by the terminal.
///
/// Nothing else runs `enable_raw_mode` for real: the unit tests in `app.rs` pass the take-over their
/// own closures. Ctrl-A is typed at the session after its first frame, then a letter that the
/// interface draws, so the letter being shown means the Ctrl-A was read before it. A terminal left
/// in cooked mode echoes the Ctrl-A as `^A` ahead of the letter. The control is a session in lines
/// in the same terminal, which is in cooked mode and does echo it, so the absence in the interface
/// is not a terminal that never echoes.
///
/// The regression it rejects is `take_over_terminal` passing a closure that does nothing where it
/// passes `enable_raw_mode`, which the interface survives while reading input a line at a time with
/// every key echoed.
#[test]
fn the_interface_that_draws_has_the_terminal_in_raw_mode_so_a_key_is_not_echoed() {
    // The control: a session in lines echoes the key.
    let plain_home = Scratch::new("cli-terminal-modes-echo-control");
    configure_a_model(&plain_home);
    let stand_in = StandIn::new(vec![]);
    let mut command = in_a_sized_terminal(&["--plain", "--dangerously-skip-permissions"]);
    environment(&mut command, &plain_home, &stand_in);
    let mut terminal = Terminal::start(command);
    let opened = terminal.shows("in lines, held/m.", Duration::from_secs(60));
    if opened {
        terminal.press(CTRL_A);
    }
    let echoed = opened && terminal.shows(ECHOED_CTRL_A, Duration::from_secs(10));
    terminal.end_input();
    let ran = terminal.finish(Duration::from_secs(30));
    assert!(
        opened,
        "the session in lines did not open (shown: {:?})",
        ran.shown
    );
    assert!(
        echoed,
        "the control does not echo Ctrl-A, so the test below could not tell raw mode from cooked (shown: {:?})",
        ran.shown
    );

    // The interface that draws.
    let home = Scratch::new("cli-terminal-modes-raw");
    configure_a_model(&home);
    let stand_in = StandIn::new(vec![]);
    let mut command = in_a_sized_terminal(&["--dangerously-skip-permissions"]);
    environment(&mut command, &home, &stand_in);
    let mut terminal = Terminal::start(command);
    let drawn = terminal.shows(FIRST_FRAME, Duration::from_secs(60));
    let mut after = false;
    let mut from = 0;
    if drawn {
        from = terminal.shown.text().len();
        terminal.press(CTRL_A);
        terminal.press(MARKER);
        // A terminal not in raw mode echoes the Ctrl-A ahead of the marker it echoes, and the interface
        // draws the marker alone, so the marker turning up after the typing is the point by which
        // either has happened.
        let stop = Instant::now() + Duration::from_secs(10);
        while Instant::now() < stop && !since(&terminal.shown.text(), from).contains(MARKER) {
            std::thread::sleep(Duration::from_millis(25));
        }
        after = since(&terminal.shown.text(), from).contains(MARKER);
    }
    let seen = terminal.shown.text();
    let ran = terminal.finish(Duration::from_millis(100));
    assert!(drawn, "the interface did not draw (shown: {:?})", ran.shown);
    assert!(
        after,
        "the interface did not show the text typed after Ctrl-A (shown: {:?})",
        ran.shown
    );
    let typed_at = since(&seen, from);
    assert!(
        !typed_at.contains(ECHOED_CTRL_A),
        "the terminal echoed a key typed at the interface, so it was not in raw mode (shown: {typed_at:?})"
    );
}

/// What `text` holds from byte `at` on, moved forward to the next character boundary, since the text
/// is a lossy reading of bytes that were still arriving when `at` was taken.
fn since(text: &str, mut at: usize) -> &str {
    while at < text.len() && !text.is_char_boundary(at) {
        at += 1;
    }
    text.get(at..).unwrap_or("")
}

/// The sequences that take the terminal for the interface that draws: the alternate screen, the
/// three mouse tracking modes and the two forms of coordinates, bracketed paste, focus reporting and
/// pushing the keyboard flags, and the query that asks whether the keyboard flags are understood.
const TAKEOVER: &[&str] = &[
    "\x1b[?1049h",
    "\x1b[?1000h",
    "\x1b[?1002h",
    "\x1b[?1003h",
    "\x1b[?1015h",
    "\x1b[?1006h",
    "\x1b[?2004h",
    "\x1b[?1004h",
    "\x1b[>1u",
    "\x1b[?u\x1b[c",
];

/// INPUT-33: a session in lines takes none of what the interface that draws takes.
///
/// `--plain` is started in a terminal and typed at. It opens with the line that names the model, which
/// shows it ran, and what the terminal showed holds none of the sequences the interface that draws
/// writes to take it (so no screen of its own, no mouse or paste or focus reporting, no keyboard
/// flags and no query of the keyboard). The terminal is not in raw mode either: what is typed is
/// echoed back by the terminal, which a terminal in raw mode does not do.
///
/// The regression it rejects is a session in lines that goes through the take-over of the interface
/// that draws. The unit test in `plain.rs` reads only the two streams the session writes, and never
/// the entry the binary runs.
#[test]
fn a_session_in_lines_takes_none_of_what_the_interface_that_draws_takes() {
    let home = Scratch::new("cli-terminal-modes-plain");
    configure_a_model(&home);
    let stand_in = StandIn::new(vec![]);
    let mut command = in_a_sized_terminal(&["--plain", "--dangerously-skip-permissions"]);
    environment(&mut command, &home, &stand_in);
    let mut terminal = Terminal::start(command);

    let opened = terminal.shows("in lines, held/m.", Duration::from_secs(60));
    if opened {
        terminal.press("typed-in-lines");
    }
    let echoed = opened && terminal.shows("typed-in-lines", Duration::from_secs(10));
    terminal.end_input();
    let ran = terminal.finish(Duration::from_secs(30));
    assert!(
        opened,
        "the session in lines did not open (shown: {:?})",
        ran.shown
    );
    for sequence in TAKEOVER {
        assert!(
            !ran.shown.contains(sequence),
            "a session in lines wrote {sequence:?} (shown: {:?})",
            ran.shown
        );
    }
    assert!(
        echoed,
        "what was typed at a session in lines was not echoed, so the terminal was in raw mode (shown: {:?})",
        ran.shown
    );
}

/// What the failing server below writes to its stderr before it exits: a window title and the
/// clipboard (OSC 52).
const WHAT_A_FAILING_SERVER_WRITES: &[&str] =
    &["\x1b]0;SERVER-TITLE\x07", "\x1b]52;c;ZXZpbA==\x07"];

/// A home that requests an approved stdio server, `weather`, which writes
/// [`WHAT_A_FAILING_SERVER_WRITES`] to its stderr and exits before it answers `initialize`, so the
/// session starts without it. The model is configured but not reachable.
fn a_home_requesting_a_server_that_fails_its_handshake(home: &Scratch) {
    let script = r"printf '\033]0;SERVER-TITLE\007\033]52;c;ZXZpbA==\007' >&2; exit 1";
    let argv = vec!["/bin/sh".to_string(), "-c".to_string(), script.to_string()];
    let declaration =
        bravebot_config::mcp::Declaration::stdio(argv.clone(), vec!["PATH".into()], None)
            .expect("a declaration");
    let declared = serde_json::json!({"servers": {"weather": {
        "transport": "stdio", "argv": argv, "variables": ["PATH"]}}});
    std::fs::write(home.path.join(".bravebot/mcp.json"), declared.to_string())
        .expect("write the declaration");
    std::fs::write(
        home.path.join(".bravebot/mcp-approved"),
        format!("{}\n", declaration.digest()),
    )
    .expect("write the approval");
    std::fs::write(
        home.path.join(".bravebot/settings.json"),
        r#"{"model": "held/m", "provider": {"held": {"options": {"baseURL": "http://127.0.0.1:1/v1"}}}, "mcp": {"request": ["weather"]}}"#,
    )
    .expect("write the settings");
}

/// MCP-8: the interface that draws shows nothing a stdio server writes to its stderr, and shows that
/// the server was left out.
///
/// The interface takes the terminal and draws over it, so a server that could write to the
/// terminal could set its window title or write the clipboard from behind the screen. The session is
/// started in a terminal with a server that writes both to its stderr and exits before it answers
/// the handshake. The server is started before the first frame, so the bytes would be in what the
/// terminal showed by the time the interface has drawn.
///
/// The control is the first frame and the note about the server: the session started the server
/// and was told it failed, so the absence of the bytes is not a session that never started it. The
/// regression it rejects is passing the session's own standard error to the server
/// (`Stream::Inherited`) where the interface starts the servers it requests, which puts both
/// sequences in what the terminal showed.
#[test]
fn the_interface_that_draws_shows_nothing_a_servers_stderr_writes() {
    if !bravebot_sandbox::confinement_works_here() {
        assert!(
            std::env::var_os("CI").is_none(),
            "a CI machine must run this check, and cannot: confinement is unavailable"
        );
        eprintln!("skipped: confinement is unavailable, so the server cannot be started");
        return;
    }
    let home = Scratch::new("cli-terminal-modes-server-stderr");
    a_home_requesting_a_server_that_fails_its_handshake(&home);
    let stand_in = StandIn::new(vec![]);
    let mut command = in_a_sized_terminal(&["--dangerously-skip-permissions"]);
    environment(&mut command, &home, &stand_in);
    let terminal = Terminal::start(command);

    let drawn = terminal.shows(FIRST_FRAME, Duration::from_secs(60));
    let told = drawn && terminal.shows("did not complete its handshake", Duration::from_secs(10));
    let ran = terminal.finish(Duration::from_millis(500));
    assert!(drawn, "the interface did not draw (shown: {:?})", ran.shown);
    assert!(
        told,
        "the interface did not say the server was left out, so the server may not have started (shown: {:?})",
        ran.shown
    );
    for effect in WHAT_A_FAILING_SERVER_WRITES {
        assert!(
            !ran.shown.contains(effect),
            "the interface that draws showed a server's stderr bytes {effect:?} (shown: {:?})",
            ran.shown
        );
    }
    assert!(
        !ran.shown.contains("SERVER-TITLE"),
        "the interface that draws showed what a server wrote to its stderr (shown: {:?})",
        ran.shown
    );
}
