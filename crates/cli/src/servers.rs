//! The MCP servers a session starts with (SERVERS-2, SERVERS-4, SERVERS-6, SERVERS-10).
//!
//! A checkout names the aliases it wants, and naming one grants nothing. Each resolves against the
//! person's own declarations, is put to them where no answer of theirs covers the declaration it
//! resolves to, and is started confined (MCP-3) holding the variables it names and no others
//! (MCP-9). A declared alias nothing requested is not started.
//!
//! What a server says is read for whether its handshake succeeded and for nothing else, and no
//! tool of one is asked for: none is offered to the planner until a call can be put to the person
//! (SERVERS-7). A session holds the servers it started and a grant naming each, and that is all.

use crate::mcp::{self as command, Home, Person, say, shown};
use bravebot_config::mcp::{self, Approvals, Declaration, Declarations, Projects};
use bravebot_core::capability::{Capability, CapabilitySet, ServerAlias};
use bravebot_core::event::RecordingSink;
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
use bravebot_i18n::t;
use bravebot_mcp::{HttpServer, McpError, McpResult, StdioServer};
use bravebot_sandbox::base::{Prelude, base};
use bravebot_sandbox::policy::SandboxPolicy;
use bravebot_sandbox::{Stream, Variables};
use std::ffi::{OsStr, OsString};
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How long the servers have, together, to answer their handshakes.
///
/// Long enough for a runner fetching a package on its first launch. A server that has not
/// answered by then is left out of the session, and its thread is left waiting: it holds the
/// server, which is stopped when it answers late or when this process exits and its stdin closes.
const HANDSHAKE: Duration = Duration::from_secs(60);

/// Who answers for a server no answer of the person's covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Asking {
    /// Whoever is at the terminal, where somebody is.
    Person,
    /// Nobody: a one-shot run is not a conversation.
    OneShot,
    /// The command line said to answer every permission question yes (SERVERS-13).
    Bypass,
}

