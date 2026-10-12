// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! A bridge that keeps its sessions while the programs that drive it come and go.
//!
//! The host starts the client program it was given over an anonymous channel
//! (`control_link::launch`) and serves it until it ends. There is no socket path or port for
//! another program to connect to. A later client is another launch of the same program, which the
//! person asks for by typing `launch` on the host's standard input; `quit` or the end of that
//! input stops the host and the work its sessions have going.
//!
//! **This is the only file here that may touch stdout or end the process,** for the reason
//! `bravebot-rpc` is.

#![forbid(unsafe_code)]

#[cfg(unix)]
fn main() {
    // First of all, as in `bravebot-rpc`: nothing else runs before the protection is requested.
    let outcome = bravebot_sandbox::inspection::refuse_same_user_inspection();
    let _ = bravebot_agent::crash::disable_core_dumps();

    let Some(start) = Start::from_arguments(std::env::args_os().skip(1)) else {
        std::process::exit(2);
    };
    let Start::Run {
        sessions_root,
        workspaces,
        settings,
        client,
    } = start
    else {
        println!(
            "bravebot-host {} (agent {})",
            env!("CARGO_PKG_VERSION"),
            bravebot_ui_bridge::agent_build()
        );
        return;
    };

    // The host serves real sessions, so it does not run where a process of the same user may be
    // able to copy the descriptors of its channel.
    let inspection = match bravebot_ui_bridge::host::inspection_report(outcome) {
        Ok(word) => word,
        Err(notice) => {
            eprintln!("bravebot-host: {notice}");
            std::process::exit(1);
        }
    };
    if let Some(missing) = workspaces.iter().find(|workspace| !workspace.is_dir()) {
        eprintln!(
            "bravebot-host: --workspace {} is not a directory",
            missing.display()
        );
        std::process::exit(1);
    }
    // The root holds every record. The host makes it private when it makes it, and uses one that is
    // there only if it is a real directory that others cannot reach: it does not change the mode of
    // a directory it did not make, which may be a person's home or a shared one.
    if let Err(error) = prepare_root(&sessions_root) {
        eprintln!("bravebot-host: {}: {error}", sessions_root.display());
        std::process::exit(1);
    }
    if let Err(already) = bravebot_session::sessions::keep_in(sessions_root) {
        eprintln!(
            "bravebot-host: sessions are already kept in {}",
            already.display()
        );
        std::process::exit(1);
    }
    bravebot_ui_bridge::settings::settle_for_process(settings.as_deref());

    let mut host = bravebot_ui_bridge::host::Host::new(bravebot_ui_bridge::host::Setup {
        workspaces,
        settings,
        inspection,
    });
    eprintln!("bravebot-host: instance {}", host.instance());

    let current: Current = Default::default();
    let (commands, received) = std::sync::mpsc::channel();
    listen_for_commands(commands, std::sync::Arc::clone(&current));
    loop {
        run_client(&mut host, &client, &current);
        // `quit` wins over a `launch` typed while the client was connected.
        let mut launch = false;
        for command in received.try_iter() {
            match command {
                Command::Quit => return stop(&mut host),
                Command::Launch => launch = true,
            }
        }
        while !launch {
            match received.recv() {
                Ok(Command::Launch) => launch = true,
                Ok(Command::Quit) | Err(_) => return stop(&mut host),
            }
        }
    }
}

/// What the person typed on the host's input.
#[cfg(unix)]
enum Command {
    Launch,
    Quit,
}

/// The near end of the client being served, kept so that `quit` can end it, and whether `quit`
/// has been asked for.
#[cfg(unix)]
#[derive(Default)]
struct Served {
    channel: std::sync::Mutex<Option<std::os::unix::net::UnixStream>>,
    quitting: std::sync::atomic::AtomicBool,
}

#[cfg(unix)]
type Current = std::sync::Arc<Served>;

/// Read `launch` and `quit` from the host's input on a thread of its own, so `quit` is heard while
/// a client is connected. Quitting ends the connection being served, which is what returns control
/// to the loop that stops the host.
#[cfg(unix)]
fn listen_for_commands(commands: std::sync::mpsc::Sender<Command>, current: Current) {
    std::thread::spawn(move || {
        let quit = |commands: &std::sync::mpsc::Sender<Command>| {
            current
                .quitting
                .store(true, std::sync::atomic::Ordering::SeqCst);
            let _ = commands.send(Command::Quit);
            if let Ok(channel) = current.channel.lock()
                && let Some(channel) = channel.as_ref()
            {
                let _ = channel.shutdown(std::net::Shutdown::Both);
            }
        };
        for line in std::io::stdin().lines() {
            match line.as_ref().map(|line| line.trim()) {
                Ok("launch") => {
                    let _ = commands.send(Command::Launch);
                }
                Ok("quit") => return quit(&commands),
                Ok(_) => eprintln!("bravebot-host: type `launch` or `quit`"),
                // A line that is not text is not the end of the input.
                Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
                    eprintln!("bravebot-host: that line is not text; type `launch` or `quit`");
                }
                Err(error) => {
                    eprintln!("bravebot-host: cannot read its input ({error}); stopping");
                    return quit(&commands);
                }
            }
        }
        eprintln!("bravebot-host: its input ended; stopping");
        quit(&commands);
    });
}

/// Make the sessions root, or accept one that is already private.
#[cfg(unix)]
fn prepare_root(root: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::symlink_metadata(root) {
        Ok(found) if found.file_type().is_symlink() => {
            Err("the sessions root is a link; name the directory itself".into())
        }
        Ok(found) if !found.is_dir() => Err("the sessions root is not a directory".into()),
        Ok(found) if found.permissions().mode() & 0o077 != 0 => Err(
            "the sessions root can be reached by other users; make it private (mode 700) or name another"
                .into(),
        ),
        Ok(_) => Ok(()),
        Err(_) => bravebot_agent::home::create_directory(root).map_err(|error| error.to_string()),
    }
    .and_then(|()| can_write_in(root))
}

