//! `bravebot sessions`, `--bg`, `attach` and `reply`: the sessions that keep running after the
//! terminal closes (BG-1, BG-2, BG-5, BG-9, BG-10, BG-11, BG-16).
//!
//! Everything drawn here is a thing the person typed, a fact about a process, or a word from a
//! fixed set. The roster has no field for anything else (BG-4), so there is nothing in it to
//! draw that a model or a tool wrote.

use crate::exit::{Ending, fail};
use bravebot_i18n::t;
use bravebot_session::jobs::{Held, Lookup, Mode, Roster, Seen, State, Stopped};
use bravebot_tui::sessions_view::{Choice, Row};
use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

/// How long a stop waits for a session to end before it is killed.
const GRACE: std::time::Duration = std::time::Duration::from_secs(5);

/// The characters of an id a line shows, which is enough to name it to `attach` and `reply`.
const ID_SHOWN: usize = 8;

/// `bravebot sessions [--json]` and `bravebot sessions stop <id>`.
///
/// The bare word is the roster of background sessions, which is what it has always been. The
/// sessions recorded for a directory are a separate listing under `list` (CLI-32).
pub(crate) fn sessions(args: &[String]) -> ExitCode {
    if wants_the_view(
        args,
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
        std::io::stderr().is_terminal(),
    ) {
        return view();
    }
    match args {
        [] => list(false),
        [flag] if flag == "--json" => list(true),
        [word, id] if word == "stop" => stop(id),
        [word, rest @ ..] if word == "import" => crate::session_import::command(rest),
        [word, rest @ ..] if word == "search" => crate::session_search::command(rest),
        [word, rest @ ..] if word == "list" => crate::sessions_list::command(rest),
        _ => fail(Ending::Argument, t!(sessions_usage)),
    }
}

/// The directory `sessions import`, `sessions search` and `sessions list` look in: the one the
/// person typed, or the current one. It has to exist and be a directory; a path that is not one is
/// an argument error.
pub(crate) fn project_directory(named: Option<PathBuf>) -> Result<PathBuf, ExitCode> {
    let directory = match named {
        Some(path) => path,
        None => match std::env::current_dir() {
            Ok(here) => here,
            Err(_) => return Err(fail(Ending::Failed, t!(cli_directory_unknown))),
        },
    };
    directory
        .canonicalize()
        .and_then(|resolved| {
            resolved
                .is_dir()
                .then_some(resolved)
                .ok_or(std::io::ErrorKind::NotADirectory.into())
        })
        .map_err(|_| {
            fail(
                Ending::Argument,
                t!(sessions_import_no_project, path = directory.display()),
            )
        })
}

/// Whether `bravebot sessions` opens the list rather than printing lines (BG-16).
///
/// Standard error is checked as well: an attach from the list needs it to say why it cannot start.
fn wants_the_view(
    args: &[String],
    input_is_terminal: bool,
    output_is_terminal: bool,
    error_is_terminal: bool,
) -> bool {
    args.is_empty() && input_is_terminal && output_is_terminal && error_is_terminal
}

/// The list grouped by state; Enter on a row attaches as `bravebot attach` does (BG-16).
fn view() -> ExitCode {
    let roster = Roster::readable();
    let read = || {
        roster
            .as_ref()
            .map(|roster| rows_of(&roster.list()))
            .unwrap_or_default()
    };
    let rows = read();
    if rows.is_empty() {
        return list(false);
    }
    match bravebot_tui::sessions_view::run(rows, read) {
        Ok(Choice::Attach(id)) => attach(&[id]),
        Ok(Choice::Leave) => ExitCode::SUCCESS,
        Err(problem) => {
            eprintln!("{}", t!(sessions_view_failed, problem = problem));
            list(false)
        }
    }
}

fn list(as_json: bool) -> ExitCode {
    let seen = Roster::readable()
        .map(|roster| roster.list())
        .unwrap_or_default();
    let mut out = std::io::stdout().lock();
    if as_json {
        let _ = writeln!(out, "{}", bravebot_session::jobs::as_json(&seen));
    } else if seen.is_empty() {
        let _ = writeln!(out, "{}", t!(sessions_none));
    } else {
        for line in lines_of(&seen) {
            let _ = writeln!(out, "{line}");
        }
    }
    ExitCode::SUCCESS
}