/// A server the session started, held for as long as it runs.
enum Started {
    Stdio(#[allow(dead_code)] StdioServer),
    Http(#[allow(dead_code)] HttpServer),
}

/// What a session reached, and what it has to say about the rest.
#[derive(Default)]
pub(crate) struct Reached {
    started: Vec<(String, Started)>,
    /// A line for each requested server that is not reached, and each answer that could not be
    /// kept, in the order they arose.
    pub(crate) notes: Vec<String>,
}

impl Reached {
    /// The aliases started, in order.
    pub(crate) fn aliases(&self) -> Vec<String> {
        self.started
            .iter()
            .map(|(alias, _)| alias.clone())
            .collect()
    }

    /// One grant for each server started, naming it and no other (SERVERS-9).
    pub(crate) fn grants(&self) -> Vec<ServerAlias> {
        self.started
            .iter()
            .map(|(alias, _)| ServerAlias::new(alias.as_str()))
            .collect()
    }

    /// Whether any process the session started is confined, which a local server is and a
    /// remote one is not.
    pub(crate) fn confined(&self) -> bool {
        self.started
            .iter()
            .any(|(_, server)| matches!(server, Started::Stdio(_)))
    }
}

/// Reach the servers the settings in force request for the workspace at `root`, putting a question
/// to `person` where one is needed and they are there to answer it.
pub(crate) fn for_this_session<R: BufRead, W: Write>(
    settings: &bravebot_config::Settings,
    root: &Path,
    asking: Asking,
    person: &mut Person<R, W>,
    diagnostics: Stream,
) -> Reached {
    let requested: Vec<(PathBuf, String)> = settings
        .mcp_requested()
        .map(|(file, alias)| (file.to_path_buf(), alias.to_string()))
        .collect();
    let project = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let home = Home {
        directory: bravebot_agent::home::directory(),
        writable: bravebot_agent::home::writable().is_some(),
    };
    reach(&requested, &project, &home, asking, person, diagnostics)
}

/// Whoever is at this process's terminal: both ends, for the reason `bravebot mcp` asks for both. A
/// question written into a pipe is one nobody read, and an answer read from one is one nobody gave.
pub(crate) fn at_the_terminal() -> Person<std::io::StdinLock<'static>, std::io::StdoutLock<'static>>
{
    Person {
        answers: std::io::stdin().lock(),
        screen: std::io::stdout().lock(),
        present: std::io::stdin().is_terminal() && std::io::stdout().is_terminal(),
    }
}

/// Nobody: what a one-shot run asks, which is no one.
pub(crate) fn nobody() -> Person<std::io::Empty, std::io::Sink> {
    Person {
        answers: std::io::empty(),
        screen: std::io::sink(),
        present: false,
    }
}

/// Reach every server `requested` names, asking whoever `asking` says where an answer is needed.
///
/// `requested` is each alias with the settings file that asked for it. `project` is the workspace
/// root, which is what answer 2 records. `diagnostics` is where a local server's stderr goes.
pub(crate) fn reach<R: BufRead, W: Write>(
    requested: &[(PathBuf, String)],
    project: &Path,
    home: &Home,
    asking: Asking,
    person: &mut Person<R, W>,
    diagnostics: Stream,
) -> Reached {
    let mut notes = Vec::new();
    let plans = settle(
        requested,
        project,
        home,
        asking,
        person,
        &|name| std::env::var_os(name),
        Prelude::current(),
        &mut notes,
    );
    let started = start(plans, diagnostics, &mut notes);
    Reached { started, notes }
}

/// How a requested server is to be started, once the question about it is answered.
#[derive(Debug)]
enum Plan {
    Stdio {
        /// The program, as the absolute path it resolved to.
        program: PathBuf,
        arguments: Vec<String>,
        variables: Variables,
        /// The directories the named `PATH` searches, which the program may need to read.
        searched: Vec<PathBuf>,
        directory: Option<PathBuf>,
    },
    Http {
        url: String,
    },
}

/// Settle each request: resolve it, and put it to the person where nothing answers it already.
///
/// Returns the servers to start. Every request that will not be is a line in `notes` saying why.
/// `prelude` is this platform's confinement base, and a local server is not asked about without one.
#[allow(clippy::too_many_arguments)]
fn settle<R: BufRead, W: Write>(
    requested: &[(PathBuf, String)],
    project: &Path,
    home: &Home,
    asking: Asking,
    person: &mut Person<R, W>,
    environment: &dyn Fn(&str) -> Option<OsString>,
    prelude: Option<Prelude>,
    notes: &mut Vec<String>,
) -> Vec<(String, Plan)> {
    if requested.is_empty() {
        return Vec::new();
    }
    let Some(directory) = &home.directory else {
        notes.push(
            t!(
                servers_none_reached,
                aliases = aliases(requested),
                reason = command::no_state_directory()
            )
            .to_string(),
        );
        return Vec::new();
    };
    let declarations = match Declarations::read(directory) {
        Ok(declarations) => declarations,
        Err(why) => {
            let reason = t!(
                mcp_unreadable,
                path = mcp::declarations_file(directory).display().to_string(),
                reason = command::unreadable(&why)
            );
            notes.push(
                t!(
                    servers_none_reached,
                    aliases = aliases(requested),
                    reason = reason
                )
                .to_string(),
            );
            return Vec::new();
        }
    };
    let mut approvals = Approvals::read(directory);
    let mut projects = Projects::read(directory);

    let mut plans = Vec::new();
    for (file, alias) in requested {
        let Some(entry) = declarations.get(alias) else {
            notes.push(
                t!(
                    servers_not_declared,
                    file = named(file, project),
                    alias = shown(alias)
                )
                .to_string(),
            );
            continue;
        };
        let declaration = match entry.declaration {
            Ok(declaration) => declaration,
            Err(found) => {
                notes.push(
                    t!(
                        mcp_unusable,
                        alias = shown(alias),
                        problem = command::problem(&found)
                    )
                    .to_string(),
                );
                continue;
            }
        };
        let plan = match planned(&declaration, environment) {
            Ok(plan) => plan,
            Err(reason) => {
                notes.push(t!(servers_not_reached, alias = alias, reason = reason).to_string());
                continue;
            }
        };
        if matches!(plan, Plan::Stdio { .. }) && prelude.is_none() {
            notes.push(t!(servers_no_confinement_here, alias = alias).to_string());
            continue;
        }

        let digest = declaration.digest();
        let changed = approvals.changed(alias, &digest);
        // A recorded project answers for a server nobody has seen here, and not for one somebody
        // saw as something else: that is a server they have not seen either (SERVERS-5).
        let answered = approvals.approves(&digest)
            || (projects.contains(project) && !changed)
            || asking == Asking::Bypass;
        if !answered {
            let nobody = match asking {
                Asking::OneShot => Some(t!(servers_nobody_in_a_one_shot, alias = alias)),
                _ if !person.present => Some(t!(servers_nobody_at_a_terminal, alias = alias)),
                _ => None,
            };
            if let Some(reason) = nobody {
                notes.push(reason.to_string());
                continue;
            }
            let question = Question {
                alias,
                file: &named(file, project),
                declaration: &declaration,
                program: match &plan {
                    Plan::Stdio { program, .. } => Some(program),
                    Plan::Http { .. } => None,
                },
                changed,
            };
            match question.put(person) {
                Answer::No => {
                    notes.push(t!(servers_declined, alias = alias).to_string());
                    continue;
                }
                answer => {
                    let kept = keep(
                        home,
                        directory,
                        &declarations,
                        &mut approvals,
                        &mut projects,
                        (answer == Answer::Project).then_some(project),
                        alias,
                        &declaration,
                    );
                    notes.extend(kept);
                }
            }
        }
        plans.push((alias.clone(), plan));
    }
    plans
}

/// The requested aliases, as one list.
fn aliases(requested: &[(PathBuf, String)]) -> String {
    requested
        .iter()
        .map(|(_, alias)| shown(alias))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A settings file as the person knows it: relative to the project where it is inside it.
///
/// `project` has its links followed, so the file is compared with its own followed too.
fn named(file: &Path, project: &Path) -> String {
    let file = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
    file.strip_prefix(project)
        .unwrap_or(&file)
        .display()
        .to_string()
}

/// What a declaration starts, read against this process's environment for the variables it names.
///
/// A program named rather than given as a path is looked for in the `PATH` the declaration names,
/// and in no other: the server runs with that one, so a program found in this process's own
/// would be a program the server's environment does not lead to.
fn planned(
    declaration: &Declaration,
    environment: &dyn Fn(&str) -> Option<OsString>,
) -> Result<Plan, String> {
    let (argv, names, directory) = match declaration {
        Declaration::Http { url } => return Ok(Plan::Http { url: url.clone() }),
        Declaration::Stdio {
            argv,
            variables,
            directory,
        } => (argv, variables, directory),
    };
    let variables = names
        .iter()
        .filter_map(|name| environment(name).map(|value| (name, value)))
        .fold(Variables::new(), |variables, (name, value)| {
            variables.with(name.as_str(), value)
        });
    let path = variables
        .iter()
        .find(|(name, _)| name == "PATH")
        .map(|(_, value)| value.clone());
    let searched: Vec<PathBuf> = path
        .as_deref()
        .map(|path| {
            std::env::split_paths(path)
                .filter(|directory| directory.is_absolute())
                .collect()
        })
        .unwrap_or_default();
    let (named, arguments) = argv
        .split_first()
        .ok_or_else(|| command::problem(&mcp::Problem::Program))?;
    let program = resolve(named, path.as_deref(), &searched)?;
    Ok(Plan::Stdio {
        program,
        arguments: arguments.to_vec(),
        variables,
        searched,
        directory: directory.as_ref().map(PathBuf::from),
    })
}

/// The path a declaration's program starts from.
fn resolve(program: &str, path: Option<&OsStr>, searched: &[PathBuf]) -> Result<PathBuf, String> {
    let given = Path::new(program);
    if given.is_absolute() {
        return Ok(given.to_path_buf());
    }
    if given.components().count() != 1 {
        return Err(t!(servers_program_relative, program = shown(program)).to_string());
    }
    if path.is_none() {
        return Err(t!(servers_program_without_path, program = shown(program)).to_string());
    }
    searched
        .iter()
        .map(|directory| directory.join(program))
        .find(|candidate| startable(candidate) && candidate.to_str().is_some())
        .ok_or_else(|| t!(servers_program_not_found, program = shown(program)).to_string())
}

/// Whether a file is there to be started.
fn startable(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        metadata.is_file()
    }
}

/// What the person said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Answer {
    /// Approve this declaration.
    Once,
    /// Approve it, and every server a checkout in this project requests from now on.
    Project,
    /// Leave it out of this session.
    No,
}

/// SERVERS-4's question about one server.
struct Question<'a> {
    alias: &'a str,
    /// The settings file that requested it.
    file: &'a str,
    declaration: &'a Declaration,
    /// Where a local server's program resolved to.
    program: Option<&'a PathBuf>,
    /// Whether the person approved this alias as something else.
    changed: bool,
}

