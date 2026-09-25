//! `bravebot mcp`: declaring an MCP server, and approving a declaration (SERVERS-3).
//!
//! What this writes is the two files [`bravebot_config::mcp`] reads, and the one question it asks
//! is whether the person approves a declaration they were just shown. Nothing here starts a server
//! or offers one to a session.
//!
//! Typing `add` is not the approval. The line a person typed says what to run; the answer to the
//! question says they read what it resolved to, and only that answer is recorded. Where nobody can
//! be asked, the declaration is written and left unapproved, which is LAYER-1's rule for this crate:
//! an effect nobody could be asked about is refused rather than applied unseen.

use crate::exit::{Ending, fail};
use bravebot_config::mcp::{
    self, Approvals, Declaration, Declarations, Entry, Field, Problem, Unreadable,
};
use bravebot_i18n::t;
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Where the files are, and whether this run may write them.
struct Home {
    /// The state directory, or `None` where the platform names no profile directory.
    directory: Option<PathBuf>,
    /// Whether anything may be written into it, which an incognito session answers no.
    writable: bool,
}

/// The other end of the question: where an answer is read, where things are shown, and whether
/// anybody is there to give one.
struct Person<R, W> {
    answers: R,
    screen: W,
    present: bool,
}

/// How a command ended, where it did not end done.
type Stopped = (Ending, String);

/// Run `bravebot mcp <command>`.
pub fn command(args: &[String]) -> ExitCode {
    let home = Home {
        directory: bravebot_agent::home::directory(),
        writable: bravebot_agent::home::writable().is_some(),
    };
    // Both ends, for the reason a one-shot run's plan asks for both: a question written into a
    // redirected stdout is one nobody read, and a pipe on stdin would be answering for them.
    let present = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let mut person = Person {
        answers: std::io::stdin().lock(),
        screen: std::io::stdout().lock(),
        present,
    };
    match run(args, &home, &mut person) {
        Ok(()) => ExitCode::SUCCESS,
        Err((ending, message)) => fail(ending, message),
    }
}

fn run<R: BufRead, W: Write>(
    args: &[String],
    home: &Home,
    person: &mut Person<R, W>,
) -> Result<(), Stopped> {
    let Some((command, rest)) = args.split_first() else {
        return Err(refused_with_the_forms(t!(mcp_needs_a_command).to_string()));
    };
    match command.as_str() {
        "add" => add(rest, home, person),
        "get" => get(one_alias(command, rest)?, home, person),
        "list" => match rest.first() {
            None => list(home, person),
            Some(extra) => Err(unexpected(command, extra)),
        },
        "approve" => approve(one_alias(command, rest)?, home, person),
        "remove" => remove(one_alias(command, rest)?, home, person),
        other => Err(refused_with_the_forms(
            t!(mcp_unknown_command, command = shown(other)).to_string(),
        )),
    }
}

/// A refusal of the command itself, with every form it could have taken under it.
fn refused_with_the_forms(message: String) -> Stopped {
    let mut said = message;
    said.push('\n');
    said.push_str(t!(mcp_forms_heading));
    for form in [
        "bravebot mcp add <alias> [--env <name>]... [--dir <path>] --stdio -- <program> [args...]",
        "bravebot mcp add <alias> --http <url>",
        "bravebot mcp get <alias>",
        "bravebot mcp list",
        "bravebot mcp approve <alias>",
        "bravebot mcp remove <alias>",
    ] {
        said.push_str("\n  ");
        said.push_str(form);
    }
    (Ending::Argument, said)
}

fn unexpected(command: &str, argument: &str) -> Stopped {
    (
        Ending::Argument,
        t!(
            mcp_unexpected_argument,
            command = command,
            argument = shown(argument)
        )
        .to_string(),
    )
}