fn stop(typed: &str) -> ExitCode {
    let Some(roster) = Roster::writable() else {
        return fail(Ending::Failed, t!(sessions_no_home));
    };
    let seen = match roster.find(typed) {
        Ok(seen) => seen,
        Err(Lookup::Missing) => {
            return fail(Ending::Argument, t!(sessions_missing, id = shown(typed)));
        }
        Err(Lookup::Ambiguous) => {
            return fail(Ending::Argument, t!(sessions_ambiguous, id = shown(typed)));
        }
    };
    match roster.stop(&seen.job.id, GRACE) {
        Ok(Stopped::Stopped) => {
            println!("{}", t!(sessions_stopped, name = shown(&seen.job.name)));
            ExitCode::SUCCESS
        }
        Ok(Stopped::AlreadyNotRunning) => {
            println!("{}", t!(sessions_not_running, name = shown(&seen.job.name)));
            ExitCode::SUCCESS
        }
        Ok(Stopped::Missing) => fail(Ending::Argument, t!(sessions_missing, id = shown(typed))),
        Err(err) => fail(Ending::Failed, t!(sessions_stop_failed, problem = err)),
    }
}

/// `bravebot --bg <prompt>`: start a session that keeps running after this terminal closes.
///
/// Only a terminal can start one (BG-2). The prompt is handed to the new process through a file only
/// that process can take, so a process that was merely run has none to start with.
pub(crate) fn start(words: &[String]) -> ExitCode {
    let prompt = words
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if prompt.is_empty() {
        return fail(Ending::Argument, t!(bg_needs_a_prompt));
    }
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return fail(Ending::Argument, t!(bg_needs_a_terminal));
    }
    let Some(roster) = Roster::writable() else {
        return fail(Ending::Failed, t!(sessions_no_home));
    };
    launch(&roster, &prompt)
}

fn launch(roster: &Roster, prompt: &str) -> ExitCode {
    let id = bravebot_session::jobs::new_id();
    match start_host(roster, &id, prompt, None, Mode::Ask) {
        Ok(()) => {
            let shown: String = id.chars().take(ID_SHOWN).collect();
            println!("{}", t!(bg_started, id = shown));
            ExitCode::SUCCESS
        }
        Err(code) => code,
    }
}