/// A directory that is private but belongs to someone else, or cannot be written, is found here
/// and not at the first record. The marker is written through the crate that owns the store's
/// modes, and is overwritten on every start.
#[cfg(unix)]
fn can_write_in(root: &std::path::Path) -> Result<(), String> {
    bravebot_agent::home::write_file(&root.join(".host-started"), b"")
        .map_err(|error| format!("cannot write in the sessions root: {error}"))
}

/// Stop the work of every session and give the threads a few seconds to finish writing their
/// records before the process ends.
#[cfg(unix)]
fn stop(host: &mut bravebot_ui_bridge::host::Host) {
    if !host.shutdown(std::time::Duration::from_secs(10)) {
        eprintln!("bravebot-host: some sessions were still working when the host stopped");
    }
}

/// Start the client over a new channel and serve it until it ends.
#[cfg(unix)]
fn run_client(
    host: &mut bravebot_ui_bridge::host::Host,
    client: &[std::ffi::OsString],
    current: &Current,
) {
    let mut command = std::process::Command::new(&client[0]);
    command.args(&client[1..]);
    let mut linked = match bravebot_ui_bridge::control_link::launch(command) {
        Ok(linked) => linked,
        Err(error) => {
            eprintln!("bravebot-host: cannot start the client: {error}");
            return;
        }
    };
    if let Ok(mut slot) = current.channel.lock() {
        *slot = linked.channel.try_clone().ok();
    }
    // A `quit` typed before the channel was kept ends it now.
    if current.quitting.load(std::sync::atomic::Ordering::SeqCst) {
        let _ = linked.channel.shutdown(std::net::Shutdown::Both);
    }
    let ended = host.serve(linked.channel);
    if let Ok(mut slot) = current.channel.lock() {
        *slot = None;
    }
    match ended {
        Ok(ended) => eprintln!("bravebot-host: client ended ({ended:?})"),
        Err(error) => eprintln!("bravebot-host: serving the client failed: {error}"),
    }
    let _ = linked.child.kill();
    let _ = linked.child.wait();
}

/// What the command line asked for.
#[cfg(unix)]
enum Start {
    Version,
    Run {
        sessions_root: std::path::PathBuf,
        workspaces: Vec<std::path::PathBuf>,
        settings: Option<std::path::PathBuf>,
        client: Vec<std::ffi::OsString>,
    },
}

#[cfg(unix)]
const USAGE: &str = "usage: bravebot-host --sessions-root DIR --workspace DIR [--workspace DIR]... \
[--settings FILE] -- CLIENT [ARGUMENT]...\n       bravebot-host --version\n\
CLIENT is the absolute path of the program to drive the host. It is started over a channel it \
shares with the host alone.";

/// The value after an option, or a word about its absence.
#[cfg(unix)]
fn value(
    arguments: &mut impl Iterator<Item = std::ffi::OsString>,
    option: &str,
) -> Option<std::ffi::OsString> {
    let given = arguments.next();
    if given.is_none() {
        eprintln!("bravebot-host: {option} needs a value\n{USAGE}");
    }
    given
}

#[cfg(unix)]
impl Start {
    /// Everything here is the person's own choice, made on the command line they typed. Nothing is
    /// read from a file a tool could write.
    // nosemgrep: rust.lang.security.args.args
    fn from_arguments(arguments: impl Iterator<Item = std::ffi::OsString>) -> Option<Self> {
        let mut arguments = arguments;
        let (mut root, mut workspaces, mut settings) = (None, Vec::new(), None);
        while let Some(argument) = arguments.next() {
            match argument.to_str() {
                Some("--version" | "-V") => return Some(Self::Version),
                Some("--sessions-root") => {
                    root = Some(value(&mut arguments, "--sessions-root")?.into())
                }
                Some("--workspace") => {
                    workspaces.push(value(&mut arguments, "--workspace")?.into())
                }
                Some("--settings") => settings = Some(value(&mut arguments, "--settings")?.into()),
                Some("--") => {
                    let client: Vec<std::ffi::OsString> = arguments.collect();
                    return Self::checked(root, workspaces, settings, client);
                }
                _ => {
                    eprintln!(
                        "bravebot-host: unknown option: {}\n{USAGE}",
                        argument.to_string_lossy()
                    );
                    return None;
                }
            }
        }
        eprintln!("bravebot-host: no client given after `--`\n{USAGE}");
        None
    }

    fn checked(
        sessions_root: Option<std::path::PathBuf>,
        workspaces: Vec<std::path::PathBuf>,
        settings: Option<std::path::PathBuf>,
        client: Vec<std::ffi::OsString>,
    ) -> Option<Self> {
        let Some(sessions_root) = sessions_root.filter(|root| root.is_absolute()) else {
            eprintln!("bravebot-host: --sessions-root must be an absolute path\n{USAGE}");
            return None;
        };
        if workspaces.is_empty() {
            eprintln!("bravebot-host: give at least one --workspace\n{USAGE}");
            return None;
        }
        if client
            .first()
            .is_none_or(|program| !std::path::Path::new(program).is_absolute())
        {
            eprintln!("bravebot-host: the client must be given as an absolute path\n{USAGE}");
            return None;
        }
        Some(Self::Run {
            sessions_root,
            workspaces,
            settings,
            client,
        })
    }
}

#[cfg(not(unix))]
fn main() {
    eprintln!("bravebot-host runs on Unix only");
    std::process::exit(2);
}
