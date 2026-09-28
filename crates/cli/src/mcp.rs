//! `bravebot mcp`: declaring an MCP server, approving a declaration (SERVERS-3), requesting one in a
//! settings file (SERVERS-2), and forgetting the standing answers given in a project.
//!
//! What this writes is the files [`bravebot_config::mcp`] reads and the `mcp.request` list of the
//! settings file `-s` names, and the one question it asks is whether the person approves a
//! declaration they were just shown. Nothing here starts a server or offers one to a session.
//!
//! Typing `add` is not the approval. The line a person typed says what to run; the answer to the
//! question says they read what it resolved to, and only that answer is recorded. Where nobody can
//! be asked, the declaration is written and left unapproved, which is LAYER-1's rule for this crate:
//! an effect nobody could be asked about is refused rather than applied unseen.

use crate::exit::{Ending, fail};
use crate::progress::printable;
use bravebot_config::Managed;
use bravebot_config::import::{Destination, Unwritable};
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

/// The directory this runs in, which is the checkout a `local` or `project` request is written
/// into, or why it could not be read.
type Cwd<'a> = Result<&'a Path, &'a str>;

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
    // The directory the settings reader takes a checkout's layers from, and for the same reason
    // not an ancestor of it: a request written anywhere else is one no session here reads.
    let cwd = std::env::current_dir().map_err(|error| error.to_string());
    let cwd = cwd.as_deref().map_err(String::as_str);
    let here = || Here::current(&bravebot_config::Settings::load());
    match run(args, cwd, &home, &Managed::load(), &here, &mut person) {
        Ok(()) => ExitCode::SUCCESS,
        Err((ending, message)) => fail(ending, message),
    }
}

