//! The loop an editor's Agent Client Protocol session runs in (docs/specs/acp.md).
//!
//! Reads a line at a time from `input`, hands each to [`Acp`] and writes what comes back to
//! `output`, one JSON value a line. Both `bravebot-acp` and `bravebot acp` call [`serve`], so there
//! is one path from an editor to the agent. The streams are passed in and the outcome returned, so
//! this file prints nothing and ends nothing, as the crate's layering test requires.

use crate::acp::Acp;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How long the end of the input waits for sessions that are still writing their records.
const FINISH: Duration = Duration::from_secs(10);

/// What the main loop waits on.
enum Input {
    Line(String),
    /// An event asked for a call that only this thread may make.
    Wake,
    End,
}

/// Serve one editor until `input` ends.
///
/// `settings` is the settings file the process was started on, if one was named. `unreadable` is
/// called, from the reading thread, for each line that is not valid UTF-8. Such a line is skipped,
/// since an editor that wrote one bad line will write another and ending loses the rest.
///
/// Answers whether every session had finished by the time the wait for them ran out. On the end of
/// the input every session is closed, which refuses anything waiting on an answer from an editor
/// that has gone away and fires `session-ended`.
pub fn serve<R, W>(
    input: R,
    output: W,
    settings: Option<PathBuf>,
    unreadable: impl Fn() + Send + 'static,
) -> bool
where
    R: BufRead + Send + 'static,
    W: Write + Send + 'static,
{
    let (events, lines) = mpsc::channel();
    let out = Arc::new(Mutex::new(output));
    let writer = Arc::clone(&out);
    let waker = events.clone();
    let mut acp = Acp::new(
        Box::new(move |message| write_line(&writer, &message)),
        Box::new(move || {
            let _ = waker.send(Input::Wake);
        }),
        settings,
    );

    let reader = events.clone();
    std::thread::spawn(move || {
        for line in input.lines() {
            let Ok(line) = line else {
                unreadable();
                continue;
            };
            if !line.trim().is_empty() && reader.send(Input::Line(line)).is_err() {
                return;
            }
        }
        let _ = reader.send(Input::End);
    });

    while let Ok(next) = lines.recv() {
        match next {
            Input::Line(line) => acp.handle_line(&line),
            Input::Wake => acp.drain(),
            Input::End => break,
        }
    }

    acp.finish(FINISH)
}

/// Write one line, or give up on writing altogether. A closed output means the editor is gone.
fn write_line<W: Write>(out: &Arc<Mutex<W>>, value: &serde_json::Value) {
    let Ok(mut handle) = out.lock() else {
        return;
    };
    let _ = writeln!(handle, "{value}");
    let _ = handle.flush();
}
