//! `bravebot schedule add|list|remove|pause|resume|pending|clear|run-now|history`: recurring
//! one-shot runs started by the operating system's scheduler: `crontab`, a launchd job or a
//! systemd timer (CLI-42, SCHEDULE-1 to SCHEDULE-6).
//!
//! An entry is written by a person at a terminal and holds a cron expression, a prompt, a directory,
//! the permission mode the person typed and, if the person named one, the program a run's result is
//! to be delivered to. It is kept as two files under the state directory, the
//! prompt alone in one so that no character in it needs escaping, and installed as one line of the
//! person's crontab (or a launchd job or systemd timer) that runs `bravebot schedule run-now <id>`. That command starts this program's
//! own one-shot `-p` path in the entry's directory. Nothing a run reads is ever written to an entry,
//! and the driver reads the child's result only for the id of the session it recorded.
//!
//! Nothing here prints or asks the terminal anything. A subcommand returns the lines it would say
//! and how it ended ([`Reply`]), and takes as arguments what the caller knows and this crate must
//! not look up: the proof that a person typed the line ([`TypedLine`]), the mode the line carried
//! and the directory the person is working in ([`Context`]). The terminal prints the lines and maps
//! the ending to a status; another front end says them its own way.

use crate::{calendar, firing};
use bravebot_core::ask::Asking;
use bravebot_i18n::t;
use bravebot_session::{search, sessions};
use firing::FIRING_VARIABLE;
use std::fmt::Display;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// A result that is not read: the file or the stream was best-effort, and a failure to write it
/// is not a reason to refuse what the person asked for. The terminal's printing uses it too.
#[expect(
    clippy::let_underscore_must_use,
    clippy::needless_pass_by_value,
    reason = "the one place a best-effort write or removal is dropped"
)]
pub fn ignored<T, E>(result: Result<T, E>) {
    let _ = result;
}

/// How a subcommand ended. The terminal maps it to an exit status and a failure identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The subcommand did what it was asked.
    Done,
    /// An argument was refused, so nothing changed.
    Argument,
    /// It failed for a reason that is not the arguments.
    Failed,
    /// A firing was declined on purpose, as the firing of a paused entry is.
    Refused,
}

/// What a subcommand says and how it ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub outcome: Outcome,
    /// The lines for standard output, in order, without line ends.
    pub out: Vec<String>,
    /// The sentence for standard error where the subcommand did not succeed. A run that ended with
    /// a status other than zero has said what it had to in `out` and carries none.
    pub err: Option<String>,
    /// A sentence for standard error that changes neither the outcome nor the lines: a delivery
    /// that did not complete (SCHEDULE-7).
    pub notice: Option<String>,
}

impl Reply {
    fn done(out: Vec<String>) -> Self {
        Self {
            outcome: Outcome::Done,
            out,
            err: None,
            notice: None,
        }
    }

    fn said(outcome: Outcome, message: impl Display) -> Self {
        Self {
            outcome,
            out: Vec::new(),
            err: Some(message.to_string()),
            notice: None,
        }
    }
}

/// A line of output from anything that displays as one.
fn line(text: impl Display) -> String {
    text.to_string()
}

/// The proof that a person typed the line: only a line a person wrote at a terminal, or typed in a
/// session, adds an entry (CLI-42). A run's tool has no terminal and nothing a turn writes is a
/// typed line, so no turn can hold one.
#[derive(Debug, Clone, Copy)]
pub struct TypedLine(());

impl TypedLine {
    /// For a caller that has checked that a person typed the line it is running.
    pub fn of_a_person() -> Self {
        Self(())
    }
}

/// What a caller knows about the line it is running, and this crate does not look up.
pub struct Context {
    /// The proof that a person typed the line, where one did.
    pub typed: Option<TypedLine>,
    /// The mode a new entry holds, or `None` where the flags the line carried cannot be one.
    pub mode: Option<Mode>,
    /// Whether the line carried a permission flag, which only `add` takes.
    pub flags_typed: bool,
    /// The directory the person is working in, where a new entry runs unless `--dir` names
    /// another, or `None` where it cannot be found.
    pub project: Option<PathBuf>,
}

/// The longest prompt printed in a listing, in characters.
const LONGEST_PROMPT: usize = 80;

/// The longest prompt an entry may hold, in bytes.
const LONGEST_PROMPT_BYTES: usize = 64 * 1024;

/// `--accept-edits`, the flag a stored `accept-edits` entry starts its run with. The command line
/// spells the same flag in `crates/cli/src/flags.rs`, and a test there holds the two equal.
pub const ACCEPT_EDITS_FLAG: &str = "--accept-edits";

/// `--dangerously-skip-permissions`, the flag a stored `bypass` entry starts its run with. Held
/// equal to the command line's spelling by the same test.
pub const SKIP_PERMISSIONS_FLAG: &str = "--dangerously-skip-permissions";

/// What a run of this entry may do, as the person typed it when writing the entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Reads and reports. Every write is refused, because nobody can be asked (CLI-1).
    Read,
    /// `--accept-edits`: a write is answered yes and nothing else is.
    AcceptEdits,
    /// `--dangerously-skip-permissions`: every question is answered yes.
    Bypass,
}

impl Mode {
    fn word(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::AcceptEdits => "accept-edits",
            Self::Bypass => "bypass",
        }
    }

    fn named(word: &str) -> Option<Self> {
        [Self::Read, Self::AcceptEdits, Self::Bypass]
            .into_iter()
            .find(|mode| mode.word() == word)
    }

    /// The flag the one-shot run is started with, if any.
    pub fn flag(self) -> Option<&'static str> {
        match self {
            Self::Read => None,
            Self::AcceptEdits => Some(ACCEPT_EDITS_FLAG),
            Self::Bypass => Some(SKIP_PERMISSIONS_FLAG),
        }
    }

    /// What `list` says the run can do.
    fn limit(self) -> &'static str {
        match self {
            Self::Read => t!(schedule_limit_read),
            Self::AcceptEdits => t!(schedule_limit_accept_edits),
            Self::Bypass => t!(schedule_limit_bypass),
        }
    }
}

/// What starts an entry's runs, as the person chose when writing the entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scheduler {
    /// One line of the person's crontab.
    Crontab,
    /// A job in `~/Library/LaunchAgents`, loaded with `launchctl`.
    Launchd,
    /// A timer and a service in the person's systemd user directory.
    Systemd,
}

impl Scheduler {
    fn word(self) -> &'static str {
        match self {
            Self::Crontab => "crontab",
            Self::Launchd => "launchd",
            Self::Systemd => "systemd",
        }
    }

    fn named(word: &str) -> Option<Self> {
        [Self::Crontab, Self::Launchd, Self::Systemd]
            .into_iter()
            .find(|scheduler| scheduler.word() == word)
    }

    /// The one a platform has when none is named: launchd on macOS, the crontab elsewhere.
    fn usual() -> Self {
        if cfg!(target_os = "macos") {
            Self::Launchd
        } else {
            Self::Crontab
        }
    }
}

/// One entry, as read back from the state directory.
#[derive(Debug, PartialEq, Eq)]
struct Entry {
    id: String,
    cron: String,
    directory: PathBuf,
    mode: Mode,
    scheduler: Scheduler,
    /// The absolute path of the program the person typed at `add` to receive a run's result, or
    /// `None` for an entry that delivers nothing. Only `add` writes it.
    destination: Option<PathBuf>,
    prompt: String,
}

/// What `schedule add` was given, once the words are read.
#[derive(Debug, PartialEq, Eq)]
struct Addition {
    cron: String,
    prompt: String,
    directory: Option<PathBuf>,
    scheduler: Option<Scheduler>,
    destination: Option<PathBuf>,
    mode: Mode,
}

/// `bravebot schedule ...`, after `schedule`.
pub fn command(args: &[String], context: &Context) -> Reply {
    match args {
        [word, rest @ ..] if word == "add" => add(rest, context),
        _ if context.flags_typed => Reply::said(Outcome::Argument, t!(schedule_usage)),
        [word] if word == "list" => list(),
        [word, id] if word == "remove" => remove(id),
        [word, id] if word == "pause" => pause(id, true),
        [word, id] if word == "resume" => pause(id, false),
        [word] if word == "pending" => pending(None),
        [word, id] if word == "pending" => pending(Some(id)),
        [word, id] if word == "clear" => clear(id),
        [word, id] if word == "run-now" => run_now(id),
        [word] if word == "history" => history(None),
        [word, id] if word == "history" => history(Some(id)),
        _ => Reply::said(Outcome::Argument, t!(schedule_usage)),
    }
}

/// The word of `add` that names the program a run's result is delivered to. Only `add` takes it,
/// and it is never a flag a run is started with (SCHEDULE-6).
pub const DELIVER_TO_WORD: &str = "--deliver-to";

/// `--cron <expression> --prompt <text> [--dir <path>] [--scheduler <name>] [--deliver-to <path>]`,
/// in any order. A repeated flag, a missing value and any other word are refused rather than
/// guessed at.
fn read_addition(words: &[String], mode: Mode) -> Option<Addition> {
    let mut cron = None;
    let mut prompt = None;
    let mut directory = None;
    let mut scheduler = None;
    let mut destination = None;
    let mut rest = words.iter();
    while let Some(word) = rest.next() {
        match word.as_str() {
            "--cron" if cron.is_none() => cron = Some(rest.next()?.clone()),
            "--prompt" if prompt.is_none() => prompt = Some(rest.next()?.clone()),
            "--dir" if directory.is_none() => directory = Some(PathBuf::from(rest.next()?)),
            "--scheduler" if scheduler.is_none() => {
                scheduler = Some(Scheduler::named(rest.next()?)?);
            }
            DELIVER_TO_WORD if destination.is_none() => {
                destination = Some(PathBuf::from(rest.next()?));
            }
            _ => return None,
        }
    }
    Some(Addition {
        cron: cron?,
        prompt: prompt?,
        directory,
        scheduler,
        destination,
        mode,
    })
}

