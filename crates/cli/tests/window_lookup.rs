//! BACKEND-64 through the built binary: the window of the model a session opens on is asked for
//! while the first frame is drawn, adopted when it arrives, and waited for by a turn that is sent
//! before it has.
//!
//! The gateway is a stand-in that holds its listing. A start that waited for the listing would never
//! draw while it is held, and a turn that did not would reach the gateway before the listing is
//! answered, so both are told from the order in which the gateway saw things and not from how long
//! anything took.
//!
//! These tests run on Linux and macOS, as `chatgpt_start.rs` does, since the home directory the
//! binary reads is set by `HOME` and a terminal is made with `script`.

#![cfg(any(target_os = "linux", target_os = "macos"))]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use std::io::{BufRead, BufReader, Write};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

mod chatgpt_run;
#[path = "../../agent/test-support/chatgpt_stand_in.rs"]
mod chatgpt_stand_in;
mod terminal;

use chatgpt_run::{Scratch, environment};
use chatgpt_stand_in::StandIn;
use terminal::{Terminal, in_a_sized_terminal};

/// The window the gateway states for its one model, which is not the default budget.
const WINDOW: u64 = 65_536;

/// What the gateway saw, in the order it saw it.
#[derive(Default)]
struct Seen {
    events: Vec<&'static str>,
    /// Whether the listing may be answered.
    released: bool,
}

/// A loopback gateway whose listing waits until it is released, or until a turn reaches it, or until
/// `at_most` has passed.
struct Gateway {
    base: String,
    seen: Arc<(Mutex<Seen>, Condvar)>,
}

impl Gateway {
    fn holding_its_listing(at_most: Duration) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
        let base = format!("http://{}", listener.local_addr().expect("an address"));
        let seen = Arc::new((Mutex::new(Seen::default()), Condvar::new()));
        std::thread::spawn({
            let seen = Arc::clone(&seen);
            move || {
                for stream in listener.incoming().flatten() {
                    let seen = Arc::clone(&seen);
                    std::thread::spawn(move || Self::answer(stream, &seen, at_most));
                }
            }
        });
        Self { base, seen }
    }

    fn answer(stream: std::net::TcpStream, seen: &Arc<(Mutex<Seen>, Condvar)>, at_most: Duration) {
        let watched = stream.try_clone().expect("the connection");
        let mut reader = BufReader::new(stream);
        let mut request = String::new();
        if reader.read_line(&mut request).is_err() {
            return;
        }
        let mut length = 0_usize;
        let mut header = String::new();
        while matches!(reader.read_line(&mut header), Ok(read) if read > 2) {
            if let Some(value) = header.to_ascii_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse().unwrap_or(0);
            }
            header.clear();
        }
        let mut body = vec![0_u8; length];
        let _ = std::io::Read::read_exact(&mut reader, &mut body);

        let (state, changed) = &**seen;
        let listing = request.starts_with("GET ") && request.contains("/models");
        let (status, text) = if listing {
            state
                .lock()
                .expect("what was seen")
                .events
                .push("listing asked");
            changed.notify_all();
            // Nothing more is sent on a request that has been read, so this read returns when the
            // client closes the connection, which a client that gave up on the listing does.
            std::thread::spawn({
                let seen = Arc::clone(seen);
                let mut watched = watched;
                move || {
                    let mut byte = [0_u8; 1];
                    if matches!(std::io::Read::read(&mut watched, &mut byte), Ok(0) | Err(_)) {
                        let (state, changed) = &*seen;
                        let mut held = state.lock().expect("what was seen");
                        if !held.events.contains(&"listing answered") {
                            held.events.push("listing abandoned");
                        }
                        drop(held);
                        changed.notify_all();
                    }
                }
            });
            let held = state.lock().expect("what was seen");
            let (mut held, _) = changed
                .wait_timeout_while(held, at_most, |held| {
                    !held.released
                        && !held.events.contains(&"turn sent")
                        && !held.events.contains(&"listing abandoned")
                })
                .expect("what was seen");
            held.events.push("listing answered");
            drop(held);
            changed.notify_all();
            (
                "200 OK",
                format!(r#"{{"data": [{{"id": "m", "context_length": {WINDOW}}}]}}"#),
            )
        } else {
            state
                .lock()
                .expect("what was seen")
                .events
                .push("turn sent");
            changed.notify_all();
            (
                "500 Internal Server Error",
                r#"{"error": {"message": "a stand-in answers no chat"}}"#.to_string(),
            )
        };
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
            text.len()
        );
        let _ = reader.into_inner().write_all(response.as_bytes());
    }

    /// Let the listing be answered.
    fn release(&self) {
        let (state, changed) = &*self.seen;
        state.lock().expect("what was seen").released = true;
        changed.notify_all();
    }

    fn events(&self) -> Vec<&'static str> {
        self.seen.0.lock().expect("what was seen").events.clone()
    }

    /// Whether the gateway has seen `event` before `bound` passes.
    fn saw(&self, event: &str, bound: Duration) -> bool {
        let stop = Instant::now() + bound;
        while Instant::now() < stop {
            if self.events().contains(&event) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        false
    }
}