/// `managed` is the machine's layer, read only to say which servers it keeps from starting
/// (SERVERS-12). `here` is read only by `list`, which is the one command reporting on a session.
fn run<R: BufRead, W: Write>(
    args: &[String],
    cwd: Cwd<'_>,
    home: &Home,
    managed: &Managed,
    here: &dyn Fn() -> Result<Here, Stopped>,
    person: &mut Person<R, W>,
) -> Result<(), Stopped> {
    let Some((command, rest)) = args.split_first() else {
        return Err(refused_with_the_forms(t!(mcp_needs_a_command).to_string()));
    };
    match command.as_str() {
        "add" => add(rest, cwd, home, managed, person),
        "enable" => enable(rest, cwd, home, managed, person),
        "disable" => disable(rest, cwd, home, person),
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
        "bravebot mcp add <alias> [-s <scope>] [--env <name>]... [--dir <path>] [--stdio] -- <program> [args...]",
        "bravebot mcp add <alias> [-s <scope>] --http <url>",
        "bravebot mcp enable <alias> [-s <scope>]",
        "bravebot mcp disable <alias> [-s <scope>]",
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

/// Declare a server, ask whether it is approved, and on a yes request it in the settings file `-s`
/// names, so the declaration a person just typed is one the next session in that scope starts.
fn add<R: BufRead, W: Write>(
    rest: &[String],
    cwd: Cwd<'_>,
    home: &Home,
    managed: &Managed,
    person: &mut Person<R, W>,
) -> Result<(), Stopped> {
    // Before the alias as well as among the flags after it, since Claude Code's own examples put
    // its options first.
    let mut scope = None;
    let mut start = 0;
    while matches!(rest.get(start).map(String::as_str), Some("-s" | "--scope")) {
        take_scope(rest.get(start + 1), &mut scope)?;
        start += 2;
    }
    let Some((alias, flags)) = rest[start..].split_first() else {
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
    let declaration = declared(flags, start + 1, &mut scope).map_err(|refusal| match refusal {
        Refusal::Said(stopped) => stopped,
        Refusal::Problem(found) => (
            Ending::Argument,
            t!(mcp_not_added, alias = alias, problem = problem(&found)).to_string(),
        ),
    })?;
    let scope = scope.unwrap_or(Scope::Local);

    let directory = writable(home, "add")?;
    let mut declarations = read(directory)?;
    // Read before anything is written, so a settings file the request cannot go in stops the
    // declaration too rather than leaving half of what was typed done.
    let mut settings = settings(scope, cwd, directory)?;
    let requested = requested(&mut settings, alias)?;
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
        Asked::No | Asked::Nobody if requested => {
            say(person, still_requested(&settings, alias));
            return Ok(());
        }
        Asked::No => {
            let command = enable_command(alias, scope);
            say(
                person,
                t!(mcp_declared_not_enabled, alias = alias, command = command),
            );
            return Ok(());
        }
        Asked::Nobody => {
            let command = enable_command(alias, scope);
            say(
                person,
                t!(mcp_nobody_asked, alias = alias, command = command),
            );
            return Ok(());
        }
    }
    enabled(&settings, directory, alias, requested, "add", person)?;
    kept_from_starting(managed, alias, &declaration, person);
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
/// A `-s` before it is read into `scope`. `before` is how many words after `add` precede `flags`,
/// so a stray word is named by its place in what was typed.
fn declared(
    flags: &[String],
    before: usize,
    scope: &mut Option<Scope>,
) -> Result<Declaration, Refusal> {
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
            "-s" | "--scope" => take_scope(value, scope)?,
            other if other.starts_with('-') => return Err(unknown_option(other).into()),
            // Named by its place and not by its text: a stray word here is most often a value,
            // as in `--env TOKEN sk-live` or `--env PATH TOKEN=sk-live`.
            _ => {
                let position = (before + index + 1) as i64;
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

/// A flag no form takes, shown up to its `=`, since `--env=NAME=value` and
/// `--http=https://user:pass@host` carry the very value SERVERS-10 never repeats.
fn unknown_option(typed: &str) -> Stopped {
    let flag = typed.split('=').next().unwrap_or(typed);
    argument(t!(cli_unknown_option, flag = shown(flag)))
}

/// Which settings file a request is written to, under the names Claude Code's `-s` gives its
/// three.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    /// `.bravebot/settings.local.json` in the directory this runs in, which is this person's own.
    Local,
    /// `.bravebot/settings.json` there, which whoever clones the checkout reads too.
    Project,
    /// `settings.json` in the state directory, which a session in any directory reads.
    User,
}

impl Scope {
    /// In the order the settings reader lays them over one another.
    const ALL: [Self; 3] = [Self::User, Self::Project, Self::Local];

    /// The file its request is kept in.
    fn file(self, cwd: Cwd<'_>, directory: &Path) -> Result<PathBuf, Stopped> {
        match self {
            Self::User => Ok(bravebot_config::user_settings_file(directory)),
            Self::Project => Ok(bravebot_config::project_settings_file(checkout(cwd)?)),
            Self::Local => Ok(bravebot_config::local_settings_file(checkout(cwd)?)),
        }
    }

    /// What names it on a command line after the alias, which for the default is nothing.
    fn flag(self) -> &'static str {
        match self {
            Self::Local => "",
            Self::Project => " -s project",
            Self::User => " -s user",
        }
    }
}

/// Read the value of a `-s` into `scope`, refusing a second one rather than letting the last win:
/// the two name different files, and writing whichever came last would be a guess at which was
/// meant.
fn take_scope(value: Option<&String>, scope: &mut Option<Scope>) -> Result<(), Stopped> {
    let value = value.ok_or_else(|| argument(t!(mcp_scope_needs_a_value)))?;
    if scope.is_some() {
        return Err(argument(t!(mcp_two_scopes)));
    }
    *scope = Some(match value.as_str() {
        "local" => Scope::Local,
        "project" => Scope::Project,
        "user" => Scope::User,
        other => return Err(argument(t!(mcp_not_a_scope, scope = shown(other)))),
    });
    Ok(())
}

fn checkout(cwd: Cwd<'_>) -> Result<&Path, Stopped> {
    cwd.map_err(|error| {
        (
            Ending::Failed,
            t!(mcp_no_current_directory, error = error).to_string(),
        )
    })
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
    let declaration = entry.declaration.map_err(|found| unusable(alias, &found))?;
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
    let directory = writable(home, "approve")?;
    let declarations = read(directory)?;
    let entry = declarations.get(alias).ok_or_else(|| not_declared(alias))?;
    let declaration = entry.declaration.map_err(|found| unusable(alias, &found))?;
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

/// Request a declared server in the settings file `-s` names, which is what makes a session there
/// start it (SERVERS-2), asking SERVERS-3's question first where it is not approved yet.
///
/// Nothing is written unless the answer is yes or was yes before: the question is the approval,
/// and a request for a server nobody approved is one every session would stop to ask about.
fn enable<R: BufRead, W: Write>(
    rest: &[String],
    cwd: Cwd<'_>,
    home: &Home,
    managed: &Managed,
    person: &mut Person<R, W>,
) -> Result<(), Stopped> {
    let (alias, scope) = alias_and_scope("enable", rest)?;
    let scope = scope.unwrap_or(Scope::Local);
    let directory = writable(home, "enable")?;
    let declarations = read(directory)?;
    let entry = declarations.get(alias).ok_or_else(|| not_declared(alias))?;
    let declaration = entry.declaration.map_err(|found| unusable(alias, &found))?;
    let mut settings = settings(scope, cwd, directory)?;
    let requested = requested(&mut settings, alias)?;
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
        Asked::No | Asked::Nobody if requested => {
            return Err((Ending::Refused, still_requested(&settings, alias)));
        }
        Asked::No => {
            return Err((
                Ending::Refused,
                t!(mcp_not_enabled, alias = alias).to_string(),
            ));
        }
        Asked::Nobody => {
            let command = enable_command(alias, scope);
            return Err((
                Ending::Refused,
                t!(mcp_nobody_to_enable, alias = alias, command = command).to_string(),
            ));
        }
    }
    enabled(&settings, directory, alias, requested, "enable", person)?;
    kept_from_starting(managed, alias, &declaration, person);
    Ok(())
}

/// Take a server out of `mcp.request`: in the file `-s` names, or in each of the three that holds
/// it where no `-s` was given, since a person who wants it gone does not have to know which file
/// asked for it.
///
/// The declaration and its approval stay, so `enable` puts the request back without asking again.
fn disable<R: BufRead, W: Write>(
    rest: &[String],
    cwd: Cwd<'_>,
    home: &Home,
    person: &mut Person<R, W>,
) -> Result<(), Stopped> {
    let (alias, scope) = alias_and_scope("disable", rest)?;
    let directory = writable(home, "disable")?;
    let scopes = match scope {
        Some(scope) => vec![scope],
        None => Scope::ALL.to_vec(),
    };
    // Every file is read before any is written, so one that cannot be read leaves all of them as
    // they were rather than the alias gone from some.
    let mut files: Vec<Destination> = Vec::new();
    for scope in scopes {
        let path = scope.file(cwd, directory)?;
        // Run from the directory the state directory is in, a checkout's file is the user's own:
        // the settings reader reads it once, and a second copy would find the first one's write.
        if files.iter().any(|file| file.path() == path) {
            continue;
        }
        files.push(open_settings(&path, directory)?);
    }
    let mut withdrawn = false;
    for file in &mut files {
        if file.withdraw(alias) {
            write_settings(file, directory, "disable")?;
            say(
                person,
                t!(
                    mcp_disabled,
                    alias = shown(alias),
                    path = file_named(file.path())
                ),
            );
            withdrawn = true;
        }
    }
    if !withdrawn {
        let paths: Vec<String> = files.iter().map(|file| file_named(file.path())).collect();
        say(
            person,
            t!(
                mcp_enabled_nowhere,
                alias = shown(alias),
                paths = paths.join(", ")
            ),
        );
    }
    Ok(())
}

/// The alias and the `-s` that `enable` and `disable` take, in either order.
///
/// The alias is not checked against [`mcp::is_alias`], for the reason [`one_alias`] gives.
fn alias_and_scope<'a>(
    command: &str,
    rest: &'a [String],
) -> Result<(&'a str, Option<Scope>), Stopped> {
    let mut alias = None;
    let mut scope = None;
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "-s" | "--scope" => {
                take_scope(rest.get(index + 1), &mut scope)?;
                index += 1;
            }
            flag if flag.starts_with('-') => return Err(unknown_option(flag)),
            extra if alias.is_some() => return Err(unexpected(command, extra)),
            word => alias = Some(word),
        }
        index += 1;
    }
    let alias = alias.ok_or_else(|| {
        (
            Ending::Argument,
            t!(mcp_needs_an_alias, command = command).to_string(),
        )
    })?;
    Ok((alias, scope))
}

