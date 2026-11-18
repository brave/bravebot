//! #3846 through the built binary: a terminal that answers nothing is not waited on for long.
//!
//! The interface asks the terminal whether it understands disambiguated keys before its first frame,
//! and the query ends with the primary device attributes request, which a terminal that is there
//! answers. Where nothing answers, as under a pty with nobody behind it, a driver or a slow ssh link,
//! the wait runs to its bound, and keys typed meanwhile wait with it. crossterm's bound is two
//! seconds; the interface sets a shorter one.
//!
//! The terminal is `script`, whose output is a pipe read here, so nothing answers any query. These
//! tests run on Linux and macOS, as `hangup.rs` does.

#![cfg(any(target_os = "linux", target_os = "macos"))]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use std::time::{Duration, Instant};

mod chatgpt_run;
#[path = "../../agent/test-support/chatgpt_stand_in.rs"]
mod chatgpt_stand_in;
mod terminal;

use chatgpt_run::{Scratch, environment};
use chatgpt_stand_in::StandIn;
use terminal::{Terminal, in_a_sized_terminal};

/// The keyboard query: the flags request, then the primary device attributes request.
const KEYBOARD_QUERY: &str = "\x1b[?u\x1b[c";
/// Entering the screen the interface draws its first frame on.
const FIRST_FRAME: &str = "\x1b[?1049h";

/// When `words` first appeared in what the terminal showed, polled until `bound` passes.
fn first_shown(
    run: &Terminal,
    words: &str,
    after: usize,
    bound: Instant,
) -> Option<(Instant, usize)> {
    while Instant::now() < bound {
        let text = run.shown.text();
        if let Some(at) = text[after.min(text.len())..].find(words) {
            return Some((Instant::now(), after + at));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    None
}

/// #3846: where nothing answers the keyboard query, the first frame comes well within the two
/// seconds crossterm waits.
///
/// The regression it rejects is the keyboard query left at crossterm's fixed two-second bound, which
/// put two seconds before the first frame on every terminal that answers nothing. The bound asserted
/// is a second and a half. The interface waits 500 ms, so a loaded runner has a second to spare,
/// and the old wait is never shorter than two seconds, so it fails even where this side's polling
/// sees the query up to half a second late.
#[test]
fn an_unanswered_keyboard_query_is_not_waited_on_for_two_seconds() {
    let home = Scratch::new("cli-startup-queries");
    std::fs::write(
        home.path.join(".bravebot/settings.json"),
        r#"{"model": "held/m", "provider": {"held": {"options": {"baseURL": "http://127.0.0.1:1/v1"}}}}"#,
    )
    .expect("write the settings");
    let stand_in = StandIn::new(vec![]);
    let mut command = in_a_sized_terminal(&["--dangerously-skip-permissions"]);
    environment(&mut command, &home, &stand_in);
    let run = Terminal::start(command);

    let bound = Instant::now() + Duration::from_secs(60);
    let (asked, at) = first_shown(&run, KEYBOARD_QUERY, 0, bound)
        .expect("the interface asked the keyboard query");
    let (drawn, _) = first_shown(&run, FIRST_FRAME, at, bound)
        .expect("the interface entered its screen after the keyboard query");
    let waited = drawn.duration_since(asked);
    let ran = run.finish(Duration::from_millis(100));
    assert!(
        waited < Duration::from_millis(1500),
        "the first frame came {waited:?} after the unanswered keyboard query (shown: {:?})",
        ran.shown
    );
}