/// A home whose only service is `gateway`, with its one model in force.
fn home_on(name: &str, gateway: &Gateway) -> Scratch {
    let home = Scratch::new(name);
    std::fs::write(
        home.path.join(".bravebot/settings.json"),
        format!(
            r#"{{"model": "held/m", "provider": {{"held": {{"options": {{"baseURL": "{}/v1"}}}}}}}}"#,
            gateway.base
        ),
    )
    .expect("write the settings");
    home
}

/// BACKEND-64: the first frame is drawn while the window is still being asked for, and the window is
/// adopted on the pass of the loop it arrives, with no key pressed.
///
/// The gateway answers its listing only when the test releases it, which is after the frame is on the
/// screen. A start that waited for the listing never draws, so the frame is not seen and the test
/// fails. The regressions it rejects are a start that holds the first frame for the service, and a
/// window asked for and never adopted.
#[test]
fn a_session_draws_while_its_window_is_asked_for_and_takes_it_when_it_comes() {
    let gateway = Gateway::holding_its_listing(Duration::from_secs(60));
    let home = home_on("cli-window-lookup-frame", &gateway);
    let stand_in = StandIn::new(vec![]);
    let mut command = in_a_sized_terminal(&["--dangerously-skip-permissions"]);
    environment(&mut command, &home, &stand_in);
    let terminal = Terminal::start(command);

    let drawn = terminal.shows("Brave Bot to do anything", Duration::from_secs(30));
    let asked = gateway.saw("listing asked", Duration::from_secs(5));
    let events_at_the_frame = gateway.events();
    gateway.release();
    let adopted = terminal.shows(&format!("above {WINDOW} tokens"), Duration::from_secs(30));
    let ran = terminal.finish(Duration::from_millis(200));

    assert!(
        drawn,
        "the first frame was not drawn while the listing was held: {}",
        ran.shown
    );
    assert!(asked, "the window was never asked for: {}", ran.shown);
    assert_eq!(
        events_at_the_frame,
        ["listing asked"],
        "the frame was drawn after the listing was answered or given up on"
    );
    assert!(
        adopted,
        "the window the gateway stated was not adopted: {}",
        ran.shown
    );
}

/// BACKEND-64: a turn sent before the window has arrived waits for it. `-i` sends its prompt as the
/// first turn at once, while the listing is still held.
///
/// The gateway answers its listing when a turn reaches it, or after two seconds. A turn that does not
/// wait reaches it first, so the gateway sees the turn before the answer. The regression it rejects
/// is a turn sized against the default budget for want of a lookup that was on its way.
#[test]
fn a_turn_sent_before_the_window_arrives_waits_for_it() {
    let gateway = Gateway::holding_its_listing(Duration::from_secs(2));
    let home = home_on("cli-window-lookup-turn", &gateway);
    let stand_in = StandIn::new(vec![]);
    let mut command = in_a_sized_terminal(&["-i", "hello", "--dangerously-skip-permissions"]);
    environment(&mut command, &home, &stand_in);
    let terminal = Terminal::start(command);

    let sent = gateway.saw("turn sent", Duration::from_secs(60));
    let ran = terminal.finish(Duration::from_millis(200));

    assert!(sent, "the turn never reached the gateway: {}", ran.shown);
    assert_eq!(
        gateway.events(),
        ["listing asked", "listing answered", "turn sent"],
        "{}",
        ran.shown
    );
}