/// The one alias `get`, `approve` and `remove` take.
///
/// Not checked against [`mcp::is_alias`]: an entry in the file under an alias that is not one is
/// still an entry somebody has to be able to read and remove, and it is found by what it is spelled.
fn one_alias<'a>(command: &str, rest: &'a [String]) -> Result<&'a str, Stopped> {
    match rest {
        [alias] => Ok(alias),
        [] => Err((
            Ending::Argument,
            t!(mcp_needs_an_alias, command = command).to_string(),
        )),
        [_, extra, ..] => Err(unexpected(command, extra)),
    }
}

fn add<R: BufRead, W: Write>(
    rest: &[String],
    home: &Home,
    person: &mut Person<R, W>,
) -> Result<(), Stopped> {
    let Some((alias, flags)) = rest.split_first() else {
        return Err((
            Ending::Argument,
            t!(mcp_needs_an_alias, command = "add").to_string(),
        ));
    };
    if !mcp::is_alias(alias) {
        return Err((
            Ending::Argument,
            t!(mcp_not_an_alias, alias = shown(alias)).to_string(),
        ));
    }
    let declaration = declared(flags).map_err(|refusal| match refusal {
        Refusal::Said(stopped) => stopped,
        Refusal::Problem(found) => (
            Ending::Argument,
            t!(mcp_not_added, alias = alias, problem = problem(&found)).to_string(),
        ),
    })?;

    let directory = writable(home)?;
    let mut declarations = read(directory)?;
    let before = declarations
        .get(alias)
        .and_then(|entry| entry.declaration.ok());
    declarations.insert(alias, &declaration);
    let mut approvals = Approvals::read(directory);
    save(directory, &declarations, &mut approvals)?;
    let path = mcp::declarations_file(directory);
    say(
        person,
        t!(
            mcp_declared,
            alias = alias,
            path = path.display().to_string()
        ),
    );

    let changed = before
        .map(|before| before.changes(&declaration))
        .unwrap_or_default();
    match ask(alias, &declaration, &changed, &approvals, person) {
        Asked::Already => say(person, already(alias, &declaration)),
        Asked::Yes => {
            record(directory, &declarations, &mut approvals, &declaration)?;
            say(person, recorded(alias, &declaration));
        }
        Asked::No => say(person, t!(mcp_left_unapproved, alias = alias)),
        Asked::Nobody => say(person, t!(mcp_nobody_asked, alias = alias)),
    }
    Ok(())
}

/// Why a command line did not make a declaration.
enum Refusal {
    /// The flags themselves were wrong, and this is what to say.
    Said(Stopped),
    /// The flags were read, and what they spelled is not a declaration.
    Problem(Problem),
}

impl From<Stopped> for Refusal {
    fn from(stopped: Stopped) -> Self {
        Self::Said(stopped)
    }
}