/// The lowest and highest value of each of the five cron fields, in order: minute, hour, day of
/// the month, month, day of the week (where 0 and 7 are both Sunday).
const FIELD_RANGES: [(u32, u32); 5] = [(0, 59), (0, 23), (1, 31), (1, 12), (0, 7)];

/// A cron number: one to four digits.
fn cron_number(text: &str) -> Option<u32> {
    if text.is_empty() || text.len() > 4 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// Whether `field` is a comma-separated list of `*`, a number or an `a-b` range inside
/// `low..=high`, each with an optional `/step` that is not zero (SCHEDULE-5).
fn field_in_range(field: &str, (low, high): (u32, u32)) -> bool {
    field.split(',').all(|part| {
        let (span, step) = match part.split_once('/') {
            Some((span, step)) => (span, Some(step)),
            None => (part, None),
        };
        if step.is_some_and(|step| cron_number(step).is_none_or(|n| n == 0)) {
            return false;
        }
        if span == "*" {
            return true;
        }
        match span.split_once('-') {
            Some((from, to)) => matches!(
                (cron_number(from), cron_number(to)),
                (Some(from), Some(to)) if low <= from && from <= to && to <= high
            ),
            None => cron_number(span).is_some_and(|n| low <= n && n <= high),
        }
    })
}

/// The expression with its fields separated by one space, or `None` where it is not five fields
/// of digits and `* / , -` with every number in the range its field allows and no step of zero.
///
/// Held to that alphabet because the expression is written into a crontab line: nothing else it
/// could hold would be an instruction to the shell, and a name such as `mon` or a `@daily` is
/// not read. Held to the ranges because a value outside them is one the scheduler would reject
/// or read in its own way after the entry was stored.
fn checked_cron(expression: &str) -> Option<String> {
    let fields: Vec<&str> = expression.split_whitespace().collect();
    if fields.len() != FIELD_RANGES.len() {
        return None;
    }
    fields
        .iter()
        .zip(FIELD_RANGES)
        .all(|(field, range)| field_in_range(field, range))
        .then(|| fields.join(" "))
}

/// The expression with its fields separated by one space, or `None` where it is not five fields
/// of digits and `* / , -`. The reading side of an entry: an entry stored before ranges were
/// checked still loads, and the alphabet still keeps anything but a cron field out of the
/// crontab line (COMPAT-001).
fn stored_cron(expression: &str) -> Option<String> {
    let fields: Vec<&str> = expression.split_whitespace().collect();
    let plain = |field: &&str| {
        !field.is_empty()
            && field
                .chars()
                .all(|c| c.is_ascii_digit() || matches!(c, '*' | '/' | ',' | '-'))
    };
    (fields.len() == FIELD_RANGES.len() && fields.iter().all(plain)).then(|| fields.join(" "))
}

/// A prompt an entry may hold: not empty, not so long that it is a file, and not one a one-shot
/// run would read as a flag.
fn usable_prompt(prompt: &str) -> bool {
    let trimmed = prompt.trim();
    !trimmed.is_empty() && !trimmed.starts_with('-') && prompt.len() <= LONGEST_PROMPT_BYTES
}

/// The directory entries are kept in, made if need be.
fn store() -> Option<PathBuf> {
    let directory = bravebot_agent::home::writable()?.join("schedules");
    bravebot_agent::home::create_directory(&directory).ok()?;
    Some(directory)
}

/// The directory entries are read from.
fn store_to_read() -> Option<PathBuf> {
    Some(bravebot_agent::home::directory()?.join("schedules"))
}

/// Whether `id` is a name this program wrote: digits only, so it is never a path.
fn is_an_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 9 && id.bytes().all(|b| b.is_ascii_digit())
}

/// Whether `path` can be stored as an entry's destination and read back as the same one: absolute,
/// valid UTF-8 and with no control character, so that it is one line of the entry file.
fn well_formed_destination(path: &Path) -> bool {
    path.is_absolute()
        && path
            .to_str()
            .is_some_and(|text| !text.chars().any(char::is_control))
}

/// Whether `path` is a destination `add` accepts: well formed, and a file that exists now. The
/// program is the person's own, typed at a terminal; nothing is looked up on a search path
/// because a scheduler's environment has almost none.
fn checked_destination(path: &Path) -> bool {
    well_formed_destination(path) && path.is_file()
}

/// The destination a stored value names, or `None` where the value is not a well formed one. The
/// reading side of an entry: a value nobody could have typed at `add` makes the entry
/// unreadable, and is never read as no destination. Existence is not asked again: a program that
/// has since gone is a failed delivery, and the entry still lists.
fn stored_destination(value: &str) -> Option<PathBuf> {
    let path = PathBuf::from(value);
    well_formed_destination(&path).then_some(path)
}

/// What an entry's file holds. The destination line is written only where there is a destination,
/// so an entry without one is the file it was before the key existed.
fn entry_text(
    cron: &str,
    directory: &Path,
    mode: Mode,
    scheduler: Scheduler,
    destination: Option<&Path>,
) -> Option<String> {
    let directory = directory.to_str()?;
    if directory.contains('\n') {
        return None;
    }
    let mut text = format!(
        "cron={cron}\ndir={directory}\nmode={}\nscheduler={}\n",
        mode.word(),
        scheduler.word()
    );
    if let Some(destination) = destination {
        if !well_formed_destination(destination) {
            return None;
        }
        text.push_str(&format!("deliver={}\n", destination.to_str()?));
    }
    Some(text)
}

/// An entry read back from `text` and the prompt file's contents.
fn read_entry(id: &str, text: &str, prompt: String) -> Option<Entry> {
    let (mut cron, mut directory, mut mode) = (None, None, None);
    // An entry written before there was a choice is a crontab line.
    let mut scheduler = Some(Scheduler::Crontab);
    // An entry written before there was a destination has none. Once a stored value is malformed
    // this stays `None`, so a later good line does not rescue the entry.
    let mut destination = Some(None);
    for line in text.lines() {
        let (key, value) = line.split_once('=')?;
        match key {
            "cron" => cron = stored_cron(value),
            "dir" => directory = Some(PathBuf::from(value)),
            "mode" => mode = Mode::named(value),
            "scheduler" => scheduler = Scheduler::named(value),
            "deliver" => destination = destination.and(stored_destination(value).map(Some)),
            _ => {}
        }
    }
    Some(Entry {
        id: id.to_string(),
        cron: cron?,
        directory: directory?,
        mode: mode?,
        scheduler: scheduler?,
        destination: destination?,
        prompt,
    })
}

/// Every entry, oldest first. A file that cannot be read as an entry is left out.
fn entries() -> Vec<Entry> {
    let Some(store) = store_to_read() else {
        return Vec::new();
    };
    let Ok(listing) = std::fs::read_dir(&store) else {
        return Vec::new();
    };
    let mut ids: Vec<u64> = listing
        .filter_map(|file| {
            file.ok()?
                .file_name()
                .to_str()?
                .strip_suffix(".entry")?
                .parse()
                .ok()
        })
        .collect();
    ids.sort_unstable();
    ids.into_iter()
        .filter_map(|id| entry_named(&store, &id.to_string()))
        .collect()
}

fn entry_named(store: &Path, id: &str) -> Option<Entry> {
    if !is_an_id(id) {
        return None;
    }
    let text = std::fs::read_to_string(store.join(format!("{id}.entry"))).ok()?;
    let prompt = std::fs::read_to_string(store.join(format!("{id}.prompt"))).ok()?;
    read_entry(id, &text, prompt)
}

/// The marker a crontab line carries so that the line can be found again.
fn marker(id: &str) -> String {
    format!("# bravebot-schedule:{id}")
}

/// `path` for a shell, in single quotes, and `%` for cron, which reads an unescaped one as a
/// newline.
fn in_a_crontab_line(path: &str) -> String {
    format!("'{}'", path.replace('\'', "'\\''")).replace('%', "\\%")
}

/// The crontab line that runs entry `id`. `None` for a program path that holds a newline.
fn crontab_line(cron: &str, program: &str, id: &str) -> Option<String> {
    if program.contains('\n') {
        return None;
    }
    Some(format!(
        "{cron} {} schedule run-now {id} >/dev/null 2>&1 {}",
        in_a_crontab_line(program),
        marker(id)
    ))
}

/// `table` without the line for `id`, and with `line` added when there is one.
fn with_line(table: &str, id: &str, line: Option<&str>) -> String {
    let marker = marker(id);
    let mut kept: Vec<&str> = table
        .lines()
        .filter(|existing| !existing.trim_end().ends_with(&marker))
        .collect();
    kept.extend(line);
    let mut text = kept.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    text
}

/// A scheduler's own program (`crontab`, `launchctl`, `systemctl`), started without this agent's
/// credentials.
#[cfg(unix)]
fn tool(name: &str) -> Option<std::process::Command> {
    let found = bravebot_sandbox::programs::find(std::ffi::OsStr::new(name))?;
    Some(bravebot_sandbox::programs::start(
        &bravebot_sandbox::programs::Resolved::found_earlier(&found),
        bravebot_sandbox::programs::Inherits::Without(&bravebot_config::scrub::own_credentials()),
        bravebot_sandbox::programs::SearchPath::Reduced,
    ))
}

/// The person's crontab as it is, or `None` where it could not be read. A person with no crontab
/// has an empty one: `crontab -l` ends with a failure status and prints nothing on stdout.
#[cfg(unix)]
fn read_table() -> Option<String> {
    let output = tool("crontab")?
        .arg("-l")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() && !output.stdout.is_empty() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// Replace the person's crontab with `table`.
#[cfg(unix)]
fn write_table(table: &str) -> bool {
    let Some(mut command) = tool("crontab") else {
        return false;
    };
    let Ok(mut child) = command
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        return false;
    };
    if let Some(mut input) = child.stdin.take()
        && input.write_all(table.as_bytes()).is_err()
    {
        ignored(child.wait());
        return false;
    }
    child.wait().is_ok_and(|status| status.success())
}

