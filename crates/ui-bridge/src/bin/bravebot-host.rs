// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! A bridge that keeps its sessions while the programs that drive it come and go.
//!
//! The host starts each client program it was given over an anonymous channel
//! (`control_link::launch`) and serves it until it ends. There is no socket path or port for
//! another program to connect to. Each client has a name and the answers it may give, both from
//! the command line. The clients start once each, in the order given, and are served at once. A
//! client that has ended is started again when the person types `launch NAME` on the host's
//! standard input; `quit` or the end of that input stops the host and the work its sessions have
//! going.
//!
//! **This is the only file here that may touch stdout or end the process,** for the reason
//! `bravebot-rpc` is.

#![forbid(unsafe_code)]

#[cfg(unix)]
fn main() {
    // First of all, as in `bravebot-rpc`: nothing else runs before the protection is requested.
    let outcome = bravebot_sandbox::inspection::refuse_same_user_inspection();
    let _ = bravebot_agent::crash::disable_core_dumps();
    // A descriptor whoever started this process left open would reach every hook and tool it
    // starts, so each one other than standard input, output and error is marked close-on-exec
    // before any thread exists. See RPCVIEW-8.
    if let Err(error) = bravebot_sandbox::process::withhold_inherited_descriptors() {
        eprintln!(
            "bravebot-host: the descriptors this process was started with could not be kept from the programs it starts: {error}"
        );
    }

    // The directories an earlier process let a stage write are left out of the search for the
    // programs this one starts, and each directory a session here opens is added to that record
    // for the next. Before a program is looked up. See SANDBOX-30.
    bravebot_agent::stage_record::load();
    bravebot_agent::stage_record::follow();

    let Some(start) = Start::from_arguments(std::env::args_os().skip(1)) else {
        std::process::exit(2);
    };
    let Start::Run {
        sessions_root,
        workspaces,
        settings,
        listen,
        clients,
    } = start
    else {
        println!(
            "{}",
            bravebot_ui_bridge::arguments::version_line("bravebot-host")
        );
        return;
    };

    // As soon as this process knows it is serving, so a panic in anything that follows is covered.
    // A build with nowhere to send reports, or a person who switched them off, gets nothing from
    // this call (CRASH-3, CRASH-4). The directory is the one `home::writable` names, which is none
    // under incognito mode, so nothing would start if this process ever engaged it.
    bravebot_backtrace::start(bravebot_backtrace::Process::Host);

    let inspection = prepare(outcome, &workspaces, sessions_root, settings.as_deref());

    // Before any client starts, the person is asked here about each workspace no yes was kept
    // about, as a session in lines asks (TRUST-23). The answers are read before `launch` and `quit`
    // are, from the same input.
    let asked = bravebot_ui_bridge::host_trust::ask_about_each(
        &workspaces,
        bravebot_agent::home::directory().as_deref(),
        bravebot_agent::home::profile().as_deref(),
        &mut std::io::stdin().lock(),
        &mut std::io::stderr(),
    );
    if asked == bravebot_ui_bridge::host_trust::Asked::Stop {
        return;
    }

    // The listener is the host's one network address. A host asked to listen where it cannot stops,
    // since a person who named an address expects to be reachable there. It starts before any
    // client does.
    let link = listen.map(|address| start_link(address, &workspaces));
    let host = bravebot_ui_bridge::host::Host::new(bravebot_ui_bridge::host::Setup {
        workspaces,
        settings,
        inspection,
    });
    eprintln!("bravebot-host: instance {}", host.instance());

    let clients = std::sync::Arc::new(clients);
    let current = std::sync::Arc::new(Served {
        channels: std::sync::Mutex::new(std::collections::HashMap::new()),
        quitting: std::sync::atomic::AtomicBool::new(false),
        // Every client starts once, in the order the command line gives.
        connected: std::sync::Mutex::new((0..clients.len()).collect()),
    });
    let names = clients.iter().map(|client| client.name.clone()).collect();
    let (commands, received) = std::sync::mpsc::channel();
    listen_for_commands(
        commands,
        std::sync::Arc::clone(&current),
        names,
        link.clone(),
    );
    let mut running: Vec<_> = (0..clients.len())
        .map(|client| start_client(&host, &clients, client, &current))
        .collect();
    loop {
        match received.recv() {
            // `quit` wins over a client still to start.
            Ok(Command::Launch(_))
                if current.quitting.load(std::sync::atomic::Ordering::SeqCst) => {}
            Ok(Command::Launch(client)) => {
                running.push(start_client(&host, &clients, client, &current));
            }
            Ok(Command::Quit) | Err(_) => break,
        }
    }
    // Every device's connection ends, and no new one is accepted.
    if let Some(link) = &link {
        link.stop();
    }
    // Every channel has been shut, so each client's connection ends and its thread returns.
    for client in running {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a client's thread that panicked has already lost its connection, and the host stops either way"
        )]
        let _ = client.join();
    }
    stop(&host);
}

