//! `bravebot mcp`: declaring an MCP server, approving a declaration (SERVERS-3), and forgetting the
//! standing answers given in a project.
//!
//! What this writes is the files [`bravebot_config::mcp`] reads, and the one question it asks is
//! whether the person approves a declaration they were just shown. Nothing here starts a server or
//! offers one to a session.
//!
//! Typing `add` is not the approval. The line a person typed says what to run; the answer to the
//! question says they read what it resolved to, and only that answer is recorded. Where nobody can
//! be asked, the declaration is written and left unapproved, which is LAYER-1's rule for this crate:
//! an effect nobody could be asked about is refused rather than applied unseen.

use crate::exit::{Ending, fail};
use crate::progress::printable;
use bravebot_config::Managed;
use bravebot_config::mcp::{
    self, Approvals, Declaration, Declarations, Entry, Field, Problem, Projects, Standing,
    Unreadable,
};
use bravebot_i18n::t;
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Where the files are, and whether this run may write them.
pub(crate) struct Home {
    /// The state directory, or `None` where the platform names no profile directory.
    pub(crate) directory: Option<PathBuf>,
    /// Whether anything may be written into it, which an incognito session answers no.
    pub(crate) writable: bool,
}

/// The other end of the question: where an answer is read, where things are shown, and whether
/// anybody is there to give one.
pub(crate) struct Person<R, W> {
    pub(crate) answers: R,
    pub(crate) screen: W,
    pub(crate) present: bool,
}

/// How a command ended, where it did not end done.
pub(crate) type Stopped = (Ending, String);

/// A session started in the directory this runs in: what SERVERS-14's request, grant and standing
/// answers are read against.
pub(crate) struct Here {
    /// The project such a session takes, with its links followed as its workspace follows them.
    pub(crate) project: PathBuf,
    /// Each alias its settings request, with the file that requested it.
    pub(crate) requested: Vec<(PathBuf, String)>,
}

impl Here {
    /// A session started here, requesting what `settings` request.
    pub(crate) fn current(settings: &bravebot_config::Settings) -> Result<Self, Stopped> {
        Ok(Self {
            project: project(None)?,
            requested: settings
                .mcp_requested()
                .map(|(file, alias)| (file.to_path_buf(), alias.to_string()))
                .collect(),
        })
    }
}

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
    let here = || Here::current(&bravebot_config::Settings::load());
    match run(args, &home, &Managed::load(), &here, &mut person) {
        Ok(()) => ExitCode::SUCCESS,
        Err((ending, message)) => fail(ending, message),
    }
}