#[cfg(not(unix))]
fn read_table() -> Option<String> {
    None
}

#[cfg(not(unix))]
fn write_table(_table: &str) -> bool {
    false
}

/// Put `line` (or nothing) for `id` in the person's crontab.
fn install(id: &str, line: Option<&str>) -> bool {
    let Some(table) = read_table() else {
        return false;
    };
    write_table(&with_line(&table, id, line))
}

/// What a scheduler is given to start entry `id`, written out and ready to install.
enum Job {
    Line(String),
    Plist(String),
    Units { service: String, timer: String },
}

/// The job for `scheduler`, or the sentence that says why there is none.
fn job(scheduler: Scheduler, cron: &str, program: &str, id: &str) -> Result<Job, &'static str> {
    let times = || calendar::groups(cron).ok_or(t!(schedule_cron_invalid));
    match scheduler {
        Scheduler::Crontab => crontab_line(cron, program, id)
            .map(Job::Line)
            .ok_or(t!(schedule_failed)),
        Scheduler::Launchd => calendar::plist(&times()?, program, id)
            .map(Job::Plist)
            .ok_or(t!(schedule_job_unwritable)),
        Scheduler::Systemd => calendar::units(&times()?, program, id)
            .map(|(service, timer)| Job::Units { service, timer })
            .ok_or(t!(schedule_job_unwritable)),
    }
}

/// The directory launchd reads a person's jobs from.
fn launch_agents() -> Option<PathBuf> {
    Some(
        bravebot_agent::home::profile()?
            .join("Library")
            .join("LaunchAgents"),
    )
}

/// The directory a person's systemd user units are read from: `$XDG_CONFIG_HOME/systemd/user`
/// where that names a directory, and `~/.config/systemd/user` otherwise.
fn systemd_units() -> Option<PathBuf> {
    let configured = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute());
    let config = match configured {
        Some(path) => path,
        None => bravebot_agent::home::profile()?.join(".config"),
    };
    Some(config.join("systemd").join("user"))
}

/// Run `program` with `arguments` and say whether it ended with success.
#[cfg(unix)]
fn succeeds(program: &str, arguments: &[&str]) -> bool {
    let Some(mut command) = tool(program) else {
        return false;
    };
    command
        .args(arguments)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(not(unix))]
fn succeeds(_program: &str, _arguments: &[&str]) -> bool {
    false
}

/// Write `text` as `directory/name`, over what is there, making the directory if need be.
fn put_file(directory: &Path, name: &str, text: &str) -> bool {
    bravebot_agent::home::create_directory(directory).is_ok()
        && bravebot_agent::home::replace(&directory.join(name), text).is_ok()
}

/// Install `job` for `id`. A job that could not be loaded leaves no file behind.
fn start(id: &str, job: &Job) -> bool {
    match job {
        Job::Line(line) => install(id, Some(line)),
        Job::Plist(text) => {
            let Some(directory) = launch_agents() else {
                return false;
            };
            let name = format!("{}.plist", calendar::label(id));
            let path = directory.join(&name);
            let loaded = put_file(&directory, &name, text)
                && path
                    .to_str()
                    .is_some_and(|path| succeeds("launchctl", &["load", "-w", path]));
            if !loaded {
                ignored(std::fs::remove_file(path));
            }
            loaded
        }
        Job::Units { service, timer } => {
            let Some(directory) = systemd_units() else {
                return false;
            };
            let unit = calendar::unit(id);
            let timer_name = format!("{unit}.timer");
            let loaded = put_file(&directory, &format!("{unit}.service"), service)
                && put_file(&directory, &timer_name, timer)
                && succeeds("systemctl", &["--user", "daemon-reload"])
                && succeeds("systemctl", &["--user", "enable", "--now", &timer_name]);
            if !loaded {
                remove_units(&directory, id);
            }
            loaded
        }
    }
}

/// Delete entry `id`'s two unit files, and tell systemd they are gone.
fn remove_units(directory: &Path, id: &str) {
    let unit = calendar::unit(id);
    ignored(std::fs::remove_file(
        directory.join(format!("{unit}.timer")),
    ));
    ignored(std::fs::remove_file(
        directory.join(format!("{unit}.service")),
    ));
    succeeds("systemctl", &["--user", "daemon-reload"]);
}

/// Take entry `id`'s job out of `scheduler`. A crontab that cannot be changed is a failure, so that
/// an entry is kept only where its line is installed. A launchd job or a systemd timer the
/// scheduler no longer knows is not one: stopping it fails exactly then, and its files are removed
/// either way.
fn stop(scheduler: Scheduler, id: &str) -> bool {
    match scheduler {
        Scheduler::Crontab => install(id, None),
        Scheduler::Launchd => {
            let Some(directory) = launch_agents() else {
                return true;
            };
            let path = directory.join(format!("{}.plist", calendar::label(id)));
            if let Some(path) = path.to_str() {
                succeeds("launchctl", &["unload", "-w", path]);
            }
            ignored(std::fs::remove_file(path));
            true
        }
        Scheduler::Systemd => {
            let Some(directory) = systemd_units() else {
                return true;
            };
            succeeds(
                "systemctl",
                &[
                    "--user",
                    "disable",
                    "--now",
                    &format!("{}.timer", calendar::unit(id)),
                ],
            );
            remove_units(&directory, id);
            true
        }
    }
}

/// `bravebot schedule add ...`, after `add`.
fn add(words: &[String], context: &Context) -> Reply {
    let Some(request) = context.mode.and_then(|mode| read_addition(words, mode)) else {
        return Reply::said(Outcome::Argument, t!(schedule_add_usage));
    };
    // A person wrote this entry. A run's tool has no terminal, so nothing a turn runs can add one
    // (CLI-42).
    if context.typed.is_none() {
        return Reply::said(Outcome::Argument, t!(schedule_add_needs_a_terminal));
    }
    let Some(cron) = checked_cron(&request.cron) else {
        return Reply::said(Outcome::Argument, t!(schedule_cron_invalid));
    };
    if !usable_prompt(&request.prompt) {
        return Reply::said(Outcome::Argument, t!(schedule_prompt_invalid));
    }
    // The program is chosen here, by the person, and by no one after: it is refused with the
    // argument status, and nothing is stored, unless it is an absolute path to a file.
    if request
        .destination
        .as_deref()
        .is_some_and(|path| !checked_destination(path))
    {
        return Reply::said(Outcome::Argument, t!(schedule_destination_invalid));
    }
    let mode = request.mode;
    let directory = match project_directory(request.directory, context.project.as_deref()) {
        Ok(path) => path,
        Err(reply) => return reply,
    };
    let scheduler = request.scheduler.unwrap_or_else(Scheduler::usual);
    let Some(text) = entry_text(
        &cron,
        &directory,
        request.mode,
        scheduler,
        request.destination.as_deref(),
    ) else {
        return Reply::said(Outcome::Argument, t!(schedule_directory_invalid));
    };
    let Ok(me) = bravebot_sandbox::programs::Resolved::own_executable() else {
        return Reply::said(Outcome::Failed, t!(schedule_failed));
    };
    let Some(store) = store() else {
        return Reply::said(Outcome::Failed, t!(schedule_no_store));
    };
    let id = next_id(&store);
    let Some(program) = me.as_path().to_str() else {
        return Reply::said(Outcome::Failed, t!(schedule_failed));
    };
    let job = match job(scheduler, &cron, program, &id) {
        Ok(job) => job,
        Err(sentence) => return Reply::said(Outcome::Failed, sentence),
    };
    let written = bravebot_agent::home::create_new_file(
        &store.join(format!("{id}.prompt")),
        request.prompt.as_bytes(),
    )
    .and_then(|()| {
        bravebot_agent::home::create_new_file(&store.join(format!("{id}.entry")), text.as_bytes())
    });
    if written.is_err() {
        forget(&store, &id);
        return Reply::said(Outcome::Failed, t!(schedule_failed));
    }
    if !start(&id, &job) {
        forget(&store, &id);
        return Reply::said(Outcome::Failed, t!(schedule_install_failed));
    }
    Reply::done(vec![
        line(t!(schedule_added, id = id.as_str())),
        line(mode.limit()),
    ])
}

/// The directory a new entry runs in: the one the person typed, or the one they are working in. It
/// has to exist and be a directory; a path that is not one is an argument error.
fn project_directory(named: Option<PathBuf>, here: Option<&Path>) -> Result<PathBuf, Reply> {
    let directory = match (named, here) {
        (Some(path), _) => path,
        (None, Some(here)) => here.to_path_buf(),
        (None, None) => return Err(Reply::said(Outcome::Failed, t!(cli_directory_unknown))),
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
            Reply::said(
                Outcome::Argument,
                // Drawn: the directory is a name the person's checkout is under (LAYER-3).
                t!(
                    sessions_import_no_project,
                    path = bravebot_approval::printable(&directory.display().to_string())
                ),
            )
        })
}

/// An id past every one the store has a file for, so a removed entry's history is never read as a
/// new entry's.
fn next_id(store: &Path) -> String {
    let highest = std::fs::read_dir(store)
        .into_iter()
        .flatten()
        .filter_map(|file| {
            let name = file.ok()?.file_name();
            let (id, _) = name.to_str()?.split_once('.')?;
            id.parse::<u64>().ok()
        })
        .max()
        .unwrap_or(0);
    highest.saturating_add(1).to_string()
}

/// Remove an entry's files. The history stays, since the sessions it names do.
fn forget(store: &Path, id: &str) {
    ignored(std::fs::remove_file(store.join(format!("{id}.entry"))));
    ignored(std::fs::remove_file(store.join(format!("{id}.prompt"))));
    firing::Store::at(store).forget(id);
}

