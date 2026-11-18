//! Scratch directories for tests, reading back what a session draws, and a terminal that counts the
//! frames drawn to it.

use crate::state::Session;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use std::path::PathBuf;

/// Where a test's scratch directories go, chosen once for every crate that includes the file. The
/// search for programs leaves the system temporary directory out (SANDBOX-30), so the root is
/// outside it when the checkout is inside it.
#[path = "../../sandbox/test-support/scratch_root.rs"]
pub(crate) mod scratch_root;

/// An absolute path under the scratch root, which is the workspace `target/test-scratch/` unless
/// the checkout is under the system temporary directory (see `sandbox/test-support/scratch_root.rs`).
///
/// Tests built these under [`std::env::temp_dir`] before. That directory is shared
/// between users and between processes with different privileges, so a fixed name
/// under it collides whenever two checkouts run the tests at once, and it is the
/// insecure-temporary-file pattern the security scan flags. `target/` is
/// per-checkout and already ignored by git.
///
/// Nothing is created here: callers make and remove the directory as they already did.
pub(crate) fn scratch_dir(name: &str) -> PathBuf {
    scratch_root::scratch_dir(name)
}

/// Run `command` in a process group of its own and wait for it to end for at most `deadline`.
///
/// A command that has not ended by then is killed together with every program in its group, and the
/// call panics with `what` in the message, so a program that never exits fails the test that started
/// it and does not stall the run. The caller names in `what` what was waiting (for example the
/// variable under test). A command that ends in time returns its status, which the caller judges.
/// The standard streams are whatever the caller set on `command`.
///
/// The group is killed only at the deadline. Whoever starts a program that starts another, as
/// `xdg-open` starts `xdg-mime`, `xprop` and a handler, gets all of them ended.
#[cfg(unix)]
pub(crate) fn status_within(
    command: &mut std::process::Command,
    deadline: std::time::Duration,
    what: &str,
) -> std::process::ExitStatus {
    use std::os::unix::process::CommandExt;

    let mut child = command
        .process_group(0)
        .spawn()
        .unwrap_or_else(|error| panic!("{what}: the program did not start: {error}"));
    let until = std::time::Instant::now() + deadline;
    loop {
        if let Some(status) = child.try_wait().expect("the wait on the program") {
            return status;
        }
        if std::time::Instant::now() >= until {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let group = rustix::process::Pid::from_child(&child);
    let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
    let _ = child.kill();
    let _ = child.wait();
    panic!("{what} did not end within {deadline:?} and was killed with the programs it started");
}

/// [`status_within`] with the program's standard output and error read back, as
/// [`std::process::Command::output`] returns them. They are written to files, so a program the
/// command starts that keeps a stream open after the command ends does not hold the call up.
#[cfg(unix)]
pub(crate) fn output_within(
    command: &mut std::process::Command,
    deadline: std::time::Duration,
    what: &str,
) -> std::process::Output {
    use std::io::{Read, Seek};

    let mut stdout = tempfile::tempfile().expect("a file for the standard output");
    let mut stderr = tempfile::tempfile().expect("a file for the standard error");
    command
        .stdin(std::process::Stdio::null())
        .stdout(stdout.try_clone().expect("the standard output file"))
        .stderr(stderr.try_clone().expect("the standard error file"));
    let status = status_within(command, deadline, what);
    let read = |file: &mut std::fs::File| {
        let mut bytes = Vec::new();
        file.rewind().expect("the start of the file");
        file.read_to_end(&mut bytes)
            .expect("what the program wrote");
        bytes
    };
    std::process::Output {
        status,
        stdout: read(&mut stdout),
        stderr: read(&mut stderr),
    }
}

/// Foreground inks of the non-space cells on every row of a frame that contains `needle`.
pub(crate) fn inks_on_row_containing(session: &Session, needle: &str) -> Vec<Color> {
    let mut terminal = Terminal::new(TestBackend::new(120, 24)).expect("terminal");
    terminal
        .draw(|frame| {
            crate::render::draw(frame, session);
        })
        .expect("draw succeeds");
    let buffer = terminal.backend().buffer();
    let area = buffer.area();

    let mut inks = Vec::new();
    for row in 0..area.height {
        let text: String = (0..area.width)
            .map(|column| buffer.cell((column, row)).expect("cell").symbol())
            .collect();
        if !text.contains(needle) {
            continue;
        }
        inks.extend((0..area.width).filter_map(|column| {
            let cell = buffer.cell((column, row)).expect("cell");
            (cell.symbol() != " ").then_some(cell.fg)
        }));
    }
    assert!(!inks.is_empty(), "nothing containing {needle:?} was drawn");
    inks
}

/// A [`ratatui::backend::TestBackend`] that counts the frames drawn to it.
///
/// A frame identical to the one before it still reaches the backend, with nothing to write, so the
/// count is of draws asked for and not of what changed on the screen. A test that holds a prompt to
/// drawing again only for an event that changed something reads it afterwards.
pub(crate) struct Counting {
    inner: ratatui::backend::TestBackend,
    /// How many frames have been drawn.
    pub(crate) draws: usize,
}

impl Counting {
    pub(crate) fn new(width: u16, height: u16) -> Self {
        Self {
            inner: ratatui::backend::TestBackend::new(width, height),
            draws: 0,
        }
    }

    /// What is on the screen.
    pub(crate) fn buffer(&self) -> &ratatui::buffer::Buffer {
        self.inner.buffer()
    }
}

impl ratatui::backend::Backend for Counting {
    type Error = std::convert::Infallible;

    fn draw<'a, I>(&mut self, content: I) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (u16, u16, &'a ratatui::buffer::Cell)>,
    {
        self.draws += 1;
        self.inner.draw(content)
    }

    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        self.inner.hide_cursor()
    }

    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        self.inner.show_cursor()
    }

    fn get_cursor_position(&mut self) -> Result<ratatui::layout::Position, Self::Error> {
        self.inner.get_cursor_position()
    }

    fn set_cursor_position<P: Into<ratatui::layout::Position>>(
        &mut self,
        position: P,
    ) -> Result<(), Self::Error> {
        self.inner.set_cursor_position(position)
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        self.inner.clear()
    }

    fn clear_region(&mut self, clear_type: ratatui::backend::ClearType) -> Result<(), Self::Error> {
        self.inner.clear_region(clear_type)
    }

    fn size(&self) -> Result<ratatui::layout::Size, Self::Error> {
        self.inner.size()
    }

    fn window_size(&mut self) -> Result<ratatui::backend::WindowSize, Self::Error> {
        self.inner.window_size()
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.inner.flush()
    }
}