impl Question<'_> {
    /// Every line drawn above the answers.
    fn lines(&self) -> Vec<String> {
        let digest = self.declaration.digest();
        let indent = command::indent(self.alias);
        let mut lines = command::drawn(self.alias, self.declaration, &digest.short());
        let mut under = vec![format!(
            "{indent}{}",
            t!(servers_requested_by, file = self.file)
        )];
        if let (Some(program), Declaration::Stdio { argv, .. }) = (self.program, self.declaration)
            && argv.first().map(Path::new) != Some(program.as_path())
        {
            under.push(format!(
                "{indent}{}",
                t!(
                    servers_program,
                    path = shown(&program.display().to_string())
                )
            ));
        }
        lines.splice(1..1, under);
        if self.changed {
            lines.push(format!("{indent}{}", t!(servers_changed)));
        }
        lines.extend(fetching(self.alias, self.declaration));
        lines
    }

    /// Draw the question and read the answer. Anything but 1 or 2, and the end of the input, is 3.
    fn put<R: BufRead, W: Write>(&self, person: &mut Person<R, W>) -> Answer {
        say(person, "");
        for line in self.lines() {
            say(person, line);
        }
        say(person, "");
        say(person, format!("  {}", t!(mcp_question)));
        say(person, format!("  1. {}", t!(servers_answer_once)));
        say(person, format!("  2. {}", t!(servers_answer_project)));
        say(person, format!("  3. {}", t!(servers_answer_no)));
        let _ = write!(person.screen, "  {} ", t!(servers_answer));
        let _ = person.screen.flush();
        let mut typed = String::new();
        match person.answers.read_line(&mut typed) {
            Ok(0) | Err(_) => Answer::No,
            Ok(_) => match typed.trim() {
                "1" => Answer::Once,
                "2" => Answer::Project,
                _ => Answer::No,
            },
        }
    }
}

/// Keep a yes: the approval of this digest, and the project where answer 2 gave one.
///
/// Returns what could not be kept, since the server is still used in this session: the person
/// said yes, and a file that would not take the answer does not unsay it.
#[allow(clippy::too_many_arguments)]
fn keep(
    home: &Home,
    directory: &Path,
    declarations: &Declarations,
    approvals: &mut Approvals,
    projects: &mut Projects,
    project: Option<&Path>,
    alias: &str,
    declaration: &Declaration,
) -> Vec<String> {
    if !home.writable {
        return vec![t!(servers_for_this_session_only, alias = alias).to_string()];
    }
    let mut unkept = Vec::new();
    if let Err((_, reason)) =
        command::record(directory, declarations, approvals, alias, declaration)
    {
        unkept.push(t!(servers_not_kept, alias = alias, reason = reason).to_string());
    }
    if let Some(project) = project {
        let recorded = projects.add(project)
            && command::replace(&mcp::projects_file(directory), &projects.to_text()).is_ok();
        if !recorded {
            unkept.push(
                t!(
                    servers_project_not_kept,
                    path = shown(&project.display().to_string())
                )
                .to_string(),
            );
        }
    }
    unkept
}

/// The lines a question about `declaration` draws where its program fetches what it runs, under the
/// margin `alias` sets (SERVERS-6). None for any other.
pub(crate) fn fetching(alias: &str, declaration: &Declaration) -> Vec<String> {
    let Declaration::Stdio { argv, .. } = declaration else {
        return Vec::new();
    };
    let Some(fetched) = fetches(argv) else {
        return Vec::new();
    };
    let indent = command::indent(alias);
    let mut lines = vec![format!(
        "{indent}{}",
        t!(servers_fetches, runner = shown(&fetched.runner))
    )];
    lines.extend(fetched.unpinned.map(|unpinned| {
        let line = match unpinned {
            Unpinned::Package(package) => t!(servers_unpinned, package = shown(&package)),
            Unpinned::Unread(flag) => t!(
                servers_unread,
                flag = shown(&flag),
                runner = shown(&fetched.runner)
            ),
        };
        format!("{indent}{line}")
    }));
    lines
}

/// A program that fetches what it runs as it starts, and what its words say of the version it runs
/// where they do not say it is one exact version.
#[derive(Debug, PartialEq, Eq)]
struct Fetches {
    /// The runner, as the words it was invoked with.
    runner: String,
    unpinned: Option<Unpinned>,
}

#[derive(Debug, PartialEq, Eq)]
enum Unpinned {
    /// The package, which names no exact version.
    Package(String),
    /// A flag this does not know, which may take the next word as its value, so which word is the
    /// package is not known.
    Unread(String),
}

/// Whether `argv` starts a runner that resolves a package when it runs (SERVERS-6).
///
/// Read off the words the person declared and nothing else, so it says what the line asks for and
/// cannot say what the runner will find.
fn fetches(argv: &[String]) -> Option<Fetches> {
    let program = Path::new(argv.first()?).file_stem()?.to_str()?;
    let rest = &argv[1..];
    let (runner, rest, ecosystem) = match (program, rest.first().map(String::as_str)) {
        ("npx" | "bunx", _) => (program.to_string(), rest, Ecosystem::Node),
        ("pnpm" | "yarn", Some("dlx")) => (format!("{program} dlx"), &rest[1..], Ecosystem::Node),
        ("npm", Some("exec")) => ("npm exec".to_string(), &rest[1..], Ecosystem::Node),
        ("uvx", _) => (program.to_string(), rest, Ecosystem::Python),
        ("uv", Some("tool")) if rest.get(1).map(String::as_str) == Some("run") => {
            ("uv tool run".to_string(), &rest[2..], Ecosystem::Python)
        }
        ("pipx", Some("run")) => ("pipx run".to_string(), &rest[1..], Ecosystem::Python),
        _ => return None,
    };
    let unpinned = match package(rest, ecosystem) {
        Ok(package) => package
            .filter(|package| !ecosystem.pinned(package))
            .map(Unpinned::Package),
        Err(flag) => Some(Unpinned::Unread(flag)),
    };
    Some(Fetches { runner, unpinned })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ecosystem {
    Node,
    Python,
}

impl Ecosystem {
    /// The flags naming the package where it is not the first word.
    fn package_flags(self) -> &'static [&'static str] {
        match self {
            Self::Node => &["-p", "--package"],
            Self::Python => &["--from", "--spec"],
        }
    }

    /// The flags before the package that take no value.
    fn switches(self) -> &'static [&'static str] {
        match self {
            Self::Node => &["-y", "--yes", "--no", "-q", "--quiet", "--bun"],
            Self::Python => &[
                "-q",
                "--quiet",
                "-v",
                "--verbose",
                "--isolated",
                "--offline",
                "-n",
                "--no-cache",
                "--refresh",
            ],
        }
    }

    /// The flags before the package whose value is some other word.
    fn valued(self) -> &'static [&'static str] {
        match self {
            Self::Node => &["--registry", "--cache", "--userconfig"],
            Self::Python => &[
                "--with",
                "-w",
                "--python",
                "-p",
                "--index",
                "--index-url",
                "-i",
            ],
        }
    }

    /// Whether a package names one exact version.
    fn pinned(self, package: &str) -> bool {
        match self {
            // `@scope/name@1.2.3`: the version follows the last `@` that is not the first
            // character. A tag, a range, or no version at all is a different program on
            // different days, and so is `1.2`, which npm reads as every `1.2.x`.
            Self::Node => package
                .char_indices()
                .rfind(|&(at, c)| c == '@' && at > 0)
                .is_some_and(|(at, _)| release(&package[at + 1..]) == Some(3)),
            // `name==1.2.3`, or uvx's own `name@1.2.3`. Here `1.2` is `1.2.0` and nothing else.
            Self::Python => package
                .split_once("==")
                .or_else(|| package.split_once('@'))
                .is_some_and(|(_, version)| release(version.trim_start_matches('=')).is_some()),
        }
    }
}

