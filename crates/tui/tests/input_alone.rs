//! Whether the event the reader last handed out arrived alone, read through the build that ships.
//!
//! `crates/tui/src/input.rs` keeps that fact in a process-wide `AtomicBool` and, under
//! `cfg(test)`, in a `thread_local` instead, so that the unit tests of that crate do not overwrite
//! one another's answer. A unit test therefore never reaches the static that ships. This file is an
//! integration test: it links the library built without `cfg(test)`, and the reader in it reads a
//! terminal.
//!
//! The terminal is a pty made by `script`, as in `crates/cli/tests/terminal/mod.rs`, with this test
//! binary run inside it as its own child. The child puts the terminal in raw mode, reads events
//! through [`bravebot_tui::input::read`] and says what
//! [`bravebot_tui::input::the_last_event_arrived_alone`] answered after each. The parent types two
//! keys in one write, which the pty delivers in one read, and then a third key by itself.
//!
//! These tests run on Linux and macOS, where `script` exists.

#![cfg(any(target_os = "linux", target_os = "macos"))]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bravebot_tui::input;
use ratatui::crossterm::terminal::{disable_raw_mode, enable_raw_mode};

/// Set in the child, which is this binary run again inside the terminal.
const CHILD: &str = "BRAVEBOT_INPUT_ALONE_CHILD";

/// The name of the test that is its own child.
const TEST: &str = "the_shipped_reader_says_whether_an_event_arrived_alone";

/// What the child does inside the terminal.
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn the_child() {
    let text = |event: &ratatui::crossterm::event::Event| {
        input::key_of(event)
            .as_ref()
            .and_then(input::text_of)
            .expect("a printable key")
    };
    let say = |line: String| {
        let mut out = std::io::stdout();
        write!(out, "{line}\r\n").expect("written");
        out.flush().expect("flushed");
    };

    enable_raw_mode().expect("raw mode on the terminal");
    say("READY".to_string());

    let first = input::read().expect("a first event");
    let first_alone = input::the_last_event_arrived_alone();
    let second = input::read().expect("a second event");
    let second_alone = input::the_last_event_arrived_alone();
    say(format!(
        "TOGETHER {} {first_alone} {} {second_alone}",
        text(&first),
        text(&second)
    ));

    let third = input::read().expect("a third event");
    let third_alone = input::the_last_event_arrived_alone();
    say(format!("ALONE {} {third_alone}", text(&third)));

    disable_raw_mode().expect("raw mode off");
}

/// What a terminal has shown, read on a thread of its own so a wait for it can be bounded.
struct Shown(Arc<Mutex<Vec<u8>>>);

impl Shown {
    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    fn read(mut source: impl Read + Send + 'static) -> Self {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        std::thread::spawn({
            let bytes = Arc::clone(&bytes);
            move || {
                let mut buffer = [0u8; 4096];
                while let Ok(read) = source.read(&mut buffer) {
                    if read == 0 {
                        break;
                    }
                    bytes
                        .lock()
                        .expect("what was shown")
                        .extend_from_slice(&buffer[..read]);
                }
            }
        });
        Self(bytes)
    }

    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().expect("what was shown")).into_owned()
    }

    /// The finished line that holds `word`, from the word to its end, once the terminal has shown
    /// it, or `None` when `bound` passes first. The test harness prints its own text on the same
    /// line as the first thing the test writes, so the word is not always at the start.
    fn line_starting(&self, word: &str, bound: Duration) -> Option<String> {
        let stop = Instant::now() + bound;
        while Instant::now() < stop {
            let shown = self.text();
            let mut lines: Vec<&str> = shown.split('\n').collect();
            // The last piece has no end yet.
            lines.pop();
            if let Some(line) = lines.iter().find_map(|line| {
                let line = line.trim();
                line.find(word)
                    .and_then(|at| line.get(at..))
                    .map(str::to_string)
            }) {
                return Some(line);
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        None
    }
}

/// The command that runs this binary again, for the one test, in a terminal.
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn in_a_terminal() -> Command {
    let exe = std::env::current_exe().expect("the path of this test binary");
    let mut command = Command::new("/usr/bin/script");
    #[cfg(target_os = "macos")]
    command.args(["-q", "/dev/null"]).arg(&exe).args([
        "--exact",
        TEST,
        "--nocapture",
        "--test-threads=1",
    ]);
    #[cfg(target_os = "linux")]
    {
        let line = format!(
            "'{}' --exact {TEST} --nocapture --test-threads=1",
            exe.display().to_string().replace('\'', r"'\''")
        );
        command.args(["-qec", &line, "/dev/null"]);
    }
    command.env(CHILD, "1");
    command
}

/// INPUT-34. Two keys that the terminal hands over in one read are each reported as not having
/// arrived alone, and a key read by itself afterwards is. This is the reader every prompt asks, so
/// the process-wide value it keeps has to be the one stored for the event last returned, with
/// the polarity the name says.
///
/// Rejects, in the build that ships and not in the unit tests: a store that never happens, which
/// leaves the starting `true` after two keys that arrived together, a store of the inverted value,
/// and a value that is set once and kept, which reports the third key as one of a crowd.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
#[expect(clippy::panic, reason = "a test fails by panicking")]
fn the_shipped_reader_says_whether_an_event_arrived_alone() {
    if std::env::var_os(CHILD).is_some() {
        the_child();
        return;
    }

    let mut child = in_a_terminal()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("script runs this test binary in a terminal");
    let shown = Shown::read(child.stdout.take().expect("the terminal"));
    let mut typed = child.stdin.take().expect("the terminal's input");
    let bound = Duration::from_secs(60);

    let outcome = (|| {
        shown.line_starting("READY", bound).ok_or("never ready")?;
        // One write, so the terminal has both keys waiting when the reader looks.
        typed.write_all(b"ab").map_err(|_| "typing failed")?;
        typed.flush().map_err(|_| "typing failed")?;
        let together = shown
            .line_starting("TOGETHER", bound)
            .ok_or("the two keys were never read")?;
        typed.write_all(b"c").map_err(|_| "typing failed")?;
        typed.flush().map_err(|_| "typing failed")?;
        let alone = shown
            .line_starting("ALONE", bound)
            .ok_or("the third key was never read")?;
        Ok::<_, &str>((together, alone))
    })();

    child.kill().ok();
    child.wait().ok();
    let (together, alone) =
        outcome.unwrap_or_else(|why| panic!("{why}; the terminal showed:\n{}", shown.text()));

    assert_eq!(
        together, "TOGETHER a false b false",
        "two keys that arrived in one read were not both reported as having arrived together"
    );
    assert_eq!(
        alone, "ALONE c true",
        "a key read by itself was not reported as having arrived alone"
    );
}
