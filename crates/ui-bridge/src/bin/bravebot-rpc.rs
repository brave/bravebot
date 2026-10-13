//! Newline-delimited JSON over stdin and stdout.
//!
//! The whole of the transport. It reads a line, hands it to the bridge, and writes what
//! comes back; everything that decides anything lives in the library, so a second
//! front-end is a second file like this one and no change above it.
//!
//! **This is the only file in the crate allowed to touch stdout or end the process.**
//! Elsewhere a stray `println!` would interleave with the protocol and no caller could
//! stop it, which is the same reason the agent's kernel never prints.

#![forbid(unsafe_code)]

use bravebot_ui_bridge::arguments::{Launch, launch};
use bravebot_ui_bridge::bridge::Bridge;
use bravebot_ui_bridge::protocol::{Event, Request, Unreadable, response};
use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex};

fn main() {
    // First of all, before anything else runs: the descriptors this process speaks over exist
    // from the moment it starts, and they are the channel to the window. Another process of the
    // same user must not be able to copy them out of it. See RPCVIEW-8. The protection starts
    // here, so a process already positioned to act before this runs is outside what it claims.
    if let Some(notice) = bravebot_sandbox::inspection::refuse_same_user_inspection().notice() {
        eprintln!("bravebot-rpc: {notice}");
    }
    // And for the reason the terminal entry point does it early: a credential is in this process's
    // memory from the first turn, and a crash after this writes no image of it. See CRED-23.
    let _ = bravebot_agent::crash::disable_core_dumps();

    // Not a security decision. The only argument is `--settings <path>`, which names a
    // settings file for a developer driving this by hand, and what that file may then grant
    // is decided by the policy layer rather than here. Nothing is authorised by an argument.
    // nosemgrep: rust.lang.security.args.args
    let args = std::env::args().skip(1);
    let settings = match launch("bravebot-rpc", "speak NDJSON on stdin/stdout", args) {
        Launch::Serve { settings } => settings,
        Launch::Version(line) => {
            println!("{line}");
            return;
        }
        Launch::Refused(lines) => {
            for line in lines {
                eprintln!("{line}");
            }
            std::process::exit(2);
        }
    };

    // Before a session is assembled, so every turn this process runs reads one answer for the
    // network its programs keep, for the lists of paths a person wrote and for the hosts a stage is
    // held to. Nothing in a window sets them. See SANDBOX-20, SANDBOX-25 and SANDBOX-24.
    bravebot_ui_bridge::settings::settle_for_process(
        std::env::current_dir().ok().as_deref(),
        settings.as_deref(),
    );

    // One writer behind a lock, so a line from an event and a line from a response cannot
    // interleave. Shared with the bridge, which emits events from whichever thread a turn
    // is running on.
    let out = Arc::new(Mutex::new(std::io::stdout()));

    let emitter = Arc::clone(&out);
    let mut bridge = Bridge::new(Box::new(move |event: Event| {
        write_line(&emitter, &event.to_value());
    }))
    .with_settings(settings);

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