/// What the host checks and sets before it asks about a workspace or starts anything: that no
/// process of the same user can copy its descriptors, that each workspace is a directory, the
/// sessions root, and the settings it reads. Says why and stops the host where one fails. Returns
/// the word `agent.info` reports for inspection.
#[cfg(unix)]
fn prepare(
    outcome: bravebot_sandbox::inspection::Inspection,
    workspaces: &[std::path::PathBuf],
    sessions_root: std::path::PathBuf,
    settings: Option<&std::path::Path>,
) -> &'static str {
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
    // The host works in several workspaces and is started from wherever its person was, so the
    // directory it was started in contributes no settings of its own.
    let process = bravebot_ui_bridge::settings::layers(None, settings);
    bravebot_ui_bridge::settings::settle_for_process(&process);
    bravebot_ui_bridge::settings::start_the_sweep_for_process(&process);
    inspection
}

/// What the person typed on the host's input.
#[cfg(unix)]
enum Command {
    /// The client at this place on the command line, which is not connected.
    Launch(usize),
    Quit,
}

/// The near end of each client's channel, kept so that `quit` can end it, whether `quit` has been
/// asked for, and which clients are connected or starting, by their place on the command line.
#[cfg(unix)]
struct Served {
    channels: std::sync::Mutex<std::collections::HashMap<usize, std::os::unix::net::UnixStream>>,
    quitting: std::sync::atomic::AtomicBool,
    connected: std::sync::Mutex<std::collections::HashSet<usize>>,
}

#[cfg(unix)]
impl Served {
    /// `client` is no longer connected, so it may be launched again.
    fn served(&self, client: usize) {
        if let Ok(mut connected) = self.connected.lock() {
            connected.remove(&client);
        }
    }

    /// End every client's connection.
    fn end_every_channel(&self) {
        if let Ok(channels) = self.channels.lock() {
            for channel in channels.values() {
                let _ = channel.shutdown(std::net::Shutdown::Both);
            }
        }
    }
}

#[cfg(unix)]
type Current = std::sync::Arc<Served>;

/// The client `named`, counted as connected from now, or why it cannot be launched. A bare
/// `launch` names the only client there is. A client already connected is not launched twice.
#[cfg(unix)]
fn to_launch(current: &Served, names: &[String], named: Option<&str>) -> Result<usize, String> {
    let client = match named {
        Some(named) => names
            .iter()
            .position(|name| name == named)
            .ok_or_else(|| format!("no client is named `{named}`"))?,
        None if names.len() == 1 => 0,
        None => {
            return Err(format!(
                "name the client to launch: `launch NAME`, where NAME is one of {}",
                names.join(", ")
            ));
        }
    };
    let mut connected = current
        .connected
        .lock()
        .map_err(|_| "the clients connected cannot be read".to_string())?;
    if !connected.insert(client) {
        return Err(format!(
            "the client named `{}` is still connected",
            names[client]
        ));
    }
    Ok(client)
}