/// How many numbers a version's release is, where it is numbers joined by dots with at most a
/// pre-release or build suffix after them, and so one release rather than a range or a tag.
fn release(version: &str) -> Option<usize> {
    let release = version.split(['-', '+']).next().unwrap_or_default();
    let parts: Vec<&str> = release.split('.').collect();
    parts
        .iter()
        .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
        .then_some(parts.len())
}

/// The package a runner's arguments name, or the flag before it that they cannot be read past.
///
/// A flag this does not know may take the next word as its value, and then that word is not the
/// package: guessing either way could name a pinned word and leave the package unnamed.
fn package(arguments: &[String], ecosystem: Ecosystem) -> Result<Option<String>, String> {
    let mut words = arguments.iter();
    while let Some(word) = words.next() {
        let (flag, value) = match word.split_once('=') {
            Some((flag, value)) if flag.starts_with('-') => (flag, Some(value)),
            _ => (word.as_str(), None),
        };
        if ecosystem.package_flags().contains(&flag) {
            return Ok(value.map(str::to_string).or_else(|| words.next().cloned()));
        }
        if word == "--" {
            return Ok(words.next().cloned());
        }
        if !word.starts_with('-') {
            return Ok(Some(word.clone()));
        }
        if value.is_some() || ecosystem.switches().contains(&flag) {
            continue;
        }
        if ecosystem.valued().contains(&flag) {
            words.next();
            continue;
        }
        return Err(word.clone());
    }
    Ok(None)
}

/// Start every planned server, and wait for their handshakes until [`HANDSHAKE`] has passed.
fn start(
    plans: Vec<(String, Plan)>,
    diagnostics: Stream,
    notes: &mut Vec<String>,
) -> Vec<(String, Started)> {
    if plans.is_empty() {
        return Vec::new();
    }
    let (sender, received) = mpsc::channel::<(String, McpResult<Started>)>();
    let mut waiting: Vec<String> = Vec::new();
    let sandbox = plans
        .iter()
        .any(|(_, plan)| matches!(plan, Plan::Stdio { .. }))
        .then(bravebot_sandbox::for_current_platform);

    for (alias, plan) in plans {
        let sender = sender.clone();
        match plan {
            Plan::Stdio {
                program,
                arguments,
                variables,
                searched,
                directory,
            } => {
                let sandbox = match &sandbox {
                    Some(Ok(sandbox)) => sandbox,
                    Some(Err(error)) => {
                        notes.push(
                            t!(
                                servers_not_confined,
                                alias = alias,
                                reason = error.to_string()
                            )
                            .to_string(),
                        );
                        continue;
                    }
                    None => unreachable!("a sandbox is looked for wherever a local server is"),
                };
                let Some(policy) = confinement_here(&program, &searched, directory.as_deref())
                else {
                    notes.push(t!(servers_no_confinement_here, alias = alias).to_string());
                    continue;
                };
                let policy = policy.nameable_under(&sandbox.capabilities()).policy;
                let launched = StdioServer::launch(
                    alias.as_str(),
                    program.to_str().unwrap_or_default(),
                    &arguments,
                    variables,
                    sandbox.as_ref(),
                    &policy,
                    diagnostics,
                );
                let mut server = match launched {
                    Ok(server) => server,
                    Err(error) => {
                        notes.push(
                            t!(
                                servers_not_reached,
                                alias = alias,
                                reason = error.to_string()
                            )
                            .to_string(),
                        );
                        continue;
                    }
                };
                waiting.push(alias.clone());
                std::thread::spawn(move || {
                    let outcome = server
                        .initialize("bravebot", env!("CARGO_PKG_VERSION"))
                        .map(|()| Started::Stdio(server));
                    let _ = sender.send((alias, outcome));
                });
            }
            Plan::Http { url } => {
                waiting.push(alias.clone());
                std::thread::spawn(move || {
                    let outcome = handshake_remote(&alias, url);
                    let _ = sender.send((alias, outcome));
                });
            }
        }
    }
    drop(sender);

    let deadline = Instant::now() + HANDSHAKE;
    let mut started = Vec::new();
    while !waiting.is_empty() {
        let Ok((alias, outcome)) =
            received.recv_timeout(deadline.saturating_duration_since(Instant::now()))
        else {
            break;
        };
        waiting.retain(|waited| *waited != alias);
        match outcome {
            Ok(server) => started.push((alias, server)),
            Err(error) => notes.push(
                t!(
                    servers_no_handshake,
                    alias = alias,
                    reason = error.to_string()
                )
                .to_string(),
            ),
        }
    }
    for alias in waiting {
        notes.push(
            t!(
                servers_too_slow,
                alias = alias,
                seconds = HANDSHAKE.as_secs()
            )
            .to_string(),
        );
    }
    started.sort_by(|(one, _), (other, _)| one.cmp(other));
    started
}

/// A remote server's handshake, through the one egress gate every request passes.
///
/// Under a policy holding the fetch capability and the grant naming this server, and no other:
/// the handshake is a request to one declared destination, and a hop off it is refused.
fn handshake_remote(alias: &str, url: String) -> McpResult<Started> {
    let mut sink = RecordingSink::new();
    let mut routing = Routing::new();
    routing.insert_trusted("server", alias);
    let mut policy = Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([
            Capability::WebFetch,
            Capability::McpCall(ServerAlias::new(alias)),
        ]),
        &mut sink,
    )
    .map_err(McpError::Denied)?;
    let egress = bravebot_net::Egress::new();
    let mut server = HttpServer::new(alias, url);
    server.initialize(&mut policy, &egress, "bravebot", env!("CARGO_PKG_VERSION"))?;
    Ok(Started::Http(server))
}