/// Start the process for `id` and wait until it is live and listening.
///
/// `first` is the line the process starts with, left where only it can take it (BG-2); an empty
/// one starts the session idle. A session started again runs in the directory it worked in, since
/// its record is found from there. The process runs in `mode` for as long as it runs (BG-8).
#[cfg(unix)]
fn start_host(
    roster: &Roster,
    id: &str,
    first: &str,
    directory: Option<&std::path::Path>,
    mode: Mode,
) -> Result<(), ExitCode> {
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    if let Err(err) = roster.leave_first_prompt(id, first) {
        return Err(fail(Ending::Failed, t!(bg_spawn_failed, problem = err)));
    }
    // The socket of the process that ended, which would otherwise be taken for the new one's. A
    // live process's socket is its own, and another terminal may have started it a moment ago.
    if !roster.get(id).is_some_and(|seen| seen.live)
        && let Some(socket) = roster.socket_of(id)
    {
        let _ = std::fs::remove_file(socket);
    }
    // The host is this same program started again; nothing is trusted from where it says it is.
    // nosemgrep: rust.lang.security.current-exe.current-exe
    let Ok(me) = std::env::current_exe() else {
        let _ = roster.take_first_prompt(id);
        return Err(fail(Ending::Failed, t!(bg_not_started)));
    };
    let mut command = host_command(&me, id, directory, mode);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Err(err) = command.spawn() {
        let _ = roster.take_first_prompt(id);
        return Err(fail(Ending::Failed, t!(bg_spawn_failed, problem = err)));
    }
    let until = Instant::now() + Duration::from_secs(10);
    while Instant::now() < until {
        if roster.get(id).is_some_and(|seen| seen.live)
            && roster.socket_of(id).is_some_and(|s| s.exists())
        {
            ensure_supervisor(roster, &me);
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = roster.take_first_prompt(id);
    Err(fail(Ending::Failed, t!(bg_not_started)))
}

/// Make sure a supervisor is running, so the process just started is started again if it ends
/// between turns (BG-11, BG-12).
///
/// Done once the process is alive, not before. A supervisor that is about to end lets go of its lock
/// and then looks at the roster once more, so a process alive by the time this looks at the lock is
/// one it finds; a lock this finds free belongs to no supervisor and a new one is started. That
/// order is what leaves no moment in which a live process has neither.
///
/// The session is running whatever becomes of this, so it is not undone when the supervisor cannot
/// be started.
#[cfg(unix)]
fn ensure_supervisor(roster: &Roster, program: &std::path::Path) {
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    if roster.has_supervisor() {
        return;
    }
    let mut command = supervisor_command(program);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let Ok(mut supervisor) = command.spawn() else {
        return;
    };
    // Until it holds its lock, so that the next `--bg` finds it rather than starting another, or
    // until it has ended: it does when nothing is alive for it to watch.
    let until = Instant::now() + Duration::from_secs(5);
    while Instant::now() < until && !roster.has_supervisor() {
        if !matches!(supervisor.try_wait(), Ok(None)) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The command that runs the supervisor: the hidden `__bg-supervisor`, from a directory that no
/// terminal holds open.
#[cfg(unix)]
fn supervisor_command(program: &std::path::Path) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    command.arg("__bg-supervisor").current_dir("/");
    command
}

/// The command that runs the process for `id`: the hidden `__bg-host` with the mode word it runs in.
#[cfg(unix)]
pub(crate) fn host_command(
    program: &std::path::Path,
    id: &str,
    directory: Option<&std::path::Path>,
    mode: Mode,
) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    command.arg("__bg-host").arg(id).arg(mode.word());
    if let Some(directory) = directory {
        command.current_dir(directory);
    }
    command
}

#[cfg(not(unix))]
fn start_host(
    _roster: &Roster,
    _id: &str,
    _first: &str,
    _directory: Option<&std::path::Path>,
    _mode: Mode,
) -> Result<(), ExitCode> {
    Err(fail(Ending::Failed, t!(bg_unsupported)))
}

/// The hidden command a started session runs under, with the mode word it was started with.
#[cfg(unix)]
pub(crate) fn host(id: &str, mode: Mode) -> ExitCode {
    crate::host::host(id, mode)
}

#[cfg(not(unix))]
pub(crate) fn host(_id: &str, _mode: Mode) -> ExitCode {
    ExitCode::FAILURE
}

/// The hidden command the supervisor runs under (BG-11).
#[cfg(unix)]
pub(crate) fn supervise() -> ExitCode {
    crate::supervisor::supervise()
}

#[cfg(not(unix))]
pub(crate) fn supervise() -> ExitCode {
    ExitCode::FAILURE
}

/// `/bg`: hand the foreground session `left` to a background process, which resumes its record in
/// `mode` and opens idle. Said on the terminal the foreground session leaves (BG-2).
///
/// Called after the interface has handed the terminal back, from the process the person ran, so
/// the start of the session is still a thing a terminal did. The failure is the exit status to end
/// with, and the record is still there to resume.
pub(crate) fn hand_over(
    left: &bravebot_session::sessions::Resumable,
    mode: Mode,
) -> Result<(), ExitCode> {
    let Some(roster) = Roster::writable() else {
        return Err(fail(Ending::Failed, t!(sessions_no_home)));
    };
    start_host(&roster, &left.id, "", Some(&left.directory), mode)?;
    let shown: String = left.id.chars().take(ID_SHOWN).collect();
    println!("{}", t!(bg_handed_over, id = shown));
    Ok(())
}

/// Whether a background session is running under `id`, the id of a session record (BG-9).
pub(crate) fn is_running(id: &str) -> bool {
    bravebot_session::jobs::is_running(id)
}

/// The refusal for `--resume` and `--continue` naming a record a running background session holds,
/// which says how to join it instead.
pub(crate) fn refuse_to_resume(id: &str) -> ExitCode {
    fail(Ending::Failed, held_by_a_background_session(id))
}

/// Why a session a background process is running cannot be resumed by a second one.
pub(crate) fn held_by_a_background_session(id: &str) -> String {
    let name = Roster::readable()
        .and_then(|roster| roster.get(id))
        .map(|seen| shown(&seen.job.name))
        .unwrap_or_default();
    let id: String = id.chars().take(ID_SHOWN).collect();
    t!(resume_held_by_background, name = name, id = id).to_string()
}

/// What a terminal does for a session that is not running.
#[derive(Clone, Copy)]
enum Wake<'a> {
    /// Join it: it starts idle.
    Attach,
    /// Send it a prompt: it starts with that prompt.
    Reply(&'a str),
}

/// The session `typed` names, running, or the complaint to give.
///
/// One that is `stopped` or `interrupted` is started again first, in the mode every session opens
/// in (BG-9, BG-10), and only from a terminal: the start of a session is a thing a person did
/// (BG-2). For `interrupted` the terminal says first that the turn it was in is not repeated, and
/// the new process tells the planner that turn ended (BG-12). The prompt a reply carries is the
/// line the new process starts with, so it is not sent again, and the third value is true.
fn running(typed: &str, wake: Wake<'_>) -> Result<(Roster, Seen, bool), ExitCode> {
    let Some(roster) = Roster::writable() else {
        return Err(fail(Ending::Failed, t!(sessions_no_home)));
    };
    match roster.find(typed) {
        Ok(seen) if seen.live => Ok((roster, seen, false)),
        Ok(seen) if matches!(seen.state(), State::Stopped | State::Interrupted) => {
            let name = shown(&seen.job.name);
            let interrupted = seen.state() == State::Interrupted;
            if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
                return Err(fail(
                    Ending::Argument,
                    if interrupted {
                        t!(bg_interrupted_needs_a_terminal, name = name)
                    } else {
                        t!(bg_restart_needs_a_terminal, name = name)
                    },
                ));
            }
            if interrupted {
                eprintln!("{}", t!(bg_interrupted_not_repeated, name = name));
            }
            let first = match wake {
                Wake::Attach => String::new(),
                Wake::Reply(text) => one_line(text),
            };
            start_host(
                &roster,
                &seen.job.id,
                &first,
                Some(std::path::Path::new(&seen.job.directory)),
                Mode::Ask,
            )?;
            match roster.get(&seen.job.id) {
                Some(started) => Ok((roster, started, true)),
                None => Err(fail(Ending::Failed, t!(bg_not_started))),
            }
        }
        Ok(seen) => Err(fail(
            Ending::Failed,
            t!(attach_not_running, name = shown(&seen.job.name)),
        )),
        Err(Lookup::Missing) => Err(fail(
            Ending::Argument,
            t!(sessions_missing, id = shown(typed)),
        )),
        Err(Lookup::Ambiguous) => Err(fail(
            Ending::Argument,
            t!(sessions_ambiguous, id = shown(typed)),
        )),
    }
}

/// `text` as the one line the socket reads: a break in it would end the prompt early.
fn one_line(text: &str) -> String {
    text.chars()
        .map(|c| if matches!(c, '\n' | '\r') { ' ' } else { c })
        .collect()
}

/// `bravebot attach <id>`: the terminal of a background session, for as long as this one stays.
///
/// What the session wrote is drawn the way a listing is: control characters pictured, so the
/// session cannot move the cursor or recolour the screen of the terminal it is read in.
pub(crate) fn attach(args: &[String]) -> ExitCode {
    let [typed] = args else {
        return fail(Ending::Argument, t!(attach_usage));
    };
    let (roster, seen, _) = match running(typed, Wake::Attach) {
        Ok(found) => found,
        Err(code) => return code,
    };
    // Lines on standard input are offered to the session as the person's answers, so a pipe or a
    // file there would be a script answering for them.
    if !std::io::stdin().is_terminal() {
        return fail(Ending::Argument, t!(attach_needs_a_terminal));
    }
    talk(&roster, &seen)
}

#[cfg(unix)]
fn talk(roster: &Roster, seen: &Seen) -> ExitCode {
    use std::io::{BufRead, BufReader, Read};

    let name = shown(&seen.job.name);
    let Some(mut stream) = roster
        .socket_of(&seen.job.id)
        .and_then(|socket| crate::host::reach(&socket).ok())
    else {
        return fail(
            Ending::Failed,
            t!(
                attach_unreachable,
                name = name,
                problem = t!(attach_not_running, name = shown(&seen.job.name))
            ),
        );
    };
    let Some(token) = roster.attach_token(&seen.job.id) else {
        return fail(Ending::Failed, t!(attach_not_running, name = name));
    };
    if writeln!(stream, "{token}\nattach").is_err() {
        return fail(Ending::Failed, t!(attach_not_running, name = name));
    }
    let Ok(listening) = stream.try_clone() else {
        return fail(Ending::Failed, t!(attach_not_running, name = name));
    };
    let mut from = BufReader::new(listening);
    match crate::host::bounded_line(&mut from).as_deref() {
        Some("ok") => {}
        Some("attached") => return fail(Ending::Failed, t!(attach_taken, name = name)),
        Some("stopping") => return fail(Ending::Failed, t!(attach_stopping, name = name)),
        _ => return fail(Ending::Failed, t!(attach_not_running, name = name)),
    }
    eprintln!("{}", t!(attach_joined, name = name.clone()));

    let sending = stream;
    let detached = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let typed_detach = std::sync::Arc::clone(&detached);
    std::thread::spawn(move || {
        let mut sending = sending;
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if leaves(&line) {
                typed_detach.store(true, std::sync::atomic::Ordering::SeqCst);
                break;
            }
            if writeln!(sending, "{line}").is_err() {
                return;
            }
        }
        let _ = sending.shutdown(std::net::Shutdown::Both);
    });

    let mut out = std::io::stdout().lock();
    let mut pending: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match from.read(&mut chunk) {
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
            Ok(0) | Err(_) => break,
            Ok(read) => {
                pending.extend_from_slice(&chunk[..read]);
                let drawn = drawable(&mut pending);
                if out
                    .write_all(drawn.as_bytes())
                    .and_then(|()| out.flush())
                    .is_err()
                {
                    break;
                }
            }
        }
    }
    if detached.load(std::sync::atomic::Ordering::SeqCst) {
        eprintln!("{}", t!(attach_detached, name = name));
    } else {
        eprintln!("{}", t!(attach_left));
    }
    ExitCode::SUCCESS
}