/// Read `launch` and `quit` from the host's input on a thread of its own, so `quit` is heard while
/// clients are connected. Quitting ends every client's connection, which is what lets the host stop.
#[cfg(unix)]
fn listen_for_commands(
    commands: std::sync::mpsc::Sender<Command>,
    current: Current,
    names: Vec<String>,
    link: Option<std::sync::Arc<bravebot_ui_bridge::device_link::Link>>,
) {
    std::thread::spawn(move || {
        let quit = |commands: &std::sync::mpsc::Sender<Command>| {
            current
                .quitting
                .store(true, std::sync::atomic::Ordering::SeqCst);
            let _ = commands.send(Command::Quit);
            current.end_every_channel();
        };
        for line in std::io::stdin().lines() {
            let launch = |named| match to_launch(&current, &names, named) {
                Ok(client) => {
                    let _ = commands.send(Command::Launch(client));
                }
                Err(refused) => eprintln!("bravebot-host: {refused}"),
            };
            match line.as_ref().map(|line| line.trim()) {
                Ok("launch") => launch(None),
                Ok(line) if line.starts_with("launch ") => {
                    launch(line.strip_prefix("launch ").map(str::trim))
                }
                Ok("quit") => return quit(&commands),
                Ok(line) => say_in_answer(link.as_deref(), line),
                // A line that is not text is not the end of the input.
                Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
                    eprintln!("bravebot-host: that line is not text; type `launch NAME` or `quit`");
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

/// Start the listener, or say why not and stop the host.
#[cfg(unix)]
fn start_link(
    address: std::net::SocketAddr,
    workspaces: &[std::path::PathBuf],
) -> std::sync::Arc<bravebot_ui_bridge::device_link::Link> {
    use std::io::IsTerminal;
    let say: bravebot_ui_bridge::device_link::Say =
        std::sync::Arc::new(|line| eprintln!("bravebot-host: {line}"));
    match bravebot_ui_bridge::device_link::Link::start(
        address,
        workspaces,
        std::io::stderr().is_terminal(),
        say,
    ) {
        Ok(link) => {
            eprintln!("bravebot-host: {}", link.listening());
            std::sync::Arc::new(link)
        }
        Err(error) => {
            eprintln!("bravebot-host: {error}");
            std::process::exit(1);
        }
    }
}

/// Say what a line on the host's input that is not `launch` or `quit` asks for: a command about
/// devices where the host is listening, and otherwise what the host does take.
#[cfg(unix)]
fn say_in_answer(link: Option<&bravebot_ui_bridge::device_link::Link>, line: &str) {
    use bravebot_ui_bridge::device_link::Said;
    match link.and_then(|link| link.command(line)) {
        Some(said) => {
            for said in said {
                match said {
                    Said::Line(line) => eprintln!("bravebot-host: {line}"),
                    Said::Block(block) => eprintln!("{block}"),
                }
            }
        }
        None if matches!(
            line.split_whitespace().next(),
            Some("pair" | "confirm" | "devices" | "revoke")
        ) =>
        {
            eprintln!("bravebot-host: start the host with --listen ADDR:PORT to pair a device");
        }
        None => eprintln!(
            "bravebot-host: type `launch NAME` or `quit`, or where it listens `pair`, `confirm`, `devices` or `revoke`"
        ),
    }
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
fn stop(host: &bravebot_ui_bridge::host::Host) {
    if !host.shutdown(std::time::Duration::from_secs(10)) {
        eprintln!("bravebot-host: some sessions were still working when the host stopped");
    }
}

/// Start the client at `client` on the command line over a new channel, and serve it on a thread
/// of its own until it ends. Said here, so the clients are said to start in the order they do.
#[cfg(unix)]
fn start_client(
    host: &bravebot_ui_bridge::host::Host,
    clients: &std::sync::Arc<Vec<Client>>,
    client: usize,
    current: &Current,
) -> std::thread::JoinHandle<()> {
    eprintln!("bravebot-host: starting client {}", clients[client].name);
    let (host, clients, current) = (
        host.clone(),
        std::sync::Arc::clone(clients),
        std::sync::Arc::clone(current),
    );
    std::thread::spawn(move || run_client(&host, &clients[client], client, &current))
}

/// Start the client over a new channel and serve it until it ends.
#[cfg(unix)]
fn run_client(
    host: &bravebot_ui_bridge::host::Host,
    client: &Client,
    place: usize,
    current: &Current,
) {
    let name = &client.name;
    let mut command = bravebot_sandbox::programs::start(
        &bravebot_sandbox::programs::Resolved::named_by_person(&client.program),
        bravebot_sandbox::programs::Inherits::All,
        bravebot_sandbox::programs::SearchPath::Inherited,
        // The client the person named, started with the environment they set (SANDBOX-30).
        bravebot_sandbox::programs::CodeSelectors::Kept,
    );
    command.args(&client.arguments);
    let mut linked = match bravebot_ui_bridge::control_link::launch(command) {
        Ok(linked) => linked,
        Err(error) => {
            current.served(place);
            eprintln!("bravebot-host: cannot start client {name}: {error}");
            return;
        }
    };
    if let Ok(mut channels) = current.channels.lock()
        && let Ok(kept) = linked.channel.try_clone()
    {
        channels.insert(place, kept);
    }
    // A `quit` typed before the channel was kept ends it now.
    if current.quitting.load(std::sync::atomic::Ordering::SeqCst) {
        let _ = linked.channel.shutdown(std::net::Shutdown::Both);
    }
    let ended = host.serve(linked.channel, &client.name, &client.answers);
    if let Ok(mut channels) = current.channels.lock() {
        channels.remove(&place);
    }
    // The host has dealt with the loss of the connection. A launch typed once the line below is
    // written is accepted.
    current.served(place);
    match ended {
        Ok(ended) => eprintln!("bravebot-host: client {name} ended ({ended:?})"),
        Err(error) => eprintln!("bravebot-host: serving client {name} failed: {error}"),
    }
    let _ = linked.child.kill();
    let _ = linked.child.wait();
}

/// A client program the person named, what to call it, and the answers it may give.
#[cfg(unix)]
#[derive(Debug, PartialEq)]
struct Client {
    name: String,
    answers: Vec<bravebot_ui_bridge::allowlist::Answer>,
    program: std::ffi::OsString,
    arguments: Vec<std::ffi::OsString>,
}

/// A client as far as the command line has named it.
#[cfg(unix)]
struct Named {
    name: String,
    answers: Option<Vec<bravebot_ui_bridge::allowlist::Answer>>,
    program: Option<std::ffi::OsString>,
    arguments: Vec<std::ffi::OsString>,
}

/// What the command line asked for.
#[cfg(unix)]
#[derive(Debug, PartialEq)]
enum Start {
    Version,
    Run {
        sessions_root: std::path::PathBuf,
        workspaces: Vec<std::path::PathBuf>,
        settings: Option<std::path::PathBuf>,
        /// The one address the host listens on for paired devices, where it was given.
        listen: Option<std::net::SocketAddr>,
        clients: Vec<Client>,
    },
}

#[cfg(unix)]
const USAGE: &str = "usage: bravebot-host --sessions-root DIR --workspace DIR [--workspace DIR]... \
[--settings FILE] [--listen ADDR:PORT] CLIENT...\n       bravebot-host --version\n\
CLIENT is --client NAME --answers KINDS --program PATH [--arg VALUE]...\n\
NAME is up to 32 of a-z, 0-9 and -, and names one client. KINDS is the answers the client may \
give, some of confirm, run, fetch, ask and trust separated by commas, or none. PATH is the \
absolute path of the program, which is started over a channel it shares with the host alone.";

/// Say why the command line is refused.
#[cfg(unix)]
fn refuse<T>(why: &str) -> Option<T> {
    eprintln!("bravebot-host: {why}\n{USAGE}");
    None
}

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

/// The value after an option that has to be text.
#[cfg(unix)]
fn text(arguments: &mut impl Iterator<Item = std::ffi::OsString>, option: &str) -> Option<String> {
    value(arguments, option)?
        .into_string()
        .ok()
        .or_else(|| refuse(&format!("the value of {option} is not text")))
}

/// The client the last `--client` named, which `option` is about.
#[cfg(unix)]
fn current<'a>(clients: &'a mut [Named], option: &str) -> Option<&'a mut Named> {
    clients
        .last_mut()
        .or_else(|| refuse(&format!("{option} comes after the --client it is about")))
}

#[cfg(unix)]
fn is_a_name(name: &str) -> bool {
    (1..=32).contains(&name.len())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

#[cfg(unix)]
impl Start {
    /// Everything here is the person's own choice, made on the command line they typed. Nothing is
    /// read from a file a tool could write, and nothing a client sends changes it.
    // nosemgrep: rust.lang.security.args.args
    fn from_arguments(arguments: impl Iterator<Item = std::ffi::OsString>) -> Option<Self> {
        let mut arguments = arguments;
        let (mut root, mut workspaces, mut settings, mut listen) =
            (None, Vec::new(), None, None::<std::net::SocketAddr>);
        let mut clients: Vec<Named> = Vec::new();
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
                Some("--listen") => {
                    let address = text(&mut arguments, "--listen")?;
                    if listen.is_some() {
                        return refuse("--listen is given once: the host listens on one address");
                    }
                    let Ok(address) = address.parse::<std::net::SocketAddr>() else {
                        return refuse(&format!("`{address}` is not ADDR:PORT"));
                    };
                    if address.ip().is_unspecified() {
                        return refuse(
                            "--listen names one address and not every interface; give the address of the interface devices reach",
                        );
                    }
                    listen = Some(address);
                }
                Some("--client") => {
                    let name = text(&mut arguments, "--client")?;
                    if !is_a_name(&name) {
                        return refuse(&format!(
                            "`{name}` is not a client name; use 1 to 32 of a-z, 0-9 and -"
                        ));
                    }
                    if clients.iter().any(|client| client.name == name) {
                        return refuse(&format!("two clients are named `{name}`"));
                    }
                    clients.push(Named {
                        name,
                        answers: None,
                        program: None,
                        arguments: Vec::new(),
                    });
                }
                Some("--answers") => {
                    let words = text(&mut arguments, "--answers")?;
                    let client = current(&mut clients, "--answers")?;
                    if client.answers.is_some() {
                        return refuse(&format!("client `{}` has two --answers", client.name));
                    }
                    match bravebot_ui_bridge::allowlist::Answer::list(&words) {
                        Ok(answers) => client.answers = Some(answers),
                        Err(why) => return refuse(&format!("client `{}`: {why}", client.name)),
                    }
                }
                Some("--program") => {
                    let program = value(&mut arguments, "--program")?;
                    let client = current(&mut clients, "--program")?;
                    if client.program.is_some() {
                        return refuse(&format!("client `{}` has two --program", client.name));
                    }
                    client.program = Some(program);
                }
                Some("--") => {
                    return refuse(
                        "name each client with --client NAME --answers KINDS --program PATH, \
                         not after `--`",
                    );
                }
                // Taken as it is, so a value that looks like an option, or is not text, is passed on.
                Some("--arg") => {
                    let argument = value(&mut arguments, "--arg")?;
                    current(&mut clients, "--arg")?.arguments.push(argument);
                }
                _ => {
                    return refuse(&format!("unknown option: {}", argument.to_string_lossy()));
                }
            }
        }
        Self::checked(root, workspaces, settings, listen, clients)
    }

    fn checked(
        sessions_root: Option<std::path::PathBuf>,
        workspaces: Vec<std::path::PathBuf>,
        settings: Option<std::path::PathBuf>,
        listen: Option<std::net::SocketAddr>,
        named: Vec<Named>,
    ) -> Option<Self> {
        let Some(sessions_root) = sessions_root.filter(|root| root.is_absolute()) else {
            return refuse("--sessions-root must be an absolute path");
        };
        if workspaces.is_empty() {
            return refuse("give at least one --workspace");
        }
        if named.is_empty() {
            return refuse("name at least one client with --client");
        }
        let mut clients = Vec::new();
        for client in named {
            // The person says what each client may answer. A client given no answers by mistake,
            // or every answer by default, would not be what they chose.
            let Some(answers) = client.answers else {
                return refuse(&format!("client `{}` needs --answers", client.name));
            };
            let Some(program) = client
                .program
                .filter(|program| std::path::Path::new(program).is_absolute())
            else {
                return refuse(&format!(
                    "client `{}` needs --program with an absolute path",
                    client.name
                ));
            };
            clients.push(Client {
                name: client.name,
                answers,
                program,
                arguments: client.arguments,
            });
        }
        Some(Self::Run {
            sessions_root,
            workspaces,
            settings,
            listen,
            clients,
        })
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use bravebot_ui_bridge::allowlist::Answer;
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    fn parsed(arguments: Vec<OsString>) -> Option<Start> {
        Start::from_arguments(arguments.into_iter())
    }

    fn words(arguments: &[&str]) -> Vec<OsString> {
        arguments.iter().map(OsString::from).collect()
    }

    const BEFORE: [&str; 4] = ["--sessions-root", "/root", "--workspace", "/work"];

    /// Each client keeps its own name, answers, program and arguments, in the order given, and an
    /// argument is passed on as it is, including one that looks like an option or is not text.
    #[test]
    fn each_client_keeps_what_its_group_named_in_order() {
        let mut arguments = words(&BEFORE);
        arguments.extend(words(&[
            "--client",
            "relay",
            "--answers",
            "confirm,ask",
            "--program",
            "/bin/node",
            "--arg",
            "--",
            "--arg",
            "--workspace",
            "--client",
            "test",
            "--answers",
            "none",
            "--program",
            "/bin/test",
            "--arg",
        ]));
        arguments.push(OsString::from_vec(b"\xff".to_vec()));
        let Some(Start::Run { clients, .. }) = parsed(arguments) else {
            panic!("refused");
        };
        assert_eq!(
            clients,
            [
                Client {
                    name: "relay".into(),
                    answers: vec![Answer::Confirm, Answer::Ask],
                    program: "/bin/node".into(),
                    arguments: words(&["--", "--workspace"]),
                },
                Client {
                    name: "test".into(),
                    answers: vec![],
                    program: "/bin/test".into(),
                    arguments: vec![OsString::from_vec(b"\xff".to_vec())],
                },
            ]
        );
    }

    /// A command line that leaves a client's answers or program to a default, names a client
    /// twice or by a name that is not one, or uses the `--` form, is refused.
    #[test]
    fn a_client_that_is_not_fully_named_is_refused() {
        for client in [
            &["--client", "a", "--program", "/bin/cat"][..],
            &["--client", "a", "--answers", "confirm"],
            &["--client", "a", "--answers", "confirm", "--program", "cat"],
            &[
                "--client",
                "a",
                "--answers",
                "write",
                "--program",
                "/bin/cat",
            ],
            &["--client", "a", "--answers", "", "--program", "/bin/cat"],
            &[
                "--client",
                "A",
                "--answers",
                "none",
                "--program",
                "/bin/cat",
            ],
            &["--client", "", "--answers", "none", "--program", "/bin/cat"],
            &[
                "--client",
                "a-name-that-is-thirty-three-chars",
                "--answers",
                "none",
                "--program",
                "/bin/cat",
            ],
            &[
                "--client",
                "a",
                "--answers",
                "none",
                "--answers",
                "run",
                "--program",
                "/bin/cat",
            ],
            &[
                "--client",
                "a",
                "--answers",
                "none",
                "--program",
                "/bin/cat",
                "--program",
                "/bin/sh",
            ],
            &[
                "--client",
                "a",
                "--answers",
                "none",
                "--program",
                "/bin/cat",
                "--client",
                "a",
                "--answers",
                "none",
                "--program",
                "/bin/cat",
            ],
            &[
                "--answers",
                "none",
                "--client",
                "a",
                "--program",
                "/bin/cat",
            ],
            &[
                "--arg",
                "x",
                "--client",
                "a",
                "--answers",
                "none",
                "--program",
                "/bin/cat",
            ],
            &["--", "/bin/cat"],
            &[],
        ] {
            let mut arguments = words(&BEFORE);
            arguments.extend(words(client));
            assert!(parsed(arguments).is_none(), "{client:?} was accepted");
        }
    }

    #[test]
    fn a_name_of_thirty_two_allowed_characters_is_one() {
        assert!(is_a_name("a-name-that-is-thirty-two-chars0"));
        assert!(!is_a_name("a_name"));
    }
}

#[cfg(not(unix))]
fn main() {
    eprintln!("bravebot-host runs on Unix only");
    std::process::exit(2);
}
