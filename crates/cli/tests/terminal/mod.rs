//! What the tests that run the built binary in a terminal and type at it share: the command that
//! runs it in a terminal of a size, what the terminal showed, and a run a person types lines at.
//!
//! The terminal is `script`, whose arguments differ between the two systems these tests run on.
//! Windows has no `script`, and these tests do not run there.
#![allow(
    dead_code,
    reason = "each test file that includes this module uses some of it"
)]

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// What a terminal has shown, read as it arrives on a thread of its own.
pub struct Shown {
    bytes: Arc<Mutex<Vec<u8>>>,
    reading: std::thread::JoinHandle<()>,
}

impl Shown {
    pub fn read(mut source: impl Read + Send + 'static) -> Self {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let reading = std::thread::spawn({
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
        Self { bytes, reading }
    }

    /// What has been shown so far.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes.lock().expect("what was shown")).into_owned()
    }

    /// Everything shown, once the terminal has closed.
    pub fn finished(self) -> String {
        let Self { bytes, reading } = self;
        reading.join().expect("the terminal was read");
        String::from_utf8_lossy(&bytes.lock().expect("what was shown")).into_owned()
    }
}

/// The command that runs the built binary with `arguments` in a terminal of a size, which the
/// interface that draws measures once at its start. `script` is the terminal, and the shell it runs
/// sets the size.
pub fn in_a_sized_terminal(arguments: &[&str]) -> Command {
    let quoted: Vec<String> = arguments
        .iter()
        .map(|argument| format!("'{}'", argument.replace('\'', r"'\''")))
        .collect();
    let line = format!(
        "/bin/stty cols 120 rows 40; exec '{}' {}",
        env!("CARGO_BIN_EXE_bravebot"),
        quoted.join(" ")
    );
    let mut command = Command::new("/usr/bin/script");
    #[cfg(target_os = "macos")]
    command.args(["-q", "/dev/null", "/bin/sh", "-c", &line]);
    #[cfg(target_os = "linux")]
    command.args(["-qec", &line, "/dev/null"]);
    command
}

/// What a run showed, and the status it ended with, or `None` where it was still running when it
/// was stopped.
pub struct Ran {
    pub shown: String,
    pub code: Option<i32>,
}

/// A run of the built binary in a terminal, which a person types lines at.
pub struct Terminal {
    child: std::process::Child,
    pub shown: Shown,
    typed: Option<std::process::ChildStdin>,
}

impl Terminal {
    pub fn start(mut command: Command) -> Self {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("script runs the built binary in a terminal");
        let shown = Shown::read(child.stdout.take().expect("the terminal"));
        let typed = child.stdin.take();
        Self {
            child,
            shown,
            typed,
        }
    }

    /// Whether the terminal has shown `words` before `bound` passes.
    pub fn shows(&self, words: &str, bound: Duration) -> bool {
        let stop = Instant::now() + bound;
        while Instant::now() < stop {
            if self.shown.text().contains(words) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        false
    }

    /// Type `line` and a return.
    pub fn type_line(&mut self, line: &str) {
        let typed = self.typed.as_mut().expect("the input is still open");
        writeln!(typed, "{line}").expect("a line typed");
    }

    /// The end of the input, which ends a session in lines.
    pub fn end_input(&mut self) {
        self.typed = None;
    }

    /// What the run showed once it ended, or when `bound` passed and it was stopped.
    pub fn finish(mut self, bound: Duration) -> Ran {
        let stop = Instant::now() + bound;
        let code = loop {
            if let Some(status) = self.child.try_wait().expect("the terminal") {
                break status.code();
            }
            if Instant::now() >= stop {
                break None;
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        let _ = self.child.kill();
        let _ = self.child.wait();
        Ran {
            shown: self.shown.finished(),
            code,
        }
    }
}