/// The declaration `add`'s flags spell, checked exactly as an entry in the file is.
///
/// Everything after `--stdio --` is the program and its arguments, as words and never as a line,
/// so a flag of this command written after it is an argument of the server's.
fn declared(flags: &[String]) -> Result<Declaration, Refusal> {
    let mut variables = Vec::new();
    let mut directory = None;
    let mut transport = None;
    let mut index = 0;
    while index < flags.len() {
        let flag = flags[index].as_str();
        let value = flags.get(index + 1);
        match flag {
            "--env" => {
                let name = value.ok_or_else(|| argument(t!(mcp_env_needs_a_name)))?;
                variables.push(name.clone());
            }
            "--dir" => {
                let path = value.ok_or_else(|| argument(t!(mcp_dir_needs_a_path)))?;
                directory = Some(place(path)?);
            }
            "--http" => {
                let url = value.ok_or_else(|| argument(t!(mcp_http_needs_a_url)))?;
                if transport.is_some() {
                    return Err(argument(t!(mcp_two_transports)).into());
                }
                transport = Some(Transport::Http(url.clone()));
            }
            "--stdio" => {
                if transport.is_some() {
                    return Err(argument(t!(mcp_two_transports)).into());
                }
                if value.map(String::as_str) != Some("--") {
                    return Err(argument(t!(mcp_stdio_needs_a_program)).into());
                }
                let argv = flags[index + 2..].to_vec();
                if argv.is_empty() {
                    return Err(argument(t!(mcp_stdio_needs_a_program)).into());
                }
                transport = Some(Transport::Stdio(argv));
                break;
            }
            other if other.starts_with('-') => {
                return Err(argument(t!(cli_unknown_option, flag = shown(other))).into());
            }
            other => return Err(unexpected("add", other).into()),
        }
        index += 2;
    }
    match transport {
        None => Err(argument(t!(mcp_needs_a_transport)).into()),
        Some(Transport::Stdio(argv)) => {
            Declaration::stdio(argv, variables, directory).map_err(Refusal::Problem)
        }
        Some(Transport::Http(_)) if !variables.is_empty() => {
            Err(Refusal::Problem(Problem::Remote("variables")))
        }
        Some(Transport::Http(_)) if directory.is_some() => {
            Err(Refusal::Problem(Problem::Remote("directory")))
        }
        Some(Transport::Http(url)) => Declaration::http(url).map_err(Refusal::Problem),
    }
}

/// Which transport the flags named, before the rest of the declaration is checked.
enum Transport {
    Stdio(Vec<String>),
    Http(String),
}

fn argument(message: impl std::fmt::Display) -> Stopped {
    (Ending::Argument, message.to_string())
}

/// The directory `--dir` named, as the absolute path of the place it is.
///
/// Resolved here rather than written as typed, so the declaration and its digest name a place and
/// not a spelling that means somewhere else from the next directory a session starts in.
fn place(typed: &str) -> Result<String, Stopped> {
    let resolved = std::fs::canonicalize(typed)
        .ok()
        .filter(|path| path.is_dir())
        .ok_or_else(|| argument(t!(mcp_dir_not_a_directory, path = shown(typed))))?;
    resolved
        .into_os_string()
        .into_string()
        .map_err(|_| argument(t!(mcp_dir_not_text, path = shown(typed))))
}

fn get<R: BufRead, W: Write>(
    alias: &str,
    home: &Home,
    person: &mut Person<R, W>,
) -> Result<(), Stopped> {
    let (directory, declarations) = readable(home)?;
    let (Some(directory), Some(entry)) = (directory, declarations.get(alias)) else {
        return Err(not_declared(alias));
    };
    let declaration = entry.declaration.map_err(|found| {
        (
            Ending::Configuration,
            t!(
                mcp_unusable,
                alias = shown(alias),
                problem = problem(&found)
            )
            .to_string(),
        )
    })?;
    let path = mcp::declarations_file(directory);
    let digest = declaration.digest();
    say(
        person,
        t!(mcp_list_declared_in, path = path.display().to_string()),
    );
    for line in drawn(alias, &declaration, &digest.to_string()) {
        say(person, line);
    }
    let indent = indent(alias);
    match Approvals::read(directory).approves(&digest) {
        true => say(person, format!("{indent}{}", t!(mcp_approved))),
        false => say(
            person,
            format!("{indent}{}", t!(mcp_unapproved_run_approve, alias = alias)),
        ),
    }
    Ok(())
}

