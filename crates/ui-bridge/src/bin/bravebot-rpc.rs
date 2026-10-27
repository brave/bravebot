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

    // The desktop window is in English only, so the notes this process builds for it are too,
    // whatever the machine's language: a French note in an English window reads as a fault. The
    // terminal chooses from the environment instead (LOCALE-7). This is the one choice, made before
    // anything is said.
    bravebot_i18n::init(bravebot_i18n::Locale::EnUs);

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

    // As soon as this process knows it is serving, so a panic in anything that follows is covered.
    // A build with nowhere to send reports, or a person who switched them off, gets nothing from
    // this call (CRASH-3, CRASH-4). The directory is the one `home::writable` names, which is none
    // under incognito mode, so nothing would start if this process ever engaged it.
    bravebot_backtrace::start(bravebot_backtrace::Process::Rpc);
    // The terminal's log, at the terminal's default level and in the same directory, so a failure
    // in the window leaves the record a bug report names (DIAG-4, DIAG-8). The window has no flag
    // to raise the level.
    bravebot_diag::configure(
        bravebot_diag::Level::Error,
        bravebot_agent::home::writable(),
    );

    // Before a session is assembled, so every turn this process runs reads one answer for the
    // network its programs keep, for the lists of paths a person wrote and for the hosts a stage is
    // held to. Nothing in a window sets them. See SANDBOX-20, SANDBOX-25 and SANDBOX-24.
    bravebot_ui_bridge::settings::settle_for_process(
        std::env::current_dir().ok().as_deref(),
        settings.as_deref(),
    );
    bravebot_ui_bridge::settings::start_the_sweep_for_process(
        std::env::current_dir().ok().as_deref(),
        settings.as_deref(),
    );
    // The hourly ask for the newest release, on a thread of its own, so nothing waits on it. It
    // asks nothing where the check is off or the session keeps nothing (UPDATE-8, UPDATE-11).
    bravebot_update::desktop_refresh(
        bravebot_ui_bridge::settings::layers(
            std::env::current_dir().ok().as_deref(),
            settings.as_deref(),
        )
        .update_check(),
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

    // EOF on stdin. Every session is closed, which refuses anything waiting on an answer from a
    // front-end that has gone away: a channel that cannot carry the question cannot carry consent
    // either. Closing is also what fires `session-ended`, so the process waits a few seconds for it
    // and for the turns still writing their records, as the host does.
    if !bridge.shut_down(std::time::Duration::from_secs(10)) {
        eprintln!("bravebot-rpc: some sessions were still working when the process stopped");
    }
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