/// The command that asks again about `alias` and requests it in `scope`.
fn enable_command(alias: &str, scope: Scope) -> String {
    format!("bravebot mcp enable {}{}", shown(alias), scope.flag())
}

/// The settings file `scope` names, read to be written back.
fn settings(scope: Scope, cwd: Cwd<'_>, directory: &Path) -> Result<Destination, Stopped> {
    open_settings(&scope.file(cwd, directory)?, directory)
}

fn open_settings(path: &Path, directory: &Path) -> Result<Destination, Stopped> {
    unlinked(path, directory)?;
    Destination::open(path).map_err(|why| settings_unwritable(why, path, ""))
}

/// Refuse a link in a checkout, at `.bravebot` or at the file, rather than follow or replace it.
///
/// It arrives with a clone, so where it leads is whoever wrote the checkout's choice: following it
/// would put the request in whichever of this person's files they named, and replacing it would
/// copy what that file holds into the checkout. A file in the state directory itself is followed,
/// as an import follows the user's: that is the user's file, and run from the directory the state
/// directory is in, the other two.
fn unlinked(path: &Path, directory: &Path) -> Result<(), Stopped> {
    let project = path.parent().unwrap_or(path);
    if project == directory {
        return Ok(());
    }
    match [project, path].into_iter().find(|at| is_link(at)) {
        Some(link) => Err((
            Ending::Configuration,
            t!(mcp_settings_link, path = file_named(link)).to_string(),
        )),
        None => Ok(()),
    }
}

fn is_link(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|found| found.file_type().is_symlink())
}

/// Put `alias` in the request `settings` holds, in memory, and say whether it was there already.
///
/// Refused where `mcp` or `mcp.request` is something other than what the settings reader reads,
/// since replacing it would lose what somebody wrote there, and where the file would outgrow what
/// the reader reads: both before the question, so a refusal leaves nothing declared or approved.
fn requested(settings: &mut Destination, alias: &str) -> Result<bool, Stopped> {
    if settings.requests(alias) {
        return Ok(true);
    }
    if !settings.request(alias) {
        return Err((
            Ending::Configuration,
            t!(mcp_settings_not_a_list, path = file_named(settings.path())).to_string(),
        ));
    }
    settings
        .text()
        .map_err(|why| settings_unwritable(why, settings.path(), ""))?;
    Ok(false)
}

/// Write the request [`requested`] made, where it was not there before, and say where it is.
fn enabled<R, W: Write>(
    settings: &Destination,
    directory: &Path,
    alias: &str,
    already: bool,
    command: &str,
    person: &mut Person<R, W>,
) -> Result<(), Stopped> {
    let path = file_named(settings.path());
    if already {
        say(person, t!(mcp_already_enabled, alias = alias, path = path));
        return Ok(());
    }
    write_settings(settings, directory, command)?;
    say(person, t!(mcp_enabled, alias = alias, path = path));
    Ok(())
}

/// What a no, or nobody to ask, comes to where the file already requests `alias`: the request
/// stands, and the next session that reads it asks.
fn still_requested(settings: &Destination, alias: &str) -> String {
    t!(
        mcp_requested_not_approved,
        alias = alias,
        path = file_named(settings.path())
    )
    .to_string()
}

/// Say so where the managed layer keeps a server just enabled from starting, which is the line
/// [`get`] draws too (SERVERS-12).
fn kept_from_starting<R, W: Write>(
    managed: &Managed,
    alias: &str,
    declaration: &Declaration,
    person: &mut Person<R, W>,
) {
    let environment = |name: &str| std::env::var_os(name);
    if let Some(reason) = crate::servers::refused_declaration(managed, declaration, &environment) {
        say(
            person,
            t!(mcp_enabled_not_started, alias = alias, reason = reason),
        );
    }
}

/// Put a settings file on disk whole, in place of the one it was read from.
///
/// Through a link rather than over it, so a settings file kept among somebody's dotfiles stays
/// where they keep it. A checkout's link is refused again here and not only when the file was
/// read, since one may have been made while the question waited.
fn write_settings(settings: &Destination, directory: &Path, command: &str) -> Result<(), Stopped> {
    let path = settings.path();
    // The question takes as long as the person does, and another program may write the file
    // meanwhile.
    if settings.changed() {
        return Err(settings_unwritable(Unwritable::Changed, path, command));
    }
    unlinked(path, directory)?;
    let text = settings
        .text()
        .map_err(|why| settings_unwritable(why, path, command))?;
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if let Some(parent) = target.parent() {
        bravebot_agent::home::create_directory(parent).map_err(|error| {
            (
                Ending::Failed,
                t!(
                    mcp_not_written,
                    path = parent.display().to_string(),
                    error = error.to_string()
                )
                .to_string(),
            )
        })?;
    }
    replace(&target, text.expose())
}