/// SERVERS-14's list half: every declared alias, its transport, and whether it is approved.
///
/// An entry that cannot be used is listed with what is wrong with it rather than left out, since a
/// list that dropped it would make a declaration somebody wrote look like one nobody read.
fn list<R: BufRead, W: Write>(home: &Home, person: &mut Person<R, W>) -> Result<(), Stopped> {
    let (directory, declarations) = readable(home)?;
    let Some(directory) = directory else {
        say(person, no_state_directory());
        return Ok(());
    };
    let path = mcp::declarations_file(directory).display().to_string();
    let entries = declarations.entries();
    if entries.is_empty() {
        say(person, t!(mcp_none_declared, path = &path));
        return Ok(());
    }
    say(person, t!(mcp_list_declared_in, path = &path));
    let approvals = Approvals::read(directory);
    let approved = t!(mcp_approved).to_string();
    let unapproved = t!(mcp_unapproved).to_string();
    let state = approved.chars().count().max(unapproved.chars().count());
    let width = entries
        .iter()
        .map(|entry| shown(&entry.alias).chars().count())
        .max()
        .unwrap_or_default();
    let mut unusable = 0usize;
    for Entry { alias, declaration } in &entries {
        let alias = pad(&shown(alias), width);
        match declaration {
            Ok(declaration) => {
                let digest = declaration.digest();
                let word = match approvals.approves(&digest) {
                    true => &approved,
                    false => &unapproved,
                };
                say(
                    person,
                    format!(
                        "  {alias}  {}  {}  {}",
                        pad(declaration.transport(), 5),
                        pad(word, state),
                        digest.short()
                    ),
                );
            }
            Err(found) => {
                unusable += 1;
                say(
                    person,
                    format!(
                        "  {alias}  {}",
                        t!(mcp_cannot_be_used, problem = problem(found))
                    ),
                );
            }
        }
    }
    match unusable {
        0 => Ok(()),
        count => Err((
            Ending::Configuration,
            t!(mcp_list_unusable, count = count, path = &path).to_string(),
        )),
    }
}

fn approve<R: BufRead, W: Write>(
    alias: &str,
    home: &Home,
    person: &mut Person<R, W>,
) -> Result<(), Stopped> {
    let directory = writable(home)?;
    let declarations = read(directory)?;
    let entry = declarations.get(alias).ok_or_else(|| not_declared(alias))?;
    let declaration = entry.declaration.map_err(|found| {
        (
            Ending::Configuration,
            t!(
                mcp_unusable,
                alias = shown(alias),
                problem = problem(&found)
            )
            .to_string(),
        )
    })?;
    let mut approvals = Approvals::read(directory);
    match ask(alias, &declaration, &[], &approvals, person) {
        Asked::Already => say(person, already(alias, &declaration)),
        Asked::Yes => {
            record(directory, &declarations, &mut approvals, &declaration)?;
            say(person, recorded(alias, &declaration));
        }
        Asked::No => {
            return Err((
                Ending::Refused,
                t!(mcp_left_unapproved, alias = alias).to_string(),
            ));
        }
        Asked::Nobody => {
            return Err((
                Ending::Refused,
                t!(mcp_nobody_to_ask, alias = alias).to_string(),
            ));
        }
    }
    Ok(())
}

fn remove<R: BufRead, W: Write>(
    alias: &str,
    home: &Home,
    person: &mut Person<R, W>,
) -> Result<(), Stopped> {
    let directory = writable(home)?;
    let mut declarations = read(directory)?;
    if !declarations.remove(alias) {
        return Err(not_declared(alias));
    }
    let mut approvals = Approvals::read(directory);
    save(directory, &declarations, &mut approvals)?;
    say(person, t!(mcp_removed, alias = shown(alias)));
    Ok(())
}

/// What became of the question.
enum Asked {
    /// This digest was approved before, so nothing was asked.
    Already,
    Yes,
    No,
    /// Nobody was there to ask, so nothing was.
    Nobody,
}