/// Whether a line typed in an attached terminal leaves the session running and goes no further
/// than this process (BG-9).
#[cfg_attr(not(unix), allow(dead_code))]
fn leaves(line: &str) -> bool {
    line.trim() == "/detach"
}

#[cfg(not(unix))]
fn talk(_roster: &Roster, _seen: &Seen) -> ExitCode {
    fail(Ending::Failed, t!(bg_unsupported))
}

/// The text in `pending` that is whole, drawn with control characters pictured except the line
/// break, which stays one. An incomplete character at the end is kept for the next read.
#[cfg_attr(not(unix), allow(dead_code))]
fn drawable(pending: &mut Vec<u8>) -> String {
    let whole = match std::str::from_utf8(pending) {
        Ok(text) => text.len(),
        Err(err) if err.error_len().is_none() => err.valid_up_to(),
        // Bytes that are no character: the replacement for each, so nothing is kept waiting.
        Err(_) => pending.len(),
    };
    let text = String::from_utf8_lossy(&pending[..whole]).into_owned();
    pending.drain(..whole);
    text.split('\n').map(shown).collect::<Vec<_>>().join("\n")
}

/// `bravebot reply <id> <prompt>`: send a prompt to a session that is idle, and only then (BG-10).
pub(crate) fn reply(args: &[String]) -> ExitCode {
    let [typed, words @ ..] = args else {
        return fail(Ending::Argument, t!(reply_usage));
    };
    let text = words.join(" ");
    if text.trim().is_empty() {
        return fail(Ending::Argument, t!(reply_usage));
    }
    let (roster, seen, started_with_it) = match running(typed, Wake::Reply(text.trim())) {
        Ok(found) => found,
        Err(code) => return code,
    };
    if started_with_it {
        println!("{}", t!(reply_sent, name = shown(&seen.job.name)));
        return ExitCode::SUCCESS;
    }
    send(&roster, &seen, text.trim())
}