fn settings_unwritable(why: Unwritable, path: &Path, command: &str) -> Stopped {
    let path = file_named(path);
    match why {
        Unwritable::NotADocument => (
            Ending::Configuration,
            t!(mcp_settings_not_a_document, path = path).to_string(),
        ),
        Unwritable::TooLarge => (
            Ending::Configuration,
            t!(mcp_settings_too_large, path = path).to_string(),
        ),
        Unwritable::Changed => (
            Ending::Failed,
            t!(mcp_settings_changed, path = path, command = command).to_string(),
        ),
    }
}

fn file_named(path: &Path) -> String {
    shown(&path.display().to_string())
}

fn remove<R: BufRead, W: Write>(
    alias: &str,
    home: &Home,
    person: &mut Person<R, W>,
) -> Result<(), Stopped> {
    let directory = writable(home, "remove")?;
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
    let directory = writable(home, "forget")?;
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

fn unusable(alias: &str, found: &Problem) -> Stopped {
    (
        Ending::Configuration,
        t!(mcp_unusable, alias = shown(alias), problem = problem(found)).to_string(),
    )
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

/// The state directory, where this run of `command` may write to it.
fn writable<'a>(home: &'a Home, command: &str) -> Result<&'a Path, Stopped> {
    match (&home.directory, home.writable) {
        (Some(directory), true) => Ok(directory),
        (Some(_), false) => Err((
            Ending::Failed,
            t!(mcp_not_while_incognito, command = command).to_string(),
        )),
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

    /// The checkout a command run by [`typing`] runs in, made where it is not there yet.
    fn checkout(directory: &Path) -> PathBuf {
        let checkout = directory.join("checkout");
        std::fs::create_dir_all(&checkout).expect("create the checkout");
        checkout
    }

    /// Run a command as a person at a terminal who types `typed`, in [`checkout`]. The machine's
    /// layer is `managed.json` in `directory`, absent unless a test writes it.
    fn typing(directory: &Path, args: &[&str], typed: &str) -> (Result<(), Stopped>, String) {
        let nothing = Here {
            project: checkout(directory),
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
        let cwd = here.project.clone();
        let here = || {
            Ok(Here {
                project: here.project.clone(),
                requested: here.requested.clone(),
            })
        };
        let outcome = run(&words(args), Ok(&cwd), &home, &managed, &here, &mut person);
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
        let declared_by = |flags: &[&str]| declared(&words(flags), 1, &mut None).ok();

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

        let refused = |flags: &[&str]| match declared(&words(flags), 1, &mut None) {
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
            let Ok(Declaration::Stdio { variables, .. }) = declared(&words(flags), 1, &mut None)
            else {
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
            Ok(&directory),
            &home,
            &Managed::default(),
            &|| unreachable!("forget reports on no session"),
            &mut person,
        );
        assert_eq!(outcome.map_err(|(ending, _)| ending), Err(Ending::Failed));
    }

    /// Run a command with nobody at a terminal, in [`checkout`].
    fn unattended(directory: &Path, args: &[&str]) -> (Result<(), Stopped>, String) {
        let home = Home {
            directory: Some(directory.to_path_buf()),
            writable: true,
        };
        let mut person = Person {
            answers: "".as_bytes(),
            screen: Vec::new(),
            present: false,
        };
        let cwd = checkout(directory);
        let outcome = run(
            &words(args),
            Ok(&cwd),
            &home,
            &Managed::default(),
            &|| unreachable!("{} reports on no session", args[0]),
            &mut person,
        );
        (outcome, String::from_utf8(person.screen).unwrap())
    }

    /// What the settings layers in force in [`checkout`] request, as a session there reads them.
    fn requested_in(directory: &Path) -> Vec<(PathBuf, String)> {
        let settings = bravebot_config::Settings::layered(
            Some(directory.to_path_buf()),
            Some(&checkout(directory)),
            None,
        );
        settings
            .mcp_requested()
            .map(|(path, alias)| (path.to_path_buf(), alias.to_string()))
            .collect()
    }

    fn text(path: &Path) -> String {
        std::fs::read_to_string(path).expect("written")
    }

    /// A file requesting weather alone, as bravebot writes one.
    const WEATHER_ALONE: &str =
        "{\n  \"mcp\": {\n    \"request\": [\n      \"weather\"\n    ]\n  }\n}\n";

    /// SERVERS-2: each of `-s`'s three values writes the file the settings reader takes that layer
    /// from, local where none is given, with `-s` before the alias or after it.
    #[test]
    fn enable_requests_the_server_in_the_file_its_scope_names() {
        let directory = scratch("cli-mcp-enable-scopes");
        let checkout = checkout(&directory);
        let local = bravebot_config::local_settings_file(&checkout);
        let project = bravebot_config::project_settings_file(&checkout);
        let user = bravebot_config::user_settings_file(&directory);
        for (args, file) in [
            (&["enable", "weather"][..], &local),
            (&["enable", "weather", "-s", "local"], &local),
            (&["enable", "weather", "-s", "project"], &project),
            (&["enable", "-s", "user", "weather"], &user),
            (&["enable", "weather", "--scope", "project"], &project),
        ] {
            for file in [&local, &project, &user] {
                let _ = std::fs::remove_file(file);
            }
            let _ = std::fs::remove_file(mcp::approvals_file(&directory));
            let (outcome, _) = typing(&directory, ADD, "n\n");
            assert!(outcome.is_ok(), "{outcome:?}");

            let (outcome, screen) = typing(&directory, args, "y\n");
            assert!(outcome.is_ok(), "{args:?}: {outcome:?}");
            assert_eq!(
                requested_in(&directory),
                [(file.clone(), "weather".to_string())],
                "{args:?}"
            );
            assert!(approved(&directory, &weather()), "{args:?}");
            let enabled = t!(mcp_enabled, alias = "weather", path = file_named(file));
            assert!(screen.contains(&enabled.to_string()), "{args:?}: {screen}");
        }
    }

    /// The file a request is added to keeps every other key, and a request already there is not
    /// written twice.
    #[test]
    fn enable_adds_the_alias_once_and_keeps_the_rest_of_the_file() {
        let directory = scratch("cli-mcp-enable-kept");
        let local = bravebot_config::local_settings_file(&checkout(&directory));
        std::fs::create_dir_all(local.parent().unwrap()).unwrap();
        std::fs::write(
            &local,
            r#"{"theme": "dark", "env": {"A": "1"}, "mcp": {"request": ["docs"]}}"#,
        )
        .unwrap();
        let (outcome, _) = typing(&directory, ADD, "n\n");
        assert!(outcome.is_ok(), "{outcome:?}");

        let (outcome, _) = typing(&directory, &["enable", "weather"], "y\n");
        assert!(outcome.is_ok(), "{outcome:?}");
        let expected = r#"{
  "env": {
    "A": "1"
  },
  "mcp": {
    "request": [
      "docs",
      "weather"
    ]
  },
  "theme": "dark"
}
"#;
        assert_eq!(text(&local), expected);

        let before = std::fs::read(&local).unwrap();
        let (outcome, screen) = typing(&directory, &["enable", "weather"], "");
        assert!(outcome.is_ok(), "{outcome:?}");
        assert_eq!(std::fs::read(&local).unwrap(), before);
        let already = t!(
            mcp_already_enabled,
            alias = "weather",
            path = file_named(&local)
        );
        assert!(screen.contains(&already.to_string()), "{screen}");
    }

    /// SERVERS-3: `enable` is the approval's question as well as the request, so a no, or nobody
    /// to ask, writes neither and ends refused, naming the command that asks again.
    #[test]
    fn enable_writes_nothing_without_a_yes() {
        let directory = scratch("cli-mcp-enable-no");
        let checkout = checkout(&directory);
        let (outcome, _) = typing(&directory, ADD, "n\n");
        assert!(outcome.is_ok(), "{outcome:?}");

        let (outcome, _) = typing(&directory, &["enable", "weather"], "n\n");
        assert_eq!(outcome.map_err(|(ending, _)| ending), Err(Ending::Refused));
        let (outcome, _) = unattended(&directory, &["enable", "weather", "-s", "project"]);
        let (ending, said) = outcome.expect_err("nobody was asked and it was enabled");
        assert_eq!(ending, Ending::Refused);
        assert!(
            said.contains("bravebot mcp enable weather -s project"),
            "{said}"
        );

        assert!(!approved(&directory, &weather()));
        assert!(requested_in(&directory).is_empty());
        assert!(!checkout.join(".bravebot").exists());
    }

    /// A file the request cannot go in without losing what it says is left byte for byte as it was,
    /// and so is everything else: the question is not asked and nothing is approved.
    #[test]
    fn enable_leaves_a_settings_file_it_cannot_add_to_as_it_is() {
        let directory = scratch("cli-mcp-enable-unreadable");
        let local = bravebot_config::local_settings_file(&checkout(&directory));
        std::fs::create_dir_all(local.parent().unwrap()).unwrap();
        let (outcome, _) = typing(&directory, ADD, "n\n");
        assert!(outcome.is_ok(), "{outcome:?}");

        for text in [
            r#"{"mcp": {"#,
            r#"{"mcp": {"request": "weather"}}"#,
            r#"{"mcp": ["weather"]}"#,
            "[]",
        ] {
            std::fs::write(&local, text).unwrap();
            let (outcome, _) = typing(&directory, &["enable", "weather"], "y\n");
            assert_eq!(
                outcome.map_err(|(ending, _)| ending),
                Err(Ending::Configuration),
                "{text}"
            );
            assert_eq!(std::fs::read_to_string(&local).unwrap(), text);
            assert!(!approved(&directory, &weather()), "{text}");
        }

        let (outcome, _) = typing(&directory, &["enable", "nowhere"], "y\n");
        assert_eq!(outcome.map_err(|(ending, _)| ending), Err(Ending::Argument));
        assert_eq!(std::fs::read_to_string(&local).unwrap(), "[]");
    }

    /// A checkout's link leads wherever its author pointed it, so a request is not written through
    /// one, whether the link is the file or the directory it is in. The user's own file is written
    /// through a link, where it stays.
    #[cfg(unix)]
    #[test]
    fn enable_writes_through_no_link_in_a_checkout() {
        let directory = scratch("cli-mcp-enable-link");
        let checkout = checkout(&directory);
        let (outcome, _) = typing(&directory, ADD, "y\n");
        assert!(outcome.is_ok(), "{outcome:?}");
        std::fs::remove_dir_all(checkout.join(".bravebot")).unwrap();
        let elsewhere = directory.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        let theirs = elsewhere.join("settings.json");
        std::fs::write(&theirs, "{}").unwrap();

        std::fs::create_dir_all(checkout.join(".bravebot")).unwrap();
        let local = bravebot_config::local_settings_file(&checkout);
        std::os::unix::fs::symlink(&theirs, &local).unwrap();
        let (outcome, _) = typing(&directory, &["enable", "weather"], "");
        assert_eq!(
            outcome.map_err(|(ending, _)| ending),
            Err(Ending::Configuration)
        );
        // Refused as the file is read, before anything is declared or approved.
        let (outcome, _) = typing(&directory, &["add", "docs", "--", "/opt/docs-mcp"], "y\n");
        assert_eq!(
            outcome.map_err(|(ending, _)| ending),
            Err(Ending::Configuration)
        );
        assert!(
            Declarations::read(&directory)
                .unwrap()
                .get("docs")
                .is_none()
        );

        std::fs::remove_dir_all(checkout.join(".bravebot")).unwrap();
        std::os::unix::fs::symlink(&elsewhere, checkout.join(".bravebot")).unwrap();
        let (outcome, _) = typing(&directory, &["enable", "weather", "-s", "project"], "");
        assert_eq!(
            outcome.map_err(|(ending, _)| ending),
            Err(Ending::Configuration)
        );
        assert_eq!(std::fs::read_to_string(&theirs).unwrap(), "{}");

        let user = bravebot_config::user_settings_file(&directory);
        std::os::unix::fs::symlink(&theirs, &user).unwrap();
        let (outcome, _) = typing(&directory, &["enable", "weather", "-s", "user"], "");
        assert!(outcome.is_ok(), "{outcome:?}");
        assert!(user.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(text(&theirs), WEATHER_ALONE);
    }

    /// Without `-s`, `disable` takes the alias out of every file that requests it and names each;
    /// with one, out of that file alone. Every other key and alias stays, and so do the declaration
    /// and its approval.
    #[test]
    fn disable_takes_the_request_out_of_each_file_that_holds_it() {
        let directory = scratch("cli-mcp-disable");
        let checkout = checkout(&directory);
        let (outcome, _) = typing(&directory, ADD, "y\n");
        assert!(outcome.is_ok(), "{outcome:?}");
        let files = [
            bravebot_config::user_settings_file(&directory),
            bravebot_config::project_settings_file(&checkout),
            bravebot_config::local_settings_file(&checkout),
        ];
        let both = r#"{"theme": "dark", "mcp": {"request": ["weather", "docs"]}}"#;
        let docs = r#"{
  "mcp": {
    "request": [
      "docs"
    ]
  },
  "theme": "dark"
}
"#;
        for file in &files {
            std::fs::write(file, both).unwrap();
        }

        let (outcome, screen) = typing(&directory, &["disable", "weather", "-s", "project"], "");
        assert!(outcome.is_ok(), "{outcome:?}");
        assert_eq!(text(&files[1]), docs);
        assert_eq!(std::fs::read_to_string(&files[0]).unwrap(), both);
        assert_eq!(std::fs::read_to_string(&files[2]).unwrap(), both);
        assert_eq!(screen.lines().count(), 1, "{screen}");

        let (outcome, screen) = typing(&directory, &["disable", "weather"], "");
        assert!(outcome.is_ok(), "{outcome:?}");
        for file in &files {
            assert_eq!(text(file), docs, "{}", file.display());
        }
        for file in [&files[0], &files[2]] {
            let disabled = t!(mcp_disabled, alias = "weather", path = file_named(file));
            assert!(screen.contains(&disabled.to_string()), "{screen}");
        }
        assert_eq!(screen.lines().count(), 2, "{screen}");
        assert!(approved(&directory, &weather()));
        assert!(
            Declarations::read(&directory)
                .unwrap()
                .get("weather")
                .is_some()
        );

        let (outcome, screen) = typing(&directory, &["disable", "weather"], "");
        assert!(outcome.is_ok(), "{outcome:?}");
        assert!(screen.contains("weather is not enabled in"), "{screen}");
    }

    /// One file `disable` cannot read stops it before any is written, so the alias is not left
    /// requested in the one file nobody could see into and gone from the rest.
    #[test]
    fn disable_writes_no_file_where_one_cannot_be_read() {
        let directory = scratch("cli-mcp-disable-unreadable");
        let checkout = checkout(&directory);
        let user = bravebot_config::user_settings_file(&directory);
        let local = bravebot_config::local_settings_file(&checkout);
        std::fs::create_dir_all(local.parent().unwrap()).unwrap();
        let requested = r#"{"mcp": {"request": ["weather"]}}"#;
        std::fs::write(&user, requested).unwrap();
        std::fs::write(&local, r#"{"mcp": "#).unwrap();

        let (outcome, _) = typing(&directory, &["disable", "weather"], "");
        let (ending, said) = outcome.expect_err("an unreadable file was passed over");
        assert_eq!(ending, Ending::Configuration);
        assert!(said.contains(&file_named(&local)), "{said}");
        assert_eq!(std::fs::read_to_string(&user).unwrap(), requested);
        assert_eq!(std::fs::read_to_string(&local).unwrap(), r#"{"mcp": "#);
    }

    /// `add` takes `-s` as `enable` does, before the alias or among its flags, and requests the
    /// server in that scope once it is approved. A no, or nobody to ask, leaves it declared and
    /// requested nowhere, and says which `enable` asks again.
    #[test]
    fn add_requests_the_server_in_its_scope_only_once_approved() {
        let directory = scratch("cli-mcp-add-scope");
        let checkout = checkout(&directory);
        let user = bravebot_config::user_settings_file(&directory);
        let project = bravebot_config::project_settings_file(&checkout);
        let argv = ["--", "npx", "-y", "weather-mcp"];

        let mut args = vec!["add", "weather", "-s", "user"];
        args.extend(argv);
        let (outcome, screen) = typing(&directory, &args, "n\n");
        assert!(outcome.is_ok(), "{outcome:?}");
        assert!(requested_in(&directory).is_empty());
        let again = t!(
            mcp_declared_not_enabled,
            alias = "weather",
            command = "bravebot mcp enable weather -s user"
        );
        assert!(screen.contains(&again.to_string()), "{screen}");

        let (outcome, screen) = unattended(&directory, &args);
        assert!(outcome.is_ok(), "{outcome:?}");
        assert!(requested_in(&directory).is_empty());
        let asked = t!(
            mcp_nobody_asked,
            alias = "weather",
            command = "bravebot mcp enable weather -s user"
        );
        assert!(screen.contains(&asked.to_string()), "{screen}");

        let (outcome, _) = typing(&directory, &args, "y\n");
        assert!(outcome.is_ok(), "{outcome:?}");
        assert_eq!(requested_in(&directory), [(user, "weather".to_string())]);

        let mut first = vec!["add", "--scope", "project", "weather"];
        first.extend(argv);
        let (outcome, _) = typing(&directory, &first, "");
        assert!(outcome.is_ok(), "{outcome:?}");
        assert_eq!(text(&project), WEATHER_ALONE);
        assert!(!bravebot_config::local_settings_file(&checkout).exists());
    }

    /// `-s` names one of three files, and names one: a missing, unknown or second value is refused
    /// before anything is written, and a word after it is counted in the place a stray one is named
    /// by.
    #[test]
    fn a_scope_is_one_of_three_and_given_once() {
        let directory = scratch("cli-mcp-scope-refused");
        let (outcome, _) = typing(&directory, ADD, "y\n");
        assert!(outcome.is_ok(), "{outcome:?}");
        let _ = std::fs::remove_dir_all(checkout(&directory).join(".bravebot"));
        for args in [
            &["enable", "weather", "-s", "global"][..],
            &["enable", "weather", "-s"],
            &["enable", "-s", "user", "weather", "-s", "project"],
            &["enable", "weather", "--scope=user"],
            &["disable", "weather", "-s", "everywhere"],
            &["add", "-s"],
            &["add", "-s", "user", "-s", "user", "weather", "--", "npx"],
            &[
                "add", "weather", "-s", "user", "--scope", "project", "--", "npx",
            ],
            &["add", "weather", "-s", "shared", "--", "npx"],
        ] {
            let (outcome, _) = typing(&directory, args, "y\n");
            assert_eq!(
                outcome.map_err(|(ending, _)| ending),
                Err(Ending::Argument),
                "{args:?}"
            );
        }
        assert!(requested_in(&directory).is_empty());
        assert!(!bravebot_config::user_settings_file(&directory).exists());

        let stray = ["add", "-s", "user", "weather", "--env", "TOKEN", "sk-live"];
        let (outcome, _) = typing(&directory, &stray, "y\n");
        let (_, said) = outcome.expect_err("a stray word was taken");
        assert_eq!(said, t!(mcp_add_stray_argument, position = 6).to_string());
    }

    /// An incognito session writes nothing, so `enable` and `disable` are refused there as every
    /// other writing command is, and the refusal names the command.
    #[test]
    fn enable_and_disable_write_nothing_incognito() {
        let directory = scratch("cli-mcp-enable-incognito");
        let (outcome, _) = typing(&directory, ADD, "y\n");
        assert!(outcome.is_ok(), "{outcome:?}");
        let local = bravebot_config::local_settings_file(&checkout(&directory));
        let before = std::fs::read(&local).expect("add enabled it");
        let home = Home {
            directory: Some(directory.clone()),
            writable: false,
        };
        for (command, args) in [
            ("disable", &["disable", "weather"][..]),
            ("enable", &["enable", "weather", "-s", "user"]),
        ] {
            let mut person = Person {
                answers: "y\n".as_bytes(),
                screen: Vec::new(),
                present: true,
            };
            let cwd = checkout(&directory);
            let outcome = run(
                &words(args),
                Ok(&cwd),
                &home,
                &Managed::default(),
                &|| unreachable!("{} reports on no session", args[0]),
                &mut person,
            );
            assert_eq!(
                outcome,
                Err((
                    Ending::Failed,
                    t!(mcp_not_while_incognito, command = command).to_string()
                ))
            );
        }
        assert_eq!(std::fs::read(&local).unwrap(), before);
        assert!(!bravebot_config::user_settings_file(&directory).exists());
    }

    /// Run a command at a terminal in `cwd`, with `directory` as the state directory.
    fn typing_from(
        cwd: &Path,
        directory: &Path,
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
        let outcome = run(
            &words(args),
            Ok(cwd),
            &home,
            &Managed::default(),
            &|| unreachable!("{} reports on no session", args[0]),
            &mut person,
        );
        (outcome, String::from_utf8(person.screen).unwrap())
    }

    /// Run from the directory the state directory is in, a checkout's settings file is the user's
    /// own, and `disable` reads and writes it once, as the settings reader reads it once.
    #[test]
    fn disable_where_the_state_directory_is_writes_its_file_once() {
        let home = scratch("cli-mcp-disable-home");
        let directory = home.join(".bravebot");
        std::fs::create_dir_all(&directory).unwrap();
        let (outcome, _) = typing_from(&home, &directory, ADD, "y\n");
        assert!(outcome.is_ok(), "{outcome:?}");
        let (outcome, _) = typing_from(&home, &directory, &["enable", "weather", "-s", "user"], "");
        assert!(outcome.is_ok(), "{outcome:?}");

        let (outcome, screen) = typing_from(&home, &directory, &["disable", "weather"], "");
        assert!(outcome.is_ok(), "{outcome:?}");
        let empty = "{\n  \"mcp\": {\n    \"request\": []\n  }\n}\n";
        for file in [
            bravebot_config::user_settings_file(&directory),
            bravebot_config::local_settings_file(&home),
        ] {
            assert_eq!(text(&file), empty, "{}", file.display());
        }
        assert_eq!(screen.lines().count(), 2, "{screen}");
    }

    /// A state directory kept among somebody's dotfiles is theirs, so a request is written through
    /// it from the directory it is in, where a checkout's `.bravebot` link would be refused.
    #[cfg(unix)]
    #[test]
    fn a_linked_state_directory_is_written_through_from_the_directory_it_is_in() {
        let home = scratch("cli-mcp-enable-home-link");
        let kept = home.join("dotfiles");
        std::fs::create_dir_all(&kept).unwrap();
        let directory = home.join(".bravebot");
        std::os::unix::fs::symlink(&kept, &directory).unwrap();
        let (outcome, _) = typing_from(&home, &directory, ADD, "y\n");
        assert!(outcome.is_ok(), "{outcome:?}");
        let (outcome, _) = typing_from(
            &home,
            &directory,
            &["enable", "weather", "-s", "project"],
            "",
        );
        assert!(outcome.is_ok(), "{outcome:?}");

        assert_eq!(text(&kept.join("settings.local.json")), WEATHER_ALONE);
        assert_eq!(text(&kept.join("settings.json")), WEATHER_ALONE);
        assert!(
            directory
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    /// Answers once it has made `link` lead to `target`: a checkout changing while the question
    /// waits.
    #[cfg(unix)]
    struct Relinking<'a> {
        link: &'a Path,
        target: &'a Path,
        answer: &'a [u8],
    }

    #[cfg(unix)]
    impl std::io::Read for Relinking<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            if self.link.symlink_metadata().is_err() {
                std::os::unix::fs::symlink(self.target, self.link)?;
            }
            std::io::Read::read(&mut self.answer, buffer)
        }
    }

    /// A link made in a checkout after its file was read and before the answer came is refused as
    /// one there from the start is, so the request does not go wherever it leads.
    #[cfg(unix)]
    #[test]
    fn a_link_made_while_the_question_waits_is_not_written_through() {
        let directory = scratch("cli-mcp-enable-relinked");
        let checkout = checkout(&directory);
        let (outcome, _) = typing(&directory, ADD, "n\n");
        assert!(outcome.is_ok(), "{outcome:?}");
        let elsewhere = directory.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        let link = checkout.join(".bravebot");
        assert!(link.symlink_metadata().is_err());

        let home = Home {
            directory: Some(directory.clone()),
            writable: true,
        };
        let mut person = Person {
            answers: std::io::BufReader::new(Relinking {
                link: &link,
                target: &elsewhere,
                answer: b"y\n",
            }),
            screen: Vec::new(),
            present: true,
        };
        let args = words(&["enable", "weather", "-s", "project"]);
        let outcome = run(
            &args,
            Ok(&checkout),
            &home,
            &Managed::default(),
            &|| unreachable!("enable reports on no session"),
            &mut person,
        );
        assert_eq!(
            outcome.map_err(|(ending, _)| ending),
            Err(Ending::Configuration)
        );
        assert!(link.symlink_metadata().is_ok(), "the question was not put");
        assert!(!elsewhere.join("settings.json").exists());
    }

    /// A request that would take the file past what the settings reader reads is refused before
    /// the declaration is written or the question put, so the refusal leaves nothing behind.
    #[test]
    fn add_refuses_a_request_the_file_cannot_hold_before_declaring_anything() {
        let directory = scratch("cli-mcp-add-too-large");
        let local = bravebot_config::local_settings_file(&checkout(&directory));
        std::fs::create_dir_all(local.parent().unwrap()).unwrap();
        // Under the limit as written, and past it once each entry is on a line of its own.
        let entries = vec!["0"; 20_000].join(",");
        let written = format!("{{\"list\":[{entries}]}}");
        std::fs::write(&local, &written).unwrap();

        let (outcome, _) = typing(&directory, ADD, "y\n");
        assert_eq!(
            outcome.map_err(|(ending, _)| ending),
            Err(Ending::Configuration)
        );
        assert!(!mcp::declarations_file(&directory).exists());
        assert!(!approved(&directory, &weather()));
        assert_eq!(text(&local), written);
    }

    /// A no, or nobody to ask, where the file already requests the server leaves the request
    /// standing and says so, rather than that the server is not enabled.
    #[test]
    fn a_no_says_a_request_already_in_the_file_still_stands() {
        let directory = scratch("cli-mcp-no-still-requested");
        let local = bravebot_config::local_settings_file(&checkout(&directory));
        std::fs::create_dir_all(local.parent().unwrap()).unwrap();
        let requested = r#"{"mcp": {"request": ["weather"]}}"#;
        std::fs::write(&local, requested).unwrap();
        let stands = t!(
            mcp_requested_not_approved,
            alias = "weather",
            path = file_named(&local)
        )
        .to_string();

        let (outcome, screen) = typing(&directory, ADD, "n\n");
        assert!(outcome.is_ok(), "{outcome:?}");
        assert!(screen.contains(&stands), "{screen}");
        let (outcome, screen) = unattended(&directory, ADD);
        assert!(outcome.is_ok(), "{outcome:?}");
        assert!(screen.contains(&stands), "{screen}");
        let (outcome, _) = typing(&directory, &["enable", "weather"], "n\n");
        assert_eq!(outcome, Err((Ending::Refused, stands.clone())));
        let (outcome, _) = unattended(&directory, &["enable", "weather"]);
        assert_eq!(outcome, Err((Ending::Refused, stands)));
        assert_eq!(std::fs::read_to_string(&local).unwrap(), requested);
    }

    /// SERVERS-12: a server the managed layer refuses is still enabled where it is asked for, since
    /// the request is the person's, and the line after it says no session starts it, and why.
    #[test]
    fn enabling_a_server_the_managed_layer_refuses_says_it_is_not_started() {
        let directory = scratch("cli-mcp-enable-managed");
        let managed = directory.join("managed.json");
        std::fs::write(
            &managed,
            r#"{"mcp": {"deny": [{"command": ["/opt/weather-mcp"]}]}}"#,
        )
        .unwrap();
        let reason = t!(
            managed_denied,
            path = managed.display().to_string(),
            entry = "command /opt/weather-mcp"
        );
        let refused = t!(mcp_enabled_not_started, alias = "weather", reason = reason).to_string();

        let add = ["add", "weather", "--", "/opt/weather-mcp"];
        let (outcome, screen) = typing(&directory, &add, "y\n");
        assert!(outcome.is_ok(), "{outcome:?}");
        assert!(screen.contains(&refused), "{screen}");
        let (outcome, screen) = typing(&directory, &["enable", "weather", "-s", "project"], "");
        assert!(outcome.is_ok(), "{outcome:?}");
        assert!(screen.contains(&refused), "{screen}");

        let (outcome, screen) = typing(&directory, &["add", "docs", "--", "/opt/docs-mcp"], "y\n");
        assert!(outcome.is_ok(), "{outcome:?}");
        assert!(!screen.contains("not started"), "{screen}");
    }
}