/// The system temporary directory with its links followed, which is how a backend matches it.
fn temporary_directory() -> PathBuf {
    // Nothing is created here: the path becomes the base's temporary row, which the spec's base
    // table grants, and a server makes its own files there under its own names.
    // nosemgrep: rust.lang.security.temp-dir.temp-dir
    let temporary = std::env::temp_dir();
    temporary.canonicalize().unwrap_or(temporary)
}

/// [`confinement`] on this machine: its platform's base, and the person's own profile directory as
/// the home kept out, which is not the state directory inside it.
fn confinement_here(
    program: &Path,
    searched: &[PathBuf],
    directory: Option<&Path>,
) -> Option<SandboxPolicy> {
    confinement(
        Prelude::current(),
        program,
        searched,
        directory,
        &temporary_directory(),
        bravebot_agent::home::profile().as_deref(),
    )
}

/// What a local server may reach, or `None` on a platform with no base to build it on.
///
/// The platform's base rows with no home directory given, so none of a person's own files is among
/// them. Then the directories the named `PATH` searches and the one the program resolved into,
/// each with its links followed, since a runner is a script whose interpreter is found through
/// `PATH` and whose code sits beside where it was installed. A `bin` directory brings its parent,
/// where an installation keeps what its programs load, and none of these reaches the home
/// directory: a `PATH` naming it, or a directory above it, is left out, and so is a parent inside
/// it, since `~/.cargo` holds a registry token beside `~/.cargo/bin`. The declared directory is the
/// one the server may write, and where it starts; without one it starts in the temporary
/// directory, and reads nothing of the workspace.
fn confinement(
    prelude: Option<Prelude>,
    program: &Path,
    searched: &[PathBuf],
    directory: Option<&Path>,
    temporary: &Path,
    home: Option<&Path>,
) -> Option<SandboxPolicy> {
    let mut policy = base(prelude?, temporary, None);
    let canonical = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    // Compared with paths whose links are followed, so a home reached through one is followed too.
    let home = home.map(canonical);
    let outside_home = |path: &Path| home.as_ref().is_none_or(|home| !home.starts_with(path));
    let beside_home = |path: &Path| {
        path.parent().is_some()
            && outside_home(path)
            && home.as_ref().is_none_or(|home| !path.starts_with(home))
    };
    let program = canonical(program);
    let readable = searched
        .iter()
        .map(|directory| canonical(directory))
        .chain(program.parent().map(Path::to_path_buf));
    for directory in readable {
        if directory.parent().is_none() || !outside_home(&directory) {
            continue;
        }
        if directory.file_name() == Some(OsStr::new("bin"))
            && let Some(parent) = directory.parent().filter(|parent| beside_home(parent))
        {
            policy = policy.allow_read(parent);
        }
        policy = policy.allow_read(directory);
    }
    Some(match directory {
        Some(directory) => {
            let directory = canonical(directory);
            policy
                .allow_read(&directory)
                .allow_write(&directory)
                .starting_in(directory)
        }
        None => policy.starting_in(temporary),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A state directory of its own under the build directory, emptied first.
    fn scratch(name: &str) -> PathBuf {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch");
        path.canonicalize().expect("canonical scratch")
    }

    fn words(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| word.to_string()).collect()
    }

    /// A program that is there to be started, in a directory of its own.
    fn installed(directory: &Path, name: &str) -> PathBuf {
        std::fs::create_dir_all(directory).expect("create bin");
        let program = directory.join(name);
        std::fs::write(&program, "#!/bin/sh\n").expect("write program");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755))
                .expect("chmod");
        }
        program
    }

    /// A state directory declaring `weather` as `argv` naming `PATH`, and the settings file a
    /// project asked for it in.
    fn declared(name: &str, argv: &[&str]) -> (PathBuf, PathBuf, Declaration) {
        let root = scratch(name);
        let home = root.join("home");
        let project = root.join("project");
        std::fs::create_dir_all(&home).expect("home");
        std::fs::create_dir_all(project.join(".bravebot")).expect("project");
        let declaration =
            Declaration::stdio(words(argv), vec!["PATH".to_string()], None).expect("declaration");
        let mut declarations = Declarations::default();
        declarations.insert("weather", &declaration);
        std::fs::write(mcp::declarations_file(&home), declarations.to_text()).expect("mcp.json");
        (home, project, declaration)
    }

    struct Settled {
        plans: Vec<(String, Plan)>,
        notes: Vec<String>,
        screen: String,
    }

    /// Settle `weather`'s request with `typed` at the terminal, a `PATH` naming `bin`, and nothing
    /// started.
    fn settled(
        home: &Path,
        project: &Path,
        asking: Asking,
        present: bool,
        writable: bool,
        typed: &str,
        bin: &Path,
    ) -> Settled {
        let requested = vec![(
            project.join(".bravebot/settings.json"),
            "weather".to_string(),
        )];
        let home = Home {
            directory: Some(home.to_path_buf()),
            writable,
        };
        let mut person = Person {
            answers: typed.as_bytes(),
            screen: Vec::new(),
            present,
        };
        let path = bin.as_os_str().to_owned();
        let environment = move |name: &str| (name == "PATH").then(|| path.clone());
        let mut notes = Vec::new();
        let plans = settle(
            &requested,
            project,
            &home,
            asking,
            &mut person,
            &environment,
            Prelude::current(),
            &mut notes,
        );
        Settled {
            plans,
            notes,
            screen: String::from_utf8(person.screen).expect("screen"),
        }
    }

    fn started(settled: &Settled) -> Vec<&str> {
        settled
            .plans
            .iter()
            .map(|(alias, _)| alias.as_str())
            .collect()
    }

    fn recorded_projects(home: &Path) -> String {
        std::fs::read_to_string(mcp::projects_file(home)).unwrap_or_default()
    }

    #[test]
    fn answer_one_approves_the_digest_answer_two_the_project_and_three_nothing() {
        for (typed, used, approved, project_recorded) in [
            ("1\n", true, true, false),
            ("2\n", true, true, true),
            ("3\n", false, false, false),
            ("yes\n", false, false, false),
            ("", false, false, false),
        ] {
            let (home, project, declaration) =
                declared("cli-servers-answers", &["weather-mcp", "--stdio"]);
            let bin = home.parent().unwrap().join("bin");
            installed(&bin, "weather-mcp");

            let settled = settled(&home, &project, Asking::Person, true, true, typed, &bin);

            assert_eq!(!settled.plans.is_empty(), used, "{typed:?}");
            assert_eq!(
                Approvals::read(&home).approves(&declaration.digest()),
                approved,
                "{typed:?}"
            );
            assert_eq!(
                Projects::read(&home).contains(&project),
                project_recorded,
                "{typed:?}"
            );
            assert!(settled.screen.contains(t!(mcp_question)), "{typed:?}");
            if !used {
                assert_eq!(
                    settled.notes,
                    vec![t!(servers_declined, alias = "weather").to_string()],
                    "{typed:?}"
                );
            }
        }
    }

    #[test]
    fn the_question_names_the_checkout_that_requested_it_and_where_the_program_resolved() {
        let (home, project, _) = declared("cli-servers-drawn", &["weather-mcp"]);
        let bin = home.parent().unwrap().join("bin");
        let program = installed(&bin, "weather-mcp");

        let settled = settled(&home, &project, Asking::Person, true, true, "3\n", &bin);

        let lines: Vec<&str> = settled.screen.lines().map(str::trim).collect();
        assert!(
            lines.contains(&"weather   stdio   weather-mcp"),
            "{lines:?}"
        );
        assert!(
            lines.contains(&t!(servers_requested_by, file = ".bravebot/settings.json").as_str()),
            "{lines:?}"
        );
        assert!(
            lines.contains(&t!(servers_program, path = program.display().to_string()).as_str()),
            "{lines:?}"
        );
        for answer in ["1.", "2.", "3."] {
            assert!(
                lines.iter().any(|line| line.starts_with(answer)),
                "{answer} {lines:?}"
            );
        }
    }

    #[test]
    fn nobody_to_ask_leaves_the_server_absent_says_why_and_records_nothing() {
        for (asking, present, reason) in [
            (
                Asking::OneShot,
                true,
                t!(servers_nobody_in_a_one_shot, alias = "weather"),
            ),
            (
                Asking::Person,
                false,
                t!(servers_nobody_at_a_terminal, alias = "weather"),
            ),
        ] {
            let (home, project, _) = declared("cli-servers-nobody", &["weather-mcp"]);
            let bin = home.parent().unwrap().join("bin");
            installed(&bin, "weather-mcp");

            let settled = settled(&home, &project, asking, present, true, "1\n", &bin);

            assert!(settled.plans.is_empty(), "{asking:?}");
            assert_eq!(settled.notes, vec![reason.to_string()], "{asking:?}");
            assert!(settled.screen.is_empty(), "{asking:?}: {}", settled.screen);
            assert!(!mcp::approvals_file(&home).exists(), "{asking:?}");
        }
    }

    #[test]
    fn a_request_nobody_declared_is_reported_and_nothing_is_started_for_it() {
        let (home, project, _) = declared("cli-servers-undeclared", &["weather-mcp"]);
        let requested = vec![(project.join(".bravebot/settings.json"), "docs".to_string())];
        let mut person = Person {
            answers: "1\n".as_bytes(),
            screen: Vec::new(),
            present: true,
        };
        let mut notes = Vec::new();
        let plans = settle(
            &requested,
            &project,
            &Home {
                directory: Some(home),
                writable: true,
            },
            Asking::Person,
            &mut person,
            &|_| None,
            Prelude::current(),
            &mut notes,
        );

        assert!(plans.is_empty());
        assert_eq!(
            notes,
            vec![
                t!(
                    servers_not_declared,
                    file = ".bravebot/settings.json",
                    alias = "docs"
                )
                .to_string()
            ]
        );
        assert!(person.screen.is_empty());
    }

    #[test]
    fn an_approved_digest_starts_unasked_and_a_recorded_project_answers_only_an_unchanged_one() {
        let (home, project, declaration) = declared("cli-servers-standing", &["weather-mcp"]);
        let bin = home.parent().unwrap().join("bin");
        installed(&bin, "weather-mcp");

        // Approved: started with nobody asked.
        let mut approvals = Approvals::default();
        approvals.approve("weather", declaration.digest());
        std::fs::write(mcp::approvals_file(&home), approvals.to_text()).expect("approve");
        let first = settled(&home, &project, Asking::OneShot, false, true, "", &bin);
        assert_eq!(started(&first), vec!["weather"]);
        assert!(first.screen.is_empty());

        // The project recorded, and the declaration changed since its approval: asked again.
        let mut projects = Projects::default();
        projects.add(&project);
        std::fs::write(mcp::projects_file(&home), projects.to_text()).expect("project");
        let changed = Declaration::stdio(
            words(&["weather-mcp", "--verbose"]),
            vec!["PATH".to_string()],
            None,
        )
        .expect("changed");
        let mut declarations = Declarations::default();
        declarations.insert("weather", &changed);
        std::fs::write(mcp::declarations_file(&home), declarations.to_text()).expect("mcp.json");
        let second = settled(&home, &project, Asking::Person, true, true, "3\n", &bin);
        assert!(second.plans.is_empty());
        assert!(
            second.screen.contains(t!(servers_changed)),
            "{}",
            second.screen
        );

        // A server nobody approved as anything: the project answers for it.
        std::fs::remove_file(mcp::approvals_file(&home)).expect("forget");
        let third = settled(&home, &project, Asking::OneShot, false, true, "", &bin);
        assert_eq!(started(&third), vec!["weather"]);
        assert!(third.notes.is_empty(), "{:?}", third.notes);
    }

    #[test]
    fn skipping_permissions_starts_the_server_unasked_and_records_nothing() {
        let (home, project, _) = declared("cli-servers-bypass", &["weather-mcp"]);
        let bin = home.parent().unwrap().join("bin");
        installed(&bin, "weather-mcp");

        let settled = settled(&home, &project, Asking::Bypass, true, true, "", &bin);

        assert_eq!(started(&settled), vec!["weather"]);
        assert!(settled.screen.is_empty(), "{}", settled.screen);
        assert!(!mcp::approvals_file(&home).exists());
        assert_eq!(recorded_projects(&home), "");
    }

    #[test]
    fn an_incognito_yes_is_for_this_session_and_writes_nothing() {
        let (home, project, _) = declared("cli-servers-incognito", &["weather-mcp"]);
        let bin = home.parent().unwrap().join("bin");
        installed(&bin, "weather-mcp");

        let settled = settled(&home, &project, Asking::Person, true, false, "2\n", &bin);

        assert_eq!(started(&settled), vec!["weather"]);
        assert_eq!(
            settled.notes,
            vec![t!(servers_for_this_session_only, alias = "weather").to_string()]
        );
        assert!(!mcp::approvals_file(&home).exists());
        assert!(!mcp::projects_file(&home).exists());
    }

    #[test]
    fn a_program_is_found_in_the_path_it_names_and_nowhere_else() {
        let root = scratch("cli-servers-resolve");
        let bin = root.join("bin");
        let program = installed(&bin, "weather-mcp");
        let path = bin.as_os_str();

        assert_eq!(
            resolve("weather-mcp", Some(path), std::slice::from_ref(&bin)),
            Ok(program.clone())
        );
        assert_eq!(
            resolve(program.to_str().unwrap(), None, &[]),
            Ok(program.clone()),
            "an absolute program needs no PATH"
        );
        assert_eq!(
            resolve("weather-mcp", None, &[]),
            Err(t!(servers_program_without_path, program = "weather-mcp").to_string())
        );
        assert_eq!(
            resolve("other-mcp", Some(path), std::slice::from_ref(&bin)),
            Err(t!(servers_program_not_found, program = "other-mcp").to_string())
        );
        assert_eq!(
            resolve("sh", Some(path), std::slice::from_ref(&bin)),
            Err(t!(servers_program_not_found, program = "sh").to_string()),
            "a program on this process's own PATH and not on the one named is not found"
        );
        assert_eq!(
            resolve("bin/weather-mcp", Some(path), std::slice::from_ref(&bin)),
            Err(t!(servers_program_relative, program = "bin/weather-mcp").to_string())
        );
    }

    #[test]
    fn a_path_the_declaration_does_not_name_resolves_nothing() {
        let (home, project, _) = declared("cli-servers-no-path", &["weather-mcp"]);
        let declaration = Declaration::stdio(words(&["weather-mcp"]), Vec::new(), None).unwrap();
        let mut declarations = Declarations::default();
        declarations.insert("weather", &declaration);
        std::fs::write(mcp::declarations_file(&home), declarations.to_text()).expect("mcp.json");
        let bin = home.parent().unwrap().join("bin");
        installed(&bin, "weather-mcp");

        let settled = settled(&home, &project, Asking::Person, true, true, "1\n", &bin);

        assert!(settled.plans.is_empty());
        assert_eq!(
            settled.notes,
            vec![
                t!(
                    servers_not_reached,
                    alias = "weather",
                    reason = t!(servers_program_without_path, program = "weather-mcp")
                )
                .to_string()
            ]
        );
    }

    #[test]
    fn a_runner_is_named_as_one_and_an_unpinned_package_as_unpinned() {
        let unpinned = |package: &str| Some(Unpinned::Package(package.to_string()));
        for (argv, runner, expected) in [
            (
                &["npx", "-y", "@dangahagan/weather-mcp@latest"][..],
                Some("npx"),
                unpinned("@dangahagan/weather-mcp@latest"),
            ),
            (&["npx", "-y", "@scope/server@1.4.2"][..], Some("npx"), None),
            (
                &["npx", "-y", "@scope/server"][..],
                Some("npx"),
                unpinned("@scope/server"),
            ),
            (
                &["npx", "server@^1.2.0"][..],
                Some("npx"),
                unpinned("server@^1.2.0"),
            ),
            (&["npx", "server@1"][..], Some("npx"), unpinned("server@1")),
            (
                &["npx", "server@1.2"][..],
                Some("npx"),
                unpinned("server@1.2"),
            ),
            (
                &[
                    "/opt/homebrew/bin/npx",
                    "--package",
                    "server@2.0.0",
                    "serve",
                ][..],
                Some("npx"),
                None,
            ),
            (
                &["npx", "--registry", "https://registry.example", "server"][..],
                Some("npx"),
                unpinned("server"),
            ),
            (
                &["npx", "--node-options=--no-warnings", "server"][..],
                Some("npx"),
                unpinned("server"),
            ),
            (
                &["npx", "--prefer-offline", "server@1.0.0"][..],
                Some("npx"),
                Some(Unpinned::Unread("--prefer-offline".to_string())),
            ),
            (&["bunx", "server"][..], Some("bunx"), unpinned("server")),
            (
                &["pnpm", "dlx", "server@3.1.0-beta.2"][..],
                Some("pnpm dlx"),
                None,
            ),
            (
                &["uvx", "mcp-server-fetch"][..],
                Some("uvx"),
                unpinned("mcp-server-fetch"),
            ),
            (&["uvx", "mcp-server-fetch==1.0.0"][..], Some("uvx"), None),
            (&["uvx", "mcp-server-fetch@1.0.0"][..], Some("uvx"), None),
            (
                &["uvx", "--from", "tool>=1", "serve"][..],
                Some("uvx"),
                unpinned("tool>=1"),
            ),
            (
                &["uvx", "--with", "helper==1.0.0", "server"][..],
                Some("uvx"),
                unpinned("server"),
            ),
            (
                &["uvx", "--python", "3.12", "server==0.3.0"][..],
                Some("uvx"),
                None,
            ),
            (&["pipx", "run", "server==0.3"][..], Some("pipx run"), None),
            (
                &["uv", "tool", "run", "server"][..],
                Some("uv tool run"),
                unpinned("server"),
            ),
            (&["pipx", "install", "server"][..], None, None),
            (&["weather-mcp", "--stdio"][..], None, None),
            (&["node", "server.js"][..], None, None),
        ] {
            let found = fetches(&words(argv));
            assert_eq!(
                found.as_ref().map(|found| found.runner.as_str()),
                runner,
                "{argv:?}"
            );
            assert_eq!(found.and_then(|found| found.unpinned), expected, "{argv:?}");
        }
    }

    #[test]
    fn a_runner_and_its_unpinned_package_are_drawn_at_the_question() {
        let (home, project, _) = declared(
            "cli-servers-runner",
            &["npx", "-y", "@dangahagan/weather-mcp@latest"],
        );
        let bin = home.parent().unwrap().join("bin");
        installed(&bin, "npx");

        let settled = settled(&home, &project, Asking::Person, true, true, "3\n", &bin);

        let lines: Vec<&str> = settled.screen.lines().map(str::trim).collect();
        assert!(
            lines.contains(&t!(servers_fetches, runner = "npx").as_str()),
            "{lines:?}"
        );
        assert!(
            lines.contains(
                &t!(servers_unpinned, package = "@dangahagan/weather-mcp@latest").as_str()
            ),
            "{lines:?}"
        );
    }

    #[test]
    fn a_package_behind_a_flag_nobody_knows_is_drawn_as_not_known() {
        let argv = words(&["npx", "--prefer-offline", "server@1.0.0"]);
        let declaration = Declaration::stdio(argv, Vec::new(), None).expect("declaration");
        let lines: Vec<String> = fetching("weather", &declaration)
            .iter()
            .map(|line| line.trim().to_string())
            .collect();
        assert_eq!(
            lines,
            vec![
                t!(servers_fetches, runner = "npx").to_string(),
                t!(servers_unread, flag = "--prefer-offline", runner = "npx").to_string(),
            ]
        );
    }

    /// Settle `requested` with `typed` at the terminal, an empty `PATH`, and `prelude` as the
    /// platform's confinement base.
    fn settled_on(
        requested: &[(PathBuf, String)],
        project: &Path,
        home: &Path,
        typed: &str,
        prelude: Option<Prelude>,
    ) -> Settled {
        let home = Home {
            directory: Some(home.to_path_buf()),
            writable: true,
        };
        let mut person = Person {
            answers: typed.as_bytes(),
            screen: Vec::new(),
            present: true,
        };
        let environment = |name: &str| (name == "PATH").then(OsString::new);
        let mut notes = Vec::new();
        let plans = settle(
            requested,
            project,
            &home,
            Asking::Person,
            &mut person,
            &environment,
            prelude,
            &mut notes,
        );
        Settled {
            plans,
            notes,
            screen: String::from_utf8(person.screen).expect("screen"),
        }
    }

    #[test]
    fn a_local_server_is_not_asked_about_where_nothing_can_confine_it() {
        let (home, project, declaration) = declared("cli-servers-unconfinable", &["/bin/cat"]);
        let requested = vec![(
            project.join(".bravebot/settings.json"),
            "weather".to_string(),
        )];

        let settled = settled_on(&requested, &project, &home, "1\n", None);

        assert!(settled.plans.is_empty());
        assert_eq!(settled.screen, "", "asked about a server it cannot start");
        assert_eq!(
            settled.notes,
            vec![t!(servers_no_confinement_here, alias = "weather").to_string()]
        );
        assert!(!Approvals::read(&home).approves(&declaration.digest()));

        let settled = settled_on(&requested, &project, &home, "1\n", Prelude::current());
        assert_eq!(started(&settled), vec!["weather"], "{:?}", settled.notes);
    }

    #[cfg(unix)]
    #[test]
    fn a_checkout_reached_through_a_link_names_its_settings_file_inside_it() {
        let (home, project, _) = declared("cli-servers-linked", &["/bin/cat"]);
        let link = project.parent().unwrap().join("link");
        std::os::unix::fs::symlink(&project, &link).expect("link");
        std::fs::write(project.join(".bravebot/settings.json"), "{}").expect("settings");
        let requested = vec![(link.join(".bravebot/settings.json"), "docs".to_string())];

        let settled = settled_on(&requested, &project, &home, "", Prelude::current());

        assert_eq!(
            settled.notes,
            vec![
                t!(
                    servers_not_declared,
                    file = ".bravebot/settings.json",
                    alias = "docs"
                )
                .to_string()
            ]
        );
    }

    fn reads(policy: &SandboxPolicy, path: &Path) -> bool {
        policy.readable.iter().any(|row| row == path)
    }

    fn writes(policy: &SandboxPolicy, path: &Path) -> bool {
        policy.writable.iter().any(|grant| grant.path == path)
    }

    #[test]
    fn a_servers_confinement_reaches_its_installation_and_nothing_of_the_home_directory() {
        let root = scratch("cli-servers-confinement");
        let home = root.join("home");
        let installation = root.join("opt");
        let program = installed(&installation.join("bin"), "npx");
        let own = home.join(".local/bin");
        std::fs::create_dir_all(&own).expect("own bin");
        let work = root.join("work");
        std::fs::create_dir_all(&work).expect("work");
        let temporary = root.join("tmp");

        let policy = confinement(
            Some(Prelude::MacOs),
            &program,
            &[
                installation.join("bin"),
                own.clone(),
                home.clone(),
                root.clone(),
            ],
            Some(&work),
            &temporary,
            Some(&home),
        )
        .expect("a policy");

        assert!(reads(&policy, &installation.join("bin")));
        assert!(
            reads(&policy, &installation),
            "a bin directory brings its parent"
        );
        assert!(
            reads(&policy, &own),
            "a program directory in the home directory is read"
        );
        assert!(
            !reads(&policy, &home.join(".local")),
            "its parent is the person's own"
        );
        assert!(
            !reads(&policy, &home),
            "the home directory is not a program directory"
        );
        assert!(!reads(&policy, &root), "nor is a directory above it");
        assert!(reads(&policy, &work) && writes(&policy, &work));
        assert_eq!(policy.starting_in.as_deref(), Some(work.as_path()));
        assert!(!writes(&policy, &installation));

        let undirected = confinement(
            Some(Prelude::MacOs),
            &program,
            &[],
            None,
            &temporary,
            Some(&home),
        )
        .expect("a policy");
        assert_eq!(undirected.starting_in.as_deref(), Some(temporary.as_path()));
        assert!(confinement(None, &program, &[], None, &temporary, Some(&home)).is_none());
    }

    /// `/tmp` is a link to `/private/tmp` on macOS, and a home under it was named as given while
    /// every row was named with its links followed, so the two never matched.
    #[cfg(unix)]
    #[test]
    fn a_home_reached_through_a_link_is_kept_out_of_a_servers_confinement() {
        let root = scratch("cli-servers-linked-home");
        let home = root.join("home");
        let own = home.join(".cargo/bin");
        let program = installed(&own, "npx");
        let linked = root.join("linked");
        std::os::unix::fs::symlink(&home, &linked).expect("link the home");

        let policy = confinement(
            Some(Prelude::MacOs),
            &program,
            &[linked.join(".cargo/bin"), linked.clone()],
            None,
            &root.join("tmp"),
            Some(&linked),
        )
        .expect("a policy");

        assert!(reads(&policy, &own), "a program directory in it is read");
        assert!(
            !reads(&policy, &home.join(".cargo")),
            "its parent is the person's own"
        );
        assert!(!reads(&policy, &home), "the home directory is not read");
    }

    #[test]
    fn the_home_kept_out_of_a_launched_server_is_the_persons_and_not_the_state_directory() {
        if Prelude::current().is_none() {
            return;
        }
        let profile = bravebot_agent::home::profile().expect("a test runs with a home directory");
        let own = profile.join(".cargo").join("bin");

        let policy = confinement_here(&own.join("server"), std::slice::from_ref(&own), None)
            .expect("a policy");

        let canonical = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        assert!(
            !reads(&policy, &canonical(&profile.join(".cargo"))),
            "a bin directory's parent in the home directory is the person's own"
        );
    }
}