/// The files kept beside the entries, in the directory `list` reads them from.
fn kept_beside(store: &Path) -> firing::Store {
    firing::Store::at(store)
}

/// `bravebot schedule list`.
fn list() -> Reply {
    let entries = entries();
    if entries.is_empty() {
        return Reply::done(vec![line(t!(schedule_none))]);
    }
    let kept = store_to_read().map(|store| kept_beside(&store));
    let out = entries
        .iter()
        .map(|entry| {
            let paused = kept.as_ref().is_some_and(|kept| kept.is_paused(&entry.id));
            row(entry, paused)
        })
        .collect();
    Reply::done(out)
}

/// One entry as `list` says it: what it runs, what it may do, where its result goes, and then
/// whether it is paused.
fn row(entry: &Entry, paused: bool) -> String {
    let destination = match &entry.destination {
        Some(path) => t!(
            schedule_destination_to,
            path = bravebot_approval::printable(&path.display().to_string())
        ),
        None => t!(schedule_destination_none).to_string(),
    };
    format!(
        "{}  {}  {}",
        t!(
            schedule_row,
            id = entry.id.as_str(),
            cron = entry.cron.as_str(),
            scheduler = entry.scheduler.word(),
            directory = bravebot_approval::printable(&entry.directory.display().to_string()),
            limit = entry.mode.limit(),
            prompt = search::clean(&entry.prompt, LONGEST_PROMPT)
        ),
        destination,
        if paused {
            t!(schedule_state_paused)
        } else {
            t!(schedule_state_active)
        }
    )
}

/// The directory to write marks and questions in, and the entry `id`, for a command that changes
/// what is kept beside an entry.
fn writable_entry(id: &str) -> Result<(PathBuf, Entry), Reply> {
    let entry = store_to_read()
        .and_then(|store| entry_named(&store, id))
        .ok_or_else(|| Reply::said(Outcome::Argument, t!(schedule_missing, id = id)))?;
    match store() {
        Some(store) => Ok((store, entry)),
        None => Err(Reply::said(Outcome::Failed, t!(schedule_no_store))),
    }
}

/// `bravebot schedule pause|resume <id>`: mark the entry paused or take the mark off. The
/// crontab line stays; a firing of a paused entry is skipped by `run-now` (SCHEDULE-1).
fn pause(id: &str, pausing: bool) -> Reply {
    let (store, _) = match writable_entry(id) {
        Ok(held) => held,
        Err(reply) => return reply,
    };
    let kept = kept_beside(&store);
    if kept.is_paused(id) == pausing {
        return Reply::said(
            Outcome::Argument,
            if pausing {
                t!(schedule_already_paused, id = id)
            } else {
                t!(schedule_not_paused, id = id)
            },
        );
    }
    let changed = if pausing {
        kept.mark_paused(id)
    } else {
        kept.unmark_paused(id)
    };
    if changed.is_err() {
        return Reply::said(Outcome::Failed, t!(schedule_failed));
    }
    Reply::done(vec![line(if pausing {
        t!(schedule_paused_now, id = id)
    } else {
        t!(schedule_resumed, id = id)
    })])
}

/// `bravebot schedule pending [<id>]`: the question each entry's last firing stopped at. Reads
/// only, and prints the lines the firing wrote as they are.
fn pending(only: Option<&String>) -> Reply {
    let wanted: Vec<Entry> = entries()
        .into_iter()
        .filter(|entry| only.is_none_or(|id| &entry.id == id))
        .collect();
    if let Some(id) = only
        && wanted.is_empty()
    {
        return Reply::said(Outcome::Argument, t!(schedule_missing, id = id.as_str()));
    }
    let kept = store_to_read().map(|store| kept_beside(&store));
    let mut out = Vec::new();
    let mut shown = 0;
    for entry in &wanted {
        let Some(lines) = kept.as_ref().and_then(|kept| kept.pending(&entry.id)) else {
            continue;
        };
        shown += 1;
        out.push(line(t!(schedule_pending_header, id = entry.id.as_str())));
        for text in lines {
            out.push(format!("  {text}"));
        }
    }
    if shown == 0 {
        out.push(line(t!(schedule_pending_none)));
    }
    Reply::done(out)
}

/// `bravebot schedule clear <id>`: delete the question the entry's last firing stopped at. Nothing
/// is read from the file and no answer is taken: the prompt is fixed when the entry is written, so
/// a person who wants the run to go on with an answer removes the entry and adds it again with a
/// prompt that holds the answer (SCHEDULE-3).
fn clear(id: &str) -> Reply {
    let (store, _) = match writable_entry(id) {
        Ok(held) => held,
        Err(reply) => return reply,
    };
    Reply::done(vec![line(if kept_beside(&store).clear_pending(id) {
        t!(schedule_cleared, id = id)
    } else {
        t!(schedule_nothing_to_clear, id = id)
    })])
}

/// The stored entry the running process was fired for, when the variable `run-now` sets names one.
/// A value that names no stored entry changes nothing. `value` is the variable
/// ([`firing::FIRING_VARIABLE`]) as the caller read it.
pub fn fired_for(value: Option<String>) -> Option<String> {
    firing::named_by(value, |value| {
        is_an_id(value)
            && store_to_read()
                .and_then(|store| entry_named(&store, value))
                .is_some()
    })
}

/// Keep the question a firing of `id` stopped at, where it stopped at one. Returns the sentence to
/// say on standard error where it did.
pub fn record_firing(id: &str, asked: Option<&Asking>) -> Option<String> {
    if let Some(store) = bravebot_agent::home::writable().map(|state| state.join("schedules")) {
        firing::record_firing(&firing::Store::at(&store), id, asked);
    }
    asked.map(|_| t!(schedule_stopped_at_question, id = id).to_string())
}

/// `bravebot schedule remove <id>`.
fn remove(id: &str) -> Reply {
    let Some(store) = store_to_read() else {
        return Reply::said(Outcome::Argument, t!(schedule_missing, id = id));
    };
    let Some(entry) = entry_named(&store, id) else {
        return Reply::said(Outcome::Argument, t!(schedule_missing, id = id));
    };
    if !stop(entry.scheduler, id) {
        return Reply::said(Outcome::Failed, t!(schedule_install_failed));
    }
    forget(&store, id);
    Reply::done(vec![line(t!(schedule_removed, id = id))])
}

/// The id of the session a `--json` result names, from its line.
///
/// Read by scanning for the key outside every string, and not by looking at what the reply says:
/// the result object writes the reply with each quote escaped, so the key cannot be found inside
/// it, and the value is accepted only where it is a session name this program wrote.
fn session_of(result: &str) -> Option<String> {
    let mut in_string = false;
    let mut escaped = false;
    let bytes = result.as_bytes();
    for (at, byte) in bytes.iter().enumerate() {
        if in_string {
            match (escaped, byte) {
                (true, _) => escaped = false,
                (false, b'\\') => escaped = true,
                (false, b'"') => in_string = false,
                _ => {}
            }
            continue;
        }
        if *byte == b'"' {
            if let Some(rest) = result
                .get(at..)
                .and_then(|text| text.strip_prefix("\"session\":\""))
            {
                let (id, _) = rest.split_once('"')?;
                return sessions::is_a_session_name(id).then(|| id.to_string());
            }
            in_string = true;
        }
    }
    None
}

/// The arguments of the one-shot run an entry starts: the result flag, the mode flag the person
/// typed when writing the entry, and the stored prompt as the one word after `-p`, unchanged.
fn run_arguments(entry: &Entry) -> Vec<String> {
    let mut arguments = vec!["--json".to_string()];
    arguments.extend(entry.mode.flag().map(str::to_string));
    arguments.push("-p".to_string());
    arguments.push(entry.prompt.clone());
    arguments
}

/// `bravebot schedule run-now <id>`: the entry's one-shot run, started now, and recorded. Blocks
/// until the run ends, and holds the entry's lock for that long.
fn run_now(id: &str) -> Reply {
    let Some(store) = store_to_read() else {
        return Reply::said(Outcome::Argument, t!(schedule_missing, id = id));
    };
    let Some(entry) = entry_named(&store, id) else {
        return Reply::said(Outcome::Argument, t!(schedule_missing, id = id));
    };
    // The crontab line of a paused entry stays, so the firing that reaches here is skipped
    // (SCHEDULE-1). A person who wants the run now resumes the entry first.
    let kept = kept_beside(&store);
    if kept.is_paused(id) {
        return Reply::said(Outcome::Refused, t!(schedule_skipped_paused, id = id));
    }
    // Held until the run is recorded (`conclude` lets go of it before any delivery), and released by
    // the operating system if the process is killed. A firing that finds the previous one still
    // running is skipped (SCHEDULE-4).
    let hold = match kept.hold(id) {
        Ok(Some(held)) => held,
        Ok(None) => return Reply::done(vec![line(t!(schedule_skipped_running, id = id))]),
        Err(_) => return Reply::said(Outcome::Failed, t!(schedule_failed)),
    };
    // The question an earlier firing stopped at is no longer the one this run is waiting on.
    kept.clear_pending(id);
    let Ok(me) = bravebot_sandbox::programs::Resolved::own_executable() else {
        return Reply::said(Outcome::Failed, t!(schedule_failed));
    };
    // The environment is this process's own, as a person's shell would give it, for the reason
    // `sessions` gives for starting its own executable again.
    let mut command = bravebot_sandbox::programs::start(
        &me,
        bravebot_sandbox::programs::Inherits::All,
        bravebot_sandbox::programs::SearchPath::Inherited,
    );
    command.args(run_arguments(&entry));
    // Names the entry, so the run stops at its first question and keeps it (SCHEDULE-2).
    command.env(FIRING_VARIABLE, id);
    command
        .current_dir(&entry.directory)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let began = sessions::now();
    let Ok(output) = command.output() else {
        return Reply::said(Outcome::Failed, t!(schedule_failed));
    };
    conclude(
        &store,
        id,
        entry.destination.as_deref(),
        began,
        output,
        hold,
        DELIVERY_LIMIT,
    )
}