/// SERVERS-3's question, put about a declaration that is already written.
///
/// Only a yes approves. Any other line, and the end of the input, is a no.
fn ask<R: BufRead, W: Write>(
    alias: &str,
    declaration: &Declaration,
    changed: &[Field],
    approvals: &Approvals,
    person: &mut Person<R, W>,
) -> Asked {
    let digest = declaration.digest();
    if approvals.approves(&digest) {
        return Asked::Already;
    }
    if !person.present {
        return Asked::Nobody;
    }
    say(person, "");
    for line in drawn(alias, declaration, &digest.short()) {
        say(person, line);
    }
    if !changed.is_empty() {
        let fields: Vec<&str> = changed.iter().map(|field| field.key()).collect();
        say(
            person,
            format!(
                "{}{}",
                indent(alias),
                t!(mcp_changed, fields = fields.join(", "))
            ),
        );
    }
    say(person, "");
    let _ = write!(person.screen, "  {} {} ", t!(mcp_question), t!(line_answer));
    let _ = person.screen.flush();
    let mut typed = String::new();
    match person.answers.read_line(&mut typed) {
        Ok(0) | Err(_) => Asked::No,
        Ok(_) if typed.trim().to_lowercase() == t!(line_answer_yes) => Asked::Yes,
        Ok(_) => Asked::No,
    }
}

/// Record the approval of `declaration`'s digest, and of nothing else: the alias is a label and is
/// not what was approved.
fn record(
    directory: &Path,
    declarations: &Declarations,
    approvals: &mut Approvals,
    declaration: &Declaration,
) -> Result<(), Stopped> {
    approvals.approve(declaration.digest());
    approvals.keep_only(&declarations.digests());
    replace(&mcp::approvals_file(directory), &approvals.to_text())
}

/// A declaration as a person reads it: the alias, the transport and what it runs or reaches on the
/// first line, and under it the names it receives, where it runs, and the digest.
///
/// Every argument is shown as the word it is, quoted where it holds a space or anything a terminal
/// would not draw as itself, so `a b` and `"a b"` are told apart on the screen as they are in argv.
fn drawn(alias: &str, declaration: &Declaration, digest: &str) -> Vec<String> {
    let what = match declaration {
        Declaration::Stdio { argv, .. } => argv
            .iter()
            .map(|word| shown(word))
            .collect::<Vec<_>>()
            .join(" "),
        Declaration::Http { url } => shown(url),
    };
    let indent = indent(alias);
    let mut lines = vec![format!(
        "  {}   {}   {what}",
        shown(alias),
        declaration.transport()
    )];
    if !declaration.variables().is_empty() {
        lines.push(format!(
            "{indent}{}",
            t!(mcp_variables, names = declaration.variables().join(", "))
        ));
    }
    if let Declaration::Stdio {
        directory: Some(directory),
        ..
    } = declaration
    {
        lines.push(format!(
            "{indent}{}",
            t!(mcp_directory, path = shown(directory))
        ));
    }
    lines.push(format!("{indent}{}", t!(mcp_digest, digest = digest)));
    lines
}

/// The margin the lines under a declaration's first one start at.
fn indent(alias: &str) -> String {
    " ".repeat(2 + shown(alias).chars().count() + 3)
}

/// `text` padded to `width` characters, counted as characters for the reason `doctor`'s columns are.
fn pad(text: &str, width: usize) -> String {
    let gap = width.saturating_sub(text.chars().count());
    format!("{text}{}", " ".repeat(gap))
}

/// A word as it is safe to draw: as it is where every character is one a terminal draws as itself
/// and none would blur where the word ends, and quoted with its escapes otherwise.
pub(crate) fn shown(word: &str) -> String {
    let plain = !word.is_empty()
        && word.chars().all(|c| match c.is_ascii() {
            true => c.is_ascii_graphic() && !matches!(c, '"' | '\'' | '\\'),
            false => c.is_alphanumeric(),
        });
    match plain {
        true => word.to_string(),
        false => format!("{word:?}"),
    }
}

fn already(alias: &str, declaration: &Declaration) -> String {
    t!(
        mcp_already_approved,
        alias = alias,
        digest = declaration.digest().short()
    )
    .to_string()
}

fn recorded(alias: &str, declaration: &Declaration) -> String {
    t!(
        mcp_recorded,
        alias = alias,
        digest = declaration.digest().short()
    )
    .to_string()
}

