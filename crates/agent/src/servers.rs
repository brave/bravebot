//! The MCP servers a session starts with (SERVERS-2, SERVERS-4, SERVERS-6, SERVERS-10, SERVERS-12).
//!
//! A checkout names the aliases it wants, and naming one grants nothing. Each resolves against the
//! person's own declarations; one the machine's managed layer keeps from starting, by the host it
//! reaches or the command it runs, goes no further in any mode, and the rest are put to the person
//! where no answer of theirs covers the declaration they resolve to, and started confined (MCP-3)
//! holding the variables they name and no others (MCP-9). A declared alias nothing requested is not
//! started.
//!
//! A server's handshake is its `initialize` and then its `tools/list`, and the list is held as the
//! one labelled text it arrived as: nothing of it reaches the planner until a person has read it at
//! the start of a turn and said yes, or said yes to the same list under the same declaration before
//! (SERVERS-8). A session holds the servers it started, their lists, and a grant naming each.
//!
//! The two questions a session puts while it starts its servers, whether to use one (SERVERS-4) and
//! whether one moved where its reply pointed (SERVERS-11), go to an [`Asker`]. A [`Person`] at a
//! terminal is one, and a front end with its own screen is another.

use crate::SessionScratch;
use crate::confirm::MoveRequest;
use crate::mcp::{Connection, Session, Unmoved, managed_refusal, shown, unreadable};
use bravebot_config::mcp::{
    self, Approvals, Declaration, Declarations, Digest, Problem, Projects, Timeouts,
};
use bravebot_config::{Managed, Server};
use bravebot_core::capability::{Capability, CapabilitySet, ServerAlias};
use bravebot_core::event::RecordingSink;
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
use bravebot_core::value::Labelled;
use bravebot_i18n::t;
use bravebot_mcp::{HttpServer, McpError, McpResult, StdioServer};
use bravebot_sandbox::base::{Prelude, base};
use bravebot_sandbox::policy::{Capabilities, SandboxPolicy};
use bravebot_sandbox::{Stream, Variables};
use std::ffi::{OsStr, OsString};
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How much longer than its two handshake requests' startup bounds a server's thread is waited for.
///
/// A server's requests end at that bound and say so, which is a reason worth giving a person. The
/// wait for its thread ends later, so a handshake that was stuck somewhere no request bound
/// reaches, a name that will not resolve for one, is still given up on.
const HANDSHAKE_SLACK: Duration = Duration::from_secs(5);

/// Who answers for a server no answer of the person's covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asking {
    /// Whoever the [`Asker`] reaches, where somebody is there.
    Person,
    /// Nobody: a one-shot run is not a conversation.
    OneShot,
    /// The command line said to answer every permission question yes (SERVERS-13).
    Bypass,
}

/// What a session reached, and what it has to say about the rest.
pub struct Reached {
    /// The servers started, or `None` where none was.
    session: Option<Session>,
    /// A line for each requested server that is not reached, and each answer that could not be
    /// kept, in the order they arose.
    pub notes: Vec<String>,
}

impl Reached {
    /// The aliases started, in order.
    pub fn aliases(&self) -> Vec<String> {
        self.session
            .as_ref()
            .map(Session::aliases)
            .unwrap_or_default()
    }

    /// Whether any process the session started is confined, which a local server is and a
    /// remote one is not.
    pub fn confined(&self) -> bool {
        self.session.as_ref().is_some_and(Session::confined)
    }

    /// The servers started, their lists and a grant naming each, for the turns to settle and call.
    pub fn session(&self) -> Option<Session> {
        self.session.clone()
    }
}

/// Where the files are, and whether this run may write them.
pub struct Home {
    /// The state directory, or `None` where the platform names no profile directory.
    pub directory: Option<PathBuf>,
    /// Whether anything may be written into it, which an incognito session answers no.
    pub writable: bool,
}

/// Whoever answers the questions a session puts while it starts its servers.
///
/// Only a yes starts or moves anything, so an asker with nobody behind it answers no to both.
pub trait Asker {
    /// Whether anybody is there to answer. Where nobody is, neither question is put.
    fn anybody_there(&self) -> bool;

    /// SERVERS-4's question about one server.
    fn ask_to_start(&mut self, question: &Question<'_>) -> Answer;

    /// SERVERS-11's question about a remote server whose handshake was redirected. Whether it moved.
    fn ask_to_move(&mut self, request: &MoveRequest) -> bool;
}

/// The other end of a question at a terminal: where an answer is read, where things are shown, and
/// whether anybody is there to give one.
pub struct Person<R, W> {
    pub answers: R,
    pub screen: W,
    pub present: bool,
}

/// Write one line to the person's screen.
pub fn say<R, W: Write>(person: &mut Person<R, W>, line: impl std::fmt::Display) {
    let _ = writeln!(person.screen, "{line}");
}

/// Reach the servers the settings in force request for the workspace at `root`, putting a question
/// to `asker` where one is needed and somebody is there to answer it.
pub fn for_this_session<A: Asker + ?Sized>(
    settings: &bravebot_config::Settings,
    root: &Path,
    asking: Asking,
    asker: &mut A,
    diagnostics: Stream,
) -> Reached {
    // No declaration is read in a safe session, so nothing is asked about and nothing starts.
    let requested: Vec<(PathBuf, String)> = match bravebot_core::safe::engaged() {
        true => Vec::new(),
        false => settings
            .mcp_requested()
            .map(|(file, alias)| (file.to_path_buf(), alias.to_string()))
            .collect(),
    };
    let project = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let home = Home {
        directory: crate::home::directory(),
        writable: crate::home::writable().is_some(),
    };
    let mut reached = reach(
        &requested,
        &project,
        &home,
        &Managed::load(),
        asking,
        asker,
        diagnostics,
    );
    // Said here because every way of starting assembles its servers through this function and puts
    // these notes in front of the person once, before the first turn.
    if bravebot_core::safe::engaged() {
        reached.notes.insert(0, t!(safe_mode_started).to_string());
    }
    reached
}

/// Whoever is at this process's terminal: both ends, for the reason `bravebot mcp` asks for both. A
/// question written into a pipe is one nobody read, and an answer read from one is one nobody gave.
pub fn at_the_terminal() -> Person<std::io::StdinLock<'static>, std::io::StdoutLock<'static>> {
    Person {
        answers: std::io::stdin().lock(),
        screen: std::io::stdout().lock(),
        present: std::io::stdin().is_terminal() && std::io::stdout().is_terminal(),
    }
}

/// Nobody: what a one-shot run asks, which is no one.
pub fn nobody() -> Person<std::io::Empty, std::io::Sink> {
    Person {
        answers: std::io::empty(),
        screen: std::io::sink(),
        present: false,
    }
}

/// Reach every server `requested` names, asking whoever `asking` says where an answer is needed.
///
/// `requested` is each alias with the settings file that asked for it. `project` is the workspace
/// root, which is what answer 2 records. `managed` is the machine's layer, whose refusals no answer
/// reaches. `diagnostics` is where a local server's stderr goes.
pub fn reach<A: Asker + ?Sized>(
    requested: &[(PathBuf, String)],
    project: &Path,
    home: &Home,
    managed: &Managed,
    asking: Asking,
    asker: &mut A,
    diagnostics: Stream,
) -> Reached {
    let mut notes = Vec::new();
    let plans = settle(
        requested,
        project,
        home,
        managed,
        asking,
        asker,
        &|name| std::env::var_os(name),
        Prelude::current(),
        &mut notes,
    );
    let mut hops = Vec::new();
    let mut started = start(plans, home, diagnostics, &mut notes, &mut hops);
    let moving = moves(hops, home, managed, asking, asker, &mut notes);
    if !moving.is_empty() {
        let plans = moving
            .iter()
            .map(|moving| {
                let plan = Plan::Http {
                    url: moving.url.clone(),
                    declared: moving.declaration.digest(),
                    timeouts: moving.declaration.timeouts(),
                };
                (moving.alias.clone(), plan)
            })
            .collect();
        let mut again = Vec::new();
        let reached = start(plans, home, diagnostics, &mut notes, &mut again);
        notes.extend(
            again
                .iter()
                .map(|hop| t!(mcp_move_again, alias = hop.alias.as_str()).to_string()),
        );
        for server in reached {
            let Some(moving) = moving.iter().find(|moving| moving.alias == server.alias()) else {
                continue;
            };
            if moved(home, moving, &mut notes) {
                started.push(server);
            }
        }
        started.sort_by(|one, other| one.alias().cmp(other.alias()));
    }
    let session = (!started.is_empty()).then(|| {
        Session::new(
            started,
            project.to_path_buf(),
            home.directory.clone(),
            home.writable,
            managed.clone(),
        )
    });
    Reached { session, notes }
}

/// A remote server whose handshake was redirected off where it is declared (SERVERS-11).
struct Hop {
    alias: String,
    /// The url the declaration names.
    url: String,
    /// The digest of that declaration.
    declared: Digest,
    timeouts: Timeouts,
    /// Where the reply pointed: the server's own bytes, until a person says it moved there.
    destination: Labelled<String>,
}

/// A server a person said moved, with the declaration a yes rewrites it to.
struct Moving {
    alias: String,
    /// The digest of the declaration it was started from.
    from: Digest,
    url: String,
    declaration: Declaration,
}

/// Put each handshake that was redirected off its declaration to the person, and say where each
/// one they said moved is to be reached (SERVERS-11).
///
/// Only a person answers. Bypassing every check refuses the move, as a one-shot run and a session
/// with nobody at the terminal do, since a move rewrites a declaration (SERVERS-13). The destination
/// is drawn and decides nothing before the yes: only then is it read as a url and held to the
/// machine's managed layer.
fn moves<A: Asker + ?Sized>(
    hops: Vec<Hop>,
    home: &Home,
    managed: &Managed,
    asking: Asking,
    asker: &mut A,
    notes: &mut Vec<String>,
) -> Vec<Moving> {
    let mut moving = Vec::new();
    for hop in hops {
        let alias = hop.alias.as_str();
        if asking != Asking::Person || !asker.anybody_there() {
            notes.push(t!(mcp_move_not_started, alias = alias).to_string());
            continue;
        }
        let mut sink = RecordingSink::new();
        let mut routing = Routing::new();
        routing.insert_trusted("server", alias);
        let Ok(mut policy) = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::default(),
            &mut sink,
        ) else {
            continue;
        };
        let shaped = policy.render_in_place("mcp_move", &hop.destination, |url| {
            let authority = bravebot_core::url::authority_of(&url).unwrap_or_default();
            (url, authority)
        });
        let (destination, authority) = {
            let proof = policy.authorise_display_release("where an MCP server's reply pointed");
            shaped.declassify(&proof)
        };
        let request = MoveRequest {
            alias: alias.to_string(),
            declared: hop.url.clone(),
            destination,
            authority,
            may_record: home.writable,
        };
        if !asker.ask_to_move(&request) {
            notes.push(t!(mcp_move_not_started, alias = alias).to_string());
            continue;
        }
        policy.endorse_server_move(alias);
        let Ok(Ok(url)) = policy
            .promote_a_server_move(alias, &hop.destination)
            .map(Labelled::into_trusted)
        else {
            continue;
        };
        let declaration = match Declaration::http(url.clone()) {
            Ok(declaration) => declaration.timing(hop.timeouts),
            Err(problem) => {
                notes.push(
                    t!(
                        mcp_move_undeclarable,
                        alias = alias,
                        problem = self::problem(&problem)
                    )
                    .to_string(),
                );
                continue;
            }
        };
        if let Some(reason) = managed_refusal(managed, Server::Remote(&url)) {
            notes.push(t!(mcp_move_refused_by_managed, alias = alias, reason = reason).to_string());
            continue;
        }
        moving.push(Moving {
            alias: hop.alias,
            from: hop.declared,
            url,
            declaration,
        });
    }
    moving
}

