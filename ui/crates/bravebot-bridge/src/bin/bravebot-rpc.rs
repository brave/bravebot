//! Newline-delimited JSON over stdin and stdout.
//!
//! The whole of the transport. It reads a line, hands it to the bridge, and writes what
//! comes back; everything that decides anything lives in the library, so a second
//! front-end is a second file like this one and no change above it.
//!
//! **This is the only file in the crate allowed to touch stdout or end the process.**
//! Elsewhere a stray `println!` would interleave with the protocol and no caller could
//! stop it, which is the same reason the agent's kernel never prints.

use bravebot_bridge::bridge::Bridge;
use bravebot_bridge::protocol::{Event, Request, Unreadable, response};
use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex};

fn main() {
    let mut args = std::env::args().skip(1);
    let mut settings = None;
    if let Some(flag) = args.next() {
        match flag.as_str() {
            "--settings" => {
                let Some(path) = args.next() else { eprintln!("--settings requires a file path"); std::process::exit(2); };
                if args.next().is_some() { eprintln!("unexpected arguments after settings file"); std::process::exit(2); }
                settings = Some(std::path::PathBuf::from(path));
            }
            "--version" | "-V" => {
                println!("bravebot-rpc {} (agent {})", env!("CARGO_PKG_VERSION"), bravebot_bridge::agent_build());
                return;
            }
            other => {
                eprintln!("unknown option: {other}");
                eprintln!("usage: bravebot-rpc            speak NDJSON on stdin/stdout");
                eprintln!("       bravebot-rpc --version");
                std::process::exit(2);
            }
        }
    }

    // One writer behind a lock, so a line from an event and a line from a response cannot
    // interleave. Shared with the bridge, which emits events from whichever thread a turn
    // is running on.
    let out = Arc::new(Mutex::new(std::io::stdout()));

    let emitter = Arc::clone(&out);
    let mut bridge = Bridge::new(Box::new(move |event: Event| {
        write_line(&emitter, &event.to_value());
    })).with_settings(settings);

    bridge.ready();

    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            // Invalid UTF-8 on the wire. Nothing addressable, so it is logged and the
            // loop carries on: a front-end that produced one bad line will produce
            // another, and exiting loses whatever else it had to say.
            eprintln!("bravebot-rpc: unreadable line");
            continue;
        };
        if line.trim().is_empty() {
            continue;
        }

        match Request::parse(&line) {
            Ok(request) => {
                let outcome = bridge.dispatch(&request);
                write_line(&out, &response(request.id, outcome));
            }
            Err(Unreadable::Answerable { id, failure }) => {
                write_line(&out, &response(id, Err(failure)));
            }
            Err(Unreadable::Unanswerable { detail }) => {
                eprintln!("bravebot-rpc: {detail}");
            }
        }
    }

    // EOF on stdin. Anything waiting on an answer from a front-end that has gone away is
    // refused, which is what dropping the bridge does: a channel that cannot carry the
    // question cannot carry consent either.
    drop(bridge);
}

/// Write one line, or give up on writing altogether.
///
/// A closed stdout is not worth a panic and not worth retrying. It means the front-end is
/// gone, and what matters then is that pending writes get refused — which happens when
/// the process ends, not here.
fn write_line(out: &Arc<Mutex<std::io::Stdout>>, value: &serde_json::Value) {
    let Ok(mut handle) = out.lock() else {
        return;
    };
    let _ = writeln!(handle, "{value}");
    let _ = handle.flush();
}
