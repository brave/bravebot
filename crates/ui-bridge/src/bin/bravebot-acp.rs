//! The Agent Client Protocol on stdin and stdout, for an editor that hosts a session.
//!
//! The whole of the transport, as `bravebot-rpc` is for the desktop window: it reads a line, hands
//! it to [`bravebot_ui_bridge::acp::Acp`] and writes what comes back. What each message means is
//! decided in the library. An editor starts this program and speaks newline-delimited JSON-RPC to
//! it.
//!
//! **This file and `bravebot-rpc` are the only ones in the crate allowed to touch stdout or end
//! the process.** Elsewhere a stray `println!` would interleave with the protocol and no editor
//! could stop it.

#![forbid(unsafe_code)]

use bravebot_ui_bridge::acp::Acp;
use std::io::{BufRead, Write};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

/// What the main loop waits on.
enum Input {
    Line(String),
    /// An event asked for a call that only this thread may make.
    Wake,
    End,
}

fn main() {
    // First of all, as the other entry points do: a credential is in this process's memory from
    // the first turn, and a crash after this writes no image of it. See CRED-23.
    let _ = bravebot_agent::crash::disable_core_dumps();

    // Not a security decision. The only argument is `--settings <path>`, which names a settings
    // file for a developer driving this by hand, and what that file may grant is decided by the
    // policy layer rather than here. Nothing is authorised by an argument.
    // nosemgrep: rust.lang.security.args.args
    let mut args = std::env::args().skip(1);
    let mut settings = None;
    if let Some(flag) = args.next() {
        match flag.as_str() {
            "--settings" => {
                let Some(path) = args.next() else {
                    eprintln!("--settings requires a file path");
                    std::process::exit(2);
                };
                if args.next().is_some() {
                    eprintln!("unexpected arguments after settings file");
                    std::process::exit(2);
                }
                settings = Some(std::path::PathBuf::from(path));
            }
            "--version" | "-V" => {
                println!(
                    "bravebot-acp {} (agent {})",
                    env!("CARGO_PKG_VERSION"),
                    bravebot_ui_bridge::agent_build()
                );
                return;
            }
            other => {
                eprintln!("unknown option: {other}");
                eprintln!("usage: bravebot-acp            speak ACP on stdin/stdout");
                eprintln!("       bravebot-acp --version");
                std::process::exit(2);
            }
        }
    }

    // Before a session is assembled, so every turn this process runs reads one answer for the
    // network its programs keep and for the lists of paths a person wrote. See SANDBOX-20 and
    // SANDBOX-25.
    let here = std::env::current_dir().ok();
    let layers = bravebot_ui_bridge::settings::layers(here.as_deref(), settings.as_deref());
    bravebot_config::settle_run_network_in(None, &layers);
    bravebot_config::settle_sandbox_filesystem_in(
        &bravebot_sandbox::rules::Lists::default(),
        &layers,
    );

    let (input, lines) = mpsc::channel();
    let out = Arc::new(Mutex::new(std::io::stdout()));
    let writer = Arc::clone(&out);
    let waker = input.clone();
    let mut acp = Acp::new(
        Box::new(move |message| write_line(&writer, &message)),
        Box::new(move || {
            let _ = waker.send(Input::Wake);
        }),
        settings,
    );

    let reader = input.clone();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            // Invalid UTF-8 on the wire has nobody to be answered to. It is skipped, since an
            // editor that wrote one bad line will write another and exiting loses the rest.
            let Ok(line) = line else {
                eprintln!("bravebot-acp: unreadable line");
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

    // EOF on stdin. Anything waiting on an answer from an editor that has gone away is refused,
    // which is what dropping the bridge does.
    drop(acp);
}

/// Write one line, or give up on writing altogether. A closed stdout means the editor is gone.
fn write_line(out: &Arc<Mutex<std::io::Stdout>>, value: &serde_json::Value) {
    let Ok(mut handle) = out.lock() else {
        return;
    };
    let _ = writeln!(handle, "{value}");
    let _ = handle.flush();
}