/// Rewrite a moved server's declaration where the session may write, now that it answered where it
/// moved to. Whether it is used.
fn moved(home: &Home, moving: &Moving, notes: &mut Vec<String>) -> bool {
    let alias = moving.alias.as_str();
    let (Some(directory), true) = (&home.directory, home.writable) else {
        notes.push(t!(mcp_move_moved, alias = alias).to_string());
        return true;
    };
    match crate::mcp::record_a_move(directory, alias, &moving.from, &moving.declaration) {
        Ok(()) => {
            notes.push(t!(mcp_move_moved, alias = alias).to_string());
            true
        }
        Err(Unmoved::Edited) => {
            notes.push(t!(mcp_move_edited, alias = alias).to_string());
            false
        }
        Err(Unmoved::NotWritten(error)) => {
            notes.push(t!(mcp_move_not_recorded, alias = alias, error = error).to_string());
            true
        }
    }
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
        /// The files the declaration says it may read.
        reads: Vec<PathBuf>,
        directory: Option<PathBuf>,
        /// The digest of the declaration, which a vouch for the server's list is recorded beside.
        declared: Digest,
        timeouts: Timeouts,
    },
    Http {
        url: String,
        declared: Digest,
        timeouts: Timeouts,
    },
}

/// Settle each request: resolve it, and put it to the person where nothing answers it already.
///
/// Returns the servers to start. Every request that will not be is a line in `notes` saying why.
/// `prelude` is this platform's confinement base, and a local server is not asked about without one.
#[allow(clippy::too_many_arguments)]
fn settle<A: Asker + ?Sized>(
    requested: &[(PathBuf, String)],
    project: &Path,
    home: &Home,
    managed: &Managed,
    asking: Asking,
    asker: &mut A,
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
                reason = no_state_directory()
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
                reason = unreadable(&why)
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
                        problem = problem(&found)
                    )
                    .to_string(),
                );
                continue;
            }
        };
        let (plan, covered) = match assess(
            alias,
            &declaration,
            project,
            &approvals,
            &projects,
            managed,
            environment,
            prelude,
        ) {
            Ok(assessed) => assessed,
            Err(Unstarted::Unplanned(reason)) => {
                notes.push(t!(servers_not_reached, alias = alias, reason = reason).to_string());
                continue;
            }
            Err(Unstarted::Refused(reason)) => {
                notes.push(
                    t!(
                        servers_refused_by_managed,
                        alias = shown(alias),
                        reason = reason
                    )
                    .to_string(),
                );
                continue;
            }
            Err(Unstarted::Unconfined) => {
                notes.push(t!(servers_no_confinement_here, alias = alias).to_string());
                continue;
            }
        };

        let changed = approvals.changed(alias, &declaration.digest());
        if !(covered || asking == Asking::Bypass) {
            let nobody = match asking {
                Asking::OneShot => Some(t!(servers_nobody_in_a_one_shot, alias = alias)),
                _ if !asker.anybody_there() => {
                    Some(t!(servers_nobody_at_a_terminal, alias = alias))
                }
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
            match asker.ask_to_start(&question) {
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

/// Why a requested server is not started, found before anybody is asked about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unstarted {
    /// The declaration does not resolve to anything to start here, and why.
    Unplanned(String),
    /// The machine's managed layer keeps it from starting, and why (SERVERS-12).
    Refused(String),
    /// A local server, on a platform with no confinement for one.
    Unconfined,
}

/// A requested server as a session in `project` finds it before a question is put: how it would
/// start, and whether an answer the person already gave covers it.
///
/// The one place both are decided, so a session and SERVERS-14's report cannot disagree about
/// which servers start unasked. The managed layer is read once the program is the path it resolved
/// to, so bypassing reaches a refused server no more than an answer would (SERVERS-12,
/// SERVERS-13). A recorded project answers for a server nobody has seen here, and not for one
/// somebody saw as something else: that is a server they have not seen either (SERVERS-5).
#[allow(clippy::too_many_arguments)]
fn assess(
    alias: &str,
    declaration: &Declaration,
    project: &Path,
    approvals: &Approvals,
    projects: &Projects,
    managed: &Managed,
    environment: &dyn Fn(&str) -> Option<OsString>,
    prelude: Option<Prelude>,
) -> Result<(Plan, bool), Unstarted> {
    let plan = planned(declaration, environment).map_err(Unstarted::Unplanned)?;
    if let Plan::Stdio {
        directory: Some(directory),
        ..
    } = &plan
        && let Some(repository) = repository_holding(directory)
    {
        return Err(Unstarted::Unplanned(in_a_repository(
            directory,
            &repository,
        )));
    }
    if let Some(reason) = refused(managed, &plan) {
        return Err(Unstarted::Refused(reason));
    }
    if matches!(plan, Plan::Stdio { .. }) && prelude.is_none() {
        return Err(Unstarted::Unconfined);
    }
    let digest = declaration.digest();
    let covered = approvals.approves(&digest)
        || (projects.contains(project) && !approvals.changed(alias, &digest));
    Ok((plan, covered))
}

/// The repository git opens when it is run in `directory`, if it opens one: the nearest of it and the
/// directories above it that holds a `.git`, or that is laid out as a repository itself.
///
/// A server may write its directory, and git runs commands a repository names, unconfined: in its
/// configuration and hooks, and in files of its work tree a relative `core.hooksPath` points at.
pub fn repository_holding(directory: &Path) -> Option<PathBuf> {
    let directory = directory
        .canonicalize()
        .unwrap_or_else(|_| directory.to_path_buf());
    directory
        .ancestors()
        .find(|level| {
            level.join(".git").symlink_metadata().is_ok()
                || (level.join("HEAD").is_file()
                    && level.join("objects").is_dir()
                    && level.join("refs").is_dir())
        })
        .map(Path::to_path_buf)
}

/// A new directory under the system temporary directory, which no repository holds, for a test to
/// declare a server's directory in. The scratch directory under `target/` is inside this checkout.
#[cfg(test)]
pub(crate) fn outside_any_repository(name: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    // Named for this process and moment, and made with create_dir so a name another run holds is
    // refused rather than shared.
    // nosemgrep: rust.lang.security.temp-dir.temp-dir
    let path = std::env::temp_dir().join(format!("bravebot-{name}-{}-{stamp}", std::process::id()));
    std::fs::create_dir(&path).expect("a directory of its own under the temporary directory");
    let path = path
        .canonicalize()
        .expect("the directory just made resolves");
    assert_eq!(
        repository_holding(&path),
        None,
        "the temporary directory is inside a repository"
    );
    path
}

/// Why a server is given no directory inside `repository`.
pub fn in_a_repository(directory: &Path, repository: &Path) -> String {
    t!(
        mcp_dir_in_a_repository,
        path = shown(&directory.display().to_string()),
        repository = shown(&repository.display().to_string())
    )
    .to_string()
}

/// What a session started in `project` holds for a server its settings request, before anybody
/// is asked anything (SERVERS-9, SERVERS-14).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Grant {
    /// An answer the person gave covers it, so the session starts it and holds the grant naming it.
    Held,
    /// Nothing covers it, so a session at a terminal asks, and holds the grant only after a yes.
    Asked,
    /// No session here starts it, so none holds a grant for it.
    Withheld(Unstarted),
}

/// SERVERS-14's capability for one requested server, read without starting it.
pub fn grant(
    alias: &str,
    declaration: &Declaration,
    project: &Path,
    approvals: &Approvals,
    projects: &Projects,
    managed: &Managed,
) -> Grant {
    match assess(
        alias,
        declaration,
        project,
        approvals,
        projects,
        managed,
        &|name| std::env::var_os(name),
        Prelude::current(),
    ) {
        Ok((_, true)) => Grant::Held,
        Ok((_, false)) => Grant::Asked,
        Err(unstarted) => Grant::Withheld(unstarted),
    }
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
pub fn named(file: &Path, project: &Path) -> String {
    let file = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
    file.strip_prefix(project)
        .unwrap_or(&file)
        .display()
        .to_string()
}

/// What a declaration starts, read against this process's environment for the variables it names
/// and with the value it stores for each of the others.
///
/// A program named rather than given as a path is looked for in the `PATH` the declaration gives
/// it, and in no other: the server runs with that one, so a program found in this process's own
/// would be a program the server's environment does not lead to.
fn planned(
    declaration: &Declaration,
    environment: &dyn Fn(&str) -> Option<OsString>,
) -> Result<Plan, String> {
    let (argv, names, env, reads, directory) = match declaration {
        Declaration::Http { url, timeouts } => {
            return Ok(Plan::Http {
                url: url.clone(),
                declared: declaration.digest(),
                timeouts: *timeouts,
            });
        }
        Declaration::Stdio {
            argv,
            variables,
            env,
            reads,
            directory,
            ..
        } => (argv, variables, env, reads, directory),
    };
    // A stored name is never in `variables`, so the environment is not read for it.
    let variables = names
        .iter()
        .filter_map(|name| environment(name).map(|value| (name, value)))
        .fold(Variables::new(), |variables, (name, value)| {
            variables.with(name.as_str(), value)
        });
    let variables = env.iter().fold(variables, |variables, (name, value)| {
        variables.with(name.as_str(), value.as_str())
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
        .ok_or_else(|| problem(&mcp::Problem::Program))?;
    let program = resolve(named, path.as_deref(), &searched)?;
    Ok(Plan::Stdio {
        program,
        arguments: arguments.to_vec(),
        variables,
        searched,
        reads: reads.iter().map(PathBuf::from).collect(),
        directory: directory.as_ref().map(PathBuf::from),
        declared: declaration.digest(),
        timeouts: declaration.timeouts(),
    })
}

/// Of `files`, the ones the confinement `declaration` would start under does not let it read, which
/// are the ones a read has to be granted for.
///
/// Read against that confinement with no file granted and the temporary directory as its own, since
/// its own directory is made only when it starts. A program that is not found yet is taken as
/// named, which grants less and so keeps more files. Where this platform has no base, every file is
/// one, since nothing is known to reach it.
pub fn unreached(
    declaration: &Declaration,
    files: Vec<PathBuf>,
    environment: &dyn Fn(&str) -> Option<OsString>,
) -> Vec<PathBuf> {
    let Declaration::Stdio {
        argv, directory, ..
    } = declaration
    else {
        return files;
    };
    let (program, searched) = match planned(declaration, environment) {
        Ok(Plan::Stdio {
            program, searched, ..
        }) => (program, searched),
        _ => (PathBuf::from(&argv[0]), Vec::new()),
    };
    let directory = directory.as_deref().map(Path::new);
    let Some(policy) =
        confinement_here(&program, &searched, &[], directory, &temporary_directory())
    else {
        return files;
    };
    files
        .into_iter()
        .filter(|file| !policy.readable.iter().any(|row| file.starts_with(row)))
        .collect()
}

/// `declaration` with a read granted for each file one of its stored values or arguments names,
/// where its confinement would otherwise refuse it (SERVERS-10).
///
/// A word is taken as a file only where it is an absolute path to one that exists now, and it is
/// recorded as it resolves, so the grant is to the file that was there when the person was shown
/// it. A directory is never granted this way: a word naming one is too broad a read to infer. Nor
/// is a file in the state directory, so a line copied from somewhere else cannot hand a server the
/// person's declarations, approvals or credentials.
pub fn with_reads(
    declaration: Declaration,
    state: &Path,
    environment: &dyn Fn(&str) -> Option<OsString>,
) -> Result<Declaration, Problem> {
    let Declaration::Stdio { argv, env, .. } = &declaration else {
        return Ok(declaration);
    };
    let state = std::fs::canonicalize(state).unwrap_or_else(|_| state.to_path_buf());
    let mut files: Vec<PathBuf> = Vec::new();
    for word in env.values().chain(argv) {
        let path = Path::new(word);
        if !path.is_absolute() {
            continue;
        }
        let Ok(file) = std::fs::canonicalize(path) else {
            continue;
        };
        if file.is_file() && !file.starts_with(&state) && !files.contains(&file) {
            files.push(file);
        }
    }
    let reads = unreached(&declaration, files, environment)
        .into_iter()
        .filter_map(|file| file.into_os_string().into_string().ok())
        .collect();
    declaration.reading(reads)
}

/// Whether `program` is a bare name, found through `PATH`, rather than a path.
pub fn is_a_bare_name(program: &str) -> bool {
    let mut parts = Path::new(program).components();
    matches!(
        (parts.next(), parts.next()),
        (Some(std::path::Component::Normal(_)), None)
    )
}

/// Why the machine's managed layer keeps `plan` from starting, where it does (SERVERS-12).
fn refused(managed: &Managed, plan: &Plan) -> Option<String> {
    match plan {
        Plan::Stdio {
            program, arguments, ..
        } => {
            let argv: Vec<String> = std::iter::once(program.to_string_lossy().into_owned())
                .chain(arguments.iter().cloned())
                .collect();
            managed_refusal(managed, Server::Local(&argv))
        }
        Plan::Http { url, .. } => managed_refusal(managed, Server::Remote(url)),
    }
}

/// Why the managed layer keeps `declaration` from starting, for `list` and `get` to say beside it.
///
/// A local server is compared by the path its program resolves to here, and one that does not
/// resolve by its argv as it was written.
pub fn refused_declaration(
    managed: &Managed,
    declaration: &Declaration,
    environment: &dyn Fn(&str) -> Option<OsString>,
) -> Option<String> {
    match (planned(declaration, environment), declaration) {
        (Ok(plan), _) => refused(managed, &plan),
        (Err(_), Declaration::Stdio { argv, .. }) => managed_refusal(managed, Server::Local(argv)),
        (Err(_), Declaration::Http { url, .. }) => managed_refusal(managed, Server::Remote(url)),
    }
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

/// What the person said to SERVERS-4's question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// Approve this declaration.
    Once,
    /// Approve it, and every server a checkout in this project requests from now on.
    Project,
    /// Leave it out of this session.
    No,
}

impl Answer {
    /// What a typed line answers: 1 and 2 approve, and anything else is a no.
    pub fn typed(line: &str) -> Self {
        match line.trim() {
            "1" => Self::Once,
            "2" => Self::Project,
            _ => Self::No,
        }
    }
}

/// SERVERS-4's question about one server.
pub struct Question<'a> {
    pub alias: &'a str,
    /// The settings file that requested it, relative to the project where it is inside it.
    pub file: &'a str,
    pub declaration: &'a Declaration,
    /// Where a local server's program resolved to.
    pub program: Option<&'a PathBuf>,
    /// Whether the person approved this alias as something else.
    pub changed: bool,
}

/// The line saying which path a local server's program resolved to, drawn only where that differs
/// from the program as the declaration gave it (SERVERS-4).
fn runs(alias: &str, declaration: &Declaration, program: &Path) -> Option<String> {
    let Declaration::Stdio { argv, .. } = declaration else {
        return None;
    };
    (argv.first().map(Path::new) != Some(program)).then(|| {
        format!(
            "{}{}",
            indent(alias),
            t!(
                servers_program,
                path = shown(&program.display().to_string())
            )
        )
    })
}

/// The `runs` line for `declaration`, resolved the way a session resolves it, for the question
/// `bravebot mcp add`, `approve` and `enable` ask (SERVERS-3). None for a remote server, for a
/// program given as the path it runs, and for one that does not resolve here.
pub fn resolved_program_line(
    alias: &str,
    declaration: &Declaration,
    environment: &dyn Fn(&str) -> Option<OsString>,
) -> Option<String> {
    match planned(declaration, environment) {
        Ok(Plan::Stdio { program, .. }) => runs(alias, declaration, &program),
        _ => None,
    }
}

impl Question<'_> {
    /// Every line drawn above the answers.
    pub fn lines(&self) -> Vec<String> {
        let digest = self.declaration.digest();
        let indent = indent(self.alias);
        let mut lines = drawn(self.alias, self.declaration, &digest.short());
        let mut under = vec![format!(
            "{indent}{}",
            t!(servers_requested_by, file = self.file)
        )];
        if let Some(program) = self.program {
            under.extend(runs(self.alias, self.declaration, program));
        }
        lines.splice(1..1, under);
        if self.changed {
            lines.push(format!("{indent}{}", t!(servers_changed)));
        }
        lines.extend(fetching(self.alias, self.declaration));
        lines
    }

    /// The question and its three answers, drawn under [`Question::lines`].
    pub fn choices(&self) -> Vec<String> {
        vec![
            format!("  {}", t!(mcp_question)),
            format!("  1. {}", t!(servers_answer_once)),
            format!("  2. {}", t!(servers_answer_project)),
            format!("  3. {}", t!(servers_answer_no)),
        ]
    }

    /// The words written after the choices, where the answer is typed.
    pub fn answer_prompt() -> String {
        format!("  {}", t!(servers_answer))
    }
}

impl<R: BufRead, W: Write> Asker for Person<R, W> {
    fn anybody_there(&self) -> bool {
        self.present
    }

    /// Draw the question and read the answer. Anything but 1 or 2, and the end of the input, is 3.
    fn ask_to_start(&mut self, question: &Question<'_>) -> Answer {
        say(self, "");
        for line in question.lines() {
            say(self, line);
        }
        say(self, "");
        for line in question.choices() {
            say(self, line);
        }
        let _ = write!(self.screen, "{} ", Question::answer_prompt());
        let _ = self.screen.flush();
        let mut typed = String::new();
        match self.answers.read_line(&mut typed) {
            Ok(0) | Err(_) => Answer::No,
            Ok(_) => Answer::typed(&typed),
        }
    }

    /// Draw where a server's reply pointed and read the answer. Only a yes moves it, and the end of
    /// the input is a no.
    fn ask_to_move(&mut self, request: &MoveRequest) -> bool {
        say(self, "");
        say(
            self,
            format!(
                "  {}",
                t!(
                    mcp_move_declared,
                    alias = request.alias.as_str(),
                    url = shown(&request.declared)
                )
            ),
        );
        say(
            self,
            format!(
                "  {}",
                t!(mcp_move_destination, url = shown(&request.destination))
            ),
        );
        say(
            self,
            format!(
                "  {}",
                t!(mcp_move_reaching, authority = shown(&request.authority))
            ),
        );
        say(self, format!("  {}", t!(mcp_move_explained)));
        if !request.may_record {
            say(self, format!("  {}", t!(mcp_move_this_session_only)));
        }
        say(self, "");
        let _ = write!(self.screen, "  {} {} ", t!(mcp_move_title), t!(line_answer));
        let _ = self.screen.flush();
        let mut typed = String::new();
        match self.answers.read_line(&mut typed) {
            Ok(0) | Err(_) => false,
            Ok(_) => typed.trim().to_lowercase() == t!(line_answer_yes),
        }
    }
}

/// A declaration as a person reads it: the alias, the transport and what it runs or reaches on the
/// first line, and under it the names it receives, the files it may read, where it runs, and the
/// digest.
///
/// Every argument is shown as the word it is, quoted where it holds a space or anything a terminal
/// would not draw as itself, so `a b` and `"a b"` are told apart on the screen as they are in argv.
/// A stored value is not shown: its name is, marked stored, and a file it names is shown as a read.
pub fn drawn(alias: &str, declaration: &Declaration, digest: &str) -> Vec<String> {
    let what = match declaration {
        Declaration::Stdio { argv, .. } => argv
            .iter()
            .map(|word| shown(word))
            .collect::<Vec<_>>()
            .join(" "),
        Declaration::Http { url, .. } => shown(url),
    };
    let indent = indent(alias);
    let mut lines = vec![format!(
        "  {}   {}   {what}",
        shown(alias),
        declaration.transport()
    )];
    let names: Vec<String> = declaration
        .stored()
        .map(|name| t!(mcp_variable_stored, name = name).to_string())
        .chain(declaration.variables().iter().cloned())
        .collect();
    if !names.is_empty() {
        lines.push(format!(
            "{indent}{}",
            t!(mcp_variables, names = names.join(", "))
        ));
    }
    for file in declaration.reads() {
        lines.push(format!("{indent}{}", t!(mcp_may_read, path = shown(file))));
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
    let timeouts = declaration.timeouts();
    if !timeouts.is_default() {
        lines.push(format!(
            "{indent}{}",
            t!(
                mcp_timeouts,
                startup = timeouts.startup().as_secs(),
                tool = timeouts.tool().as_secs()
            )
        ));
    }
    lines.push(format!("{indent}{}", t!(mcp_digest, digest = digest)));
    lines
}

/// The margin the lines under a declaration's first one start at.
pub fn indent(alias: &str) -> String {
    " ".repeat(2 + shown(alias).chars().count() + 3)
}

/// What is wrong with a declaration, as a person reads it.
pub fn problem(found: &Problem) -> String {
    match found {
        Problem::Alias => t!(mcp_problem_alias).to_string(),
        Problem::NotAnObject => t!(mcp_problem_not_an_object).to_string(),
        Problem::Transport => t!(mcp_problem_transport).to_string(),
        Problem::Key(key) => t!(mcp_problem_key, key = shown(key)).to_string(),
        Problem::Program => t!(mcp_problem_program).to_string(),
        Problem::Name => t!(mcp_problem_name).to_string(),
        Problem::Env => t!(mcp_problem_env).to_string(),
        Problem::Value(name) => t!(mcp_problem_value, name = name).to_string(),
        Problem::Twice(name) => t!(mcp_problem_twice, name = name).to_string(),
        Problem::Reads => t!(mcp_problem_reads).to_string(),
        Problem::Directory => t!(mcp_problem_directory).to_string(),
        Problem::Url => t!(mcp_problem_url).to_string(),
        Problem::Credentials => t!(mcp_problem_credentials).to_string(),
        Problem::Remote(key) => t!(mcp_problem_remote, key = *key).to_string(),
        Problem::Timeout(key) => t!(
            mcp_problem_timeout,
            key = *key,
            most = mcp::MAX_TIMEOUT_SECS
        )
        .to_string(),
    }
}

/// Why nothing is declared or recorded on a machine with no state directory.
pub fn no_state_directory() -> String {
    t!(
        mcp_no_state_directory,
        variables = crate::home::PROFILE_VARIABLES.join(" or ")
    )
    .to_string()
}

/// Record the approval of `declaration`'s digest, and of nothing else: the alias is kept beside it
/// as the name it was asked about under, and is not what was approved. The error is why the file
/// was not written.
pub fn record(
    directory: &Path,
    declarations: &Declarations,
    approvals: &mut Approvals,
    alias: &str,
    declaration: &Declaration,
) -> Result<(), String> {
    approvals.approve(alias, declaration.digest());
    approvals.keep_only(declarations);
    let path = mcp::approvals_file(directory);
    crate::mcp::replace(&path, approvals.to_text()).map_err(|error| {
        t!(
            mcp_not_written,
            path = path.display().to_string(),
            error = error.to_string()
        )
        .to_string()
    })
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
    if let Err(reason) = record(directory, declarations, approvals, alias, declaration) {
        unkept.push(t!(servers_not_kept, alias = alias, reason = reason).to_string());
    }
    if let Some(project) = project {
        let recorded = projects.add(project)
            && crate::mcp::replace(&mcp::projects_file(directory), projects.to_text()).is_ok();
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
pub fn fetching(alias: &str, declaration: &Declaration) -> Vec<String> {
    let Declaration::Stdio { argv, .. } = declaration else {
        return Vec::new();
    };
    let Some(fetched) = fetches(argv) else {
        return Vec::new();
    };
    let indent = indent(alias);
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

/// Start every planned server, and wait for each handshake for the startup bound its declaration
/// gives it.
///
/// A remote server whose handshake was redirected off its declaration is not started, and is in
/// `hops` for the person to be asked about.
fn start(
    plans: Vec<(String, Plan)>,
    home: &Home,
    diagnostics: Stream,
    notes: &mut Vec<String>,
    hops: &mut Vec<Hop>,
) -> Vec<crate::mcp::Reached> {
    start_under(None, plans, home, diagnostics, notes, hops)
}

/// [`start`], resolving each policy against `granting` rather than against what this machine's
/// backend reports.
///
/// The same split [`confinement`] has from [`confinement_here`], and for the same reason: which
/// paths a backend may name is a fact about the host, and a test that read it off this one could
/// only check the answer this machine gives. Whether a path missing from disk may be named is
/// `false` on Linux and `true` on macOS, so the dropping, and the note saying what was dropped,
/// are reachable on one and unreachable on the other.
///
/// The backend still spawns. Only the capabilities the policy is resolved against are the
/// caller's, so a narrower policy than this machine demanded is installed by the real sandbox,
/// which every backend can do: a profile naming fewer paths is one any of them accepts.
fn start_under(
    granting: Option<Capabilities>,
    plans: Vec<(String, Plan)>,
    home: &Home,
    diagnostics: Stream,
    notes: &mut Vec<String>,
    hops: &mut Vec<Hop>,
) -> Vec<crate::mcp::Reached> {
    if plans.is_empty() {
        return Vec::new();
    }
    let (sender, received) =
        mpsc::channel::<(String, McpResult<crate::mcp::Reached>, Option<Hop>)>();
    let mut waiting: Vec<(String, Instant, u64)> = Vec::new();
    let sandbox = plans
        .iter()
        .any(|(_, plan)| matches!(plan, Plan::Stdio { .. }))
        .then(bravebot_sandbox::for_current_platform);
    // A declaration's file can be edited by hand, so `add` keeping reads out of here is not enough.
    let state = home
        .directory
        .as_deref()
        .map(|state| state.canonicalize().unwrap_or_else(|_| state.to_path_buf()));

    for (alias, plan) in plans {
        let sender = sender.clone();
        match plan {
            Plan::Stdio {
                program,
                arguments,
                variables,
                searched,
                mut reads,
                directory,
                declared,
                timeouts,
            } => {
                reads.retain(|file| state.as_ref().is_none_or(|state| !file.starts_with(state)));
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
                let (own, throwaway) = match own_home(home, &declared) {
                    Ok(made) => made,
                    Err((place, error)) => {
                        notes.push(
                            t!(
                                servers_no_home,
                                alias = alias,
                                path = place.display().to_string(),
                                reason = error.to_string()
                            )
                            .to_string(),
                        );
                        continue;
                    }
                };
                let Some(policy) =
                    confinement_here(&program, &searched, &reads, directory.as_deref(), &own)
                else {
                    notes.push(t!(servers_no_confinement_here, alias = alias).to_string());
                    continue;
                };
                let granted = granting.clone().unwrap_or_else(|| sandbox.capabilities());
                let (policy, left_out) = nameable(&alias, policy, &granted);
                notes.extend(left_out);
                let launched = StdioServer::launch(
                    alias.as_str(),
                    program.to_str().unwrap_or_default(),
                    &arguments,
                    at_home(variables, &own),
                    sandbox.as_ref(),
                    &policy,
                    diagnostics,
                );
                let server = match launched {
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
                waiting.push(wait_for(&alias, timeouts));
                std::thread::spawn(move || {
                    // Held apart from the handshake, so a server that fails one has been stopped
                    // by the time its home is removed.
                    let outcome =
                        handshake_local(server, declared, timeouts).map(
                            |reached| match throwaway {
                                Some(throwaway) => reached.holding(throwaway),
                                None => reached,
                            },
                        );
                    let _ = sender.send((alias, outcome, None));
                });
            }
            Plan::Http {
                url,
                declared,
                timeouts,
            } => {
                waiting.push(wait_for(&alias, timeouts));
                std::thread::spawn(move || {
                    let (outcome, hop) = handshake_remote(&alias, url.clone(), declared, timeouts);
                    let hop = hop.map(|destination| Hop {
                        alias: alias.clone(),
                        url,
                        declared,
                        timeouts,
                        destination,
                    });
                    let _ = sender.send((alias, outcome, hop));
                });
            }
        }
    }
    drop(sender);

    let mut started = Vec::new();
    let mut slow: Vec<(String, u64)> = Vec::new();
    while !waiting.is_empty() {
        // Each server is waited for until its own deadline, so one given longer than another
        // does not hold the rest of them to its figure.
        let next = waiting
            .iter()
            .map(|(_, deadline, _)| *deadline)
            .min()
            .unwrap_or_else(Instant::now);
        let Ok((alias, outcome, hop)) =
            received.recv_timeout(next.saturating_duration_since(Instant::now()))
        else {
            let now = Instant::now();
            for (alias, _, seconds) in waiting.iter().filter(|(_, deadline, _)| *deadline <= now) {
                slow.push((alias.clone(), *seconds));
            }
            waiting.retain(|(_, deadline, _)| *deadline > now);
            continue;
        };
        if !waiting.iter().any(|(waited, _, _)| *waited == alias) {
            // Answered after it was given up on.
            continue;
        }
        waiting.retain(|(waited, _, _)| *waited != alias);
        match (outcome, hop) {
            (Ok(server), _) => started.push(server),
            (Err(_), Some(hop)) => hops.push(hop),
            (Err(error), None) => notes.push(
                t!(
                    servers_no_handshake,
                    alias = alias,
                    reason = error.to_string()
                )
                .to_string(),
            ),
        }
    }
    for (alias, seconds) in slow {
        notes.push(t!(servers_too_slow, alias = alias, seconds = seconds).to_string());
    }
    started.sort_by(|one, other| one.alias().cmp(other.alias()));
    started
}

/// The server `alias`, the time its handshake is given up on, and the seconds each of its two
/// requests was given. A `tools/list` of several pages gives each page the bound and is still
/// given up on at this time.
fn wait_for(alias: &str, timeouts: Timeouts) -> (String, Instant, u64) {
    (
        alias.to_string(),
        Instant::now() + 2 * timeouts.startup() + HANDSHAKE_SLACK,
        timeouts.startup().as_secs(),
    )
}

/// A local server's handshake, and then its list, each within the startup bound it was given.
///
/// A request that is not answered in time stops the process. Once the handshake is done the server
/// is held to its call bound instead.
fn handshake_local(
    mut server: StdioServer,
    declared: Digest,
    timeouts: Timeouts,
) -> McpResult<crate::mcp::Reached> {
    server.set_bound(timeouts.startup());
    server.initialize("bravebot", env!("CARGO_PKG_VERSION"))?;
    let alias = server.name().to_string();
    let listing = bravebot_mcp::listed_or_none(&alias, server.list_tools())?;
    server.set_bound(timeouts.tool());
    Ok(crate::mcp::Reached::new(
        Connection::Stdio(server),
        listing,
        declared,
    ))
}

/// A remote server's handshake, through the one egress gate every request passes, with where it
/// was redirected where a hop off its declaration is what refused it.
///
/// Under a policy holding the fetch capability and the grant naming this server, and no other:
/// the handshake is a request to one declared destination, and a hop off it is refused.
fn handshake_remote(
    alias: &str,
    url: String,
    declared: Digest,
    timeouts: Timeouts,
) -> (McpResult<crate::mcp::Reached>, Option<Labelled<String>>) {
    let mut sink = RecordingSink::new();
    let mut routing = Routing::new();
    routing.insert_trusted("server", alias);
    let mut policy = match Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([
            Capability::WebFetch,
            Capability::McpCall(ServerAlias::new(alias)),
        ]),
        &mut sink,
    ) {
        Ok(policy) => policy,
        Err(denial) => return (Err(McpError::Denied(denial)), None),
    };
    let egress = bravebot_net::Egress::new();
    let reached = handshake_at(&mut policy, &egress, alias, url, declared, timeouts);
    let hop = match &reached {
        Err(McpError::Denied(_)) => policy.take_server_hop(),
        _ => None,
    };
    (reached, hop)
}

fn handshake_at(
    policy: &mut Policy<'_, RecordingSink>,
    egress: &bravebot_net::Egress,
    alias: &str,
    url: String,
    declared: Digest,
    timeouts: Timeouts,
) -> McpResult<crate::mcp::Reached> {
    let mut server = HttpServer::new(alias, url).starting_within(timeouts.startup());
    server.initialize(policy, egress, "bravebot", env!("CARGO_PKG_VERSION"))?;
    let listing = bravebot_mcp::listed_or_none(alias, server.list_tools(policy, egress))?;
    server.set_bound(timeouts.tool());
    Ok(crate::mcp::Reached::new(
        Connection::Http(server),
        listing,
        declared,
    ))
}

/// The system temporary directory with its links followed, which is how a backend matches it.
fn temporary_directory() -> PathBuf {
    // Nothing is created here: the path becomes the base's temporary row, which the spec's base
    // table grants, and a server makes its own files there under its own names.
    // nosemgrep: rust.lang.security.temp-dir.temp-dir
    let temporary = std::env::temp_dir();
    temporary.canonicalize().unwrap_or(temporary)
}

/// The part of `policy` this backend can be asked for, and the note saying what that left out.
///
/// A backend that cannot name a path missing from the disk is asked for the rest of the policy
/// rather than refusing the server over a toolchain the machine never had (SANDBOX-9), so the
/// server starts holding fewer paths than were granted it. The note is how that is said: without
/// it, a person reading a refusal from the server cannot tell a path it was refused from one it
/// was never granted, and neither can the person who declared it.
///
/// Every path it left out, rather than how many. The prelude names spellings a distribution may
/// lack, so the list is long and usually of no interest, and a count hides the one path somebody
/// is looking for, which is the whole of what this is for. Each is drawn as a word from a
/// declaration is drawn, since a `PATH` the declaration names is granted as written and is left
/// out here where it is not on disk.
fn nameable(
    alias: &str,
    policy: SandboxPolicy,
    capabilities: &Capabilities,
) -> (SandboxPolicy, Option<String>) {
    let resolved = policy.nameable_under(capabilities);
    let left_out = (!resolved.omitted.is_empty()).then(|| {
        let paths: Vec<String> = resolved
            .omitted
            .iter()
            .map(|path| shown(&path.display().to_string()))
            .collect();
        t!(
            servers_paths_left_out,
            alias = alias,
            paths = paths.join(", ")
        )
        .to_string()
    });
    (resolved.policy, left_out)
}

/// [`confinement`] on this machine: its platform's base, and the person's own profile directory as
/// the home kept out, which is not the state directory inside it.
fn confinement_here(
    program: &Path,
    searched: &[PathBuf],
    reads: &[PathBuf],
    directory: Option<&Path>,
    own: &Path,
) -> Option<SandboxPolicy> {
    confinement(
        Prelude::current(),
        program,
        searched,
        reads,
        directory,
        own,
        &temporary_directory(),
        crate::home::profile().as_deref(),
    )
}

/// The directory a local server is handed as its home, made before it starts, with what keeps a
/// throwaway one for as long as the server runs.
///
/// Under the state directory and keyed by the declaration where anything may be written there, so
/// what a runner fetched on one launch is there on the next. Otherwise one of its own in the system
/// temporary directory. The error names the directory it was to be made in.
fn own_home(
    home: &Home,
    declared: &Digest,
) -> Result<(PathBuf, Option<SessionScratch>), (PathBuf, std::io::Error)> {
    let Some(state) = home.directory.as_deref().filter(|_| home.writable) else {
        return SessionScratch::for_a_server()
            .map(|made| (made.path().to_path_buf(), Some(made)))
            .map_err(|error| (temporary_directory(), error));
    };
    let own = mcp::server_home(state, declared);
    let made = crate::home::create_directory(&own).and_then(|()| own.canonicalize());
    match made {
        Ok(made) => Ok((made, None)),
        Err(error) => Err((own.parent().map(Path::to_path_buf).unwrap_or(own), error)),
    }
}

/// `variables` with `HOME` naming the server's own directory, unless the declaration named `HOME`
/// itself, which is the person's word and is kept.
fn at_home(variables: Variables, own: &Path) -> Variables {
    let declared = variables.names().any(|name| name == "HOME");
    match declared {
        true => variables,
        false => variables.with("HOME", own),
    }
}

/// What a local server may reach, or `None` on a platform with no base to build it on.
///
/// The platform's base rows with no home directory given, so none of a person's own files is among
/// them. Then the directories the named `PATH` searches and the one the program resolved into,
/// each with its links followed, since a runner is a script whose interpreter is found through
/// `PATH` and whose code sits beside where it was installed. A `bin` directory brings its parent,
/// where an installation keeps what its programs load, and none of these reaches the home
/// directory: a `PATH` naming it, or a directory above it, is left out, and so is a parent inside
/// it, since `~/.cargo` holds a registry token beside `~/.cargo/bin`. The program's own `bin`
/// directory is the exception where it sits deeper in the home than that, as `nvm` installs one,
/// since that parent is the installation the program came from. `own` is the server's own
/// directory, which it may read and write. `reads` are the files the declaration says it may read,
/// each granted as the one file it is while it is one at the path recorded, and never the directory
/// it is in. The
/// declared directory may be written too, and is where it starts; without one it starts in the
/// temporary directory, and reads nothing of the workspace.
#[allow(clippy::too_many_arguments)]
fn confinement(
    prelude: Option<Prelude>,
    program: &Path,
    searched: &[PathBuf],
    reads: &[PathBuf],
    directory: Option<&Path>,
    own: &Path,
    temporary: &Path,
    home: Option<&Path>,
) -> Option<SandboxPolicy> {
    let mut policy = base(prelude?, temporary, None, None);
    let canonical = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let own = canonical(own);
    policy = policy.allow_read(&own).allow_write(&own);
    for directory in crate::confine::program_reads(program, searched, home) {
        policy = policy.allow_read(directory);
    }
    // A recorded read is resolved already, so one that resolves elsewhere now is a file replaced by
    // a link since it was shown, or reached through a directory that was, and not the file shown.
    for file in reads {
        let resolved = file.canonicalize().is_ok_and(|resolved| &resolved == file);
        if resolved && file.is_file() {
            policy = policy.allow_read(file);
        }
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
    use crate::mcp::entry;
    use bravebot_config::Rule;
    use bravebot_sandbox::policy::ConfinementLevel;

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
    /// started. The machine's layer is `managed.json` beside `home`, absent unless a test writes it.
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
        let managed = Managed::at(&managed_beside(home));
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
            &managed,
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

    /// Where [`settled`] reads the machine's layer from, for a state directory at `home`.
    fn managed_beside(home: &Path) -> PathBuf {
        home.parent().expect("a scratch root").join("managed.json")
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
            &Managed::default(),
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

    /// An administrator's refusal is not a question put to the person running the program, so no
    /// answer reaches past it: not a yes at the prompt, not an approval already recorded, and not
    /// bypassing every prompt. The program is compared as the path it resolved to, so a bare name
    /// the declaration's `PATH` finds is refused by an entry naming that path, and the line names
    /// the file and why, since nothing else tells a person why a server they approved is gone.
    #[test]
    fn a_server_the_managed_layer_refuses_is_started_in_no_mode_and_nothing_is_asked_or_recorded() {
        for (lists, asking, present, approved) in [
            (
                r#""deny": [{"command": [PROGRAM]}]"#,
                Asking::Person,
                true,
                false,
            ),
            (
                r#""deny": [{"command": [PROGRAM]}]"#,
                Asking::OneShot,
                false,
                true,
            ),
            (
                r#""deny": [{"command": [PROGRAM]}]"#,
                Asking::Bypass,
                true,
                false,
            ),
            (r#""allow": []"#, Asking::Bypass, true, true),
            (
                r#""allow": [{"host": "*.corp.example"}]"#,
                Asking::Bypass,
                true,
                false,
            ),
            (
                r#""allow": [{"command": [PROGRAM]}], "deny": [{"command": [PROGRAM]}]"#,
                Asking::Bypass,
                true,
                false,
            ),
        ] {
            let case = format!("{lists} {asking:?}");
            let (home, project, declaration) = declared("cli-servers-refused", &["weather-mcp"]);
            let bin = home.parent().unwrap().join("bin");
            let program = installed(&bin, "weather-mcp");
            let managed = managed_beside(&home);
            let spelled = format!("\"{}\"", program.display());
            let lists = lists.replace("PROGRAM", &spelled);
            std::fs::write(&managed, format!(r#"{{"mcp": {{{lists}}}}}"#)).expect("managed.json");
            if approved {
                let mut approvals = Approvals::default();
                approvals.approve("weather", declaration.digest());
                std::fs::write(mcp::approvals_file(&home), approvals.to_text()).expect("approve");
            }
            let recorded = std::fs::read_to_string(mcp::approvals_file(&home)).ok();

            let settled = settled(&home, &project, asking, present, true, "2\n", &bin);

            assert!(settled.plans.is_empty(), "{case}");
            let path = managed.display().to_string();
            let reason = match lists.contains("deny") {
                true => t!(
                    managed_denied,
                    path = path,
                    entry = entry(&Rule::Command(vec![program.display().to_string()]))
                ),
                false => t!(managed_not_allowed, path = path),
            };
            assert_eq!(
                settled.notes,
                vec![
                    t!(
                        servers_refused_by_managed,
                        alias = "weather",
                        reason = reason
                    )
                    .to_string()
                ],
                "{case}"
            );
            assert!(settled.screen.is_empty(), "{case}: {}", settled.screen);
            assert_eq!(
                std::fs::read_to_string(mcp::approvals_file(&home)).ok(),
                recorded,
                "{case}"
            );
            assert_eq!(recorded_projects(&home), "", "{case}");
        }
    }

    /// What the managed layer does not refuse is the person's to decide about as before: an entry
    /// naming the resolved program lets it through an allow list, and an alias, a host or another
    /// argv names nothing a local server is compared by.
    #[test]
    fn a_server_the_managed_layer_does_not_refuse_is_settled_as_before() {
        for lists in [
            r#""allow": [{"command": [PROGRAM]}]"#,
            r#""allow": [{"host": "*.corp.example"}, {"command": [PROGRAM]}]"#,
            r#""deny": ["weather", {"host": "weather.example"}, {"command": ["weather-mcp"]}]"#,
            r#""deny": [{"command": [PROGRAM, "--verbose"]}]"#,
        ] {
            let (home, project, _) = declared("cli-servers-not-refused", &["weather-mcp"]);
            let bin = home.parent().unwrap().join("bin");
            let program = installed(&bin, "weather-mcp");
            let spelled = format!("\"{}\"", program.display());
            let lists = lists.replace("PROGRAM", &spelled);
            std::fs::write(managed_beside(&home), format!(r#"{{"mcp": {{{lists}}}}}"#))
                .expect("managed.json");

            let settled = settled(&home, &project, Asking::Bypass, true, true, "", &bin);

            assert_eq!(started(&settled), vec!["weather"], "{lists}");
            assert!(settled.notes.is_empty(), "{lists}: {:?}", settled.notes);
        }
    }

    /// A remote server is compared by the host its url names, and bypassing reaches a refused one
    /// no more than it reaches a local one.
    #[test]
    fn a_remote_server_is_refused_by_its_host() {
        for (url, lists, reason) in [
            (
                "https://mcp.corp.example/mcp",
                r#""allow": [{"host": "*.corp.example"}]"#,
                None,
            ),
            (
                "https://mcp.elsewhere.example/mcp",
                r#""allow": [{"host": "*.corp.example"}]"#,
                Some("allow"),
            ),
            (
                "https://staging.corp.example/mcp",
                r#""allow": [{"host": "*.corp.example"}], "deny": [{"host": "staging.corp.example"}]"#,
                Some("deny"),
            ),
            (
                r"https://evil.test\.staging.corp.example/",
                r#""deny": [{"host": "*.corp.example"}]"#,
                Some("unread"),
            ),
        ] {
            let root = scratch("cli-servers-remote-refused");
            let home = root.join("home");
            let project = root.join("project");
            std::fs::create_dir_all(&home).expect("home");
            std::fs::create_dir_all(project.join(".bravebot")).expect("project");
            let declaration = Declaration::http(url.to_string()).expect("declaration");
            let mut declarations = Declarations::default();
            declarations.insert("weather", &declaration);
            std::fs::write(mcp::declarations_file(&home), declarations.to_text())
                .expect("mcp.json");
            let managed = managed_beside(&home);
            std::fs::write(&managed, format!(r#"{{"mcp": {{{lists}}}}}"#)).expect("managed.json");

            let settled = settled(&home, &project, Asking::Bypass, true, true, "", &root);

            let path = managed.display().to_string();
            let expected: Vec<String> = match reason {
                None => Vec::new(),
                Some(reason) => {
                    let reason = match reason {
                        "allow" => t!(managed_not_allowed, path = path),
                        "deny" => t!(
                            managed_denied,
                            path = path,
                            entry = entry(&Rule::Host("staging.corp.example".into()))
                        ),
                        _ => t!(managed_host_unread, path = path),
                    };
                    vec![
                        t!(
                            servers_refused_by_managed,
                            alias = "weather",
                            reason = reason
                        )
                        .to_string(),
                    ]
                }
            };
            assert_eq!(settled.notes, expected, "{url}");
            assert_eq!(settled.plans.is_empty(), reason.is_some(), "{url}");
        }
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

    /// SERVERS-10: a server may write its directory, and git runs, unconfined, the commands a
    /// repository's configuration and hooks name. So a directory git would open a repository from
    /// is refused before anybody is asked: one holding `.git`, one below a work tree whose `.git`
    /// is a file, one inside a bare repository, and a `.git` itself. A directory in no repository
    /// is not, so the refusals are of the repository and not of declaring a directory.
    #[test]
    fn a_declared_directory_holding_a_repository_is_refused() {
        let root = outside_any_repository("cli-servers-repository");
        let home = root.join("home");
        let project = root.join("project");
        std::fs::create_dir_all(&home).expect("home");
        std::fs::create_dir_all(project.join(".bravebot")).expect("project");
        let bin = root.join("bin");
        let program = installed(&bin, "weather-mcp");

        let holding = root.join("holding");
        std::fs::create_dir_all(holding.join(".git")).expect("a repository");
        let linked = root.join("linked");
        std::fs::create_dir_all(linked.join("src")).expect("a work tree");
        std::fs::write(
            linked.join(".git"),
            "gitdir: /elsewhere/.git/worktrees/linked\n",
        )
        .expect("a linked work tree's .git");
        let bare = root.join("bare.git");
        for directory in ["objects", "refs", "hooks"] {
            std::fs::create_dir_all(bare.join(directory)).expect("a bare repository");
        }
        std::fs::write(bare.join("HEAD"), "ref: refs/heads/main\n").expect("HEAD");
        let plain = root.join("plain");
        std::fs::create_dir_all(&plain).expect("a directory in no repository");

        let declaring = |directory: &Path| {
            Declaration::stdio(
                words(&[program.to_str().unwrap()]),
                Vec::new(),
                Some(directory.display().to_string()),
            )
            .unwrap()
        };
        let assessed = |directory: &Path| {
            assess(
                "weather",
                &declaring(directory),
                &project,
                &Approvals::default(),
                &Projects::default(),
                &Managed::default(),
                &|_| None,
                Prelude::current(),
            )
        };
        for (directory, repository) in [
            (holding.clone(), &holding),
            (holding.join(".git"), &holding),
            (linked.join("src"), &linked),
            (bare.join("hooks"), &bare),
        ] {
            assert_eq!(
                assessed(&directory).err(),
                Some(Unstarted::Unplanned(in_a_repository(
                    &directory, repository
                ))),
                "{}",
                directory.display()
            );
        }
        assert!(
            !matches!(assessed(&plain), Err(Unstarted::Unplanned(_))),
            "a directory in no repository was refused"
        );

        let mut declarations = Declarations::default();
        declarations.insert("weather", &declaring(&holding));
        std::fs::write(mcp::declarations_file(&home), declarations.to_text()).expect("mcp.json");
        let settled = settled(&home, &project, Asking::Person, true, true, "1\n", &bin);
        assert!(settled.plans.is_empty());
        assert_eq!(settled.screen, "", "somebody was asked about it");
        assert_eq!(
            settled.notes,
            vec![
                t!(
                    servers_not_reached,
                    alias = "weather",
                    reason = in_a_repository(&holding, &holding)
                )
                .to_string()
            ]
        );

        let _ = std::fs::remove_dir_all(&root);
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
            &Managed::default(),
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
        let kept = root.join("kept");
        std::fs::create_dir_all(&kept).expect("the server's own directory");
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
            &[],
            Some(&work),
            &kept,
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
        assert!(
            reads(&policy, &kept) && writes(&policy, &kept),
            "a server keeps its own files in the directory it was given"
        );

        let undirected = confinement(
            Some(Prelude::MacOs),
            &program,
            &[],
            &[],
            None,
            &kept,
            &temporary,
            Some(&home),
        )
        .expect("a policy");
        assert_eq!(undirected.starting_in.as_deref(), Some(temporary.as_path()));
        assert!(
            confinement(
                None,
                &program,
                &[],
                &[],
                None,
                &kept,
                &temporary,
                Some(&home)
            )
            .is_none()
        );
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
            &[],
            None,
            &root.join("kept"),
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
        let profile = crate::home::profile().expect("a test runs with a home directory");
        let own = profile.join(".cargo").join("bin");

        let kept = scratch("cli-servers-kept-here");
        let policy = confinement_here(
            &own.join("server"),
            std::slice::from_ref(&own),
            &[],
            None,
            &kept,
        )
        .expect("a policy");

        let canonical = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        assert!(
            !reads(&policy, &canonical(&profile.join(".cargo"))),
            "a bin directory's parent in the home directory is the person's own"
        );
    }

    /// `nvm` installs a runner as a link from a `bin` directory deep in the home directory to a
    /// script inside the package it loads the rest of itself from.
    #[cfg(unix)]
    #[test]
    fn a_programs_own_installation_deep_in_the_home_directory_is_read() {
        let root = scratch("cli-servers-installation-in-home");
        let home = root.join("home");
        let node = home.join(".nvm/versions/node/v24");
        let npm = node.join("lib/node_modules/npm");
        let script = installed(&npm.join("bin"), "npx-cli.js");
        std::fs::create_dir_all(node.join("bin")).expect("node's bin");
        let program = node.join("bin/npx");
        std::os::unix::fs::symlink(&script, &program).expect("link the runner");

        let policy = confinement(
            Some(Prelude::MacOs),
            &program,
            &[node.join("bin")],
            &[],
            None,
            &root.join("kept"),
            &root.join("tmp"),
            Some(&home),
        )
        .expect("a policy");

        assert!(
            reads(&policy, &npm),
            "the installation the program came from is read"
        );
        assert!(reads(&policy, &node.join("bin")));
        assert!(
            !reads(&policy, &node),
            "a searched bin directory's parent in the home directory is the person's own"
        );
        assert!(!reads(&policy, &home.join(".nvm")));
        assert!(!reads(&policy, &home));
    }

    /// SERVERS-10: a file the declaration says it may read is granted as that one file while it is
    /// one, and read-only: never the directory it is in, and nothing where the path is a directory,
    /// is missing, is a link, or is reached through one, which is not the file that was shown.
    #[cfg(unix)]
    #[test]
    fn a_declared_read_grants_the_one_file_and_nothing_beside_it() {
        // Resolved, as `add` records a read.
        let root = scratch("cli-servers-reads")
            .canonicalize()
            .expect("the scratch directory resolves");
        let home = root.join("home");
        let keys = home.join("keys");
        std::fs::create_dir_all(&keys).expect("keys");
        let key = keys.join("brave-api-key");
        std::fs::write(&key, "k").expect("the key");
        let linked = keys.join("linked");
        std::os::unix::fs::symlink(&key, &linked).expect("link the key");
        let through = home.join("linked-keys");
        std::os::unix::fs::symlink(&keys, &through).expect("link the keys directory");
        let program = installed(&root.join("opt/bin"), "npx");

        let policy = confinement(
            Some(Prelude::MacOs),
            &program,
            &[],
            &[
                key.clone(),
                keys.clone(),
                keys.join("missing"),
                linked.clone(),
                through.join("brave-api-key"),
            ],
            None,
            &root.join("kept"),
            &root.join("tmp"),
            Some(&home),
        )
        .expect("a policy");

        assert!(reads(&policy, &key));
        assert!(!writes(&policy, &key));
        let in_home: Vec<&PathBuf> = policy
            .readable
            .iter()
            .filter(|row| row.starts_with(&home))
            .collect();
        assert_eq!(in_home, [&key], "more of the home directory was granted");
    }

    /// A capability set whose backend grants a path that is not on disk, or one whose backend
    /// cannot. Written here rather than read off this machine, so both answers are checked on
    /// every platform that runs the suite.
    fn a_backend_naming_an_absent_path(grants: bool) -> Capabilities {
        Capabilities {
            level: ConfinementLevel::Partial,
            mechanisms: vec!["test"],
            network_denial_enforced: false,
            grants_paths_that_do_not_exist: grants,
        }
    }

    /// SANDBOX-9: a server started under fewer paths than were granted it will be refused one
    /// somebody meant it to reach, and nothing else tells the person reading that refusal apart
    /// from a path that was never granted. Each path by name, since a count hides the one they are
    /// looking for.
    #[test]
    fn a_server_started_without_paths_its_backend_cannot_name_has_a_note_naming_each_one() {
        let root = scratch("cli-servers-left-out");
        let there = root.join("there");
        std::fs::create_dir_all(&there).expect("there");
        let absent = root.join("absent");
        let blurred = root.join("absent directory");
        let wanted = SandboxPolicy::strict()
            .allow_read(&there)
            .allow_read(&absent)
            .allow_write(&blurred);

        let (policy, note) = nameable("weather", wanted, &a_backend_naming_an_absent_path(false));

        assert_eq!(policy.readable, vec![there]);
        assert!(policy.writable.is_empty());
        assert_eq!(
            note,
            Some(
                t!(
                    servers_paths_left_out,
                    alias = "weather",
                    paths = format!("{}, {:?}", absent.display(), blurred.display().to_string())
                )
                .to_string()
            ),
            "the note did not name both paths as the launch left them"
        );
    }

    /// Resolution leaves a path out only where the backend cannot name one, so a backend that can
    /// leaves none out and there is nothing to say. A note on every launch there is a note nobody
    /// reads.
    #[test]
    fn a_server_whose_backend_names_an_absent_path_is_started_with_no_such_note() {
        let absent = scratch("cli-servers-nothing-left-out").join("absent");
        let wanted = SandboxPolicy::strict().allow_read(&absent);

        let (policy, note) = nameable("weather", wanted, &a_backend_naming_an_absent_path(true));

        assert_eq!(policy.readable, vec![absent]);
        assert_eq!(note, None);
    }

    /// SERVERS-10: a stored value reaches the server as the declaration holds it, and this
    /// process's environment is read only for the names it declares. A stored `PATH` is the one a
    /// bare name is looked for in.
    #[test]
    fn a_stored_value_is_handed_to_the_server_and_the_environment_is_not_read_for_it() {
        let root = scratch("cli-servers-stored");
        let bin = root.join("bin");
        let program = installed(&bin, "weather-mcp");
        let env = [
            ("PATH".to_string(), bin.to_str().unwrap().to_string()),
            ("WEATHER_TOKEN".to_string(), "stored".to_string()),
        ]
        .into();
        let declaration = Declaration::stdio(words(&["weather-mcp"]), words(&["REGION"]), None)
            .and_then(|declaration| declaration.storing(env))
            .expect("a declaration");
        let asked = std::cell::RefCell::new(Vec::new());
        let environment = |name: &str| {
            asked.borrow_mut().push(name.to_string());
            Some(OsString::from(format!("from {name}")))
        };

        let Ok(Plan::Stdio {
            program: found,
            variables,
            searched,
            ..
        }) = planned(&declaration, &environment)
        else {
            panic!("no local plan");
        };

        assert_eq!(found, program);
        assert_eq!(searched, std::slice::from_ref(&bin));
        assert_eq!(
            variables,
            Variables::new()
                .with("REGION", "from REGION")
                .with("PATH", &bin)
                .with("WEATHER_TOKEN", "stored")
        );
        assert_eq!(asked.into_inner(), ["REGION"]);
    }

    fn digested(argv: &[&str]) -> Digest {
        Declaration::stdio(words(argv), vec!["PATH".to_string()], None)
            .expect("declaration")
            .digest()
    }

    /// The bounds a declaration gives are shown where the person is asked about it, and a
    /// declaration that gives none shows none, so no line is added to what is drawn today.
    #[test]
    fn a_declarations_own_timeouts_are_drawn_and_the_defaults_are_not() {
        let plain = Declaration::http("https://a.example.com".into()).expect("declaration");
        let timed = plain
            .clone()
            .timing(Timeouts::new(Some(5), None).expect("timeouts"));

        let drawn_plain = drawn("remote", &plain, "abcd1234");
        let drawn_timed = drawn("remote", &timed, "abcd1234");

        assert!(!drawn_plain.iter().any(|line| line.contains("seconds")));
        let [.., line, _digest] = drawn_timed.as_slice() else {
            panic!("{drawn_timed:?}");
        };
        assert!(
            line.contains("5 seconds to start") && line.contains("120 seconds for each call"),
            "{line}"
        );
    }

    /// One per declaration, so what a runner fetched is there on its next launch and a declaration
    /// edited to run something else starts with nothing the one before it wrote.
    #[test]
    fn a_server_keeps_a_home_of_its_own_under_the_state_directory() {
        let state = scratch("cli-servers-own-home");
        let home = Home {
            directory: Some(state.clone()),
            writable: true,
        };
        let weather = digested(&["weather-mcp"]);

        let (own, throwaway) = own_home(&home, &weather).expect("a home");

        assert!(throwaway.is_none(), "a kept home is not removed");
        assert_eq!(own, mcp::server_home(&state, &weather));
        assert!(own.is_dir());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&own)
                .expect("its metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o700, "mode was {:o}", mode & 0o777);
        }
        assert_eq!(own_home(&home, &weather).expect("again").0, own);
        assert_ne!(
            own_home(&home, &digested(&["other-mcp"]))
                .expect("another")
                .0,
            own
        );
    }

    #[test]
    fn a_server_in_a_session_that_keeps_nothing_is_given_a_home_that_goes_with_it() {
        let state = scratch("cli-servers-throwaway-home");
        let home = Home {
            directory: Some(state.clone()),
            writable: false,
        };

        let (own, throwaway) = own_home(&home, &digested(&["weather-mcp"])).expect("a home");
        let throwaway = throwaway.expect("a home that goes");

        assert_eq!(own, throwaway.path());
        assert!(own.is_dir());
        assert!(own.starts_with(temporary_directory()));
        assert_eq!(
            std::fs::read_dir(&state)
                .expect("the state directory")
                .count(),
            0,
            "a session that keeps nothing wrote under the state directory"
        );
        drop(throwaway);
        assert!(!own.exists(), "{} outlived its server", own.display());
    }

    #[test]
    fn a_server_is_handed_its_own_home_unless_the_declaration_names_one() {
        let own = Path::new("/state/mcp-home/weather");

        assert_eq!(
            at_home(Variables::new().with("PATH", "/usr/bin"), own),
            Variables::new().with("PATH", "/usr/bin").with("HOME", own)
        );
        let declared = Variables::new().with("HOME", "/somewhere/else");
        assert_eq!(at_home(declared.clone(), own), declared);
    }

    /// Says where its home is in the directory it starts in, and answers only once it has written
    /// a file there.
    #[cfg(unix)]
    const HOME_WRITING_SERVER: &str = r#"#!/bin/sh
printf '%s' "$HOME" > home-was
touch "$HOME/was-here" || exit 1
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *'"initialize"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2025-06-18","capabilities":{},"serverInfo":{"name":"fake","version":"1"}}}\n' "$id"
      ;;
    *'"tools/list"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[]}}\n' "$id"
      ;;
  esac
done
"#;

    /// Start [`HOME_WRITING_SERVER`] from `work` under this machine's confinement, or `None` where
    /// there is none to start it under.
    #[cfg(unix)]
    fn started_writing_home(
        root: &Path,
        work: &Path,
        home: &Home,
        notes: &mut Vec<String>,
    ) -> Option<Vec<crate::mcp::Reached>> {
        if Prelude::current().is_none() || !bravebot_sandbox::confinement_works_here() {
            eprintln!("SKIPPED (no confinement here)");
            return None;
        }
        let program = installed(&root.join("bin"), "weather-mcp");
        std::fs::write(&program, HOME_WRITING_SERVER).expect("write the server");
        std::fs::create_dir_all(work).expect("work");
        let plan = Plan::Stdio {
            program,
            arguments: Vec::new(),
            variables: Variables::new(),
            searched: Vec::new(),
            reads: Vec::new(),
            directory: Some(work.to_path_buf()),
            declared: digested(&["weather-mcp"]),
            timeouts: Timeouts::default(),
        };
        Some(start(
            vec![("weather".to_string(), plan)],
            home,
            Stream::Null,
            notes,
            &mut Vec::new(),
        ))
    }

    /// The note a launch of `program` from `work` with `own` as its home leaves about what this
    /// machine's confinement left out, and nothing where it left out nothing.
    ///
    /// Which of the prelude's spellings a distribution lacks is the machine's, so the note is built
    /// from the policy the launch built rather than written down here. Only the launch's other
    /// notes are what the tests below are about. Takes the same program and declared reads the
    /// launch does, since a row that is on disk is not one the resolution leaves out.
    #[cfg(unix)]
    fn left_out_of(program: &Path, reads: &[PathBuf], work: &Path, own: &Path) -> Vec<String> {
        let sandbox = bravebot_sandbox::for_current_platform().expect("a backend");
        let policy = confinement_here(program, &[], reads, Some(work), own)
            .expect("a policy to start under");
        Vec::from_iter(nameable("weather", policy, &sandbox.capabilities()).1)
    }

    #[cfg(unix)]
    #[test]
    fn a_started_server_writes_its_own_files_in_the_home_kept_for_it() {
        let root = scratch("cli-servers-started-kept-home");
        let state = root.join("state");
        std::fs::create_dir_all(&state).expect("state");
        let work = root.join("work");
        let home = Home {
            directory: Some(state.clone()),
            writable: true,
        };
        let mut notes = Vec::new();

        let Some(started) = started_writing_home(&root, &work, &home, &mut notes) else {
            return;
        };

        let own = mcp::server_home(&state, &digested(&["weather-mcp"]));
        assert_eq!(
            notes,
            left_out_of(&root.join("bin").join("weather-mcp"), &[], &work, &own)
        );
        assert_eq!(started.len(), 1);
        assert_eq!(
            std::fs::read_to_string(work.join("home-was")).expect("where its home was"),
            own.display().to_string()
        );
        drop(started);
        assert!(
            own.join("was-here").exists(),
            "what a server wrote is there for its next launch"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_started_server_in_a_session_that_keeps_nothing_has_a_home_that_goes_with_it() {
        let root = scratch("cli-servers-started-throwaway-home");
        let state = root.join("state");
        std::fs::create_dir_all(&state).expect("state");
        let work = root.join("work");
        let home = Home {
            directory: Some(state.clone()),
            writable: false,
        };
        let mut notes = Vec::new();

        let Some(started) = started_writing_home(&root, &work, &home, &mut notes) else {
            return;
        };

        let own = PathBuf::from(
            std::fs::read_to_string(work.join("home-was")).expect("where its home was"),
        );
        assert_eq!(
            notes,
            left_out_of(&root.join("bin").join("weather-mcp"), &[], &work, &own)
        );
        assert_eq!(started.len(), 1);
        assert!(own.join("was-here").exists());
        assert!(own.starts_with(temporary_directory()));
        assert_eq!(std::fs::read_dir(&state).expect("state").count(), 0);
        drop(started);
        assert!(!own.exists(), "{} outlived its server", own.display());
    }

    /// A server at a port of its own answering each request with `answer`, and every body it was
    /// sent.
    fn serving(answer: fn(&str, &str) -> String, to: String) -> (String, mpsc::Receiver<String>) {
        use std::io::Read;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok((mut stream, _)) = listener.accept() {
                let mut reader = std::io::BufReader::new(stream.try_clone().expect("clone"));
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some((name, value)) = line.split_once(':')
                        && name.eq_ignore_ascii_case("content-length")
                    {
                        length = value.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0; length];
                let _ = reader.read_exact(&mut body);
                let body = String::from_utf8_lossy(&body).into_owned();
                let _ = sender.send(body.clone());
                let _ = stream.write_all(answer(&body, &to).as_bytes());
                let _ = stream.flush();
            }
        });
        (format!("http://127.0.0.1:{port}/mcp"), receiver)
    }

    /// The weather server's reply to `body`, which names its method and, where it wants an
    /// answer, its numeric id.
    fn weather(body: &str, _: &str) -> String {
        let id: String = body
            .split_once(r#""id":"#)
            .map(|(_, rest)| rest.chars().take_while(char::is_ascii_digit).collect())
            .unwrap_or_else(|| "null".to_string());
        let result = if body.contains(r#""initialize""#) {
            r#"{"protocolVersion":"2025-06-18","capabilities":{},"serverInfo":{"name":"weather","version":"1"}}"#
        } else if body.contains(r#""tools/list""#) {
            r#"{"tools":[{"name":"get_forecast","description":"the forecast","inputSchema":{"type":"object"}}]}"#
        } else {
            "{}"
        };
        let reply = format!(r#"{{"jsonrpc":"2.0","id":{id},"result":{result}}}"#);
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
            reply.len()
        )
    }

    /// A redirect of every request to `to`, which is what a server that moved answers and what a
    /// server sending its traffic somewhere else answers too (SERVERS-11).
    fn redirecting(_: &str, to: &str) -> String {
        format!(
            "HTTP/1.1 307 Temporary Redirect\r\nLocation: {to}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
    }

    /// A weather server declared and approved at a url that redirects its handshake to a second
    /// one, and what that second one is sent.
    struct Redirected {
        home: PathBuf,
        project: PathBuf,
        declared: Declaration,
        declared_url: String,
        moved_to: String,
        first: mpsc::Receiver<String>,
        second: mpsc::Receiver<String>,
    }

    fn redirected(name: &str) -> Redirected {
        let root = scratch(name);
        let home = root.join("home");
        let project = root.join("project");
        std::fs::create_dir_all(&home).expect("home");
        std::fs::create_dir_all(project.join(".bravebot")).expect("project");
        let (moved_to, second) = serving(weather, String::new());
        let (declared_url, first) = serving(redirecting, moved_to.clone());
        let declared = Declaration::http(declared_url.clone()).expect("declaration");
        let mut declarations = Declarations::default();
        declarations.insert("weather", &declared);
        std::fs::write(mcp::declarations_file(&home), declarations.to_text()).expect("mcp.json");
        let mut approvals = Approvals::default();
        approvals.approve("weather", declared.digest());
        std::fs::write(mcp::approvals_file(&home), approvals.to_text()).expect("mcp-approved");
        Redirected {
            home,
            project,
            declared,
            declared_url,
            moved_to,
            first,
            second,
        }
    }

    /// Reach the redirected server with `typed` at the terminal, and what was drawn there.
    fn reached_redirected(
        redirected: &Redirected,
        asking: Asking,
        present: bool,
        writable: bool,
        typed: &str,
    ) -> (Reached, String) {
        let requested = vec![(
            redirected.project.join(".bravebot/settings.json"),
            "weather".to_string(),
        )];
        let home = Home {
            directory: Some(redirected.home.clone()),
            writable,
        };
        let mut person = Person {
            answers: typed.as_bytes(),
            screen: Vec::new(),
            present,
        };
        let reached = reach(
            &requested,
            &redirected.project,
            &home,
            &Managed::at(&managed_beside(&redirected.home)),
            asking,
            &mut person,
            Stream::Null,
        );
        (reached, String::from_utf8(person.screen).expect("screen"))
    }

    fn declared_in(home: &Path) -> Declaration {
        Declarations::read(home)
            .expect("mcp.json")
            .get("weather")
            .expect("weather is declared")
            .declaration
            .expect("a declaration")
    }

    /// SERVERS-11: a handshake redirected off the declaration is put to the person with where it
    /// points and what that reaches, and a yes starts the server there, rewrites its declaration
    /// and approves it in the same write.
    #[test]
    fn a_handshake_redirected_off_its_declaration_is_moved_on_a_yes() {
        let redirected = redirected("cli-servers-move-yes");

        let (reached, screen) = reached_redirected(&redirected, Asking::Person, true, true, "y\n");

        assert_eq!(reached.aliases(), vec!["weather"], "{:?}", reached.notes);
        assert_eq!(
            reached.notes,
            vec![t!(mcp_move_moved, alias = "weather").to_string()]
        );
        let authority = redirected
            .moved_to
            .trim_start_matches("http://")
            .trim_end_matches("/mcp");
        for drawn in [
            t!(
                mcp_move_declared,
                alias = "weather",
                url = redirected.declared_url.as_str()
            ),
            t!(mcp_move_destination, url = redirected.moved_to.as_str()),
            t!(mcp_move_reaching, authority = authority),
            t!(mcp_move_title).to_string(),
        ] {
            assert!(
                screen.contains(drawn.as_str()),
                "{drawn} is not drawn in {screen}"
            );
        }
        assert!(!screen.contains(t!(mcp_move_this_session_only)));
        let second: Vec<String> = redirected.second.try_iter().collect();
        assert!(
            second
                .first()
                .is_some_and(|body| body.contains(r#""initialize""#)),
            "the server was not started where it moved: {second:?}"
        );
        let now = Declaration::http(redirected.moved_to.clone()).expect("moved");
        assert_eq!(declared_in(&redirected.home), now);
        let approvals = Approvals::read(&redirected.home);
        assert!(approvals.approves(&now.digest()));
        assert!(!approvals.approves(&redirected.declared.digest()));
    }

    /// SERVERS-11: a no starts nothing, sends nothing where the reply pointed and rewrites
    /// nothing.
    #[test]
    fn a_handshake_redirected_off_its_declaration_starts_nothing_on_a_no() {
        let redirected = redirected("cli-servers-move-no");
        let approved =
            std::fs::read_to_string(mcp::approvals_file(&redirected.home)).expect("approvals");

        let (reached, screen) = reached_redirected(&redirected, Asking::Person, true, true, "n\n");

        assert!(screen.contains(t!(mcp_move_title)), "{screen}");
        assert!(reached.aliases().is_empty());
        assert_eq!(
            reached.notes,
            vec![t!(mcp_move_not_started, alias = "weather").to_string()]
        );
        assert!(redirected.second.try_iter().next().is_none());
        assert_eq!(declared_in(&redirected.home), redirected.declared);
        assert_eq!(
            std::fs::read_to_string(mcp::approvals_file(&redirected.home)).expect("approvals"),
            approved
        );
    }

    /// SERVERS-13: bypassing every check refuses the move unasked, as a one-shot run and a session
    /// with nobody at the terminal do.
    #[test]
    fn a_redirected_handshake_nobody_is_asked_about_starts_nothing() {
        for (asking, present) in [
            (Asking::Bypass, true),
            (Asking::OneShot, true),
            (Asking::Person, false),
        ] {
            let redirected = redirected("cli-servers-move-unasked");

            let (reached, screen) = reached_redirected(&redirected, asking, present, true, "y\n");

            assert_eq!(screen, "", "{asking:?} asked");
            assert!(reached.aliases().is_empty(), "{asking:?}");
            assert_eq!(
                reached.notes,
                vec![t!(mcp_move_not_started, alias = "weather").to_string()],
                "{asking:?}"
            );
            assert_eq!(
                redirected.first.try_iter().count(),
                1,
                "{asking:?}: the handshake was not tried where it is declared"
            );
            assert!(
                redirected.second.try_iter().next().is_none(),
                "{asking:?} sent something where the reply pointed"
            );
            assert_eq!(
                declared_in(&redirected.home),
                redirected.declared,
                "{asking:?}"
            );
        }
    }

    /// SERVERS-11: a project a person said to use every server in answers for what the checkout
    /// requests and not for where a server went, so the move is still asked, and a yes approves
    /// where it moved in `mcp-approved` rather than leaving it to the project.
    #[test]
    fn a_project_that_answers_for_its_servers_does_not_answer_a_move() {
        let redirected = redirected("cli-servers-move-project");
        std::fs::remove_file(mcp::approvals_file(&redirected.home)).expect("unapproved");
        let mut projects = Projects::default();
        projects.add(&redirected.project);
        std::fs::write(mcp::projects_file(&redirected.home), projects.to_text()).expect("project");

        let (reached, screen) = reached_redirected(&redirected, Asking::Person, true, true, "y\n");

        assert!(screen.contains(t!(mcp_move_title)), "{screen}");
        assert_eq!(reached.aliases(), vec!["weather"], "{:?}", reached.notes);
        let now = Declaration::http(redirected.moved_to.clone()).expect("moved");
        assert_eq!(declared_in(&redirected.home), now);
        assert!(Approvals::read(&redirected.home).approves(&now.digest()));
    }

    /// A session that writes nothing says so on the prompt, and a yes starts the server where it
    /// moved for the session only.
    #[test]
    fn a_move_in_a_session_that_writes_nothing_is_for_the_session() {
        let redirected = redirected("cli-servers-move-unwritable");

        let (reached, screen) = reached_redirected(&redirected, Asking::Person, true, false, "y\n");

        assert!(screen.contains(t!(mcp_move_this_session_only)), "{screen}");
        assert_eq!(reached.aliases(), vec!["weather"], "{:?}", reached.notes);
        assert_eq!(
            reached.notes,
            vec![t!(mcp_move_moved, alias = "weather").to_string()]
        );
        assert_eq!(declared_in(&redirected.home), redirected.declared);
    }

    /// SANDBOX-9: a launch under a backend that cannot name a path missing from disk leaves the
    /// note saying which granted paths it was started without, and the launch is where that note
    /// has to come from.
    ///
    /// The capabilities are the test's rather than this machine's, so the dropping is reached on
    /// every platform that runs the suite. Read off the host instead, this would check nothing on
    /// macOS, where a backend names a path that is not there and so leaves nothing out: the note
    /// would be absent, an expectation built the same way would be absent too, and the two would
    /// agree with the reporting taken out of [`start`] altogether.
    #[cfg(unix)]
    #[test]
    fn a_launch_under_a_backend_that_cannot_name_an_absent_path_leaves_the_note() {
        let root = scratch("cli-servers-launch-left-out")
            .canonicalize()
            .expect("the scratch directory resolves");
        if Prelude::current().is_none() || !bravebot_sandbox::confinement_works_here() {
            eprintln!("SKIPPED (no confinement here)");
            return;
        }
        // A declared PATH directory, which `confinement` grants as written: a read of a named file
        // is dropped where the file is not there, so a row that is absent has to come from here.
        // Outside the home directory, which is the other thing that decides whether it is granted.
        let absent = temporary_directory().join("bravebot-absent-path-directory");
        assert!(!absent.exists(), "{} is on disk", absent.display());
        let program = installed(&root.join("bin"), "weather-mcp");
        std::fs::write(&program, HOME_WRITING_SERVER).expect("write the server");
        let work = root.join("work");
        std::fs::create_dir_all(&work).expect("work");
        let state = root.join("state");
        std::fs::create_dir_all(&state).expect("state");
        let home = Home {
            directory: Some(state.clone()),
            writable: true,
        };
        let plan = Plan::Stdio {
            program,
            arguments: Vec::new(),
            variables: Variables::new(),
            searched: vec![absent.clone()],
            reads: Vec::new(),
            directory: Some(work.clone()),
            declared: digested(&["weather-mcp"]),
            timeouts: Timeouts::default(),
        };
        let mut notes = Vec::new();

        let started = start_under(
            Some(a_backend_naming_an_absent_path(false)),
            vec![("weather".to_string(), plan)],
            &home,
            Stream::Null,
            &mut notes,
            &mut Vec::new(),
        );

        assert_eq!(started.len(), 1, "the server did not start: {notes:?}");
        // One note, naming the row this test made absent. Not the whole list: the base names
        // spellings a distribution may lack, so on Linux the prelude's own absent rows are in here
        // too and are no part of what this is about. What the note says in full is
        // `a_server_started_without_paths_its_backend_cannot_name_has_a_note_naming_each_one`,
        // which writes the whole policy down and so does not read this machine's.
        let [said] = notes.as_slice() else {
            panic!("one note, saying which granted paths the launch left out: {notes:?}");
        };
        assert!(
            said.contains(&absent.display().to_string()),
            "the launch did not say it was started without {}: {said}",
            absent.display()
        );
    }

    /// SERVERS-10: a started server reads the files its declaration says it may, a key file a
    /// stored value names and one an argument names, and nothing beside them. It exits before it
    /// answers where it cannot read either. A read in the state directory, which only a hand edit
    /// of the file can declare, is not granted.
    #[cfg(unix)]
    #[test]
    fn a_started_server_reads_the_files_it_was_declared_to_and_nothing_beside_them() {
        let root = scratch("cli-servers-started-reads")
            .canonicalize()
            .expect("the scratch directory resolves");
        if Prelude::current().is_none() || !bravebot_sandbox::confinement_works_here() {
            eprintln!("SKIPPED (no confinement here)");
            return;
        }
        let keys = root.join("keys");
        std::fs::create_dir_all(&keys).expect("keys");
        let key = keys.join("brave-api-key");
        std::fs::write(&key, "the key").expect("the key");
        std::fs::write(keys.join("beside"), "not granted").expect("a file beside it");
        let argument = root.join("argument");
        std::fs::write(&argument, "the argument").expect("the argument");
        // Not in a `bin` directory, which would bring its parent and every file above.
        let program = installed(&root.join("server"), "weather-mcp");
        let (_, answering) = HOME_WRITING_SERVER
            .split_once("while ")
            .expect("the server answers in a loop");
        let reading = [
            "#!/bin/sh",
            r#"cat "$KEY_FILE" > key-was || exit 1"#,
            r#"cat "$1" > argument-was || exit 1"#,
            r#"if cat "${KEY_FILE%/*}/beside" > /dev/null 2>&1; then touch beside-read; fi"#,
            r#"if cat "$STATE_FILE" > /dev/null 2>&1; then touch state-read; fi"#,
        ];
        std::fs::write(
            &program,
            format!("{}\nwhile {answering}", reading.join("\n")),
        )
        .expect("write the server");
        let work = root.join("work");
        std::fs::create_dir_all(&work).expect("work");
        let state = root.join("state");
        std::fs::create_dir_all(&state).expect("state");
        let approvals = state.join("mcp-approvals.json");
        std::fs::write(&approvals, "{}").expect("a file in the state directory");
        let home = Home {
            directory: Some(state.clone()),
            writable: true,
        };
        let reads = vec![key.clone(), argument.clone(), approvals.clone()];
        let plan = Plan::Stdio {
            program: program.clone(),
            arguments: vec![argument.to_str().unwrap().to_string()],
            variables: Variables::new()
                .with("KEY_FILE", &key)
                .with("STATE_FILE", &approvals),
            searched: Vec::new(),
            reads: reads.clone(),
            directory: Some(work.clone()),
            declared: digested(&["weather-mcp"]),
            timeouts: Timeouts::default(),
        };
        let mut notes = Vec::new();

        let started = start(
            vec![("weather".to_string(), plan)],
            &home,
            Stream::Null,
            &mut notes,
            &mut Vec::new(),
        );

        let own = mcp::server_home(&state, &digested(&["weather-mcp"]));
        assert_eq!(notes, left_out_of(&program, &reads, &work, &own));
        assert_eq!(started.len(), 1);
        let read = |name: &str| std::fs::read_to_string(work.join(name)).expect("what it read");
        assert_eq!(read("key-was"), "the key");
        assert_eq!(read("argument-was"), "the argument");
        assert!(
            !work.join("beside-read").exists(),
            "the server read a file beside the one it was granted"
        );
        assert!(
            !work.join("state-read").exists(),
            "the server read a file in the state directory"
        );
    }

    /// Answers its handshake after `handshake` seconds and a call after `call`, so a bound on
    /// either decides whether the answer is read.
    #[cfg(unix)]
    fn slow_server(handshake: u32, call: u32) -> String {
        format!(
            r#"#!/bin/sh
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *'"initialize"'*)
      sleep {handshake}
      printf '{{"jsonrpc":"2.0","id":%s,"result":{{"protocolVersion":"2025-06-18","capabilities":{{}},"serverInfo":{{"name":"fake","version":"1"}}}}}}\n' "$id"
      ;;
    *'"tools/list"'*)
      printf '{{"jsonrpc":"2.0","id":%s,"result":{{"tools":[]}}}}\n' "$id"
      ;;
    *'"tools/call"'*)
      sleep {call}
      printf '{{"jsonrpc":"2.0","id":%s,"result":{{"content":[{{"type":"text","text":"done"}}]}}}}\n' "$id"
      ;;
  esac
done
"#
        )
    }

    /// Start the server `script` is from `root` under this machine's confinement, given
    /// `timeouts`, or `None` where there is none to start it under.
    #[cfg(unix)]
    fn started_slow(
        root: &Path,
        script: &str,
        timeouts: Timeouts,
        notes: &mut Vec<String>,
    ) -> Option<Vec<crate::mcp::Reached>> {
        if Prelude::current().is_none() || !bravebot_sandbox::confinement_works_here() {
            eprintln!("SKIPPED (no confinement here)");
            return None;
        }
        let state = root.join("state");
        let work = root.join("work");
        std::fs::create_dir_all(&state).expect("state");
        std::fs::create_dir_all(&work).expect("work");
        let program = installed(&root.join("bin"), "weather-mcp");
        std::fs::write(&program, script).expect("write the server");
        let plan = Plan::Stdio {
            program,
            arguments: Vec::new(),
            variables: Variables::new(),
            searched: Vec::new(),
            reads: Vec::new(),
            directory: Some(work),
            declared: digested(&["weather-mcp"]),
            timeouts,
        };
        let home = Home {
            directory: Some(state),
            writable: true,
        };
        Some(start(
            vec![("weather".to_string(), plan)],
            &home,
            Stream::Null,
            notes,
            &mut Vec::new(),
        ))
    }

    /// A server that does not answer within the startup bound its declaration gives is left out
    /// with a note saying so, when that bound passes and not when the default does.
    #[cfg(unix)]
    #[test]
    fn a_server_not_answering_its_handshake_within_its_startup_bound_is_left_out_at_that_bound() {
        let root = scratch("cli-servers-startup-bound");
        let mut notes = Vec::new();
        let timeouts = Timeouts::new(Some(1), None).expect("timeouts");

        let started = std::time::Instant::now();
        let Some(reached) = started_slow(&root, &slow_server(30, 0), timeouts, &mut notes) else {
            return;
        };

        assert!(reached.is_empty(), "a server that never answered was kept");
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "the handshake was waited on past its bound: {:?}",
            started.elapsed()
        );
        let about_handshake: Vec<&String> = notes
            .iter()
            .filter(|note| note.contains("handshake"))
            .collect();
        let [note] = about_handshake.as_slice() else {
            panic!("notes were {notes:?}");
        };
        assert!(
            note.contains("weather") && note.contains("initialize timed out after 1 seconds"),
            "{note}"
        );
    }

    /// A handshake given longer than the server takes completes, so the bound is the declaration's
    /// and a slow but working server is not left out.
    #[cfg(unix)]
    #[test]
    fn a_server_answering_its_handshake_within_a_longer_startup_bound_is_kept() {
        let root = scratch("cli-servers-startup-longer");
        let mut notes = Vec::new();
        let timeouts = Timeouts::new(Some(30), None).expect("timeouts");

        let Some(reached) = started_slow(&root, &slow_server(2, 0), timeouts, &mut notes) else {
            return;
        };

        assert_eq!(reached.len(), 1, "{notes:?}");
        assert!(
            !notes.iter().any(|note| note.contains("handshake")),
            "{notes:?}"
        );
    }

    /// Once the handshake is done a server is held to the call bound its declaration gives, which
    /// is not the startup bound: this one answers its handshake at once, takes three seconds to
    /// answer a call, and is given one.
    #[cfg(unix)]
    #[test]
    fn a_started_server_is_held_to_the_call_bound_its_declaration_gives() {
        let root = scratch("cli-servers-call-bound");
        let mut notes = Vec::new();
        let timeouts = Timeouts::new(Some(30), Some(1)).expect("timeouts");
        let Some(reached) = started_slow(&root, &slow_server(0, 3), timeouts, &mut notes) else {
            return;
        };
        assert_eq!(reached.len(), 1, "{notes:?}");
        let session = Session::new(
            reached,
            root.join("work"),
            Some(root.join("state")),
            true,
            Managed::default(),
        );
        let mut sink = RecordingSink::new();
        let mut routing = Routing::new();
        routing.insert_trusted("task", "call a tool");
        let mut policy = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::from_iter([Capability::McpCall(ServerAlias::new("weather"))]),
            &mut sink,
        )
        .expect("policy");

        let called = std::time::Instant::now();
        let error = session
            .call(
                &mut policy,
                &bravebot_net::Egress::new(),
                "weather",
                "lookup",
                serde_json::json!({}),
            )
            .expect_err("the answer came after the bound");

        assert_eq!(error.to_string(), "tool 'lookup' timed out after 1 seconds");
        assert!(called.elapsed() < Duration::from_millis(2900));
    }
}
