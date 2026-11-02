//! `bravebot schedule add|list|remove|run-now|history`: recurring one-shot runs started by the
//! operating system's `crontab` (CLI-42).
//!
//! An entry is written by a person at a terminal and holds a cron expression, a prompt, a directory
//! and the permission mode the person typed. It is kept as two files under the state directory, the
//! prompt alone in one so that no character in it needs escaping, and installed as one line of the
//! person's crontab that runs `bravebot schedule run-now <id>`. That command starts this program's
//! own one-shot `-p` path in the entry's directory. Nothing a run reads is ever written to an entry,
//! and the driver reads the child's result only for the id of the session it recorded.

use crate::exit::{Ending, fail};
use bravebot_i18n::t;
use bravebot_session::{search, sessions};
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// A result that is not read: the file or the stream was best-effort, and a failure to write it
/// is not a reason to refuse what the person asked for.
#[expect(
    clippy::let_underscore_must_use,
    clippy::needless_pass_by_value,
    reason = "the one place a best-effort write or removal is dropped"
)]
fn ignored<T, E>(result: Result<T, E>) {
    let _ = result;
}

/// The longest prompt printed in a listing, in characters.
const LONGEST_PROMPT: usize = 80;

/// The longest prompt an entry may hold, in bytes.
const LONGEST_PROMPT_BYTES: usize = 64 * 1024;