/// What happens once a firing's run has ended: it is recorded, the hold on the entry is let go of,
/// and the bytes the run printed are handed to the entry's destination, where it has one.
///
/// The one read of the result is the session name `session_of` accepts. The bytes handed on are
/// `output.stdout` itself, moved and not copied, so nothing between the run and the destination
/// holds a second reading of them. Whether to deliver depends on the entry (a destination or none),
/// and on nothing the run printed or the status it ended with: a run that failed, was refused or
/// stopped at a question hands over what it printed as any other does (SCHEDULE-7).
fn conclude(
    store: &Path,
    id: &str,
    destination: Option<&Path>,
    began: u64,
    output: std::process::Output,
    hold: std::fs::File,
    limit: Duration,
) -> Reply {
    let code = output.status.code().unwrap_or(-1);
    let session = std::str::from_utf8(&output.stdout)
        .ok()
        .and_then(|text| text.lines().last().and_then(session_of));
    record(store, id, began, code, session.as_deref());
    // Let go before the destination starts, so a destination that never ends cannot make every
    // later firing of the entry look like one still running (SCHEDULE-4, SCHEDULE-7).
    drop(hold);
    let notice = destination.and_then(|program| {
        deliver(program, output.stdout, limit)
            .err()
            .map(|why| why.sentence(id, program, limit))
    });
    let said = vec![line(t!(
        schedule_ran,
        id = id,
        status = code,
        session = session.as_deref().unwrap_or("-")
    ))];
    Reply {
        outcome: if code == 0 {
            Outcome::Done
        } else {
            Outcome::Failed
        },
        out: said,
        err: None,
        notice,
    }
}

/// How long a destination has to take what it was handed before it is stopped.
const DELIVERY_LIMIT: Duration = Duration::from_secs(60);

/// How often a running destination is looked at.
const TICK: Duration = Duration::from_millis(50);

/// Why a delivery did not complete. Each is a fact about the process and not about what it was
/// handed.
enum Undelivered {
    /// The program could not be started.
    NotStarted,
    /// It ended with a status other than success.
    Failed,
    /// It was still running when the limit passed and was stopped.
    TimedOut,
}

impl Undelivered {
    /// The sentence for standard error. It names the entry and the program the person chose, and
    /// carries nothing the program or the run printed.
    fn sentence(&self, id: &str, program: &Path, limit: Duration) -> String {
        let path = bravebot_approval::printable(&program.display().to_string());
        match self {
            Self::NotStarted => t!(schedule_delivery_not_started, id = id, path = path),
            Self::Failed => t!(schedule_delivery_failed, id = id, path = path),
            Self::TimedOut => t!(
                schedule_delivery_timed_out,
                id = id,
                path = path,
                seconds = limit.as_secs()
            ),
        }
    }
}

/// Start `program`, write `bytes` to its standard input and wait for it for at most `limit`.
///
/// The bytes are written and never looked at. The program is started as a hook's is: without this
/// agent's credentials, with the reduced search path, in a process group of its own so the limit
/// ends whatever it started, in the person's home and not in the entry's directory, with no
/// arguments, and with standard output and error discarded. The write is made on a thread of its
/// own, so a program that never reads cannot stop the limit being kept.
fn deliver(program: &Path, bytes: Vec<u8>, limit: Duration) -> Result<(), Undelivered> {
    let withheld = bravebot_config::scrub::withheld();
    let mut command = bravebot_sandbox::programs::start(
        &bravebot_sandbox::programs::Resolved::named_by_person(program),
        bravebot_sandbox::programs::Inherits::Without(&withheld),
        bravebot_sandbox::programs::SearchPath::Reduced,
    );
    command
        .current_dir(bravebot_agent::home::profile().unwrap_or_else(std::env::temp_dir))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = bravebot_sandbox::process::starting_a_program(|| command.spawn())
        .map_err(|_| Undelivered::NotStarted)?;
    // Not joined: a program that ended while something it started still holds the pipe would keep
    // a join waiting. The thread ends when the pipe does, or with the process.
    if let Some(mut stdin) = child.stdin.take() {
        std::thread::spawn(move || ignored(stdin.write_all(&bytes)));
    }
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) | Err(_) => return Err(Undelivered::Failed),
            Ok(None) => {}
        }
        if started.elapsed() >= limit {
            // The group first: the child has not been waited for, so its number still names its
            // group and belongs to nothing else.
            bravebot_session::jobs::end_group(child.id(), true);
            ignored(child.kill());
            // Reaped rather than left as a zombie.
            ignored(child.wait());
            return Err(Undelivered::TimedOut);
        }
        std::thread::sleep(TICK);
    }
}

/// One line appended to the entry's history: when, the status, and the session or `-`.
fn record(store: &Path, id: &str, began: u64, code: i32, session: Option<&str>) {
    if let Ok(mut file) = bravebot_agent::home::append_to_file(&store.join(format!("{id}.history")))
    {
        ignored(writeln!(file, "{began} {code} {}", session.unwrap_or("-")));
    }
}

/// A history line read back: when, the status and the session.
fn read_run(line: &str) -> Option<(u64, i32, Option<String>)> {
    let mut parts = line.split(' ');
    let when = parts.next()?.parse().ok()?;
    let code = parts.next()?.parse().ok()?;
    let session = parts
        .next()
        .filter(|word| *word != "-" && sessions::is_a_session_name(word))
        .map(str::to_string);
    Some((when, code, session))
}