#[cfg(unix)]
fn send(roster: &Roster, seen: &Seen, text: &str) -> ExitCode {
    use std::io::BufReader;

    let name = shown(&seen.job.name);
    let Some(mut stream) = roster
        .socket_of(&seen.job.id)
        .and_then(|socket| crate::host::reach(&socket).ok())
    else {
        return fail(Ending::Failed, t!(attach_not_running, name = name));
    };
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(10)));
    let Some(token) = roster.attach_token(&seen.job.id) else {
        return fail(Ending::Failed, t!(attach_not_running, name = name));
    };
    // One line, as the socket reads it: a break in the text would end the reply early.
    let text = one_line(text);
    if writeln!(stream, "{token}\nreply\n{text}").is_err() {
        return fail(Ending::Failed, t!(reply_not_sent, name = name));
    }
    let answer = crate::host::bounded_line(&mut BufReader::new(stream));
    match answer.as_deref() {
        Some("ok") => {
            println!("{}", t!(reply_sent, name = name));
            ExitCode::SUCCESS
        }
        Some("working") => fail(Ending::Failed, t!(reply_working, name = name)),
        Some("stopping") => fail(Ending::Failed, t!(reply_stopping, name = name)),
        Some("needs-input") => {
            let id: String = seen.job.id.chars().take(ID_SHOWN).collect();
            fail(Ending::Failed, t!(reply_needs_input, name = name, id = id))
        }
        _ => fail(Ending::Failed, t!(reply_not_sent, name = name)),
    }
}