fn not_declared(alias: &str) -> Stopped {
    (
        Ending::Argument,
        t!(mcp_not_declared, alias = shown(alias)).to_string(),
    )
}

fn say<R, W: Write>(person: &mut Person<R, W>, line: impl std::fmt::Display) {
    let _ = writeln!(person.screen, "{line}");
}

fn no_state_directory() -> String {
    t!(
        mcp_no_state_directory,
        variables = bravebot_agent::home::PROFILE_VARIABLES.join(" or ")
    )
    .to_string()
}

/// The state directory, where this run may write to it.
fn writable(home: &Home) -> Result<&Path, Stopped> {
    match (&home.directory, home.writable) {
        (Some(directory), true) => Ok(directory),
        (Some(_), false) => Err((Ending::Failed, t!(mcp_not_while_incognito).to_string())),
        (None, _) => Err((Ending::Configuration, no_state_directory())),
    }
}

/// The state directory and what it declares. A machine with none declares nothing.
fn readable(home: &Home) -> Result<(Option<&Path>, Declarations), Stopped> {
    match &home.directory {
        Some(directory) => Ok((Some(directory), read(directory)?)),
        None => Ok((None, Declarations::default())),
    }
}

fn read(directory: &Path) -> Result<Declarations, Stopped> {
    Declarations::read(directory).map_err(|why| {
        let path = mcp::declarations_file(directory).display().to_string();
        (
            Ending::Configuration,
            t!(mcp_unreadable, path = path, reason = unreadable(&why)).to_string(),
        )
    })
}

/// Write the declarations, and then the approvals with every digest no declaration resolves to
/// dropped, so the two files agree about what is approved (SERVERS-5).
fn save(
    directory: &Path,
    declarations: &Declarations,
    approvals: &mut Approvals,
) -> Result<(), Stopped> {
    bravebot_agent::home::create_directory(directory).map_err(|error| {
        (
            Ending::Failed,
            t!(
                mcp_not_written,
                path = directory.display().to_string(),
                error = error.to_string()
            )
            .to_string(),
        )
    })?;
    replace(&mcp::declarations_file(directory), &declarations.to_text())?;
    approvals.keep_only(&declarations.digests());
    replace(&mcp::approvals_file(directory), &approvals.to_text())
}

/// Write `text` over `path` through a temporary file beside it, so an interrupted write leaves the
/// file as it was rather than half of it.
fn replace(path: &Path, text: &str) -> Result<(), Stopped> {
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    bravebot_agent::home::write_file(&temporary, text.as_bytes())
        .and_then(|()| std::fs::rename(&temporary, path))
        .map_err(|error| {
            (
                Ending::Failed,
                t!(
                    mcp_not_written,
                    path = path.display().to_string(),
                    error = error.to_string()
                )
                .to_string(),
            )
        })
}

fn problem(found: &Problem) -> String {
    match found {
        Problem::Alias => t!(mcp_problem_alias).to_string(),
        Problem::NotAnObject => t!(mcp_problem_not_an_object).to_string(),
        Problem::Transport => t!(mcp_problem_transport).to_string(),
        Problem::Key(key) => t!(mcp_problem_key, key = shown(key)).to_string(),
        Problem::Values => t!(mcp_problem_values).to_string(),
        Problem::Program => t!(mcp_problem_program).to_string(),
        Problem::Name => t!(mcp_problem_name).to_string(),
        Problem::Assignment(name) => t!(mcp_problem_assignment, name = name).to_string(),
        Problem::Directory => t!(mcp_problem_directory).to_string(),
        Problem::Url => t!(mcp_problem_url).to_string(),
        Problem::Credentials => t!(mcp_problem_credentials).to_string(),
        Problem::Remote(key) => t!(mcp_problem_remote, key = *key).to_string(),
    }
}