/// `bravebot schedule history [<id>]`: each run, newest first, with what its session spent.
fn history(only: Option<&String>) -> Reply {
    let Some(store) = store_to_read() else {
        return Reply::done(vec![line(t!(schedule_history_none))]);
    };
    let wanted: Vec<Entry> = entries()
        .into_iter()
        .filter(|entry| only.is_none_or(|id| &entry.id == id))
        .collect();
    if let Some(id) = only
        && wanted.is_empty()
    {
        return Reply::said(Outcome::Argument, t!(schedule_missing, id = id.as_str()));
    }
    let mut runs = Vec::new();
    for entry in &wanted {
        let Ok(text) = std::fs::read_to_string(store.join(format!("{}.history", entry.id))) else {
            continue;
        };
        for (when, code, session) in text.lines().filter_map(read_run) {
            let tokens = session
                .as_deref()
                .and_then(|id| sessions::load(&entry.directory, id))
                .map(|record| record.tokens);
            runs.push((when, entry.id.clone(), code, session, tokens));
        }
    }
    if runs.is_empty() {
        return Reply::done(vec![line(t!(schedule_history_none))]);
    }
    runs.sort_by_key(|run| std::cmp::Reverse(run.0));
    let mut out = Vec::new();
    for (when, id, code, session, tokens) in runs {
        out.push(line(t!(
            schedule_history_row,
            id = id.as_str(),
            when = sessions::how_long_ago(when),
            status = code,
            session = session.as_deref().unwrap_or("-"),
            tokens = tokens.map_or_else(|| "-".to_string(), |n| n.to_string())
        )));
    }
    Reply::done(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cron_expression_is_five_fields_of_digits_and_the_four_operators() {
        assert_eq!(
            checked_cron("0  9 * *  1-5").as_deref(),
            Some("0 9 * * 1-5")
        );
        assert_eq!(
            checked_cron("*/15 0,12 * * *").as_deref(),
            Some("*/15 0,12 * * *")
        );
        for refused in [
            "0 9 * *",
            "0 9 * * 1 2",
            "@daily",
            "0 9 * * mon",
            "0 9 * * *;",
            "0 9 * * 1;2",
            "0 9 * * *;id",
            "0 9 * * 1`id`",
            "0 9 * * *\\",
            "0 9 * * $(id)",
            "0 9 * * %",
            "",
        ] {
            assert_eq!(checked_cron(refused), None, "{refused}");
        }
    }

    #[test]
    fn an_entry_stored_with_a_field_outside_its_range_still_reads() {
        let entry = read_entry(
            "1",
            "cron=* * 0 * *\ndir=/work\nmode=read\n",
            "hello".to_string(),
        );
        assert_eq!(entry.map(|e| e.cron).as_deref(), Some("* * 0 * *"));
        assert!(read_entry("1", "cron=* * $(x) * *\ndir=/w\nmode=read\n", String::new()).is_none());
        assert!(read_entry("1", "cron=* * * *\ndir=/w\nmode=read\n", String::new()).is_none());
    }

    #[test]
    fn a_cron_field_outside_its_range_or_with_a_zero_step_is_refused() {
        for kept in [
            "0 0 1 1 0",
            "59 23 31 12 7",
            "*/5 */2 */3 */4 */7",
            "0-59 0-23 1-31 1-12 0-7",
            "0,30 9-17 * * 1-5",
        ] {
            assert_eq!(checked_cron(kept).as_deref(), Some(kept), "{kept}");
        }
        for refused in [
            "60 * * * *",
            "* 24 * * *",
            "* * 0 * *",
            "* * 32 * *",
            "* * * 0 *",
            "* * * 13 *",
            "* * * * 8",
            "*/0 * * * *",
            "* * * * */0",
            "5-1 * * * *",
            "1-60 * * * *",
            "1- * * * *",
            "-1 * * * *",
            "*/ * * * *",
            "1,,2 * * * *",
            "1,60 * * * *",
            "*/-1 * * * *",
            "00000 * * * *",
        ] {
            assert_eq!(checked_cron(refused), None, "{refused}");
        }
    }

    #[test]
    fn the_destination_is_an_add_word_stored_under_its_own_key() {
        // The person names the program at `add`, and the entry file holds it under a key
        // of its own. A wrong implementation that kept it in the prompt file, or under `dir`,
        // would not read back as the destination.
        let base = ["--cron", "0 9 * * *", "--prompt", "hi"];
        let with = |extra: &[&str]| -> Vec<String> {
            base.iter().chain(extra).map(|s| s.to_string()).collect()
        };
        let named =
            read_addition(&with(&[DELIVER_TO_WORD, "/bin/cat"]), Mode::Read).expect("reads");
        assert_eq!(named.destination, Some(PathBuf::from("/bin/cat")));
        assert_eq!(
            read_addition(&with(&[]), Mode::Read).unwrap().destination,
            None
        );
        // A repeat and a missing value are refused rather than guessed at.
        assert!(
            read_addition(
                &with(&[DELIVER_TO_WORD, "/bin/cat", DELIVER_TO_WORD, "/bin/ls"]),
                Mode::Read
            )
            .is_none()
        );
        assert!(read_addition(&with(&[DELIVER_TO_WORD]), Mode::Read).is_none());

        let text = entry_text(
            "0 9 * * *",
            Path::new("/work"),
            Mode::Read,
            Scheduler::Crontab,
            Some(Path::new("/bin/cat")),
        )
        .expect("an entry");
        assert!(text.ends_with("deliver=/bin/cat\n"), "{text}");
        let entry = read_entry("1", &text, "hi".into()).expect("reads");
        assert_eq!(entry.destination, Some(PathBuf::from("/bin/cat")));
        // An entry with no destination is the file it was before the key existed.
        let plain = entry_text(
            "0 9 * * *",
            Path::new("/work"),
            Mode::Read,
            Scheduler::Crontab,
            None,
        )
        .expect("an entry");
        assert_eq!(
            plain,
            "cron=0 9 * * *\ndir=/work\nmode=read\nscheduler=crontab\n"
        );
    }

    #[test]
    fn an_entry_stored_before_the_destination_key_reads_as_no_destination() {
        // An absent key is no destination, and the entry still loads.
        let old = "cron=0 9 * * *\ndir=/work\nmode=read\nscheduler=launchd\n";
        let entry = read_entry("1", old, "hi".into()).expect("still loads");
        assert_eq!(entry.destination, None);
        let older = "cron=0 9 * * *\ndir=/work\nmode=read\n";
        assert_eq!(
            read_entry("1", older, String::new())
                .expect("loads")
                .destination,
            None
        );
    }

    #[test]
    fn a_malformed_stored_destination_makes_the_entry_unread_and_not_destinationless() {
        // A value nobody could have typed at `add` is an entry that is not read, and is never
        // read as one with no destination (which would silently stop delivering) or kept with the
        // value ignored. The wrong implementation falls back to `None` for an unparsable value.
        let text = |line: &str| format!("cron=0 9 * * *\ndir=/work\nmode=read\n{line}");
        for bad in [
            "deliver=\n",
            "deliver=notify\n",
            "deliver=./notify\n",
            "deliver=/bin/a\rb\n",
            "deliver=/bin/a\tb\n",
            "deliver=/bin/a\u{7f}\n",
            // A good line after a bad one does not rescue the entry.
            "deliver=bad\ndeliver=/bin/cat\n",
        ] {
            assert!(
                read_entry("1", &text(bad), String::new()).is_none(),
                "{bad:?}"
            );
        }
        // An entry text is refused at the writing side for the same values.
        for bad in ["notify", "", "/bin/a\nb", "/bin/a\rb"] {
            assert_eq!(
                entry_text(
                    "0 9 * * *",
                    Path::new("/work"),
                    Mode::Read,
                    Scheduler::Crontab,
                    Some(Path::new(bad))
                ),
                None,
                "{bad:?}"
            );
        }
    }

    #[test]
    fn a_destination_must_be_an_absolute_path_to_a_file_that_exists() {
        // A destination must be absolute, exist and be a file.
        let scratch = tempfile::tempdir().expect("a directory");
        let program = scratch.path().join("notify");
        std::fs::write(&program, "#!/bin/sh\n").expect("writes");
        assert!(checked_destination(&program));
        assert!(!checked_destination(&scratch.path().join("missing")));
        assert!(!checked_destination(scratch.path()), "a directory");
        assert!(!checked_destination(Path::new("notify")), "a bare name");
        assert!(!checked_destination(Path::new("./notify")), "relative");
        assert!(!checked_destination(Path::new("")));
        let control = scratch.path().join("no\ttab");
        std::fs::write(&control, "x").expect("writes");
        assert!(!checked_destination(&control), "a control character");
    }

    #[test]
    fn add_refuses_a_destination_that_is_not_usable_with_the_argument_status_before_storing() {
        // The refusal has the argument status, and is reached before anything is stored. The
        // refusal precedes `store()`, so the state directory is not read or made.
        let scratch = tempfile::tempdir().expect("a directory");
        let context = Context {
            typed: Some(TypedLine::of_a_person()),
            mode: Some(Mode::Read),
            flags_typed: false,
            project: Some(scratch.path().to_path_buf()),
        };
        for bad in [
            "notify",
            "/no/such/program",
            scratch.path().to_str().unwrap(),
        ] {
            let words: Vec<String> = [
                "--cron",
                "0 9 * * *",
                "--prompt",
                "hi",
                "--scheduler",
                "crontab",
                DELIVER_TO_WORD,
                bad,
            ]
            .iter()
            .map(|s| s.to_string())
            .collect();
            let reply = add(&words, &context);
            assert_eq!(reply.outcome, Outcome::Argument, "{bad}");
            assert_eq!(
                reply.err.as_deref(),
                Some(t!(schedule_destination_invalid)),
                "{bad}"
            );
            assert!(reply.out.is_empty(), "{bad}");
        }
    }

    #[test]
    fn list_shows_the_destination_before_the_paused_or_active_word() {
        // The row ends with the destination and then the state word.
        let entry = |destination: Option<&str>| Entry {
            id: "1".into(),
            cron: "0 9 * * *".into(),
            directory: PathBuf::from("/work"),
            mode: Mode::Read,
            scheduler: Scheduler::Crontab,
            destination: destination.map(PathBuf::from),
            prompt: "hi".into(),
        };
        let to = t!(schedule_destination_to, path = "/opt/notify").to_string();
        let paused = row(&entry(Some("/opt/notify")), true);
        assert!(
            paused.ends_with(&format!("{to}  {}", t!(schedule_state_paused))),
            "{paused}"
        );
        let active = row(&entry(Some("/opt/notify")), false);
        assert!(
            active.ends_with(&format!("{to}  {}", t!(schedule_state_active))),
            "{active}"
        );
        let none = row(&entry(None), false);
        assert!(
            none.ends_with(&format!(
                "{}  {}",
                t!(schedule_destination_none),
                t!(schedule_state_active)
            )),
            "{none}"
        );
    }

    #[test]
    fn nothing_a_run_printed_reaches_the_entry_or_its_destination() {
        // What a run prints is carried only into the history line, and only as the session
        // name `session_of` accepts. A run that prints a destination, a key line or a second
        // session cannot change the entry file, and the history holds no path.
        let store = tempfile::tempdir().expect("a directory");
        let entry_text = entry_text(
            "0 9 * * *",
            Path::new("/work"),
            Mode::Read,
            Scheduler::Crontab,
            Some(Path::new("/opt/notify")),
        )
        .expect("an entry");
        std::fs::write(store.path().join("1.entry"), &entry_text).expect("writes");
        std::fs::write(store.path().join("1.prompt"), "hi").expect("writes");
        let before = entry_named(store.path(), "1").expect("reads");

        let planted = [
            "{\"reply\":\"x\\ndeliver=/evil\\n\",\"session\":\"/evil\"}",
            "deliver=/evil\n{\"session\":\"20250101-120000-ab12 deliver=/evil\"}",
            "{\"reply\":\"x\\\",\\\"session\\\":\\\"20250101-120000-ffff\"}",
        ];
        for result in planted {
            let session = result.lines().last().and_then(session_of);
            record(store.path(), "1", 100, 0, session.as_deref());
        }
        let history = std::fs::read_to_string(store.path().join("1.history")).expect("history");
        assert!(!history.contains("evil"), "{history}");
        assert_eq!(history.lines().count(), planted.len());
        assert_eq!(
            std::fs::read_to_string(store.path().join("1.entry")).expect("reads"),
            entry_text
        );
        assert_eq!(entry_named(store.path(), "1"), Some(before));
    }

    #[test]
    fn a_stored_run_carries_one_of_three_modes_and_never_an_approval_flag() {
        // `add` reads the cron, the prompt and the directory and refuses any other word, so the
        // flag that lets a program approve a write is not stored (SCHEDULE-6).
        let base = ["--cron", "0 9 * * *", "--prompt", "hi"];
        for flag in [
            "--approve-input-lines",
            "--auto",
            "--bypass-approvals-and-sandbox",
        ] {
            let words: Vec<String> = base.iter().chain(&[flag]).map(|s| s.to_string()).collect();
            assert!(read_addition(&words, Mode::Read).is_none(), "{flag}");
        }
        assert_eq!(Mode::Read.flag(), None);
        assert_eq!(Mode::AcceptEdits.flag(), Some(ACCEPT_EDITS_FLAG));
        assert_eq!(Mode::Bypass.flag(), Some(SKIP_PERMISSIONS_FLAG));
    }

    #[test]
    fn the_stored_prompt_is_the_one_word_after_dash_p_unchanged() {
        // The prompt is the person's own words (CLI-42): it reaches the run as one argument, with
        // nothing added to it, left out of it or read out of it.
        for prompt in [
            "list merged pull requests",
            "  spaces and 'quotes' and \"doubles\" ",
            "line one\nline two $(id) `id` ; rm -rf / %",
            "ünïcode ✓",
        ] {
            for mode in [Mode::Read, Mode::AcceptEdits, Mode::Bypass] {
                let entry = Entry {
                    id: "1".to_string(),
                    cron: "0 9 * * *".to_string(),
                    directory: PathBuf::from("/work"),
                    mode,
                    scheduler: Scheduler::Crontab,
                    destination: Some(PathBuf::from("/opt/notify")),
                    prompt: prompt.to_string(),
                };
                let arguments = run_arguments(&entry);
                let at = arguments.len() - 2;
                assert_eq!(arguments[at], "-p", "{prompt:?}");
                assert_eq!(arguments[at + 1], prompt, "{prompt:?}");
                assert_eq!(arguments[0], "--json");
                assert_eq!(arguments.len(), 3 + usize::from(mode.flag().is_some()));
                // The destination is never a word a run is started with (SCHEDULE-6).
                assert!(
                    arguments
                        .iter()
                        .all(|word| word != "/opt/notify" && word != DELIVER_TO_WORD),
                    "{arguments:?}"
                );
            }
        }
    }

    #[test]
    fn a_typed_line_stores_no_approval_flag_whatever_mode_it_carries() {
        // The command line hands `add` one of the three modes and the words after `add`. The flag
        // that lets a program approve a write is refused in the words under each of them and is no
        // mode's flag (SCHEDULE-6).
        let base = ["--cron", "0 9 * * *", "--prompt", "hi"];
        for mode in [Mode::Read, Mode::AcceptEdits, Mode::Bypass] {
            assert_ne!(mode.flag(), Some("--approve-input-lines"));
            let plain: Vec<String> = base.iter().map(|s| s.to_string()).collect();
            assert_eq!(read_addition(&plain, mode).map(|a| a.mode), Some(mode));
            let approving: Vec<String> = base
                .iter()
                .chain(&["--approve-input-lines"])
                .map(|s| s.to_string())
                .collect();
            assert!(read_addition(&approving, mode).is_none());
        }
    }

    #[test]
    fn a_prompt_that_would_be_read_as_a_flag_or_is_empty_is_not_usable() {
        assert!(usable_prompt("summarize merged pull requests"));
        assert!(!usable_prompt("   "));
        assert!(!usable_prompt("--dangerously-skip-permissions"));
        assert!(!usable_prompt(&"x".repeat(LONGEST_PROMPT_BYTES + 1)));
    }

    #[test]
    fn a_repeat_a_missing_value_and_a_stray_word_are_refused() {
        let base = ["--cron", "0 9 * * *", "--prompt", "hi"];
        let with = |extra: &[&str]| -> Vec<String> {
            base.iter().chain(extra).map(|s| s.to_string()).collect()
        };
        assert!(read_addition(&with(&[]), Mode::Read).is_some());
        assert!(read_addition(&with(&["--dir", "a", "--dir", "b"]), Mode::Read).is_none());
        assert!(read_addition(&with(&["extra"]), Mode::Read).is_none());
        assert!(read_addition(&with(&["--dir"]), Mode::Read).is_none());
        assert!(read_addition(&with(&["--accept-edits"]), Mode::Read).is_none());
        let named = read_addition(&with(&["--scheduler", "systemd"]), Mode::Read).expect("reads");
        assert_eq!(named.scheduler, Some(Scheduler::Systemd));
        assert!(read_addition(&with(&["--scheduler", "cron"]), Mode::Read).is_none());
        assert!(
            read_addition(
                &with(&["--scheduler", "launchd", "--scheduler", "systemd"]),
                Mode::Read
            )
            .is_none()
        );
    }

    #[test]
    fn an_entry_reads_back_as_it_was_written() {
        let text = entry_text(
            "0 9 * * 1-5",
            Path::new("/work/here"),
            Mode::AcceptEdits,
            Scheduler::Systemd,
            None,
        )
        .expect("a plain path");
        let entry = read_entry("3", &text, "do it".into()).expect("reads");
        assert_eq!(
            entry,
            Entry {
                id: "3".into(),
                cron: "0 9 * * 1-5".into(),
                directory: PathBuf::from("/work/here"),
                mode: Mode::AcceptEdits,
                scheduler: Scheduler::Systemd,
                destination: None,
                prompt: "do it".into(),
            }
        );
        assert_eq!(
            entry_text(
                "0 9 * * *",
                Path::new("a\nb"),
                Mode::Read,
                Scheduler::Crontab,
                None
            ),
            None
        );
        // A mode the program did not write reads as no entry, and never as read-only.
        assert!(read_entry("3", "cron=0 9 * * *\ndir=/x\nmode=root\n", String::new()).is_none());
        // A scheduler the program did not write is no entry, and an entry from before the choice
        // was a crontab line.
        let text = |extra: &str| format!("cron=0 9 * * *\ndir=/x\nmode=read\n{extra}");
        assert!(read_entry("3", &text("scheduler=cron\n"), String::new()).is_none());
        let old = read_entry("3", &text(""), String::new()).expect("reads");
        assert_eq!(old.scheduler, Scheduler::Crontab);
    }

    #[cfg(unix)]
    #[test]
    fn a_directory_that_is_not_utf8_is_refused_and_not_written_as_its_lookalike() {
        use std::os::unix::ffi::OsStrExt;
        let directory = Path::new(std::ffi::OsStr::from_bytes(b"/work/\xff"));
        assert_eq!(
            entry_text("0 9 * * *", directory, Mode::Read, Scheduler::Crontab, None),
            None
        );
        // The lookalike a lossy conversion would write is itself a valid directory name.
        let lookalike = Path::new("/work/\u{FFFD}");
        let text = entry_text("0 9 * * *", lookalike, Mode::Read, Scheduler::Crontab, None)
            .expect("an entry");
        assert!(text.contains("dir=/work/\u{FFFD}\n"));
    }

    #[test]
    fn a_crontab_line_quotes_the_program_and_escapes_the_percent() {
        let line = crontab_line("0 9 * * *", "/opt/it's 100%/bravebot", "2").expect("a line");
        assert_eq!(
            line,
            "0 9 * * * '/opt/it'\\''s 100\\%/bravebot' schedule run-now 2 >/dev/null 2>&1 \
             # bravebot-schedule:2"
        );
        assert_eq!(crontab_line("0 9 * * *", "/a\nb", "2"), None);
    }

    #[test]
    fn a_table_keeps_every_other_line_and_replaces_only_its_own() {
        let table = "MAILTO=me\n0 1 * * * backup # bravebot-schedule:12\n5 5 * * * mine # bravebot-schedule:1\n";
        // Entry 1 is not entry 12: the marker is matched to the end of the line.
        let removed = with_line(table, "1", None);
        assert_eq!(
            removed,
            "MAILTO=me\n0 1 * * * backup # bravebot-schedule:12\n"
        );
        let replaced = with_line(table, "1", Some("7 7 * * * new # bravebot-schedule:1"));
        assert!(replaced.ends_with("7 7 * * * new # bravebot-schedule:1\n"));
        assert_eq!(replaced.matches("bravebot-schedule:1\n").count(), 1);
        assert_eq!(with_line("", "1", None), "");
    }

    #[test]
    fn the_session_is_found_outside_the_reply_and_never_inside_it() {
        let honest = r#"{"schema":1,"reply":"done","session":"20250101-120000-ab12","steps":1}"#;
        let planted =
            r#"{"schema":1,"reply":"x\",\"session\":\"20250101-120000-ffff","session":null}"#;
        assert_eq!(session_of(honest).as_deref(), Some("20250101-120000-ab12"));
        assert_eq!(session_of(planted), None);
        assert_eq!(session_of(r#"{"session":"../../etc"}"#), None);
    }

    #[test]
    fn a_history_line_reads_back_and_a_bad_session_word_is_dropped() {
        assert_eq!(read_run("100 0 -"), Some((100, 0, None)));
        assert_eq!(read_run("100 3 ../x"), Some((100, 3, None)));
        assert_eq!(read_run("soon 0 -"), None);
    }

    /// A stand-in destination: a script that runs `body`, in `directory`.
    #[cfg(unix)]
    fn stand_in(directory: &Path, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = directory.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("writes");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("mode");
        path
    }

    /// What a run that ended with `code` and printed `stdout` leaves for `conclude`.
    #[cfg(unix)]
    fn ended(code: i32, stdout: &[u8]) -> std::process::Output {
        use std::os::unix::process::ExitStatusExt;
        std::process::Output {
            status: std::process::ExitStatus::from_raw(code << 8),
            stdout: stdout.to_vec(),
            stderr: Vec::new(),
        }
    }

    /// `conclude` for entry `1` in `store`, as `run_now` calls it.
    #[cfg(unix)]
    fn concluded(
        store: &Path,
        destination: Option<&Path>,
        output: std::process::Output,
        limit: Duration,
    ) -> Reply {
        let hold = kept_beside(store).hold("1").expect("held").expect("free");
        conclude(store, "1", destination, 100, output, hold, limit)
    }

    #[cfg(unix)]
    const SECOND: Duration = Duration::from_secs(1);

    /// SCHEDULE-7: the bytes a run printed reach the destination's standard input as they were,
    /// whatever they hold. A wrong implementation that decoded the text, cut it, trimmed it or
    /// stopped at a line, or one that wrote less than a pipe's buffer, would change one of these.
    #[cfg(unix)]
    #[test]
    fn the_bytes_a_run_printed_reach_the_destination_unmodified() {
        let directory = tempfile::tempdir().expect("a directory");
        let caught = directory.path().join("caught");
        let program = stand_in(
            directory.path(),
            "notify",
            &format!("exec /bin/cat > '{}'", caught.display()),
        );
        let mut large = b"{\"reply\":\"x\",\"session\":\"20250101-120000-ffff\"}\n".to_vec();
        large.extend(std::iter::repeat_n(b'a', 1 << 20));
        let payloads: Vec<Vec<u8>> = vec![
            b"{\"reply\":\"r\",\"session\":\"20250101-120000-ab12\"}\n".to_vec(),
            b"-rf $(id) `id`; rm -rf / %s \"session\":\"/evil\" --deliver-to /evil\n\n  \n"
                .to_vec(),
            b"\xff\xfe\x00 not text \r\n\x1b[2J".to_vec(),
            Vec::new(),
            large,
        ];
        for payload in payloads {
            let store = tempfile::tempdir().expect("a directory");
            let reply = concluded(
                store.path(),
                Some(&program),
                ended(0, &payload),
                Duration::from_secs(30),
            );
            assert_eq!(reply.notice, None);
            assert_eq!(reply.outcome, Outcome::Done);
            assert_eq!(std::fs::read(&caught).expect("caught"), payload);
            std::fs::remove_file(&caught).expect("removed");
        }
    }

    /// SCHEDULE-7: what is run, recorded and said is the same whether the run printed hostile text
    /// or nothing, and the destination is started with no argument at all. A wrong implementation
    /// that put the text, or a word from it, on the command line, or took the destination or the
    /// session from it, would change these.
    #[cfg(unix)]
    #[test]
    fn hostile_text_changes_neither_what_is_run_nor_what_is_recorded() {
        let directory = tempfile::tempdir().expect("a directory");
        let arguments = directory.path().join("arguments");
        let program = stand_in(
            directory.path(),
            "notify",
            &format!(
                "/bin/cat > /dev/null\nprintf '%s' \"$#\" > '{}'",
                arguments.display()
            ),
        );
        let evil = directory.path().join("evil-ran");
        let evil_program = stand_in(
            directory.path(),
            "evil",
            &format!(": > '{}'", evil.display()),
        );
        let hostile = format!(
            "{{\"reply\":\"x\\\",\\\"session\\\":\\\"20250101-120000-ffff\",\"session\":null}}\n\
             deliver={}\n- --deliver-to {} $(id) ; `id`\n",
            evil_program.display(),
            evil_program.display()
        );
        let store = tempfile::tempdir().expect("a directory");
        let reply = concluded(
            store.path(),
            Some(&program),
            ended(0, hostile.as_bytes()),
            Duration::from_secs(30),
        );
        assert_eq!(reply.notice, None);
        assert_eq!(std::fs::read_to_string(&arguments).expect("ran"), "0");
        assert!(!evil.exists(), "the program the text named was started");
        // Recorded as any run is: the status and no session, since the last line is not one.
        assert_eq!(
            std::fs::read_to_string(store.path().join("1.history")).expect("history"),
            "100 0 -\n"
        );
        assert!(
            reply.out[0].contains("session -") && !reply.out[0].contains("evil"),
            "{:?}",
            reply.out
        );
    }

    /// SCHEDULE-7: a destination that fails, cannot be started or is not executable leaves the
    /// lines, the status and the history as they are for an entry with no destination, and says so
    /// in the notice only, with the sentence for that case. A wrong implementation that returned
    /// the failure as the outcome, skipped the record, or gave one sentence for every case would
    /// change one.
    #[cfg(unix)]
    #[test]
    fn a_destination_that_fails_changes_nothing_but_the_notice() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().expect("a directory");
        let failing = stand_in(directory.path(), "fails", "/bin/cat > /dev/null\nexit 3");
        let not_executable = directory.path().join("plain");
        std::fs::write(&not_executable, "#!/bin/sh\n").expect("writes");
        std::fs::set_permissions(&not_executable, std::fs::Permissions::from_mode(0o644))
            .expect("mode");
        let gone = directory.path().join("gone");
        let path = |program: &Path| bravebot_approval::printable(&program.display().to_string());
        let cases = [
            (
                &failing,
                t!(schedule_delivery_failed, id = "1", path = path(&failing)).to_string(),
            ),
            (
                &gone,
                t!(schedule_delivery_not_started, id = "1", path = path(&gone)).to_string(),
            ),
            (
                &not_executable,
                t!(
                    schedule_delivery_not_started,
                    id = "1",
                    path = path(&not_executable)
                )
                .to_string(),
            ),
        ];
        for code in [0, 2] {
            let alone_store = tempfile::tempdir().expect("a directory");
            let alone = concluded(alone_store.path(), None, ended(code, b"out\n"), SECOND);
            assert_eq!(alone.notice, None);
            let alone_history =
                std::fs::read_to_string(alone_store.path().join("1.history")).expect("history");
            for (program, sentence) in &cases {
                let store = tempfile::tempdir().expect("a directory");
                let reply = concluded(store.path(), Some(program), ended(code, b"out\n"), SECOND);
                assert_eq!(reply.outcome, alone.outcome, "{program:?}");
                assert_eq!(reply.out, alone.out, "{program:?}");
                assert_eq!(reply.err, None);
                assert_eq!(
                    std::fs::read_to_string(store.path().join("1.history")).expect("history"),
                    alone_history
                );
                assert_eq!(reply.notice.as_ref(), Some(sentence), "{program:?}");
            }
        }
    }

    /// SCHEDULE-7: the result is handed over whatever status the run ended with. The decision to
    /// deliver is made from the entry alone. A wrong implementation that delivered only after a
    /// success would fail the status 2 case.
    #[cfg(unix)]
    #[test]
    fn a_run_that_failed_hands_over_what_it_printed_as_any_run_does() {
        let directory = tempfile::tempdir().expect("a directory");
        let caught = directory.path().join("caught");
        let program = stand_in(
            directory.path(),
            "notify",
            &format!("exec /bin/cat > '{}'", caught.display()),
        );
        let store = tempfile::tempdir().expect("a directory");
        let reply = concluded(
            store.path(),
            Some(&program),
            ended(2, b"{\"error\":\"refused\"}\n"),
            Duration::from_secs(30),
        );
        assert_eq!(reply.outcome, Outcome::Failed);
        assert_eq!(
            std::fs::read(&caught).expect("caught"),
            b"{\"error\":\"refused\"}\n"
        );
    }

    /// SCHEDULE-4, SCHEDULE-7: the hold on the entry is let go of before the destination starts,
    /// so a destination that is slow does not make the next firing look like one still running. A
    /// wrong implementation that kept the hold until `run_now` returned would not let this test
    /// take it while the destination runs.
    #[cfg(unix)]
    #[test]
    fn the_hold_is_let_go_of_while_the_destination_runs() {
        let directory = tempfile::tempdir().expect("a directory");
        let started = directory.path().join("started");
        let program = stand_in(
            directory.path(),
            "slow",
            &format!(": > '{}'\n/bin/sleep 2", started.display()),
        );
        let store = tempfile::tempdir().expect("a directory");
        let kept = kept_beside(store.path());
        let hold = kept.hold("1").expect("held").expect("free");
        std::thread::scope(|scope| {
            let delivering = scope.spawn(|| {
                conclude(
                    store.path(),
                    "1",
                    Some(&program),
                    100,
                    ended(0, b"x"),
                    hold,
                    Duration::from_secs(30),
                )
            });
            let waited = Instant::now();
            while !started.exists() {
                assert!(waited.elapsed() < Duration::from_secs(20), "never started");
                std::thread::sleep(Duration::from_millis(20));
            }
            assert!(!delivering.is_finished(), "the destination had ended");
            assert!(
                kept.hold("1").expect("held").is_some(),
                "the next firing would be skipped as still running"
            );
            assert_eq!(delivering.join().expect("joined").notice, None);
        });
    }

    /// SCHEDULE-7: a destination that never ends is stopped at the limit with everything it
    /// started, and the run is recorded and reported as it would be without one. A wrong
    /// implementation that waited for it, or ended only the program the entry named, would hang
    /// this test or let `late` appear.
    #[cfg(unix)]
    #[test]
    fn a_destination_that_never_ends_is_stopped_with_what_it_started() {
        let directory = tempfile::tempdir().expect("a directory");
        let late = directory.path().join("late");
        let program = stand_in(
            directory.path(),
            "hangs",
            &format!("( /bin/sleep 2; : > '{}' ) &\nwait", late.display()),
        );
        let store = tempfile::tempdir().expect("a directory");
        let began = Instant::now();
        let reply = concluded(
            store.path(),
            Some(&program),
            ended(0, b"x"),
            Duration::from_millis(300),
        );
        assert!(began.elapsed() < Duration::from_secs(20), "waited for it");
        assert_eq!(reply.outcome, Outcome::Done);
        let sentence = t!(
            schedule_delivery_timed_out,
            id = "1",
            path = bravebot_approval::printable(&program.display().to_string()),
            seconds = 0
        )
        .to_string();
        assert_eq!(reply.notice, Some(sentence));
        assert_eq!(
            std::fs::read_to_string(store.path().join("1.history")).expect("history"),
            "100 0 -\n"
        );
        std::thread::sleep(Duration::from_secs(3));
        assert!(!late.exists(), "a program the destination started lived on");
        // The next firing can take the hold.
        assert!(kept_beside(store.path()).hold("1").expect("held").is_some());
    }

    /// SCHEDULE-7: the destination is started in the person's home and not in the directory the
    /// entry runs in. A wrong implementation that started it in the entry's directory or the
    /// directory of the calling process would change the directory it reports.
    #[cfg(unix)]
    #[test]
    fn the_destination_starts_outside_the_entry_directory() {
        let directory = tempfile::tempdir().expect("a directory");
        let at = directory.path().join("at");
        let program = stand_in(
            directory.path(),
            "where",
            &format!("/bin/cat > /dev/null\n/bin/pwd -P > '{}'", at.display()),
        );
        let store = tempfile::tempdir().expect("a directory");
        let reply = concluded(
            store.path(),
            Some(&program),
            ended(0, b"x"),
            Duration::from_secs(30),
        );
        assert_eq!(reply.notice, None);
        let home = std::fs::canonicalize(
            bravebot_agent::home::profile().unwrap_or_else(std::env::temp_dir),
        )
        .expect("canonical");
        let there = std::fs::read_to_string(&at).expect("ran");
        assert_eq!(there.trim_end(), home.display().to_string());
    }
}