/// `managed` is the machine's layer, read only to say which servers it keeps from starting
/// (SERVERS-12). `here` is read only by `list`, which is the one command reporting on a session.
fn run<R: BufRead, W: Write>(
    args: &[String],
    home: &Home,
    managed: &Managed,
    here: &dyn Fn() -> Result<Here, Stopped>,
    person: &mut Person<R, W>,
) -> Result<(), Stopped> {
    let Some((command, rest)) = args.split_first() else {
        return Err(refused_with_the_forms(t!(mcp_needs_a_command).to_string()));
    };
    match command.as_str() {
        "add" => add(rest, home, person),
        "get" => get(one_alias(command, rest)?, home, managed, person),
        "list" => match rest.first() {
            None => list(home, managed, here, person),
            Some(extra) => Err(unexpected(command, extra)),
        },
        "approve" => approve(one_alias(command, rest)?, home, person),
        "remove" => remove(one_alias(command, rest)?, home, person),
        "forget" => match rest {
            [] => forget(None, home, person),
            [path] => forget(Some(path), home, person),
            [_, extra, ..] => Err(unexpected(command, extra)),
        },
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
        "bravebot mcp add <alias> [--env <name>]... [--dir <path>] [--stdio] -- <program> [args...]",
        "bravebot mcp add <alias> --http <url>",
        "bravebot mcp get <alias>",
        "bravebot mcp list",
        "bravebot mcp approve <alias>",
        "bravebot mcp remove <alias>",
        "bravebot mcp forget [path]",
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
    let mut approvals = Approvals::read(directory);
    // A digest written with no alias beside it takes the alias it resolves to now, before the
    // declaration it approved is replaced and nothing would say this alias changed.
    approvals.keep_only(&declarations);
    declarations.insert(alias, &declaration);
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
            record(
                directory,
                &declarations,
                &mut approvals,
                alias,
                &declaration,
            )?;
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
/// Everything after a bare `--`, or after `--stdio --`, is the program and its arguments, as words
/// and never as a line, so a flag of this command written after it is an argument of the server's.
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
            "--stdio" | "--" => {
                if transport.is_some() {
                    return Err(argument(t!(mcp_two_transports)).into());
                }
                let program = match flag {
                    "--" => index + 1,
                    _ if value.map(String::as_str) == Some("--") => index + 2,
                    _ => return Err(argument(t!(mcp_stdio_needs_a_program)).into()),
                };
                let argv = flags[program..].to_vec();
                if argv.is_empty() {
                    return Err(argument(t!(mcp_stdio_needs_a_program)).into());
                }
                transport = Some(Transport::Stdio(argv));
                break;
            }
            // Shown up to its `=`, since `--env=NAME=value` and `--http=https://user:pass@host`
            // carry the very value SERVERS-10 never repeats.
            other if other.starts_with('-') => {
                let flag = other.split('=').next().unwrap_or(other);
                return Err(argument(t!(cli_unknown_option, flag = shown(flag))).into());
            }
            // Named by its place and not by its text: a stray word here is most often a value,
            // as in `--env TOKEN sk-live` or `--env PATH TOKEN=sk-live`.
            _ => {
                let position = index as i64 + 2;
                return Err(argument(t!(mcp_add_stray_argument, position = position)).into());
            }
        }
        index += 2;
    }
    match transport {
        None => Err(argument(t!(mcp_needs_a_transport)).into()),
        Some(Transport::Stdio(argv)) => {
            // A bare name is looked for only in the PATH a declaration names (SERVERS-10), so
            // one typed without it would be a server that can never start.
            if is_a_bare_name(&argv[0]) {
                variables.push("PATH".to_string());
            }
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

fn is_a_bare_name(program: &str) -> bool {
    let mut parts = Path::new(program).components();
    matches!(
        (parts.next(), parts.next()),
        (Some(std::path::Component::Normal(_)), None)
    )
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
    managed: &Managed,
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
    if let Some(refused) = refused(managed, &declaration) {
        say(person, format!("{indent}{refused}"));
    }
    Ok(())
}

/// What `get` and `list` add for a server the managed layer keeps from starting, which is the line
/// that stops an approved server looking reachable when no session will start it (SERVERS-12,
/// SERVERS-14).
fn refused(managed: &Managed, declaration: &Declaration) -> Option<String> {
    crate::servers::refused_declaration(managed, declaration, &|name| std::env::var_os(name))
        .map(|reason| t!(mcp_refused_by_managed, reason = reason).to_string())
}

/// SERVERS-14's `list`: [`report`] under the file it read and the directory it was read for.
fn list<R: BufRead, W: Write>(
    home: &Home,
    managed: &Managed,
    here: &dyn Fn() -> Result<Here, Stopped>,
    person: &mut Person<R, W>,
) -> Result<(), Stopped> {
    let (directory, declarations) = readable(home)?;
    let Some(directory) = directory else {
        say(person, no_state_directory());
        return Ok(());
    };
    let here = here()?;
    let path = mcp::declarations_file(directory).display().to_string();
    let (rows, unusable) = report(directory, &declarations, managed, &here);
    match declarations.entries().is_empty() {
        true => say(person, t!(mcp_none_declared, path = &path)),
        false => say(person, t!(mcp_list_declared_in, path = &path)),
    }
    if rows.is_empty() {
        return Ok(());
    }
    say(
        person,
        t!(
            mcp_list_here,
            path = printable(&here.project.display().to_string())
        ),
    );
    for row in rows {
        say(person, row);
    }
    match unusable {
        0 => Ok(()),
        count => Err((
            Ending::Configuration,
            t!(mcp_list_unusable, count = count, path = &path).to_string(),
        )),
    }
}

/// SERVERS-14's half in `doctor`: the rows `list` prints, under one heading naming the file and the
/// directory, and whether anything in them is a failure.
///
/// A failure where the declarations cannot be read or one cannot be used, which is where `list`
/// fails too. Not the configuration status: a session here opens and works without that server.
pub(crate) fn examined(
    home: &Home,
    managed: &Managed,
    here: Result<Here, Stopped>,
) -> (Vec<String>, bool) {
    let Some(directory) = &home.directory else {
        return (vec![no_state_directory()], false);
    };
    let here = match here {
        Ok(here) => here,
        Err((_, why)) => return (vec![why], true),
    };
    let declarations = match read(directory) {
        Ok(declarations) => declarations,
        Err((_, why)) => return (vec![why], true),
    };
    let path = mcp::declarations_file(directory).display().to_string();
    let project = printable(&here.project.display().to_string());
    let (rows, unusable) = report(directory, &declarations, managed, &here);
    let heading = match declarations.entries().is_empty() {
        true => t!(doctor_mcp_none, path = path, project = project),
        false => t!(doctor_mcp_servers, path = path, project = project),
    };
    let mut lines = vec![heading.to_string()];
    lines.extend(rows);
    (lines, unusable > 0)
}

/// SERVERS-14's report: one row for each declared server and for each server requested here and
/// declared nowhere, and how many declarations cannot be used.
///
/// A declaration's row is its transport, its approval, its digest, and where the managed layer
/// refuses it, why. Under it, which checkout requested it and whether a session started `here`
/// holds the grant to call it, read from the one place a session decides that
/// ([`crate::servers::grant`]), and then what was answered about it here. An entry that cannot be
/// used is listed with its problem rather than left out, since a list that dropped it would make a
/// declaration somebody wrote look like one nobody read.
pub(crate) fn report(
    directory: &Path,
    declarations: &Declarations,
    managed: &Managed,
    here: &Here,
) -> (Vec<String>, usize) {
    use crate::servers::{Grant, Unstarted, grant, named};

    let entries = declarations.entries();
    let undeclared: Vec<&(PathBuf, String)> = here
        .requested
        .iter()
        .filter(|(_, alias)| declarations.get(alias).is_none())
        .collect();
    let approvals = Approvals::read(directory);
    let projects = Projects::read(directory);
    let standing = Standing::read(directory).in_project(&here.project);
    let approved = t!(mcp_approved).to_string();
    let unapproved = t!(mcp_unapproved).to_string();
    let state = approved.chars().count().max(unapproved.chars().count());
    let width = entries
        .iter()
        .map(|entry| entry.alias.as_str())
        .chain(undeclared.iter().map(|(_, alias)| alias.as_str()))
        .map(|alias| shown(alias).chars().count())
        .max()
        .unwrap_or_default();
    let under = " ".repeat(2 + width + 2);
    let mut rows = Vec::new();
    let mut unusable = 0usize;
    for Entry { alias, declaration } in &entries {
        let file = here
            .requested
            .iter()
            .find(|(_, requested)| requested == alias)
            .map(|(file, _)| printable(&named(file, &here.project)));
        let row = pad(&shown(alias), width);
        let (request, covered) = match declaration {
            Ok(declaration) => {
                let digest = declaration.digest();
                let covered = file.is_some() && !approvals.changed(alias, &digest);
                let word = match approvals.approves(&digest) {
                    true => &approved,
                    false => &unapproved,
                };
                let line = format!(
                    "  {row}  {}  {}  {}",
                    pad(declaration.transport(), 5),
                    pad(word, state),
                    digest.short()
                );
                rows.push(match refused(managed, declaration) {
                    Some(refusal) => format!("{line}  {refusal}"),
                    None => line,
                });
                let request = file.map(|file| {
                    match grant(
                        alias,
                        declaration,
                        &here.project,
                        &approvals,
                        &projects,
                        managed,
                    ) {
                        Grant::Held => t!(mcp_requested_held, file = file),
                        Grant::Asked => t!(mcp_requested_asked, file = file),
                        Grant::Withheld(Unstarted::Unplanned(reason)) => {
                            t!(mcp_requested_withheld, file = file, reason = reason)
                        }
                        Grant::Withheld(Unstarted::Unconfined) => t!(
                            mcp_requested_withheld,
                            file = file,
                            reason = t!(mcp_no_confinement_here)
                        ),
                        // Its row already ends on the managed layer's reason.
                        Grant::Withheld(Unstarted::Refused(_)) => {
                            t!(mcp_requested_not_started, file = file)
                        }
                    }
                });
                (request, covered)
            }
            Err(found) => {
                unusable += 1;
                rows.push(format!(
                    "  {row}  {}",
                    t!(mcp_cannot_be_used, problem = problem(found))
                ));
                (
                    file.map(|file| t!(mcp_requested_not_started, file = file)),
                    false,
                )
            }
        };
        let request = request.unwrap_or_else(|| t!(mcp_not_requested_here).to_string());
        rows.push(format!("{under}{request}"));
        let project = covered && projects.contains(&here.project);
        for answer in answered(alias, project, &standing) {
            rows.push(format!("{under}{answer}"));
        }
    }
    for (file, alias) in undeclared {
        rows.push(format!(
            "  {}  {}",
            pad(&shown(alias), width),
            t!(
                mcp_requested_undeclared,
                file = printable(&named(file, &here.project))
            )
        ));
        for answer in answered(alias, false, &standing) {
            rows.push(format!("{under}{answer}"));
        }
    }
    (rows, unusable)
}

/// The standing answers recorded for `alias` in the project `standing` was read for: answer 2 at
/// the question before a server starts, where `project` says it is recorded and covers this one,
/// and answer 2 at a call, one line naming each tool it covers.
///
/// The project answer covers a server the project requests whose declaration nobody saw as
/// something else (SERVERS-5), so it is said under no other.
fn answered(alias: &str, project: bool, standing: &[String]) -> Vec<String> {
    // Each is `alias:tool`, and neither an alias nor a tool word holds a colon.
    let tools: Vec<&str> = standing
        .iter()
        .filter_map(|answer| answer.split_once(':'))
        .filter(|(answered, _)| *answered == alias)
        .map(|(_, tool)| tool)
        .collect();
    let mut lines = Vec::new();
    if project {
        lines.push(t!(mcp_standing_project).to_string());
    }
    if !tools.is_empty() {
        lines.push(t!(mcp_standing_tools, tools = tools.join(", ")).to_string());
    }
    if lines.is_empty() {
        lines.push(t!(mcp_standing_none).to_string());
    }
    lines
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
            record(
                directory,
                &declarations,
                &mut approvals,
                alias,
                &declaration,
            )?;
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

/// Drop the standing answers recorded for a project: answer 2 at a server's question, and each
/// tool answer 2 at a call's question stopped asking about there.
///
/// The project is `path`, or the directory this runs in, as the absolute path it resolves to. A
/// path that no longer resolves is taken as it was typed, made absolute: a checkout that was
/// deleted is the one somebody most wants forgotten, and its answers are recorded under a path
/// nothing exists at any more.
fn forget<R: BufRead, W: Write>(
    path: Option<&str>,
    home: &Home,
    person: &mut Person<R, W>,
) -> Result<(), Stopped> {
    let directory = writable(home)?;
    let project = project(path)?;
    let unreadable = |file: PathBuf, why: Unreadable| -> Stopped {
        (
            Ending::Failed,
            t!(
                mcp_unreadable,
                path = shown(&file.display().to_string()),
                reason = bravebot_agent::mcp::unreadable_record(&why)
            )
            .to_string(),
        )
    };
    let mut projects = Projects::to_change(directory)
        .map_err(|why| unreadable(mcp::projects_file(directory), why))?;
    let mut standing = Standing::to_change(directory)
        .map_err(|why| unreadable(mcp::tools_file(directory), why))?;
    let every_server = projects.remove(&project);
    let tools = standing.in_project(&project);
    standing.forget(&project);

    if every_server {
        replace(&mcp::projects_file(directory), &projects.to_text())?;
    }
    if !tools.is_empty() {
        replace(&mcp::tools_file(directory), &standing.to_text())?;
    }

    let named = shown(&project.display().to_string());
    if !every_server && tools.is_empty() {
        say(person, t!(mcp_forgot_nothing, path = &named));
    }
    if every_server {
        say(person, t!(mcp_forgot_servers, path = &named));
    }
    for tool in &tools {
        say(person, t!(mcp_forgot_tool, tool = tool, path = &named));
    }
    Ok(())
}

/// The project `forget` means: the path given, or the directory this runs in.
fn project(path: Option<&str>) -> Result<PathBuf, Stopped> {
    let here = std::env::current_dir().map_err(|error| {
        (
            Ending::Failed,
            t!(mcp_no_current_directory, error = error.to_string()).to_string(),
        )
    })?;
    let typed = match path {
        Some(path) => here.join(path),
        None => here,
    };
    Ok(typed.canonicalize().unwrap_or(typed))
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
    for line in crate::servers::fetching(alias, declaration) {
        say(person, line);
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

/// Record the approval of `declaration`'s digest, and of nothing else: the alias is kept beside it
/// as the name it was asked about under, and is not what was approved.
pub(crate) fn record(
    directory: &Path,
    declarations: &Declarations,
    approvals: &mut Approvals,
    alias: &str,
    declaration: &Declaration,
) -> Result<(), Stopped> {
    approvals.approve(alias, declaration.digest());
    approvals.keep_only(declarations);
    replace(&mcp::approvals_file(directory), &approvals.to_text())
}

/// A declaration as a person reads it: the alias, the transport and what it runs or reaches on the
/// first line, and under it the names it receives, where it runs, and the digest.
///
/// Every argument is shown as the word it is, quoted where it holds a space or anything a terminal
/// would not draw as itself, so `a b` and `"a b"` are told apart on the screen as they are in argv.
pub(crate) fn drawn(alias: &str, declaration: &Declaration, digest: &str) -> Vec<String> {
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
pub(crate) fn indent(alias: &str) -> String {
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

pub(crate) fn say<R, W: Write>(person: &mut Person<R, W>, line: impl std::fmt::Display) {
    let _ = writeln!(person.screen, "{line}");
}

pub(crate) fn no_state_directory() -> String {
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
    approvals.keep_only(declarations);
    replace(&mcp::approvals_file(directory), &approvals.to_text())
}

/// Write `text` over `path` through a temporary file beside it, so an interrupted write leaves the
/// file as it was rather than half of it.
pub(crate) fn replace(path: &Path, text: &str) -> Result<(), Stopped> {
    bravebot_agent::mcp::replace(path, text).map_err(|error| {
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

pub(crate) fn problem(found: &Problem) -> String {
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

pub(crate) fn unreadable(why: &Unreadable) -> String {
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
        Declaration::stdio(words(&["npx", "-y", "weather-mcp"]), words(&["PATH"]), None).unwrap()
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

    /// Run a command as a person at a terminal who types `typed`. The machine's layer is
    /// `managed.json` in `directory`, absent unless a test writes it.
    fn typing(directory: &Path, args: &[&str], typed: &str) -> (Result<(), Stopped>, String) {
        let nothing = Here {
            project: directory.join("checkout"),
            requested: Vec::new(),
        };
        typing_in(directory, &nothing, args, typed)
    }

    /// The same, where `here` is the directory the command runs in and what its settings request.
    fn typing_in(
        directory: &Path,
        here: &Here,
        args: &[&str],
        typed: &str,
    ) -> (Result<(), Stopped>, String) {
        let home = Home {
            directory: Some(directory.to_path_buf()),
            writable: true,
        };
        let mut person = Person {
            answers: typed.as_bytes(),
            screen: Vec::new(),
            present: true,
        };
        let managed = Managed::at(&directory.join("managed.json"));
        let here = || {
            Ok(Here {
                project: here.project.clone(),
                requested: here.requested.clone(),
            })
        };
        let outcome = run(&words(args), &home, &managed, &here, &mut person);
        (outcome, String::from_utf8(person.screen).unwrap())
    }

    /// No session starts a server the machine's administrator refused, whatever its approval says,
    /// so `list` and `get` say so beside the approval, and why. Without it an approved server reads
    /// as one the next session starts, which is the report SERVERS-14 exists to keep true.
    #[test]
    fn list_and_get_say_why_the_managed_layer_refuses_a_server_and_no_other() {
        let directory = scratch("cli-mcp-refused");
        for add in [
            &[
                "add",
                "weather",
                "--stdio",
                "--",
                "/opt/weather-mcp",
                "--stdio",
            ][..],
            &["add", "docs", "--stdio", "--", "/opt/docs-mcp"],
            &["add", "maps", "--http", "https://maps.example/mcp"],
        ] {
            let (outcome, _) = typing(&directory, add, "y\n");
            assert!(outcome.is_ok(), "{add:?}: {outcome:?}");
        }
        let managed = directory.join("managed.json");
        std::fs::write(
            &managed,
            r#"{"mcp": {
                "allow": [
                    {"command": ["/opt/weather-mcp", "--stdio"]},
                    {"command": ["/opt/docs-mcp"]}
                ],
                "deny": [{"command": ["/opt/weather-mcp", "--stdio"]}]
            }}"#,
        )
        .expect("managed.json");
        let path = managed.display().to_string();
        let denied = t!(
            mcp_refused_by_managed,
            reason = t!(
                managed_denied,
                path = path.clone(),
                entry = "command /opt/weather-mcp --stdio"
            )
        )
        .to_string();
        let not_allowed = t!(
            mcp_refused_by_managed,
            reason = t!(managed_not_allowed, path = path.clone())
        )
        .to_string();

        let (outcome, listed) = typing(&directory, &["list"], "");
        assert!(outcome.is_ok(), "{outcome:?}");
        let row = |alias: &str| {
            listed
                .lines()
                .find(|line| line.split_whitespace().next() == Some(alias))
                .unwrap_or_else(|| panic!("{alias} is not listed: {listed}"))
        };
        assert!(row("weather").ends_with(&denied), "{listed}");
        assert!(row("maps").ends_with(&not_allowed), "{listed}");
        let approved = t!(mcp_approved).to_string();
        assert_eq!(
            row("weather").split_whitespace().nth(2),
            Some(approved.as_str()),
            "the approval is kept beside the refusal: {listed}"
        );
        assert_eq!(row("docs").split_whitespace().count(), 4, "{listed}");

        let (outcome, got) = typing(&directory, &["get", "weather"], "");
        assert!(outcome.is_ok(), "{outcome:?}");
        assert!(got.lines().any(|line| line.trim() == denied), "{got}");
        let (_, maps) = typing(&directory, &["get", "maps"], "");
        assert!(
            maps.lines().any(|line| line.trim() == not_allowed),
            "{maps}"
        );
        let (_, docs) = typing(&directory, &["get", "docs"], "");
        assert!(!docs.contains(&path), "{docs}");
    }

    /// A checkout under `directory` whose settings file requests `aliases`, as a session there
    /// finds it.
    fn requesting(directory: &Path, aliases: &[&str]) -> Here {
        let checkout = directory.join("checkout");
        std::fs::create_dir_all(checkout.join(".bravebot")).expect("checkout");
        let project = checkout.canonicalize().expect("canonical");
        let file = project.join(".bravebot/settings.json");
        std::fs::write(&file, "{}").expect("settings");
        Here {
            project,
            requested: aliases
                .iter()
                .map(|alias| (file.clone(), alias.to_string()))
                .collect(),
        }
    }

    /// That settings file as a row names it, relative to the checkout.
    fn requested_by() -> String {
        Path::new(".bravebot")
            .join("settings.json")
            .display()
            .to_string()
    }

    /// The lines `list` printed under `alias`'s row, up to the next row.
    fn under<'a>(listed: &'a str, alias: &str) -> Vec<&'a str> {
        listed
            .lines()
            .skip_while(|line| line.split_whitespace().next() != Some(alias))
            .skip(1)
            .take_while(|line| line.starts_with("   "))
            .map(str::trim)
            .collect()
    }

    /// SERVERS-14's other columns: under each server, which checkout here requested it, whether a
    /// session started here holds the grant to call it or asks first, and which of answer 2 at
    /// either question stands for it here. Answers given in another project are not these.
    #[test]
    fn list_says_who_requested_each_server_whether_a_session_here_holds_its_grant_and_what_stands()
    {
        let directory = scratch("cli-mcp-list-here");
        for (add, typed) in [
            (
                &["add", "weather", "--http", "https://weather.example/mcp"][..],
                "y\n",
            ),
            (
                &["add", "docs", "--http", "https://docs.example/mcp"],
                "n\n",
            ),
            (
                &["add", "maps", "--http", "https://maps.example/mcp"],
                "n\n",
            ),
        ] {
            let (outcome, _) = typing(&directory, add, typed);
            assert!(outcome.is_ok(), "{add:?}: {outcome:?}");
        }
        let here = requesting(&directory, &["docs", "weather", "calendar"]);
        let mut standing = Standing::default();
        assert!(standing.add("weather", "get_forecast", &here.project));
        assert!(standing.add("weather", "get_alerts", &directory.join("elsewhere")));
        assert!(standing.add("calendar", "list_events", &here.project));
        std::fs::write(mcp::tools_file(&directory), standing.to_text()).expect("tools");
        let file = &requested_by();

        let (outcome, listed) = typing_in(&directory, &here, &["list"], "");
        assert!(outcome.is_ok(), "{outcome:?}");
        let project = here.project.display().to_string();
        assert!(
            listed
                .lines()
                .any(|line| line == t!(mcp_list_here, path = project.clone())),
            "{listed}"
        );
        assert_eq!(
            under(&listed, "weather"),
            [
                t!(mcp_requested_held, file = file),
                t!(mcp_standing_tools, tools = "get_forecast"),
            ],
            "{listed}"
        );
        assert_eq!(
            under(&listed, "docs"),
            [
                t!(mcp_requested_asked, file = file),
                t!(mcp_standing_none).to_string(),
            ],
            "{listed}"
        );
        assert_eq!(
            under(&listed, "maps"),
            [
                t!(mcp_not_requested_here).to_string(),
                t!(mcp_standing_none).to_string(),
            ],
            "{listed}"
        );
        let calendar = listed
            .lines()
            .find(|line| line.split_whitespace().next() == Some("calendar"))
            .unwrap_or_else(|| panic!("a request nothing declares is left out: {listed}"));
        assert!(
            calendar.ends_with(&t!(mcp_requested_undeclared, file = file)),
            "{listed}"
        );
        assert_eq!(
            under(&listed, "calendar"),
            [t!(mcp_standing_tools, tools = "list_events")],
            "{listed}"
        );

        // Answer 2 at the question before a server starts, recorded for this project: every server
        // it requests is started unasked, except one whose declaration changed since it was seen,
        // and the answer is said under those alone.
        let mut projects = Projects::default();
        assert!(projects.add(&here.project));
        std::fs::write(mcp::projects_file(&directory), projects.to_text()).expect("projects");
        let (outcome, _) = typing(
            &directory,
            &["add", "weather", "--http", "https://weather.example/v2"],
            "n\n",
        );
        assert!(outcome.is_ok(), "{outcome:?}");
        let (_, listed) = typing_in(&directory, &here, &["list"], "");
        let project_answer = t!(mcp_standing_project).to_string();
        assert_eq!(
            under(&listed, "docs"),
            [t!(mcp_requested_held, file = file), project_answer],
            "{listed}"
        );
        assert_eq!(
            under(&listed, "weather"),
            [
                t!(mcp_requested_asked, file = file),
                t!(mcp_standing_tools, tools = "get_forecast"),
            ],
            "{listed}"
        );
        assert_eq!(
            under(&listed, "maps"),
            [
                t!(mcp_not_requested_here).to_string(),
                t!(mcp_standing_none).to_string(),
            ],
            "{listed}"
        );
    }

    /// A requested server no session here starts is said to hold no grant, with the reason where
    /// its row does not already give one: a program nothing resolves, a server the managed layer
    /// refuses, and a declaration that cannot be used.
    #[test]
    fn list_says_a_requested_server_no_session_here_starts_holds_no_grant() {
        let directory = scratch("cli-mcp-list-withheld");
        std::fs::write(
            mcp::declarations_file(&directory),
            r#"{"servers": {
                "local": {"transport": "stdio", "argv": ["local-mcp"]},
                "maps": {"transport": "http", "url": "https://maps.example/mcp"},
                "leaky": {"transport": "stdio", "argv": ["x"], "env": {"TOKEN": "hunter2"}}
            }}"#,
        )
        .expect("mcp.json");
        std::fs::write(
            directory.join("managed.json"),
            r#"{"mcp": {"allow": [{"command": ["/opt/docs-mcp"]}]}}"#,
        )
        .expect("managed.json");
        let here = requesting(&directory, &["local", "maps", "leaky"]);
        let file = &requested_by();

        let (outcome, listed) = typing_in(&directory, &here, &["list"], "");
        assert_eq!(
            outcome.map_err(|(ending, _)| ending),
            Err(Ending::Configuration),
            "{listed}"
        );
        assert_eq!(
            under(&listed, "local").first().copied(),
            Some(
                t!(
                    mcp_requested_withheld,
                    file = file,
                    reason = t!(servers_program_without_path, program = "local-mcp")
                )
                .as_str()
            ),
            "{listed}"
        );
        let not_started = t!(mcp_requested_not_started, file = file);
        assert_eq!(
            under(&listed, "maps").first().copied(),
            Some(not_started.as_str()),
            "{listed}"
        );
        assert_eq!(
            under(&listed, "leaky").first().copied(),
            Some(not_started.as_str()),
            "{listed}"
        );
        assert!(!listed.contains("hunter2"), "{listed}");
    }

    /// A control character in the directory or in a settings file's path is shown as its picture,
    /// so a name cannot write a row of its own in `list` or in `doctor`.
    #[test]
    fn a_control_character_in_a_path_cannot_write_a_row() {
        let directory = scratch("cli-mcp-list-pictured");
        let (outcome, _) = typing(
            &directory,
            &["add", "maps", "--http", "https://maps.example/mcp"],
            "n\n",
        );
        assert!(outcome.is_ok(), "{outcome:?}");
        let project = directory.join("app\nweather  stdio  approved");
        let here = Here {
            project: project.clone(),
            requested: vec![
                (project.join("maps\n.json"), "maps".to_string()),
                (project.join("calendar\n.json"), "calendar".to_string()),
            ],
        };

        let (outcome, listed) = typing_in(&directory, &here, &["list"], "");
        assert!(outcome.is_ok(), "{outcome:?}");
        assert!(
            listed.lines().all(|line| !line.starts_with("weather")),
            "{listed}"
        );
        for pictured in [
            "app\u{240a}weather",
            "maps\u{240a}.json",
            "calendar\u{240a}.json",
        ] {
            assert!(listed.contains(pictured), "{pictured}: {listed}");
        }

        let home = Home {
            directory: Some(directory.clone()),
            writable: false,
        };
        let (lines, _) = examined(&home, &Managed::default(), Ok(here));
        assert!(lines.iter().all(|line| !line.contains('\n')), "{lines:?}");
    }

    /// `doctor` reports the rows `list` does, under one heading naming the file and the directory,
    /// and fails where `list` does, on declarations it cannot read or use. A state directory that
    /// declares nothing, or a machine with none, is no failure.
    #[test]
    fn doctor_reports_what_list_does_and_fails_where_it_does() {
        let directory = scratch("cli-mcp-examined");
        let here = requesting(&directory, &["weather"]);
        let home = Home {
            directory: Some(directory.clone()),
            writable: false,
        };
        let managed = Managed::default();
        let again = || {
            Ok(Here {
                project: here.project.clone(),
                requested: here.requested.clone(),
            })
        };
        let path = mcp::declarations_file(&directory).display().to_string();
        let project = here.project.display().to_string();

        let (lines, failed) = examined(&home, &managed, again());
        assert!(!failed, "{lines:?}");
        assert_eq!(
            lines[0],
            t!(
                doctor_mcp_none,
                path = path.clone(),
                project = project.clone()
            )
        );
        assert!(
            lines[1].ends_with(&t!(mcp_requested_undeclared, file = requested_by())),
            "{lines:?}"
        );

        let (outcome, _) = typing(
            &directory,
            &["add", "weather", "--http", "https://weather.example/mcp"],
            "y\n",
        );
        assert!(outcome.is_ok(), "{outcome:?}");
        let (lines, failed) = examined(&home, &managed, again());
        assert!(!failed, "{lines:?}");
        let (_, listed) = typing_in(&directory, &here, &["list"], "");
        assert_eq!(
            lines,
            std::iter::once(t!(
                doctor_mcp_servers,
                path = path.clone(),
                project = project.clone()
            ))
            .chain(listed.lines().skip(2).map(str::to_string))
            .collect::<Vec<_>>(),
            "{listed}"
        );

        std::fs::write(
            mcp::declarations_file(&directory),
            r#"{"servers": {"weather": {"transport": "stdio"}}}"#,
        )
        .expect("mcp.json");
        let (lines, failed) = examined(&home, &managed, again());
        assert!(failed, "an unusable declaration: {lines:?}");

        std::fs::write(mcp::declarations_file(&directory), "not json").expect("mcp.json");
        let (lines, failed) = examined(&home, &managed, again());
        assert!(failed, "an unreadable file: {lines:?}");
        assert_eq!(lines.len(), 1, "{lines:?}");

        let nowhere = Home {
            directory: None,
            writable: false,
        };
        let (lines, failed) = examined(&nowhere, &managed, again());
        assert!(!failed);
        assert_eq!(lines, [no_state_directory()]);
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

    /// SERVERS-3: a bare `--` starts the program as `--stdio --` does, which is how `claude mcp add`
    /// spells it, with the flags before it read the same and every word after it the server's.
    #[test]
    fn a_bare_double_dash_declares_the_program_after_it() {
        let directory = scratch("cli-mcp-bare-dashes");
        let place = std::fs::canonicalize(&directory).unwrap();
        let place = place.to_str().unwrap();
        let declared_by = |flags: &[&str]| declared(&words(flags)).ok();

        assert_eq!(
            declared_by(&["--", "npx", "-y", "weather-mcp"]),
            Some(weather())
        );
        let flagged = Declaration::stdio(
            words(&["/opt/srv", "--http", "x"]),
            words(&["TOKEN"]),
            Some(place.to_string()),
        )
        .unwrap();
        assert_eq!(
            declared_by(&[
                "--env", "TOKEN", "--dir", place, "--", "/opt/srv", "--http", "x"
            ]),
            Some(flagged)
        );

        let refused = |flags: &[&str]| match declared(&words(flags)) {
            Err(Refusal::Said((Ending::Argument, said))) => said,
            _ => panic!("{flags:?} was not refused as an argument"),
        };
        assert_eq!(refused(&["--"]), t!(mcp_stdio_needs_a_program).to_string());
        assert_eq!(
            refused(&["--http", "https://mcp.example.com/mcp", "--", "npx"]),
            t!(mcp_two_transports).to_string()
        );
    }

    /// SERVERS-10: a program named by a bare name is looked for only in the `PATH` its declaration
    /// names, so `add` names `PATH` for one, once, whichever way it was typed. A program given as a
    /// path is declared with only the variables named.
    #[test]
    fn a_program_named_by_a_bare_name_is_declared_with_path() {
        for (flags, expected) in [
            (&["--", "npx", "weather-mcp"][..], &["PATH"][..]),
            (&["--stdio", "--", "npx", "weather-mcp"], &["PATH"]),
            (&["--env", "PATH", "--", "npx", "weather-mcp"], &["PATH"]),
            (
                &["--env", "TOKEN", "--", "npx", "weather-mcp"],
                &["TOKEN", "PATH"],
            ),
            (&["--", "/opt/weather-mcp"], &[]),
            (&["--", "bin/weather-mcp"], &[]),
            (&["--env", "TOKEN", "--", "/opt/weather-mcp"], &["TOKEN"]),
        ] {
            let Ok(Declaration::Stdio { variables, .. }) = declared(&words(flags)) else {
                panic!("{flags:?} declared no local server");
            };
            assert_eq!(variables, words(expected), "{flags:?}");
        }
    }

    /// The `PATH` `add` names is shown at the question like one typed, since the approval is of
    /// the declaration and the declaration hands the server its value.
    #[test]
    fn the_question_shows_the_path_a_bare_name_was_given() {
        let directory = scratch("cli-mcp-bare-path");
        let short = ["add", "weather", "--", "npx", "-y", "weather-mcp"];
        let (outcome, screen) = typing(&directory, &short, "y\n");
        assert!(outcome.is_ok(), "{outcome:?}");
        let shown = t!(mcp_variables, names = "PATH").to_string();
        assert!(screen.lines().any(|line| line.trim() == shown), "{screen}");
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
    fn the_question_names_a_runner_and_the_package_it_leaves_unpinned() {
        let directory = scratch("cli-mcp-runner");
        let (_, screen) = typing(&directory, ADD, "n\n");
        let lines: Vec<&str> = screen.lines().map(str::trim).collect();
        assert!(
            lines.contains(&t!(servers_fetches, runner = "npx").as_str()),
            "{screen}"
        );
        assert!(
            lines.contains(&t!(servers_unpinned, package = "weather-mcp").as_str()),
            "{screen}"
        );

        let pinned = [
            "add",
            "pinned",
            "--stdio",
            "--",
            "npx",
            "-y",
            "weather-mcp@1.2.0",
        ];
        let (_, screen) = typing(&directory, &pinned, "n\n");
        let lines: Vec<&str> = screen.lines().map(str::trim).collect();
        assert!(
            lines.contains(&t!(servers_fetches, runner = "npx").as_str()),
            "{screen}"
        );
        assert!(!screen.contains("names no exact version"), "{screen}");
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

    #[test]
    fn replacing_a_declaration_approved_under_no_alias_still_says_it_changed() {
        let directory = scratch("cli-mcp-changed-unnamed");
        let (outcome, _) = typing(&directory, ADD, "n\n");
        assert!(outcome.is_ok());
        let unnamed = format!("{}\n", weather().digest());
        std::fs::write(mcp::approvals_file(&directory), unnamed).expect("approvals");
        let pinned = [
            "add",
            "weather",
            "--stdio",
            "--",
            "npx",
            "-y",
            "weather-mcp@1.2.0",
        ];
        let (outcome, _) = typing(&directory, &pinned, "n\n");
        assert!(outcome.is_ok());
        let replaced = Declaration::stdio(
            words(&["npx", "-y", "weather-mcp@1.2.0"]),
            words(&["PATH"]),
            None,
        )
        .unwrap();
        assert!(Approvals::read(&directory).changed("weather", &replaced.digest()));
    }

    /// Record answer 2 at a server's question for `project`, and answer 2 at a call's question for
    /// `weather:get_forecast` there and `weather:get_alerts` somewhere else.
    fn answered_in(directory: &Path, project: &Path, elsewhere: &Path) {
        let mut projects = Projects::default();
        assert!(projects.add(project));
        std::fs::write(mcp::projects_file(directory), projects.to_text()).expect("projects");
        let mut standing = Standing::default();
        assert!(standing.add("weather", "get_forecast", project));
        assert!(standing.add("weather", "get_alerts", elsewhere));
        std::fs::write(mcp::tools_file(directory), standing.to_text()).expect("tools");
    }

    #[test]
    fn forget_drops_a_projects_standing_answers_and_nobody_elses() {
        let directory = scratch("cli-mcp-forget");
        let project = directory.join("project");
        std::fs::create_dir_all(&project).expect("project");
        let project = project.canonicalize().expect("canonical");
        let elsewhere = directory.join("elsewhere");
        answered_in(&directory, &project, &elsewhere);

        let (outcome, screen) = typing(
            &directory,
            &["forget", project.to_str().expect("utf-8")],
            "",
        );
        assert!(outcome.is_ok(), "{outcome:?}");
        assert!(!Projects::read(&directory).contains(&project));
        let standing = Standing::read(&directory);
        assert!(!standing.covers("weather", "get_forecast", &project));
        assert!(
            standing.covers("weather", "get_alerts", &elsewhere),
            "another project's answer went with it"
        );
        let named = shown(&project.display().to_string());
        assert!(
            screen.contains(&t!(mcp_forgot_servers, path = &named)),
            "{screen}"
        );
        assert!(
            screen.contains(&t!(
                mcp_forgot_tool,
                tool = "weather:get_forecast",
                path = &named
            )),
            "{screen}"
        );

        let (outcome, screen) = typing(
            &directory,
            &["forget", project.to_str().expect("utf-8")],
            "",
        );
        assert!(outcome.is_ok(), "{outcome:?}");
        assert_eq!(
            screen,
            format!("{}\n", t!(mcp_forgot_nothing, path = &named))
        );
    }

    /// A deleted checkout is the one most worth forgetting, and nothing resolves its path.
    #[test]
    fn forget_takes_a_path_that_no_longer_resolves_as_typed() {
        let directory = scratch("cli-mcp-forget-gone");
        let gone = directory.join("deleted-checkout");
        answered_in(&directory, &gone, &directory.join("elsewhere"));

        let (outcome, _) = typing(&directory, &["forget", gone.to_str().expect("utf-8")], "");
        assert!(outcome.is_ok(), "{outcome:?}");
        assert!(!Projects::read(&directory).contains(&gone));
        assert!(!Standing::read(&directory).covers("weather", "get_forecast", &gone));
    }

    /// A record that is there and cannot be read is left as it is and said to be, rather than
    /// written back as the one line fewer it would be once read as empty.
    #[test]
    fn forget_leaves_a_record_it_cannot_read_as_it_is() {
        let directory = scratch("cli-mcp-forget-unreadable");
        let project = directory.join("checkout");
        answered_in(&directory, &project, &directory.join("elsewhere"));
        let too_large = " ".repeat(64 * 1024 + 1);
        std::fs::write(mcp::tools_file(&directory), &too_large).unwrap();
        let projects = std::fs::read_to_string(mcp::projects_file(&directory)).unwrap();

        let (outcome, _) = typing(
            &directory,
            &["forget", project.to_str().expect("utf-8")],
            "",
        );
        let (ending, said) = outcome.expect_err("an unreadable record was forgotten from");
        assert_eq!(ending, Ending::Failed);
        assert!(said.contains(t!(mcp_record_too_large)), "{said}");
        assert_eq!(
            std::fs::read_to_string(mcp::tools_file(&directory)).unwrap(),
            too_large
        );
        assert_eq!(
            std::fs::read_to_string(mcp::projects_file(&directory)).unwrap(),
            projects,
            "a record was written though the other could not be read"
        );
    }

    #[test]
    fn forget_takes_one_path_at_most_and_writes_nothing_incognito() {
        let directory = scratch("cli-mcp-forget-refused");
        let (outcome, _) = typing(&directory, &["forget", "a", "b"], "");
        assert_eq!(outcome.map_err(|(ending, _)| ending), Err(Ending::Argument));

        let home = Home {
            directory: Some(directory.clone()),
            writable: false,
        };
        let mut person = Person {
            answers: "".as_bytes(),
            screen: Vec::new(),
            present: true,
        };
        let outcome = run(
            &words(&["forget"]),
            &home,
            &Managed::default(),
            &|| unreachable!("forget reports on no session"),
            &mut person,
        );
        assert_eq!(outcome.map_err(|(ending, _)| ending), Err(Ending::Failed));
    }
}