fn unreadable(why: &Unreadable) -> String {
    match why {
        Unreadable::TooLarge => t!(mcp_unreadable_too_large).to_string(),
        Unreadable::NotRead => t!(mcp_unreadable_not_read).to_string(),
        Unreadable::NotJson => t!(mcp_unreadable_not_json).to_string(),
        Unreadable::NotAnObject => t!(mcp_unreadable_not_an_object).to_string(),
        Unreadable::Servers => t!(mcp_unreadable_servers).to_string(),
        Unreadable::Key(key) => t!(mcp_unreadable_key, key = shown(key)).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approved(directory: &Path, declaration: &Declaration) -> bool {
        Approvals::read(directory).approves(&declaration.digest())
    }

    /// A state directory of its own under the build directory, emptied first.
    fn scratch(name: &str) -> PathBuf {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch");
        path
    }

    fn words(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| word.to_string()).collect()
    }

    fn weather() -> Declaration {
        Declaration::stdio(words(&["npx", "-y", "weather-mcp"]), Vec::new(), None).unwrap()
    }

    const ADD: &[&str] = &[
        "add",
        "weather",
        "--stdio",
        "--",
        "npx",
        "-y",
        "weather-mcp",
    ];

    /// Run a command as a person at a terminal who types `typed`.
    fn typing(directory: &Path, args: &[&str], typed: &str) -> (Result<(), Stopped>, String) {
        let home = Home {
            directory: Some(directory.to_path_buf()),
            writable: true,
        };
        let mut person = Person {
            answers: typed.as_bytes(),
            screen: Vec::new(),
            present: true,
        };
        let outcome = run(&words(args), &home, &mut person);
        (outcome, String::from_utf8(person.screen).unwrap())
    }

    #[test]
    fn a_yes_at_the_question_records_the_digest_and_nothing_else_does() {
        for (typed, recorded) in [("y\n", true), ("n\n", false), ("\n", false), ("", false)] {
            let directory = scratch("cli-mcp-answer");
            let (outcome, _) = typing(&directory, ADD, typed);
            assert!(outcome.is_ok(), "{typed:?}: {outcome:?}");
            assert_eq!(approved(&directory, &weather()), recorded, "{typed:?}");
        }
    }

    #[test]
    fn approve_records_only_on_a_yes_and_a_no_ends_refused() {
        let directory = scratch("cli-mcp-approve");
        let (outcome, _) = typing(&directory, ADD, "n\n");
        assert!(outcome.is_ok());
        let (outcome, _) = typing(&directory, &["approve", "weather"], "n\n");
        assert_eq!(outcome.map_err(|(ending, _)| ending), Err(Ending::Refused));
        assert!(!approved(&directory, &weather()));
        let (outcome, _) = typing(&directory, &["approve", "weather"], "y\n");
        assert!(outcome.is_ok(), "{outcome:?}");
        assert!(approved(&directory, &weather()));
    }

    #[test]
    fn the_question_shows_every_argument_as_the_word_it_is() {
        let directory = scratch("cli-mcp-shown");
        let args = ["add", "spaced", "--stdio", "--", "run", "a b", "c"];
        let (_, screen) = typing(&directory, &args, "n\n");
        assert!(screen.contains("run \"a b\" c"), "{screen}");
    }

    #[test]
    fn replacing_a_declaration_names_what_changed_and_asks_again() {
        let directory = scratch("cli-mcp-changed");
        let (outcome, _) = typing(&directory, ADD, "y\n");
        assert!(outcome.is_ok());
        let pinned = [
            "add",
            "weather",
            "--stdio",
            "--",
            "npx",
            "-y",
            "weather-mcp@1.2.0",
        ];
        let (outcome, screen) = typing(&directory, &pinned, "n\n");
        assert!(outcome.is_ok());
        assert!(screen.contains("argv"), "{screen}");
        assert!(
            !approved(&directory, &weather()),
            "the old digest outlived its declaration"
        );
    }
}