/// What a run of this entry may do, as the person typed it when writing the entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
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
    fn flag(self) -> Option<&'static str> {
        match self {
            Self::Read => None,
            Self::AcceptEdits => Some(crate::flags::ACCEPT_EDITS),
            Self::Bypass => Some(crate::flags::SKIP_PERMISSIONS),
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

/// One entry, as read back from the state directory.
#[derive(Debug, PartialEq, Eq)]
struct Entry {
    id: String,
    cron: String,
    directory: PathBuf,
    mode: Mode,
    prompt: String,
}

/// What `schedule add` was given, once the words are read.
#[derive(Debug, PartialEq, Eq)]
struct Addition {
    cron: String,
    prompt: String,
    directory: Option<PathBuf>,
    mode: Mode,
}

/// The permission flags the command line carried. `main` takes them out of every command line
/// before dispatching, so `add` reads them here and every other word refuses them.
pub(crate) struct Typed {
    /// The flag that answers every question, as it was spelled, where one was typed.
    pub(crate) bypass: Option<&'static str>,
    pub(crate) starts_in: Option<bravebot_agent::PermissionMode>,
}

impl Typed {
    /// The mode an entry holds, or `None` where the flags cannot be one: `--auto` needs a
    /// classifier a recurring run has no way to be watched through, and the combined flag also
    /// switches off the sandbox, which the entry has no way to say.
    fn mode(&self) -> Option<Mode> {
        match (self.bypass, self.starts_in) {
            (None, None) => Some(Mode::Read),
            (None, Some(bravebot_agent::PermissionMode::AcceptEdits)) => Some(Mode::AcceptEdits),
            (Some(flag), None) if flag == crate::flags::SKIP_PERMISSIONS => Some(Mode::Bypass),
            _ => None,
        }
    }
}

/// `bravebot schedule ...`, after `schedule`.
pub(crate) fn command(args: &[String], typed: &Typed) -> ExitCode {
    match args {
        [word, rest @ ..] if word == "add" => add(rest, typed),
        _ if typed.bypass.is_some() || typed.starts_in.is_some() => {
            fail(Ending::Argument, t!(schedule_usage))
        }
        [word] if word == "list" => list(),
        [word, id] if word == "remove" => remove(id),
        [word, id] if word == "run-now" => run_now(id),
        [word] if word == "history" => history(None),
        [word, id] if word == "history" => history(Some(id)),
        _ => fail(Ending::Argument, t!(schedule_usage)),
    }
}

/// `--cron <expression> --prompt <text> [--dir <path>]`, in any order. A repeated flag, a missing
/// value and any other word are refused rather than guessed at.
fn read_addition(words: &[String], mode: Mode) -> Option<Addition> {
    let mut cron = None;
    let mut prompt = None;
    let mut directory = None;
    let mut rest = words.iter();
    while let Some(word) = rest.next() {
        match word.as_str() {
            "--cron" if cron.is_none() => cron = Some(rest.next()?.clone()),
            "--prompt" if prompt.is_none() => prompt = Some(rest.next()?.clone()),
            "--dir" if directory.is_none() => directory = Some(PathBuf::from(rest.next()?)),
            _ => return None,
        }
    }
    Some(Addition {
        cron: cron?,
        prompt: prompt?,
        directory,
        mode,
    })
}

/// The expression with its fields separated by one space, or `None` where it is not five fields
/// of digits and `* / , -`.
///
/// Held to that alphabet because the expression is written into a crontab line: nothing else it
/// could hold would be an instruction to the shell, and a name such as `mon` or a `@daily` is
/// not read.
fn checked_cron(expression: &str) -> Option<String> {
    let fields: Vec<&str> = expression.split_whitespace().collect();
    if fields.len() != 5 {
        return None;
    }
    let plain = |field: &str| {
        !field.is_empty()
            && field
                .chars()
                .all(|c| c.is_ascii_digit() || matches!(c, '*' | '/' | ',' | '-'))
    };
    fields
        .iter()
        .all(|field| plain(field))
        .then(|| fields.join(" "))
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

/// What an entry's file holds.
fn entry_text(cron: &str, directory: &Path, mode: Mode) -> Option<String> {
    let directory = &*directory.to_string_lossy();
    if directory.contains('\n') {
        return None;
    }
    Some(format!(
        "cron={cron}\ndir={directory}\nmode={}\n",
        mode.word()
    ))
}

/// An entry read back from `text` and the prompt file's contents.
fn read_entry(id: &str, text: &str, prompt: String) -> Option<Entry> {
    let (mut cron, mut directory, mut mode) = (None, None, None);
    for line in text.lines() {
        let (key, value) = line.split_once('=')?;
        match key {
            "cron" => cron = checked_cron(value),
            "dir" => directory = Some(PathBuf::from(value)),
            "mode" => mode = Mode::named(value),
            _ => {}
        }
    }
    Some(Entry {
        id: id.to_string(),
        cron: cron?,
        directory: directory?,
        mode: mode?,
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

/// The `crontab` program, started without this agent's credentials.
#[cfg(unix)]
fn crontab() -> Option<std::process::Command> {
    let found = bravebot_sandbox::programs::find(std::ffi::OsStr::new("crontab"))?;
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
    let output = crontab()?
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
    let Some(mut command) = crontab() else {
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

/// `bravebot schedule add ...`, after `add`.
fn add(words: &[String], typed: &Typed) -> ExitCode {
    let Some(request) = typed.mode().and_then(|mode| read_addition(words, mode)) else {
        return fail(Ending::Argument, t!(schedule_add_usage));
    };
    // A person at a terminal wrote this entry. A run's tool has no terminal, so nothing a turn
    // runs can add one (CLI-42).
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return fail(Ending::Argument, t!(schedule_add_needs_a_terminal));
    }
    let Some(cron) = checked_cron(&request.cron) else {
        return fail(Ending::Argument, t!(schedule_cron_invalid));
    };
    if !usable_prompt(&request.prompt) {
        return fail(Ending::Argument, t!(schedule_prompt_invalid));
    }
    let mode = request.mode;
    let directory = match crate::background::project_directory(request.directory) {
        Ok(path) => path,
        Err(code) => return code,
    };
    let Some(text) = entry_text(&cron, &directory, request.mode) else {
        return fail(Ending::Argument, t!(schedule_directory_invalid));
    };
    let Ok(me) = bravebot_sandbox::programs::Resolved::own_executable() else {
        return fail(Ending::Failed, t!(schedule_failed));
    };
    let Some(store) = store() else {
        return fail(Ending::Failed, t!(schedule_no_store));
    };
    let id = next_id(&store);
    let Some(line) = me
        .as_path()
        .to_str()
        .and_then(|program| crontab_line(&cron, program, &id))
    else {
        return fail(Ending::Failed, t!(schedule_failed));
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
        return fail(Ending::Failed, t!(schedule_failed));
    }
    if !install(&id, Some(&line)) {
        forget(&store, &id);
        return fail(Ending::Failed, t!(schedule_install_failed));
    }
    println!("{}", t!(schedule_added, id = id.as_str()));
    println!("{}", mode.limit());
    ExitCode::SUCCESS
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
}

/// `bravebot schedule list`.
fn list() -> ExitCode {
    let entries = entries();
    if entries.is_empty() {
        println!("{}", t!(schedule_none));
        return ExitCode::SUCCESS;
    }
    let mut out = std::io::stdout().lock();
    for entry in &entries {
        ignored(writeln!(
            out,
            "{}",
            t!(
                schedule_row,
                id = entry.id.as_str(),
                cron = entry.cron.as_str(),
                directory = crate::progress::printable_path(&entry.directory),
                limit = entry.mode.limit(),
                prompt = search::clean(&entry.prompt, LONGEST_PROMPT)
            )
        ));
    }
    ExitCode::SUCCESS
}

/// `bravebot schedule remove <id>`.
fn remove(id: &str) -> ExitCode {
    let Some(store) = store_to_read() else {
        return fail(Ending::Argument, t!(schedule_missing, id = id));
    };
    if entry_named(&store, id).is_none() {
        return fail(Ending::Argument, t!(schedule_missing, id = id));
    }
    if !install(id, None) {
        return fail(Ending::Failed, t!(schedule_install_failed));
    }
    forget(&store, id);
    println!("{}", t!(schedule_removed, id = id));
    ExitCode::SUCCESS
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

/// `bravebot schedule run-now <id>`: the entry's one-shot run, started now, and recorded.
fn run_now(id: &str) -> ExitCode {
    let Some(store) = store_to_read() else {
        return fail(Ending::Argument, t!(schedule_missing, id = id));
    };
    let Some(entry) = entry_named(&store, id) else {
        return fail(Ending::Argument, t!(schedule_missing, id = id));
    };
    let Ok(me) = bravebot_sandbox::programs::Resolved::own_executable() else {
        return fail(Ending::Failed, t!(schedule_failed));
    };
    // The environment is this process's own, as a person's shell would give it, for the reason
    // `sessions` gives for starting its own executable again.
    let mut command = bravebot_sandbox::programs::start(
        &me,
        bravebot_sandbox::programs::Inherits::All,
        bravebot_sandbox::programs::SearchPath::Inherited,
    );
    command.arg("--json");
    command.args(entry.mode.flag());
    command
        .arg("-p")
        .arg(&entry.prompt)
        .current_dir(&entry.directory)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let began = sessions::now();
    let Ok(output) = command.output() else {
        return fail(Ending::Failed, t!(schedule_failed));
    };
    let code = output.status.code().unwrap_or(-1);
    let session = std::str::from_utf8(&output.stdout)
        .ok()
        .and_then(|text| text.lines().last().and_then(session_of));
    record(&store, id, began, code, session.as_deref());
    println!(
        "{}",
        t!(
            schedule_ran,
            id = id,
            status = code,
            session = session.as_deref().unwrap_or("-")
        )
    );
    if code == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
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
fn history(only: Option<&String>) -> ExitCode {
    let Some(store) = store_to_read() else {
        println!("{}", t!(schedule_history_none));
        return ExitCode::SUCCESS;
    };
    let wanted: Vec<Entry> = entries()
        .into_iter()
        .filter(|entry| only.is_none_or(|id| &entry.id == id))
        .collect();
    if let Some(id) = only
        && wanted.is_empty()
    {
        return fail(Ending::Argument, t!(schedule_missing, id = id.as_str()));
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
        println!("{}", t!(schedule_history_none));
        return ExitCode::SUCCESS;
    }
    runs.sort_by_key(|run| std::cmp::Reverse(run.0));
    let mut out = std::io::stdout().lock();
    for (when, id, code, session, tokens) in runs {
        ignored(writeln!(
            out,
            "{}",
            t!(
                schedule_history_row,
                id = id.as_str(),
                when = sessions::how_long_ago(when),
                status = code,
                session = session.as_deref().unwrap_or("-"),
                tokens = tokens.map_or_else(|| "-".to_string(), |n| n.to_string())
            )
        ));
    }
    ExitCode::SUCCESS
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
    fn a_prompt_that_would_be_read_as_a_flag_or_is_empty_is_not_usable() {
        assert!(usable_prompt("summarize merged pull requests"));
        assert!(!usable_prompt("   "));
        assert!(!usable_prompt("--dangerously-skip-permissions"));
        assert!(!usable_prompt(&"x".repeat(LONGEST_PROMPT_BYTES + 1)));
    }

    #[test]
    fn the_mode_is_read_only_unless_a_flag_was_typed() {
        use bravebot_agent::PermissionMode;
        let typed = |bypass, starts_in| Typed { bypass, starts_in }.mode();
        assert_eq!(typed(None, None), Some(Mode::Read));
        assert_eq!(
            typed(None, Some(PermissionMode::AcceptEdits)),
            Some(Mode::AcceptEdits)
        );
        assert_eq!(
            typed(Some(crate::flags::SKIP_PERMISSIONS), None),
            Some(Mode::Bypass)
        );
        // The combined flag unconfines the sandbox as well, which an entry cannot say, and the
        // auto mode has no classifier to answer a run nobody starts by hand.
        assert_eq!(
            typed(Some(crate::flags::BYPASS_APPROVALS_AND_SANDBOX), None),
            None
        );
        assert_eq!(typed(None, Some(PermissionMode::Auto)), None);
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
    }

    #[test]
    fn an_entry_reads_back_as_it_was_written() {
        let text = entry_text("0 9 * * 1-5", Path::new("/work/here"), Mode::AcceptEdits)
            .expect("a plain path");
        let entry = read_entry("3", &text, "do it".into()).expect("reads");
        assert_eq!(
            entry,
            Entry {
                id: "3".into(),
                cron: "0 9 * * 1-5".into(),
                directory: PathBuf::from("/work/here"),
                mode: Mode::AcceptEdits,
                prompt: "do it".into(),
            }
        );
        assert_eq!(entry_text("0 9 * * *", Path::new("a\nb"), Mode::Read), None);
        // A mode the program did not write reads as no entry, and never as read-only.
        assert!(read_entry("3", "cron=0 9 * * *\ndir=/x\nmode=root\n", String::new()).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn a_directory_that_is_not_utf8_is_refused_and_not_written_as_its_lookalike() {
        use std::os::unix::ffi::OsStrExt;
        let directory = Path::new(std::ffi::OsStr::from_bytes(b"/work/\xff"));
        assert_eq!(entry_text("0 9 * * *", directory, Mode::Read), None);
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
}
