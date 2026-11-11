//! The Agent Client Protocol on stdin and stdout, for an editor that hosts a session.
//!
//! The whole of the transport, as `bravebot-rpc` is for the desktop window: it reads a line, hands
//! it to [`bravebot_ui_bridge::acp::Acp`] and writes what comes back. What each message means is
//! decided in the library, and the loop is [`bravebot_ui_bridge::acp_loop::serve`], which
//! `bravebot acp` runs as well. An editor starts this program and speaks newline-delimited
//! JSON-RPC to it.
//!
//! **This file and `bravebot-rpc` are the only ones in the crate allowed to touch stdout or end
//! the process.** Elsewhere a stray `println!` would interleave with the protocol and no editor
//! could stop it.

#![forbid(unsafe_code)]

use bravebot_ui_bridge::arguments::{Launch, launch};

fn main() {
    // First of all, before anything else runs: the descriptors this process speaks over exist
    // from the moment it starts, and they are the channel to the editor. Another process of the
    // same user must not be able to copy them out of it. See RPCVIEW-8. The protection starts
    // here, so a process already positioned to act before this runs is outside what it claims.
    if let Some(notice) = bravebot_sandbox::inspection::refuse_same_user_inspection().notice() {
        eprintln!("bravebot-acp: {notice}");
    }
    // And for the reason the terminal entry point does it early: a credential is in this process's
    // memory from the first turn, and a crash after this writes no image of it. See CRED-23.
    let _ = bravebot_agent::crash::disable_core_dumps();

    // Not a security decision. The only argument is `--settings <path>`, which names a settings
    // file for a developer driving this by hand, and what that file may grant is decided by the
    // policy layer rather than here. Nothing is authorised by an argument.
    // nosemgrep: rust.lang.security.args.args
    let args = std::env::args().skip(1);
    let settings = match launch("bravebot-acp", "speak ACP on stdin/stdout", args) {
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
    bravebot_backtrace::start(bravebot_backtrace::Process::Acp);

    // Before a session is assembled, so every turn this process runs reads one answer. See
    // SANDBOX-20, SANDBOX-25 and SANDBOX-24.
    let process = bravebot_ui_bridge::settings::layers(
        std::env::current_dir().ok().as_deref(),
        settings.as_deref(),
    );
    bravebot_ui_bridge::settings::settle_for_process(&process);
    bravebot_ui_bridge::settings::start_the_sweep_for_process(&process);

    // EOF on stdin ends the loop with every session closed, which refuses anything waiting on an
    // answer from an editor that has gone away and fires `session-ended`.
    let finished = bravebot_ui_bridge::acp_loop::serve(
        std::io::BufReader::new(std::io::stdin()),
        std::io::stdout(),
        settings,
        || eprintln!("bravebot-acp: unreadable line"),
    );
    if !finished {
        eprintln!("bravebot-acp: some sessions were still working when the process stopped");
    }
}