#[cfg(not(unix))]
fn send(_roster: &Roster, _seen: &Seen, _text: &str) -> ExitCode {
    fail(Ending::Failed, t!(bg_unsupported))
}

/// Text the person typed, as it can be drawn: control characters pictured, and a tab a space, so
/// nothing typed can move the cursor, recolour the line or start another one.
fn shown(typed: &str) -> String {
    crate::progress::printable(typed).replace('\t', " ")
}

/// One row for each session: the start of its id, its name, its state, where it works, how long
/// since its last turn, and the last prompt the person typed.
///
/// A prompt that is held shows as needing input and the one word for its kind, and nothing of
/// what it asks (BG-5). Both the lines and the list (BG-16) are drawn from these.
fn rows_of(seen: &[Seen]) -> Vec<Row> {
    seen.iter()
        .map(|seen| {
            let job = &seen.job;
            Row {
                id: job.id.clone(),
                short: job.id.chars().take(ID_SHOWN).collect(),
                name: shown(&job.name),
                state: seen.state(),
                state_words: state_in_words(seen),
                since: bravebot_session::sessions::how_long_ago(
                    job.last_turn.unwrap_or(job.started),
                ),
                directory: shown(&job.directory),
                prompt: shown(&bravebot_session::sessions::title_from(&job.prompt)),
            }
        })
        .collect()
}

fn lines_of(seen: &[Seen]) -> Vec<String> {
    rows_of(seen)
        .iter()
        .map(|row| {
            format!(
                "{}  {}  {}  {}  {}  {}",
                row.short, row.name, row.state_words, row.directory, row.since, row.prompt
            )
        })
        .collect()
}

fn state_in_words(seen: &Seen) -> String {
    match (seen.state(), seen.held()) {
        (State::Working, _) => t!(sessions_state_working).to_string(),
        (State::NeedsInput, Some(held)) => t!(
            sessions_state_needs_input,
            kind = match held {
                Held::Write => t!(sessions_held_write),
                Held::Run => t!(sessions_held_run),
                Held::Read => t!(sessions_held_read),
                Held::Fetch => t!(sessions_held_fetch),
                Held::Server => t!(sessions_held_server),
                Held::Vouch => t!(sessions_held_vouch),
                Held::Tools => t!(sessions_held_tools),
                Held::Move => t!(sessions_held_move),
                Held::Manifest => t!(sessions_held_manifest),
                Held::Question => t!(sessions_held_question),
            }
        )
        .to_string(),
        (State::NeedsInput, None) => t!(sessions_state_needs_input_unnamed).to_string(),
        (State::Idle, _) => t!(sessions_state_idle).to_string(),
        (State::Stopped, _) => t!(sessions_state_stopped).to_string(),
        (State::Interrupted, _) => t!(sessions_state_interrupted).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// BG-8: the process is started with the word for the mode it runs in, in the directory the
    /// session worked in, and a start that names no mode asks.
    #[cfg(unix)]
    #[test]
    fn the_host_is_started_with_the_mode_it_runs_in() {
        let command = host_command(
            std::path::Path::new("/bin/bravebot"),
            "3f2a9c1e",
            Some(std::path::Path::new("/work")),
            Mode::AcceptEdits,
        );
        let arguments: Vec<_> = command.get_args().collect();
        assert_eq!(arguments, ["__bg-host", "3f2a9c1e", "accept-edits"]);
        assert_eq!(
            command.get_current_dir(),
            Some(std::path::Path::new("/work"))
        );
    }
    /// BG-11: the supervisor is started as the hidden command, away from any directory a terminal
    /// is in, so closing the terminal that started it does not hold a directory open.
    #[cfg(unix)]
    #[test]
    fn the_supervisor_is_started_as_the_hidden_command_from_the_root() {
        let command = supervisor_command(std::path::Path::new("/bin/bravebot"));
        let arguments: Vec<_> = command.get_args().collect();
        assert_eq!(arguments, ["__bg-supervisor"]);
        assert_eq!(command.get_current_dir(), Some(std::path::Path::new("/")));
    }
    use bravebot_session::jobs::{Job, Mode};
    use std::path::Path;

    /// BG-9: `/detach` leaves, with space around it too, and nothing that only contains it does,
    /// so a prompt that mentions the word is still sent to the session.
    #[test]
    fn only_a_line_of_detach_leaves() {
        assert!(leaves("/detach"));
        assert!(leaves("  /detach \t"));
        assert!(!leaves("/detach now"));
        assert!(!leaves("please /detach"));
        assert!(!leaves("detach"));
        assert!(!leaves(""));
    }

    fn seen(prompt: &str, state: State, held: Option<Held>, live: bool) -> Seen {
        let mut job = Job::starting(
            "11111111-1111-4111-8111-111111111111".to_string(),
            Path::new("/work/project"),
            prompt,
            Mode::Ask,
        );
        job.is(state, held);
        Seen { job, live }
    }

    /// BG-5: a name, a prompt or a directory with a control character in it cannot start another
    /// line, move the cursor or recolour the screen.
    #[test]
    fn a_control_character_in_what_was_typed_cannot_write_a_row() {
        let mut one = seen("fix\x1b[31m the\nbuild\r\x07", State::Idle, None, true);
        one.job.directory = "/work/\x1b]0;title\x07/pro\nject".to_string();
        let rows = lines_of(&[one]);
        assert_eq!(rows.len(), 1, "{rows:?}");
        for row in &rows {
            assert!(!row.contains('\n'), "{row:?}");
            assert!(!row.chars().any(|c| c.is_control()), "{row:?}");
        }
        assert!(rows[0].contains('\u{241b}'), "{rows:?}");
    }

    /// BG-5: a held prompt shows as needing input and the word for its kind.
    #[test]
    fn a_held_prompt_shows_its_kind_and_nothing_else() {
        let rows = lines_of(&[seen(
            "fix the build",
            State::NeedsInput,
            Some(Held::Run),
            true,
        )]);
        assert!(
            rows[0].contains(&t!(sessions_held_run).to_string()),
            "{rows:?}"
        );
        assert!(rows[0].contains("fix the build"), "{rows:?}");
    }

    /// BG-6: the list says what is true of the process now. An entry that said it was working and
    /// has no process is listed as interrupted, and its kind is not listed at all.
    #[test]
    fn a_session_with_no_process_is_not_listed_as_working() {
        let working = t!(sessions_state_working).to_string();
        let held = t!(sessions_held_run).to_string();
        let gone = seen("fix the build", State::Working, None, false);
        let row = &lines_of(&[gone])[0];
        assert!(!row.contains(&working), "{row}");
        assert!(
            row.contains(&t!(sessions_state_interrupted).to_string()),
            "{row}"
        );

        let gone = seen("fix the build", State::NeedsInput, Some(Held::Run), false);
        let row = &lines_of(&[gone])[0];
        assert!(!row.contains(&held), "{row}");
    }

    /// BG-9: what a session wrote reaches the terminal with its control characters pictured and its
    /// line breaks kept, and a character split across two reads is drawn whole.
    #[test]
    fn what_a_session_wrote_is_drawn_without_its_control_characters() {
        let mut pending = b"red \x1b[31mtext\x07\nnext".to_vec();
        let drawn = drawable(&mut pending);
        assert_eq!(drawn, "red \u{241b}[31mtext\u{2407}\nnext");
        assert!(pending.is_empty());

        let mut pending = "caf\u{e9}".as_bytes()[..4].to_vec();
        assert_eq!(drawable(&mut pending), "caf");
        pending.extend_from_slice(&"caf\u{e9}".as_bytes()[4..]);
        assert_eq!(drawable(&mut pending), "\u{e9}");
    }

    /// BG-16: `bravebot sessions` opens the list only for no other word, with standard input,
    /// output and error all terminals, so a pipe or a redirect, `--json` or `stop` is not taken over.
    #[test]
    fn the_list_opens_only_for_no_other_word_on_three_terminals() {
        let words = |words: &[&str]| words.iter().map(|w| w.to_string()).collect::<Vec<_>>();
        assert!(wants_the_view(&words(&[]), true, true, true));
        assert!(!wants_the_view(&words(&[]), false, true, true));
        assert!(!wants_the_view(&words(&[]), true, false, true));
        assert!(!wants_the_view(&words(&[]), true, true, false));
        assert!(!wants_the_view(&words(&["--json"]), true, true, true));
        assert!(!wants_the_view(
            &words(&["stop", "3f2a9c1e"]),
            true,
            true,
            true
        ));
        assert!(!wants_the_view(
            &words(&["search", "ledger"]),
            true,
            true,
            true
        ));
    }

    /// BG-16, BG-6: a session is grouped by the state of its process now. An entry that said it
    /// was working or holding a prompt and has no process is in the interrupted group, and an idle
    /// one with no process is in the stopped group, so the list never offers a dead session under
    /// "working" or "needs input".
    #[test]
    fn a_session_is_grouped_by_the_state_of_its_process_and_not_the_one_it_last_wrote() {
        let state_of =
            |written, held, live| rows_of(&[seen("fix the build", written, held, live)])[0].state;
        assert_eq!(state_of(State::Working, None, false), State::Interrupted);
        assert_eq!(
            state_of(State::NeedsInput, Some(Held::Run), false),
            State::Interrupted
        );
        assert_eq!(state_of(State::Idle, None, false), State::Stopped);
        assert_eq!(state_of(State::Working, None, true), State::Working);
        assert_eq!(
            state_of(State::NeedsInput, Some(Held::Run), true),
            State::NeedsInput
        );
        assert_eq!(state_of(State::Idle, None, true), State::Idle);
    }

    /// BG-16: a row holds the whole id for `attach`, the start of it for the screen, and the kind of
    /// a held prompt as a word and nothing of what it asks.
    #[test]
    fn a_row_holds_the_whole_id_and_the_start_of_it_and_names_the_kind_of_a_held_prompt() {
        let row = &rows_of(&[seen(
            "fix the build",
            State::NeedsInput,
            Some(Held::Fetch),
            true,
        )])[0];
        assert_eq!(row.id, "11111111-1111-4111-8111-111111111111");
        assert_eq!(row.short, "11111111");
        assert!(
            row.state_words
                .contains(&t!(sessions_held_fetch).to_string()),
            "{row:?}"
        );
    }

    /// BG-5, BG-16: no field of a row the list draws holds a control character, whichever field
    /// the person's typing reached.
    #[test]
    fn a_control_character_in_what_was_typed_cannot_reach_any_field_of_a_row() {
        let mut one = seen("fix\x1b[31m the\nbuild\r\x07", State::Idle, None, true);
        one.job.name = "na\x1b]0;me\x07\tx".to_string();
        one.job.directory = "/work/\x1b[2J/pro\nject".to_string();
        let row = &rows_of(&[one])[0];
        for field in [
            &row.name,
            &row.directory,
            &row.prompt,
            &row.state_words,
            &row.since,
            &row.short,
        ] {
            assert!(!field.chars().any(|c| c.is_control()), "{field:?}");
        }
        assert!(
            row.name.contains('\u{241b}') && row.directory.contains('\u{241b}'),
            "{row:?}"
        );
    }
}
