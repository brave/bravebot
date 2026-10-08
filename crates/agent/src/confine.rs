//! The profile a program a person asked for runs under.
//!
//! `run` starts the programs a plan names. On Linux and macOS each step reads the machine except the
//! locations that hold a credential, writes the directories the session was opened on, its scratch
//! directory, the temporary directory and the toolchain caches, and reads a credential directory
//! only where the scope its argv names says so. On Windows a step is held to the platform's base,
//! the list its resolved binary brings, the scope its argv names, the places its binary is
//! installed and the session's directories, and to nothing else a person's account can reach.
//! `docs/specs/sandboxing.md` decides every row; this composes them for one step.
//!
//! Nothing a program printed, and no value the model supplied, reaches a row. The inputs are the
//! compiled [`Step`], which a person read, and the session's own directories.

use crate::confirm::{Carried, Confined, Remembered};
use crate::exec::ExecError;
use crate::reach::{Grant, Reached};
use bravebot_core::command::Step;
use bravebot_sandbox::SandboxMode;
use bravebot_sandbox::Variables;
use bravebot_sandbox::base::{Prelude, base, run_base};
use bravebot_sandbox::network::{Network, program_talks_to_a_remote};
use bravebot_sandbox::policy::SandboxPolicy;
use bravebot_sandbox::rules::{Lists, Rules};
use bravebot_sandbox::scope::{Reach, Scope, environment_reach};
use bravebot_sandbox::toolchain::Toolchain;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

/// What a `run` program may reach in this session, before any step is read.
#[derive(Debug, Clone)]
pub struct Confinement {
    prelude: Prelude,
    temporary: PathBuf,
    home: Option<PathBuf>,
    roots: Vec<PathBuf>,
    scratch: Option<PathBuf>,
    /// What the session decided about the network for the stages it starts.
    network: Network,
    /// How much of the machine a step reads (SANDBOX-22). `Off` never reaches a confinement: a turn
    /// in that mode builds none ([`Confinement::here`] is not asked), so the rows below are the
    /// two modes that confine.
    mode: SandboxMode,
    /// The reach a person remembered for commands, which attaches to the steps its shape names.
    grants: Vec<Grant>,
    /// The person's own filesystem lists, resolved against this session's directories.
    filesystem: Rules,
    /// The program a test has the platform fail to confine, which no machine's real mechanism does
    /// on demand.
    #[cfg(test)]
    unconfinable: Option<String>,
}

impl Confinement {
    /// The confinement for a session on this machine, or `None` where the platform has no base to
    /// build one on. Every platform the agent runs on has one.
    ///
    /// `roots` are the directories the person opened the session on, each read and written. `home`
    /// is the account's profile directory, which is what `~` means and what no row reaches.
    pub fn here(roots: Vec<PathBuf>, scratch: Option<&Path>, home: Option<&Path>) -> Option<Self> {
        let prelude = Prelude::current()?;
        Some(Self::new(
            prelude,
            canonical(&temporary_directory()),
            home,
            roots,
            scratch,
        ))
    }

    /// A confinement against a prelude and directories given, so the rows are decided by code every
    /// platform's job runs.
    pub fn new(
        prelude: Prelude,
        temporary: PathBuf,
        home: Option<&Path>,
        roots: Vec<PathBuf>,
        scratch: Option<&Path>,
    ) -> Self {
        Self {
            prelude,
            temporary,
            home: home.map(canonical),
            roots: roots.iter().map(|root| canonical(root)).collect(),
            scratch: scratch.map(canonical),
            network: Network::Open,
            mode: SandboxMode::Standard,
            grants: Vec::new(),
            filesystem: Rules::none(),
            #[cfg(test)]
            unconfinable: None,
        }
    }

    /// This confinement with the session's decision about the network: with it closed, a stage
    /// keeps egress only where [`Confinement::egress`] says it carries a reason to.
    pub fn with_network(mut self, network: Network) -> Self {
        self.network = network;
        self
    }

    /// This confinement with the person's own filesystem lists (`sandbox.filesystem`), resolved
    /// now against the session's home and its first directory: globs are listed here, so a file a
    /// program makes afterwards is outside what one named, and a link is judged by where it leads.
    ///
    /// Every stage of every line this confinement starts gets them, a stage of a line left running
    /// included, because [`Confinement::policy`] is the one place a stage's rows are made.
    pub fn with_filesystem(mut self, lists: &Lists) -> Self {
        let base = self
            .roots
            .first()
            .cloned()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        self.filesystem = bravebot_sandbox::rules::resolve(lists, self.home.as_deref(), &base);
        self
    }

    /// The entries of the person's filesystem lists, each with what became of it, for a report.
    pub fn filesystem(&self) -> &Rules {
        &self.filesystem
    }

    /// Whether the stage `step` starts may reach the network.
    ///
    /// The one place it is decided, read by [`Confinement::policy`], [`Confinement::describe`] and
    /// [`Confinement::profile`] so they cannot disagree. Open, every stage has the egress the base
    /// grants. Closed, a stage has it where its resolved program is a toolchain's that fetches, where
    /// its argv names a credential scope, or where the program exists to talk to one.
    /// Decided from the compiled step and the session's setting and from nothing a program printed,
    /// and apart from the home directory: a `cargo build` fetches whether or not a cache is named.
    pub fn egress(&self, step: &Step) -> bool {
        !self.network.is_closed() || self.egress_reason(step).is_some()
    }

    /// Why a stage keeps the network under a closed setting, in words fixed here so the line a
    /// person reads never carries a name the plan chose.
    fn egress_reason(&self, step: &Step) -> Option<&'static str> {
        // Every reason below is read from the file name, so a file the plan could have written
        // under a directory it may write to must not earn the network by being named `curl`.
        if self.writable_by_the_plan(&step.resolved) {
            return None;
        }
        if Toolchain::of(&step.resolved).is_some_and(|toolchain| toolchain.fetches(&step.resolved))
        {
            Some("a toolchain that fetches")
        } else if Scope::of(&step.resolved, &step.args, &step.environment).is_some()
            || self
                .granted(step)
                .any(|grant| matches!(grant.reached, Reached::Scope(_)))
        {
            Some("a credential scope")
        } else if program_talks_to_a_remote(&step.resolved) {
            Some("a program that talks to a remote")
        } else {
            None
        }
    }

    fn writable_by_the_plan(&self, file: &Path) -> bool {
        let file = canonical(file);
        self.roots
            .iter()
            .chain(self.scratch.iter())
            .any(|dir| file.starts_with(dir))
            || file.starts_with(&self.temporary)
            || file.starts_with(canonical(&self.temporary))
    }

    /// What the trail records about the network for the stages of one run, or `None` where the
    /// session left it open and there is nothing to say.
    ///
    /// Each stage that keeps it is named by its place in the line and one of the fixed reasons,
    /// never by a program name or an argument the plan chose.
    pub fn network_for_the_trail(&self, steps: &[&Step]) -> Option<String> {
        if !self.network.is_closed() {
            return None;
        }
        let kept: Vec<String> = steps
            .iter()
            .enumerate()
            .filter_map(|(at, step)| {
                Some(format!("stage {} ({})", at + 1, self.egress_reason(step)?))
            })
            .collect();
        Some(match kept.is_empty() {
            true => "the network was closed for every stage of this run".to_string(),
            false => format!(
                "the network was closed for this run except for {}",
                kept.join(", ")
            ),
        })
    }

    /// This confinement held to `mode`.
    #[must_use]
    pub fn with_mode(mut self, mode: SandboxMode) -> Self {
        self.mode = mode;
        self
    }

    /// This confinement with the reach a person remembered for commands.
    ///
    /// Grants are inputs from a person's recorded answer and nothing else. They add rows to the
    /// steps they cover, and the plan says so before the step runs.
    pub fn with_grants(mut self, grants: Vec<Grant>) -> Self {
        self.grants = grants;
        self
    }

    /// The grants that attach to `step`, where this confinement can judge a reach at all.
    ///
    /// None without a home directory, since a directory is judged against it, and none for a step
    /// with an assignment in front of it ([`Grant::covers`]).
    fn granted<'a>(&'a self, step: &'a Step) -> impl Iterator<Item = &'a Grant> {
        self.grants
            .iter()
            .filter(move |grant| self.home.is_some() && grant.covers(step))
    }

    /// This confinement, failing for the step that starts `program` as the platform would for one
    /// it cannot confine.
    #[cfg(test)]
    pub(crate) fn failing_for(mut self, program: &str) -> Self {
        self.unconfinable = Some(program.to_string());
        self
    }

    /// Whether a step reads the machine except the credential locations, which is every step where
    /// the platform has a mechanism that can subtract from a read and the session names a home
    /// directory to find them under. With no home the credential rows cannot be built, and a read
    /// of the whole machine with no refusal is the one thing this must not grant, so the step is
    /// held to the listed rows instead. So is every step in the strict mode, which is that choice
    /// made by a person where the other is made by a missing home (SANDBOX-22).
    fn reads_the_machine(&self) -> bool {
        self.prelude != Prelude::Windows && self.home.is_some() && self.mode != SandboxMode::Strict
    }

    /// The toolchain list and the credential scope a step brings to its profile.
    ///
    /// The one place either is decided, read by [`Confinement::policy`] to build the rows and by
    /// [`Confinement::describe`] and [`Confinement::profile`] to say which rows there are. Neither
    /// is brought where the session names no home directory, since both are rows under it. Where
    /// the machine is read no step brings a toolchain list: the installs are readable already and
    /// every cache is written by every step.
    fn carries(&self, step: &Step) -> (Option<Toolchain>, Option<Scope>) {
        if self.home.is_none() {
            return (None, None);
        }
        (
            Toolchain::of(&step.resolved).filter(|_| !self.reads_the_machine()),
            Scope::of(&step.resolved, &step.args, &step.environment),
        )
    }

    /// What the profile of each step of `steps` holds beyond the base and the places its programs
    /// are installed, for the prompt a person approves from.
    pub fn describe(&self, steps: &[&Step]) -> Confined {
        self.describe_in(steps, &process_environment())
    }

    /// [`Confinement::describe`] for a stage that starts with `environment`.
    fn describe_in(&self, steps: &[&Step], environment: &[(String, String)]) -> Confined {
        Confined {
            reads_the_machine: self.reads_the_machine(),
            directories: self
                .roots
                .iter()
                .chain(self.scratch.iter())
                .cloned()
                .collect(),
            network: self.network,
            filesystem: self.filesystem.counts(),
            carried: steps
                .iter()
                .filter_map(|step| {
                    let (toolchain, scope) = self.carries(step);
                    // The process's own environment is what the executor starts a step with, and a
                    // step with an assignment in front of it carries no scope to move.
                    let reaches = match (scope, self.home.as_deref()) {
                        (Some(scope), Some(home)) => {
                            reaches(&step.resolved, scope, home, environment)
                        }
                        _ => Vec::new(),
                    };
                    let network = self.network.is_closed() && self.egress(step);
                    let remembered: Vec<Remembered> = self
                        .granted(step)
                        .filter(|grant| match (&grant.reached, self.home.as_deref()) {
                            (Reached::Directory(_), Some(home)) => grant.directory(home).is_some(),
                            _ => true,
                        })
                        .map(|grant| Remembered {
                            reached: match (&grant.reached, self.home.as_deref()) {
                                (Reached::Directory(_), Some(home)) => grant
                                    .directory(home)
                                    .map_or(grant.reached.clone(), Reached::Directory),
                                _ => grant.reached.clone(),
                            },
                            write: grant.write,
                            allowed: grant.allowed.clone(),
                        })
                        .collect();
                    (toolchain.is_some() || scope.is_some() || network || !remembered.is_empty())
                        .then(|| Carried {
                            program: step.program.clone(),
                            toolchain,
                            scope,
                            reaches,
                            network,
                            remembered,
                        })
                })
                .collect(),
        }
    }

    /// The policy one step runs under, started in `directory`.
    ///
    /// `environment` is the step's own, after the session's variables and the step's assignments
    /// are applied, since it is what the program will search `PATH` by.
    pub fn policy(
        &self,
        step: &Step,
        directory: &Path,
        environment: &[(String, String)],
    ) -> SandboxPolicy {
        let Step {
            program: _,
            resolved,
            started_as,
            args: _,
            environment: _,
            routes: _,
        } = step;
        let mut policy = if self.reads_the_machine() {
            run_base(self.prelude, &self.temporary, self.home.as_deref())
        } else {
            base(self.prelude, &self.temporary, None, self.home.as_deref())
        }
        .allow_git_directory_writes();
        if !self.egress(step) {
            policy = policy.without_network_egress();
        }

        if let Some(home) = self.home.as_deref() {
            if self.reads_the_machine() {
                policy = Toolchain::grant_every_cache(policy, self.prelude, home);
            }
            let (toolchain, scope) = self.carries(step);
            if let Some(toolchain) = toolchain {
                policy = toolchain.grant(policy, self.prelude, home);
            }
            if let Some(scope) = scope {
                policy = scope.grant(policy, home);
                if !self.reads_the_machine() {
                    for reach in reaches(resolved, scope, home, environment) {
                        policy = policy.allow_read(reach.path);
                    }
                }
                if scope == Scope::Remote
                    && let Some(socket) = variable(environment, "SSH_AUTH_SOCK")
                {
                    policy = policy.allow_write(socket);
                }
            }
            for grant in self.granted(step) {
                policy = match &grant.reached {
                    Reached::Scope(scope) => scope.grant(policy, home),
                    Reached::Directory(_) => match grant.directory(home) {
                        Some(path) if grant.write => policy.allow_read(&path).allow_write(path),
                        Some(path) => policy.allow_read(path),
                        None => policy,
                    },
                };
            }
        }

        if !self.reads_the_machine() {
            let searched: Vec<PathBuf> = variable(environment, "PATH")
                .map(|path| std::env::split_paths(&path).collect())
                .unwrap_or_default();
            for path in program_reads(resolved, &searched, self.home.as_deref()) {
                policy = policy.allow_read(path);
            }
            // The two files the step starts, as files: a program installed inside the home is in
            // no directory row, and a person read exactly these two.
            policy = policy.allow_read(canonical(resolved));
            if started_as != resolved {
                policy = policy.allow_read(started_as);
            }
        }

        for root in &self.roots {
            policy = policy.allow_read(root).allow_write(root);
        }
        if let Some(scratch) = &self.scratch {
            policy = policy.allow_read(scratch).allow_write(scratch);
        }
        self.filesystem.apply(policy).starting_in(directory)
    }

    /// The one sentence that says what the programs of `steps` ran under, for a result whose
    /// steps did not exit zero.
    ///
    /// Composed from this confinement and the compiled steps, which a person read, and from
    /// nothing a program printed or exited with: the same line follows a refusal, a failing test
    /// and a typo. It names the directories the session owns and the lists by name, and no path a
    /// program chose, since a program's error text is not where the profile is decided.
    pub fn profile(&self, steps: &[&Step]) -> String {
        let mut directories: Vec<String> = Vec::new();
        for directory in self.roots.iter().chain(self.scratch.as_ref()) {
            let shown = directory.display().to_string();
            if !directories.contains(&shown) {
                directories.push(shown);
            }
        }
        let mut toolchains = std::collections::BTreeSet::new();
        let mut scopes = std::collections::BTreeSet::new();
        let mut reaching = std::collections::BTreeSet::new();
        for step in steps {
            let (toolchain, scope) = self.carries(step);
            toolchains.extend(toolchain.map(Toolchain::name));
            scopes.extend(scope.map(Scope::name));
            if self.network.is_closed() {
                reaching.extend(self.egress_reason(step));
            }
            for grant in self.granted(step) {
                if let Reached::Scope(remembered) = grant.reached {
                    scopes.insert(remembered.name());
                }
            }
        }
        let named = |names: std::collections::BTreeSet<&str>| match names.is_empty() {
            true => "none".to_string(),
            false => names.into_iter().collect::<Vec<_>>().join(", "),
        };
        let network = match self.network {
            Network::Open => "open".to_string(),
            Network::Closed => format!("closed, kept only by steps with: {}", named(reaching),),
        };
        // The count of each list and never an entry: a path a person wrote is not text this line
        // has any use for repeating to the planner, and a glob's matches are the machine's.
        let rules = self.filesystem.counts();
        let rules = match rules.is_empty() {
            true => String::new(),
            false => format!(
                " The person's own rules also applied: {} allowRead, {} denyRead, {} allowWrite, \
                 {} denyWrite.",
                rules.allow_read, rules.deny_read, rules.allow_write, rules.deny_write,
            ),
        };
        if self.reads_the_machine() {
            return format!(
                "Confinement: programs could read this machine except the places that hold a \
                 credential, and write {} and the temporary directory and the toolchain caches; \
                 a place that holds a credential was read only where a credential scope added it \
                 for the steps that named one (credential scopes: {}). Any other path is refused \
                 by the operating system as `Operation not permitted` or `Permission denied`. \
                 Network: {}.{}",
                directories.join(", "),
                named(scopes),
                network,
                rules,
            );
        }
        format!(
            "Confinement: programs could read and write {} and the temporary directory, and read \
             the system and program directories and git's configuration files; beyond those \
             they reached only what a toolchain list or credential scope added for the steps \
             that named one (toolchain lists: {}; credential scopes: {}). Any other path is \
             refused by the operating system as `Operation not permitted` or `Permission \
             denied`. Network: {}.{}",
            directories.join(", "),
            named(toolchains),
            named(scopes),
            network,
            rules,
        )
    }

    /// What `command` is started as under this confinement, or the reason it cannot be.
    ///
    /// Refused rather than started unconfined where the platform's mechanism is missing or will
    /// not apply the policy: a program a person approved runs under the profile or not at all.
    fn prepared(
        &self,
        command: &Command,
        step: &Step,
        directory: &Path,
    ) -> Result<Prepared, ExecError> {
        let environment = effective_environment(command);
        let not_confined = |detail: String| ExecError::NotConfined {
            program: step.program.clone(),
            detail,
        };
        #[cfg(test)]
        if self.unconfinable.as_deref() == Some(step.program.as_str()) {
            return Err(not_confined("a test made the platform refuse".to_string()));
        }
        let sandbox = bravebot_sandbox::for_current_platform()
            .map_err(|error| not_confined(error.to_string()))?;
        let capabilities = sandbox.capabilities();
        let readable: Vec<(String, String)> = environment
            .iter()
            .filter_map(|(name, value)| {
                Some((name.to_str()?.to_string(), value.to_str()?.to_string()))
            })
            .collect();
        if let Some(item) = self.filesystem.unapplied_denial() {
            return Err(not_confined(format!(
                "sandbox.filesystem.{} names `{}`, which cannot be applied, and a stage started \
                 without it would reach the path it holds back",
                item.list.key(),
                item.entry.path
            )));
        }
        let wanted = self.policy(step, directory, &readable);
        if let Some(detail) = cannot_close_the_network(&wanted, &capabilities) {
            return Err(not_confined(detail));
        }
        let _ = wanted.create_missing_write_rows(&capabilities);
        let policy = wanted.nameable_under(&capabilities).policy;
        let variables = environment
            .into_iter()
            .fold(Variables::new(), |held, (name, value)| {
                held.with(name, value)
            });
        Ok(Prepared {
            sandbox,
            program: command.get_program().to_string_lossy().into_owned(),
            args: command
                .get_args()
                .map(|argument| argument.to_string_lossy().into_owned())
                .collect(),
            policy,
            variables,
        })
    }

    /// `command`, confined to what its step may reach, or the reason it cannot be.
    #[cfg(unix)]
    pub fn wrap(
        &self,
        command: Command,
        step: &Step,
        directory: &Path,
    ) -> Result<Command, ExecError> {
        use bravebot_sandbox::Environment;

        let Prepared {
            sandbox,
            program,
            args,
            policy,
            variables,
        } = self.prepared(&command, step, directory)?;
        sandbox
            .command(&program, &args, &policy, &Environment::Only(variables))
            .map_err(|error| ExecError::NotConfined {
                program: step.program.clone(),
                detail: error.to_string(),
            })
    }
}

/// A step ready to be started, under this session's confinement where it has one.
///
/// Decided before any file a redirection names is opened, so a step the platform will not
/// confine is refused ahead of every effect the line has. The standard streams are given when it
/// is started, since a pipeline cannot say where they go until the stage before has.
pub(crate) enum Launch {
    /// A command that is confined already, or that nobody asked to confine.
    Command(Command),
    /// A step to be started in a container, where confinement is an argument to the call that
    /// creates the process and so there is no command to hand back.
    #[cfg(windows)]
    Container(Box<Container>),
}

/// What a container step is started with.
#[cfg(windows)]
pub(crate) struct Container {
    step: String,
    prepared: Prepared,
}

/// A process started in a container, and the sandbox it is held to.
///
/// The sandbox goes with the process because the entries it wrote onto the person's directories
/// are taken off, and its profile deleted, as it is dropped: dropped before the process was gone,
/// the process would lose the access it was granted while running. The process is the first field
/// so it is the first dropped.
#[cfg(windows)]
pub(crate) struct Contained {
    pub(crate) child: bravebot_sandbox::ConfinedChild,
    _sandbox: Box<dyn bravebot_sandbox::Sandbox>,
}

/// `command`, ready to start under `confinement`, or the reason it cannot be.
pub(crate) fn begin(
    confinement: Option<&Confinement>,
    command: Command,
    step: &Step,
    directory: &Path,
) -> Result<Launch, ExecError> {
    let Some(confinement) = confinement else {
        return Ok(Launch::Command(command));
    };
    #[cfg(unix)]
    {
        confinement
            .wrap(command, step, directory)
            .map(Launch::Command)
    }
    #[cfg(windows)]
    {
        if let Some(refusal) =
            bravebot_sandbox::windows::refusal_for_program(&command.get_program().to_string_lossy())
        {
            return Err(ExecError::NotConfined {
                program: step.program.clone(),
                detail: refusal.to_string(),
            });
        }
        let prepared = confinement.prepared(&command, step, directory)?;
        Ok(Launch::Container(Box::new(Container {
            step: step.program.clone(),
            prepared,
        })))
    }
}

#[cfg(windows)]
impl Container {
    /// Start the step on the handles given, or say why it did not start.
    ///
    /// A process that could not be created at all is told apart from one the platform would not
    /// confine, as a spawn failure is on the other platforms.
    pub(crate) fn start(
        self,
        attached: bravebot_sandbox::Attached,
    ) -> Result<Contained, ExecError> {
        use bravebot_sandbox::{Environment, SandboxError};

        let Prepared {
            sandbox,
            program,
            args,
            policy,
            variables,
        } = self.prepared;
        let program_name = self.step;
        match sandbox.spawn_attached(
            &program,
            &args,
            &policy,
            attached,
            Environment::Only(variables),
        ) {
            Ok(child) => Ok(Contained {
                child,
                _sandbox: sandbox,
            }),
            Err(SandboxError::SpawnFailed(error)) => Err(ExecError::NotStarted {
                program: program_name,
                detail: error.to_string(),
            }),
            Err(other) => Err(ExecError::NotConfined {
                program: program_name,
                detail: other.to_string(),
            }),
        }
    }
}

/// What the planner is told about the programs `run` starts, for a turn that asks for them to be
/// confined, or `None` for one that does not.
///
/// A turn that does not confine says nothing, so a planner is never told of a boundary its
/// programs do not have. Where the platform has no base to confine on, it is told the opposite.
///
/// The `off` mode says nothing either, for the same reason: the programs have no boundary to state.
pub fn stated_to_the_planner(confine_runs: bool, mode: SandboxMode) -> Option<String> {
    stated(
        confine_runs,
        Prelude::current(),
        bravebot_config::run_network(),
        mode,
    )
}

fn stated(
    confine_runs: bool,
    prelude: Option<Prelude>,
    network: Network,
    mode: SandboxMode,
) -> Option<String> {
    if !confine_runs || mode == SandboxMode::Off {
        return None;
    }
    let listed = mode == SandboxMode::Strict;
    let mut said = String::from(match prelude {
        Some(Prelude::Windows) => {
            "Programs this tool starts are confined. Each may reach only the directories the \
             session was opened on, the scratch directory and the temporary directory, all read \
             and written, the system and program directories and git's configuration files, \
             read, the caches of the toolchain it belongs to, and the credential scope its \
             command names. A path outside those is refused by the operating system as \
             `Operation not permitted` or `Permission denied`, so a program that reports either \
             for such a path was stopped by the sandbox and not by a fault in the machine. Only \
             the person widens it (`/add-dir`, `--add-dir`); a command cannot ask for more."
        }
        Some(Prelude::Linux | Prelude::MacOs) if listed => {
            "Programs this tool starts are confined. Each may reach only the directories the \
             session was opened on, the scratch directory and the temporary directory, all read \
             and written, the system and program directories and git's configuration files, \
             read, the caches of the toolchain it belongs to, and the credential scope its \
             command names. A path outside those is refused by the operating system as \
             `Operation not permitted` or `Permission denied`, so a program that reports either \
             for such a path was stopped by the sandbox and not by a fault in the machine. Only \
             the person widens it (`/add-dir`, `--add-dir`); a command cannot ask for more."
        }
        Some(Prelude::Linux | Prelude::MacOs) => {
            "Programs this tool starts are confined. Each may read this machine except the places \
             that hold a credential: ssh private keys, cloud and container logins, keychains, \
             browser profiles and password stores. It may write only the directories the session \
             was opened on, the scratch directory, the temporary directory and the toolchain \
             caches, and reads a credential directory only where the command's scope names it. \
             A path outside those is refused by the operating system as `Operation not \
             permitted` or `Permission denied`, so a program that reports either for such a path \
             was stopped by the sandbox and not by a fault in the machine. Only the person \
             widens it (`/add-dir`, `--add-dir`); a command cannot ask for more."
        }
        None => {
            "Programs this tool starts are not confined on this platform: they run with the \
             access of the person's own account."
        }
    });
    if prelude.is_some() && network.is_closed() {
        said.push_str(
            " The network is closed: a program has no network access unless it is a package \
             manager's fetch, `git` or `gh` with a remote operation, `curl`, `ssh`, or a \
             command that names a remote credential scope. A connection refused or a host that \
             does not resolve for any other program was stopped by the sandbox.",
        );
    }
    Some(said)
}

/// Why a backend cannot apply `policy`, where the policy withholds the network and the backend does
/// not enforce that, or `None`.
///
/// Refused here with the setting named rather than left to the backend's own words: a person who
/// closed the network and is told only that confinement failed does not know which setting asked
/// for what the platform cannot do, and a backend that applied the rest and left the network open
/// would be the silent fall back to `open` the setting exists to forbid.
fn cannot_close_the_network(
    policy: &SandboxPolicy,
    capabilities: &bravebot_sandbox::policy::Capabilities,
) -> Option<String> {
    (!policy.allow_network && !capabilities.network_denial_enforced).then(|| {
        "the network is closed for this session (run.network) and this platform cannot deny it \
         to a program that does not need it"
            .to_string()
    })
}

/// What a step is started with, once its policy is decided.
struct Prepared {
    sandbox: Box<dyn bravebot_sandbox::Sandbox>,
    program: String,
    args: Vec<String>,
    policy: SandboxPolicy,
    variables: Variables,
}

/// Whether variable names are compared without regard to case, as Windows does.
const FOLD_CASE: bool = cfg!(windows);

type Pair = (std::ffi::OsString, std::ffi::OsString);

/// The variables `command` will start with: this process's, less the names removed and with the
/// ones set.
fn effective_environment(command: &Command) -> Vec<Pair> {
    overlay(std::env::vars_os().collect(), command.get_envs(), FOLD_CASE)
}

/// `held` less every name `changes` removes or sets, plus the values it sets.
fn overlay<'a>(
    mut held: Vec<Pair>,
    changes: impl Iterator<Item = (&'a std::ffi::OsStr, Option<&'a std::ffi::OsStr>)>,
    fold_case: bool,
) -> Vec<Pair> {
    for (name, value) in changes {
        held.retain(|(existing, _)| !same_variable(existing, name, fold_case));
        if let Some(value) = value {
            held.push((name.to_os_string(), value.to_os_string()));
        }
    }
    held
}

fn same_variable(left: &std::ffi::OsStr, right: &std::ffi::OsStr, fold_case: bool) -> bool {
    if fold_case {
        left.eq_ignore_ascii_case(right)
    } else {
        left == right
    }
}

/// Where `environment` moves what the scope of the step that resolved to `resolved` reads.
fn reaches(
    resolved: &Path,
    scope: Scope,
    home: &Path,
    environment: &[(String, String)],
) -> Vec<Reach> {
    let program = resolved
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    environment_reach(scope, program, home, environment)
}

/// This process's variables, those whose name and value are text.
fn process_environment() -> Vec<(String, String)> {
    std::env::vars().collect()
}

fn variable(environment: &[(String, String)], name: &str) -> Option<String> {
    lookup(environment, name, FOLD_CASE)
}

fn lookup(environment: &[(String, String)], name: &str, fold_case: bool) -> Option<String> {
    environment
        .iter()
        .find(|(held, _)| same_variable(held.as_ref(), name.as_ref(), fold_case))
        .map(|(_, value)| value.clone())
        .filter(|value| !value.is_empty())
}

fn canonical(path: &Path) -> PathBuf {
    without_verbatim_prefix(path.canonicalize().unwrap_or_else(|_| path.to_path_buf()))
}

/// The longest path, in characters, the ordinary spelling of a Windows path can name.
const MAX_PATH: usize = 260;

/// `path` without the `\\?\` that Windows puts in front of a resolved drive path.
///
/// The prefix means "do not interpret this", and the call that writes a grant onto a path is
/// documented for the ordinary spelling. Left on, a row would also differ in text from the same
/// directory named by the person or found on `PATH`. A network path keeps its prefix, since
/// without it the path names something else, and so does a path too long for the ordinary spelling,
/// which only the prefixed one can name.
fn without_verbatim_prefix(path: PathBuf) -> PathBuf {
    let stripped = path
        .to_string_lossy()
        .strip_prefix(r"\\?\")
        .filter(|rest| {
            let bytes = rest.as_bytes();
            bytes.len() >= 3
                && rest.len() < MAX_PATH
                && bytes[0].is_ascii_alphabetic()
                && bytes[1] == b':'
                && bytes[2] == b'\\'
        })
        .map(PathBuf::from);
    stripped.unwrap_or(path)
}

/// The system temporary directory with its links followed, which is how a backend matches it.
fn temporary_directory() -> PathBuf {
    // Nothing is created here: the path becomes the base's temporary row.
    // nosemgrep: rust.lang.security.temp-dir.temp-dir
    std::env::temp_dir()
}

/// The directories a program and the tools it starts are read from.
///
/// The directories `searched` names and the one `program` resolved into, each with its links
/// followed, since a runner is a script whose interpreter is found through `PATH` and whose code
/// sits beside where it was installed. A `bin` directory brings its parent, where an installation
/// keeps what its programs load. None of these reaches the home directory: a `PATH` naming it, or a
/// directory above it, is left out, and so is a parent inside it, since `~/.cargo` holds a registry
/// token beside `~/.cargo/bin`. The program's own `bin` directory is the exception where it sits
/// deeper in the home than that, as `nvm` installs one, since that parent is the installation the
/// program came from.
pub(crate) fn program_reads(
    program: &Path,
    searched: &[PathBuf],
    home: Option<&Path>,
) -> Vec<PathBuf> {
    let home = home.map(canonical);
    let outside_home = |path: &Path| home.as_ref().is_none_or(|home| !home.starts_with(path));
    let beside_home = |path: &Path| {
        path.parent().is_some()
            && outside_home(path)
            && home.as_ref().is_none_or(|home| !path.starts_with(home))
    };
    let below_the_top_of_home = |path: &Path| {
        home.as_ref().is_some_and(|home| {
            path.strip_prefix(home)
                .is_ok_and(|below| below.components().count() > 1)
        })
    };
    let program = canonical(program);
    let mut reads = Vec::new();
    if let Some(installation) = program
        .parent()
        .filter(|directory| directory.file_name() == Some(OsStr::new("bin")))
        .and_then(Path::parent)
        .filter(|installation| below_the_top_of_home(installation))
    {
        reads.push(installation.to_path_buf());
    }
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
            reads.push(parent.to_path_buf());
        }
        reads.push(directory);
    }
    reads
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_sandbox::policy::PathKind;

    const HOME: &str = "/home/person";

    fn step(resolved: &str, args: &[&str]) -> Step {
        Step {
            program: resolved.rsplit('/').next().unwrap_or(resolved).to_string(),
            resolved: PathBuf::from(resolved),
            started_as: PathBuf::from(resolved),
            args: args.iter().map(|argument| argument.to_string()).collect(),
            environment: Vec::new(),
            routes: Vec::new(),
        }
    }

    /// A confinement on a platform that lists what a step reaches, where a toolchain brings a list
    /// and the base reads nothing of the home.
    fn confinement(roots: &[&str]) -> Confinement {
        Confinement::new(
            Prelude::Windows,
            PathBuf::from("/tmp"),
            Some(Path::new(HOME)),
            roots.iter().map(PathBuf::from).collect(),
            Some(Path::new("/var/scratch")),
        )
    }

    /// A confinement on a platform that reads the machine except the credential locations.
    fn reading_confinement(prelude: Prelude, roots: &[&str]) -> Confinement {
        Confinement::new(
            prelude,
            PathBuf::from("/tmp"),
            Some(Path::new(HOME)),
            roots.iter().map(PathBuf::from).collect(),
            Some(Path::new("/var/scratch")),
        )
    }

    fn refuses(policy: &SandboxPolicy, path: &str) -> bool {
        policy
            .unreadable
            .iter()
            .any(|row| Path::new(path).starts_with(row))
            && !policy
                .readable
                .iter()
                .any(|row| Path::new(path).starts_with(row) && row != Path::new("/"))
    }

    fn reads(policy: &SandboxPolicy, path: &str) -> bool {
        policy.readable.iter().any(|row| row == Path::new(path))
    }

    fn writes(policy: &SandboxPolicy, path: &str) -> bool {
        policy
            .writable
            .iter()
            .any(|row| row.path == Path::new(path))
    }

    /// The directories a session was opened on are the only places of the person's own a program
    /// reads and writes, and a program started in one may run `git` there.
    #[test]
    fn the_session_directories_are_read_and_written_and_nothing_else_of_the_persons() {
        let policy = confinement(&["/work/project", "/work/added"]).policy(
            &step("/bin/ls", &[]),
            Path::new("/work/project"),
            &[],
        );

        for root in ["/work/project", "/work/added", "/var/scratch"] {
            assert!(reads(&policy, root) && writes(&policy, root), "{root}");
        }
        assert!(!reads(&policy, "/work") && !writes(&policy, "/work"));
        assert!(policy.git_directories_writable);
        assert_eq!(
            policy.starting_in.as_deref(),
            Some(Path::new("/work/project"))
        );
    }

    /// No step that names no credential reaches any file in the account's home, whatever program
    /// it is: the base is the same for every plan.
    #[test]
    fn a_step_whose_plan_names_no_credential_reaches_nothing_in_the_home() {
        let confined = confinement(&["/work/project"]);
        for (program, args) in [
            ("/bin/cat", vec!["notes.txt"]),
            ("/usr/bin/git", vec!["status"]),
            ("/usr/bin/git", vec!["commit", "-m", "x"]),
            ("/usr/bin/make", vec!["check"]),
        ] {
            let policy = confined.policy(&step(program, &args), Path::new("/work/project"), &[]);
            let gitconfig = Path::new(HOME).join(".gitconfig");
            let reached: Vec<_> = policy
                .readable
                .iter()
                .filter(|row| Path::new(HOME).starts_with(row) || row.starts_with(HOME))
                .filter(|row| {
                    **row != gitconfig && **row != Path::new(HOME).join(".config/git/config")
                })
                .collect();
            assert!(
                reached.is_empty(),
                "`{program} {args:?}` reached {reached:?}"
            );
        }
    }

    /// What the prompt is told a stage carries is what the profile grants it: the stage the
    /// profile gives a registry and a remote is the stage described as bringing them, and a stage
    /// that brings neither is absent. A description built from a second reading of the step could
    /// disagree with the profile in either direction.
    #[test]
    fn what_a_prompt_says_a_stage_carries_is_what_its_profile_grants() {
        let confined = confinement(&["/work/project", "/work/added"]);
        let cargo = step("/usr/bin/cargo", &["build"]);
        let push = step("/usr/bin/git", &["push"]);
        let status = step("/usr/bin/git", &["status"]);
        let make = step("/usr/bin/make", &["check"]);

        let described = confined.describe(&[&cargo, &push, &status, &make]);

        assert_eq!(
            described.directories,
            [
                PathBuf::from("/work/project"),
                PathBuf::from("/work/added"),
                PathBuf::from("/var/scratch")
            ]
        );
        let carried: Vec<_> = described
            .carried
            .iter()
            .map(|stage| (stage.program.as_str(), stage.toolchain, stage.scope))
            .collect();
        assert_eq!(
            carried,
            [
                ("cargo", Some(Toolchain::Cargo), None),
                ("git", None, Some(Scope::Remote)),
            ]
        );
        let registry = format!("{HOME}/.cargo/registry");
        let policy = |step: &Step| confined.policy(step, Path::new("/work/project"), &[]);
        assert!(writes(&policy(&cargo), &registry));
        assert!(!writes(&policy(&make), &registry));
        assert!(reads(&policy(&push), &format!("{HOME}/.config/gh")));
        assert!(!reads(&policy(&status), &format!("{HOME}/.config/gh")));
    }

    /// A script that starts `gh`, `git` or `cargo` is no program the plan resolved to a toolchain,
    /// so the stage reads the machine and holds back only the credential locations: its reads are
    /// the root and the lifts, and its refusals are the table.
    #[test]
    fn a_stage_reads_the_machine_and_is_refused_the_credential_locations() {
        for prelude in [Prelude::Linux, Prelude::MacOs] {
            let policy = reading_confinement(prelude, &["/work/project"]).policy(
                &step("/bin/sh", &["-c", "gh pr list"]),
                Path::new("/work/project"),
                &[],
            );

            assert!(reads(&policy, "/"), "{prelude:?}");
            for held_back in [
                format!("{HOME}/.ssh/id_ed25519"),
                format!("{HOME}/.aws/credentials"),
                format!("{HOME}/.kube/config"),
                format!("{HOME}/.docker/config.json"),
                format!("{HOME}/.azure/accessTokens.json"),
                format!("{HOME}/.config/gcloud/credentials.db"),
                format!("{HOME}/.gnupg/private-keys-v1.d/key"),
            ] {
                assert!(refuses(&policy, &held_back), "{prelude:?} {held_back}");
            }
            for read in [
                format!("{HOME}/.ssh/config"),
                format!("{HOME}/.ssh/known_hosts"),
                format!("{HOME}/.ssh/id_ed25519.pub"),
                format!("{HOME}/.config/gh/hosts.yml"),
                format!("{HOME}/.gitconfig"),
                format!("{HOME}/.npmrc"),
            ] {
                assert!(!refuses(&policy, &read), "{prelude:?} {read}");
            }
        }
    }

    /// SANDBOX-22: `strict` is the profile a session with no home gets, on a platform that has one:
    /// the base, the toolchain lists the program brings, and the scope its argv names. A script
    /// that starts `gh` is no longer given the machine to read, so its root row and the credential
    /// table are not there. The regression it rejects is a mode that changes the sentence the
    /// planner is told and not the profile.
    #[test]
    fn a_strict_stage_does_not_read_the_machine() {
        for prelude in [Prelude::Linux, Prelude::MacOs] {
            let script = step("/bin/sh", &["-c", "gh pr list"]);
            let standard = reading_confinement(prelude, &["/work/project"]).policy(
                &script,
                Path::new("/work/project"),
                &[],
            );
            let strict = reading_confinement(prelude, &["/work/project"])
                .with_mode(SandboxMode::Strict)
                .policy(&script, Path::new("/work/project"), &[]);

            assert!(
                reads(&standard, "/"),
                "{prelude:?}: the control reads nothing"
            );
            assert!(
                !reads(&strict, "/"),
                "{prelude:?}: strict reads the machine"
            );
            assert!(
                strict
                    .readable
                    .iter()
                    .any(|row| row == Path::new("/work/project")),
                "{prelude:?}: strict does not read the session's own directory"
            );
        }
    }

    /// Writes stay with the session: the directories it was opened on, the scratch directory, the
    /// temporary directory and the null device, and the caches. Nothing of the person's home
    /// outside those is written, whatever program the stage runs.
    #[test]
    fn a_stage_writes_only_the_session_the_temporary_directory_and_the_caches() {
        for prelude in [Prelude::Linux, Prelude::MacOs] {
            let confined = reading_confinement(prelude, &["/work/project", "/work/added"]);
            let policy = confined.policy(&step("/usr/bin/make", &[]), Path::new("/work"), &[]);

            let mut written: Vec<_> = policy
                .writable
                .iter()
                .map(|row| row.path.to_string_lossy().into_owned())
                .collect();
            written.sort();
            let caches: Vec<_> = written
                .iter()
                .filter(|path| path.starts_with(HOME))
                .collect();
            for session in ["/work/project", "/work/added", "/var/scratch", "/tmp"] {
                assert!(written.iter().any(|path| path == session), "{session}");
            }
            for path in &caches {
                assert!(
                    [
                        ".cargo/registry",
                        ".cargo/git",
                        ".cargo/.package-cache",
                        ".npm/_cacache",
                        "pip",
                        "go-build",
                        "go/pkg/mod",
                        "go/pkg/sumdb",
                        ".m2/repository",
                        ".gradle/caches",
                        ".gradle/wrapper",
                        ".gradle/native",
                    ]
                    .iter()
                    .any(|cache| path.ends_with(cache)),
                    "{prelude:?} writes {path}"
                );
            }
            assert!(
                written.iter().all(|path| !path.ends_with(".cargo")
                    && !path.ends_with(".npm")
                    && *path != HOME),
                "{written:?}"
            );
        }
    }

    /// A tool's own scope lifts its own directory and no other, and a stage that names none keeps
    /// all three refused. The prompt, the profile line and the policy are one table.
    #[test]
    fn the_prompt_the_line_and_the_policy_agree_on_which_credential_a_stage_lifts() {
        for prelude in [Prelude::Linux, Prelude::MacOs] {
            let confined = reading_confinement(prelude, &["/work/project"]);
            for (program, args, lifted) in [
                ("/usr/bin/aws", vec!["s3", "ls"], Some((".aws", "aws"))),
                (
                    "/usr/bin/kubectl",
                    vec!["get", "pods"],
                    Some((".kube", "kubernetes")),
                ),
                ("/usr/bin/docker", vec!["ps"], Some((".docker", "docker"))),
                ("/usr/bin/make", vec!["check"], None),
                ("/usr/bin/git", vec!["status"], None),
            ] {
                let step = step(program, &args);
                let policy = confined.policy(&step, Path::new("/work"), &[]);
                let line = confined.profile(&[&step]);
                let described = confined.describe(&[&step]);

                for directory in [".aws", ".kube", ".docker"] {
                    let path = format!("{HOME}/{directory}/credentials");
                    let expected = lifted.is_some_and(|(own, _)| own == directory);
                    assert_eq!(
                        !refuses(&policy, &path),
                        expected,
                        "{prelude:?} {program} {directory}"
                    );
                }
                match lifted {
                    Some((_, scope)) => {
                        assert!(line.contains(&format!("scopes: {scope})")), "{line}");
                        assert_eq!(described.carried.len(), 1);
                    }
                    None => {
                        assert!(line.contains("scopes: none)"), "{line}");
                        assert!(described.carried.is_empty());
                    }
                }
                assert!(described.reads_the_machine);
                assert!(
                    described
                        .carried
                        .iter()
                        .all(|stage| stage.toolchain.is_none())
                );
            }
        }
    }

    /// The platform that lists is described as it always was, and the others are not: the heading
    /// and the planner's sentence differ, so neither platform is told the other's boundary.
    #[test]
    fn the_description_follows_the_platform_it_describes() {
        let machine = reading_confinement(Prelude::Linux, &["/work/project"]);
        let listed = confinement(&["/work/project"]);

        assert!(machine.describe(&[]).reads_the_machine);
        assert!(!listed.describe(&[]).reads_the_machine);
        assert_ne!(
            stated(
                true,
                Some(Prelude::Linux),
                Network::Open,
                SandboxMode::Standard
            ),
            stated(
                true,
                Some(Prelude::Windows),
                Network::Open,
                SandboxMode::Standard
            )
        );
        assert_eq!(
            stated(
                true,
                Some(Prelude::Linux),
                Network::Open,
                SandboxMode::Standard
            ),
            stated(
                true,
                Some(Prelude::MacOs),
                Network::Open,
                SandboxMode::Standard
            )
        );
        assert!(
            stated(
                true,
                Some(Prelude::Linux),
                Network::Open,
                SandboxMode::Standard
            )
            .is_some_and(|said| said.contains("except the places that hold a credential"))
        );
    }

    fn listed(deny_read: &[&str], allow_write: &[&str], deny_write: &[&str]) -> Lists {
        let entries = |paths: &[&str]| {
            paths
                .iter()
                .map(|path| bravebot_sandbox::rules::Entry {
                    path: (*path).to_string(),
                    by: None,
                    pinned: false,
                })
                .collect()
        };
        Lists {
            deny_read: entries(deny_read),
            allow_write: entries(allow_write),
            deny_write: entries(deny_write),
            ..Lists::default()
        }
    }

    /// The regression it rejects: a person's refusal that the rows a stage brings for itself lift.
    /// A push carries the remote scope, whose read of `~/.ssh/known_hosts` is a lift of the table's
    /// refusal of `~/.ssh`; the person refused that file by name, and the stage must not read it.
    /// The same stage without the list is the control that the scope does lift it.
    #[test]
    fn a_refusal_of_the_persons_is_not_lifted_by_the_scope_a_stage_carries() {
        // A home that exists with its links followed, as a real one is: the list resolves a path
        // through the links of its deepest part on disk, and `/home` is a link on macOS.
        let home = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch/confine-unit-refusal-home");
        std::fs::create_dir_all(&home).expect("home directory");
        let home = home.canonicalize().expect("canonical home");
        let known_hosts = format!("{}/.ssh/known_hosts", home.display());
        let push = step("/usr/bin/git", &["push"]);
        let plain = Confinement::new(
            Prelude::Linux,
            PathBuf::from("/tmp"),
            Some(&home),
            vec![PathBuf::from("/work/project")],
            Some(Path::new("/var/scratch")),
        );
        let held = plain
            .clone()
            .with_filesystem(&listed(&[&known_hosts], &[], &[]));

        let without = plain.policy(&push, Path::new("/work/project"), &[]);
        let with = held.policy(&push, Path::new("/work/project"), &[]);

        assert!(reads(&without, &known_hosts), "the scope lifts it unlisted");
        assert!(!reads(&with, &known_hosts), "a scope lifted the refusal");
        assert!(with.unreadable.contains(&PathBuf::from(&known_hosts)));
    }

    /// Every stage of a line is built by the one function, so a stage of a pipeline and a stage of a
    /// line left running hold the lists as the first does; a stage that carries a toolchain or a
    /// scope holds them as well.
    #[test]
    fn every_kind_of_stage_holds_the_lists() {
        let held = reading_confinement(Prelude::Linux, &["/work/project"]).with_filesystem(
            &listed(&["/work/project/secret"], &[], &["/work/project/.env"]),
        );
        for step in [
            step("/bin/cat", &["file"]),
            step("/usr/bin/git", &["push"]),
            step("/usr/bin/cargo", &["build"]),
        ] {
            let policy = held.policy(&step, Path::new("/work/project"), &[]);
            assert!(
                policy
                    .unreadable
                    .contains(&PathBuf::from("/work/project/secret")),
                "{}",
                step.program
            );
            assert!(
                policy
                    .unwritable
                    .contains(&PathBuf::from("/work/project/.env")),
                "{}",
                step.program
            );
        }
    }

    /// The prompt's description and the failure sentence say how many entries are in force and never
    /// which paths, and say nothing where there are none.
    #[test]
    fn the_counts_reach_the_description_and_the_profile_and_no_path_does() {
        let plain = reading_confinement(Prelude::Linux, &["/work/project"]);
        let held = plain.clone().with_filesystem(&listed(
            &["/work/project/secret-path", "/work/project/other"],
            &["/work/extra"],
            &["/work/project/.env"],
        ));
        let cat = step("/bin/cat", &["file"]);

        let described = held.describe(&[&cat]).filesystem;
        let said = held.profile(&[&cat]);

        assert_eq!(
            (
                described.deny_read,
                described.allow_write,
                described.deny_write
            ),
            (2, 1, 1)
        );
        assert!(
            said.contains("2 denyRead") && said.contains("1 allowWrite"),
            "{said}"
        );
        assert!(
            !said.contains("secret-path") && !said.contains(".env"),
            "{said}"
        );
        assert!(plain.describe(&[&cat]).filesystem.is_empty());
        assert!(!plain.profile(&[&cat]).contains("denyRead"));
    }

    /// A session that names no home directory cannot read `~` as a path, so a refusal spelled with
    /// it is not applied and the stage is not started, rather than started without the refusal.
    #[test]
    fn a_refusal_spelled_with_a_home_the_session_lacks_is_unapplied() {
        let held = Confinement::new(
            Prelude::Linux,
            PathBuf::from("/tmp"),
            None,
            vec![PathBuf::from("/work/project")],
            None,
        )
        .with_filesystem(&listed(&["~/notes"], &[], &[]));

        assert!(held.filesystem().unapplied_denial().is_some());
    }

    /// A session that names no home directory grants no row under it, so it describes none.
    #[test]
    fn a_session_with_no_home_describes_no_toolchain_and_no_scope() {
        let confined = Confinement::new(
            Prelude::Linux,
            PathBuf::from("/tmp"),
            None,
            vec![PathBuf::from("/work/project")],
            None,
        );

        let described = confined.describe(&[
            &step("/usr/bin/cargo", &["build"]),
            &step("/usr/bin/git", &["push"]),
        ]);

        assert!(described.carried.is_empty());
        assert_eq!(described.directories, [PathBuf::from("/work/project")]);
    }

    /// The regression it rejects: a session with no home directory granted a read of the machine
    /// with no credential row to subtract, which is every credential on it read by absolute path.
    #[test]
    fn a_session_with_no_home_is_not_granted_the_machine() {
        for prelude in [Prelude::Linux, Prelude::MacOs] {
            let confined = Confinement::new(
                prelude,
                PathBuf::from("/tmp"),
                None,
                vec![PathBuf::from("/work/project")],
                None,
            );

            let policy =
                confined.policy(&step("/usr/bin/make", &["check"]), Path::new("/work"), &[]);

            assert!(
                !policy.readable.iter().any(|row| row == Path::new("/")),
                "{prelude:?} read the machine with no home: {:?}",
                policy.readable
            );
            assert!(!confined.describe(&[]).reads_the_machine);
        }
    }

    /// The list a toolchain brings follows the binary the step resolved to, so a `cargo` stage
    /// reaches the registry and a `make` stage beside it does not.
    #[test]
    fn a_toolchains_cache_is_granted_to_its_own_binary_only() {
        let confined = confinement(&["/work/project"]);
        let registry = format!("{HOME}/.cargo/registry");

        let cargo = confined.policy(&step("/usr/bin/cargo", &["build"]), Path::new("/work"), &[]);
        let make = confined.policy(&step("/usr/bin/make", &["build"]), Path::new("/work"), &[]);

        assert!(writes(&cargo, &registry));
        assert!(!writes(&make, &registry));
    }

    /// A scope follows the operation the argv names: a push reaches the remote scope and a status
    /// in the same repository does not, and a key is in neither.
    #[test]
    fn a_push_reaches_the_remote_scope_and_a_status_does_not() {
        let confined = confinement(&["/work/project"]);
        let known_hosts = format!("{HOME}/.ssh/known_hosts");

        let push = confined.policy(&step("/usr/bin/git", &["push"]), Path::new("/work"), &[]);
        let status = confined.policy(&step("/usr/bin/git", &["status"]), Path::new("/work"), &[]);

        assert!(reads(&push, &known_hosts));
        assert!(
            push.writable
                .iter()
                .any(|row| row.path == Path::new(&known_hosts) && row.kind == PathKind::File)
        );
        assert!(!reads(&status, &known_hosts));
        for policy in [&push, &status] {
            assert!(!reads(policy, &format!("{HOME}/.ssh")));
            assert!(!reads(policy, &format!("{HOME}/.ssh/id_ed25519")));
        }
    }

    /// The agent socket is written only by a step that carries the remote scope, and is the
    /// process's own value for it.
    #[test]
    fn the_agent_socket_goes_to_a_remote_step_and_to_no_other() {
        let confined = confinement(&["/work/project"]);
        let environment = vec![("SSH_AUTH_SOCK".to_string(), "/run/agent.sock".to_string())];

        let push = confined.policy(
            &step("/usr/bin/git", &["push"]),
            Path::new("/work"),
            &environment,
        );
        let make = confined.policy(
            &step("/usr/bin/make", &[]),
            Path::new("/work"),
            &environment,
        );
        let bare = confined.policy(&step("/usr/bin/git", &["push"]), Path::new("/work"), &[]);

        assert!(writes(&push, "/run/agent.sock"));
        assert!(!writes(&make, "/run/agent.sock"));
        assert!(!writes(&bare, "/run/agent.sock"));

        // Closing the network leaves the socket where it was: the rule is about files, and the
        // stage that reaches a remote is the one that keeps both.
        let closed = confined.with_network(Network::Closed);
        let push = closed.policy(
            &step("/usr/bin/git", &["push"]),
            Path::new("/work"),
            &environment,
        );
        let make = closed.policy(
            &step("/usr/bin/make", &[]),
            Path::new("/work"),
            &environment,
        );
        assert!(writes(&push, "/run/agent.sock") && push.allow_network);
        assert!(!writes(&make, "/run/agent.sock") && !make.allow_network);
    }

    /// A step with an assignment in front of it carries no scope, so the socket does not follow it
    /// either.
    #[test]
    fn an_assignment_in_front_of_a_push_removes_its_scope() {
        let confined = confinement(&["/work/project"]);
        let mut assigned = step("/usr/bin/git", &["push"]);
        assigned.environment = vec![("GIT_SSH_COMMAND".to_string(), "ssh".to_string())];
        let environment = vec![("SSH_AUTH_SOCK".to_string(), "/run/agent.sock".to_string())];

        let policy = confined.policy(&assigned, Path::new("/work"), &environment);

        assert!(!reads(&policy, &format!("{HOME}/.ssh/known_hosts")));
        assert!(!writes(&policy, "/run/agent.sock"));
    }

    fn remembered(binary: &str, operation: Option<&str>, reached: Reached, write: bool) -> Grant {
        Grant {
            binary: PathBuf::from(binary),
            operation: operation.map(str::to_string),
            reached,
            write,
            allowed: "2026-10-07".to_string(),
            lifetime: crate::reach::Lifetime::Always,
        }
    }

    fn a_scope(word: &str) -> Reached {
        Reached::Scope(Scope::named(word).expect("a scope"))
    }

    /// A scope a person remembered for `make` is a row of `make`'s stage, and of no other program's.
    /// The regressions it rejects: a grant that attaches to every stage of the plan, and one that
    /// is ignored because the program is not one the scope table names.
    #[test]
    fn a_remembered_scope_reaches_the_command_it_was_made_for_and_no_other() {
        let known_hosts = format!("{HOME}/.ssh/known_hosts");
        let confined = confinement(&["/work/project"]).with_grants(vec![remembered(
            "/usr/bin/make",
            None,
            a_scope("remote"),
            false,
        )]);

        let make = confined.policy(&step("/usr/bin/make", &[]), Path::new("/work"), &[]);
        let ls = confined.policy(&step("/bin/ls", &[]), Path::new("/work"), &[]);
        let other_make = confined.policy(&step("/opt/make", &[]), Path::new("/work"), &[]);
        let target = confined.policy(&step("/usr/bin/make", &["check"]), Path::new("/work"), &[]);

        assert!(reads(&make, &known_hosts));
        assert!(!reads(&make, &format!("{HOME}/.ssh/id_ed25519")));
        for policy in [&ls, &other_make, &target] {
            assert!(!reads(policy, &known_hosts));
        }
    }

    /// A remembered scope is a credential scope for the closed network too, and a remembered
    /// directory is not. The regressions it rejects: a remote credential lent to a stage with no
    /// way to use it, and a directory read earning the network.
    #[test]
    fn a_remembered_scope_keeps_a_closed_network_and_a_remembered_directory_does_not() {
        let named = crate::testutil::scratch_dir("confine-remembered-network");
        let _ = std::fs::remove_dir_all(&named);
        std::fs::create_dir_all(&named).expect("directory");
        let named = std::fs::canonicalize(named).expect("canonical");
        let closed = confinement(&["/work/project"])
            .with_network(Network::Closed)
            .with_grants(vec![
                remembered("/usr/bin/make", None, a_scope("remote"), false),
                remembered("/bin/cat", None, Reached::Directory(named), false),
            ]);
        let mut assigned = step("/usr/bin/make", &[]);
        assigned.environment = vec![("A".to_string(), "b".to_string())];

        assert!(closed.egress(&step("/usr/bin/make", &[])));
        assert!(!closed.egress(&step("/bin/cat", &[])));
        assert!(!closed.egress(&step("/bin/ls", &[])));
        assert!(!closed.egress(&assigned));
        assert_eq!(
            closed.network_for_the_trail(&[&step("/usr/bin/make", &[])]),
            Some(
                "the network was closed for this run except for stage 1 (a credential scope)"
                    .to_string()
            )
        );
    }

    /// A directory is read, and written only where the grant says. The regression it rejects:
    /// every remembered directory written.
    #[test]
    fn a_remembered_directory_is_read_and_written_only_where_the_grant_says() {
        let named = crate::testutil::scratch_dir("confine-remembered-directory");
        let _ = std::fs::remove_dir_all(&named);
        std::fs::create_dir_all(&named).expect("directory");
        let named = std::fs::canonicalize(named).expect("canonical");
        let path = named.to_str().expect("utf-8");
        let confined = confinement(&["/work/project"]).with_grants(vec![
            remembered(
                "/usr/bin/make",
                None,
                Reached::Directory(named.clone()),
                false,
            ),
            remembered(
                "/usr/bin/cargo",
                None,
                Reached::Directory(named.clone()),
                true,
            ),
        ]);

        let make = confined.policy(&step("/usr/bin/make", &[]), Path::new("/work"), &[]);
        let cargo = confined.policy(&step("/usr/bin/cargo", &[]), Path::new("/work"), &[]);
        let ls = confined.policy(&step("/bin/ls", &[]), Path::new("/work"), &[]);

        assert!(reads(&make, path) && !writes(&make, path));
        assert!(reads(&cargo, path) && writes(&cargo, path));
        assert!(!reads(&ls, path) && !writes(&ls, path));

        let (make_step, cargo_step, ls_step) = (
            step("/usr/bin/make", &[]),
            step("/usr/bin/cargo", &[]),
            step("/bin/ls", &[]),
        );
        let said = confined.describe(&[&make_step]).sentences();
        // cargo's own toolchain sentence comes first; the remembered one is the last.
        let wrote = confined.describe(&[&cargo_step]).sentences();
        assert_eq!(said.len(), 1, "{said:?}");
        let (read, write) = (&said[0], wrote.last().expect("a sentence"));
        for sentence in [read, write] {
            assert!(sentence.contains(path), "{sentence}");
            assert!(sentence.contains("2026-10-07"), "{sentence}");
        }
        assert!(read.contains("make also reads "), "{said:?}");
        assert!(!read.contains("writes"), "{said:?}");
        assert!(write.contains("cargo also reads and writes "), "{wrote:?}");
        assert!(confined.describe(&[&ls_step]).sentences().is_empty());
    }

    /// A step with an assignment, a session with no home and a directory that has since become a
    /// link to `~/.ssh` each get nothing from a grant. The regressions they reject: a grant
    /// outliving the assignment's removal of scopes, rows judged against no home, and a directory
    /// checked when it was allowed and never again.
    #[test]
    fn a_remembered_reach_is_withheld_where_the_step_or_the_machine_has_changed() {
        let known_hosts = format!("{HOME}/.ssh/known_hosts");
        let grants = vec![remembered("/usr/bin/make", None, a_scope("remote"), false)];
        let mut assigned = step("/usr/bin/make", &[]);
        assigned.environment = vec![("GIT_SSH_COMMAND".to_string(), "ssh".to_string())];

        let with_assignment = confinement(&["/work/project"])
            .with_grants(grants.clone())
            .policy(&assigned, Path::new("/work"), &[]);
        let homeless = Confinement::new(
            Prelude::Windows,
            PathBuf::from("/tmp"),
            None,
            vec![PathBuf::from("/work/project")],
            Some(Path::new("/var/scratch")),
        )
        .with_grants(grants);
        let make = step("/usr/bin/make", &[]);
        let described = homeless.describe(&[&make]).sentences();
        let profile = homeless.profile(&[&make]);
        let homeless = homeless.policy(&make, Path::new("/work"), &[]);
        assert!(described.is_empty(), "{described:?}");
        assert!(profile.contains("credential scopes: none"), "{profile}");

        assert!(!reads(&with_assignment, &known_hosts));
        assert!(!reads(&homeless, &known_hosts));

        #[cfg(unix)]
        {
            let profile = crate::testutil::scratch_dir("confine-remembered-link");
            let _ = std::fs::remove_dir_all(&profile);
            std::fs::create_dir_all(profile.join(".ssh")).expect(".ssh");
            let profile = std::fs::canonicalize(profile).expect("canonical");
            let named = profile.join("shared");
            std::os::unix::fs::symlink(profile.join(".ssh"), &named).expect("link");
            let confined = Confinement::new(
                Prelude::Windows,
                PathBuf::from("/tmp"),
                Some(&profile),
                vec![PathBuf::from("/work/project")],
                Some(Path::new("/var/scratch")),
            )
            .with_grants(vec![remembered(
                "/usr/bin/make",
                None,
                Reached::Directory(named.clone()),
                true,
            )]);

            let policy = confined.policy(&step("/usr/bin/make", &[]), Path::new("/work"), &[]);

            let keys = profile.join(".ssh");
            let keys = keys.to_str().expect("utf-8");
            assert!(!reads(&policy, keys) && !writes(&policy, keys));
            assert!(!reads(&policy, named.to_str().expect("utf-8")));
            let described = confined.describe(&[&step("/usr/bin/make", &[])]);
            assert!(
                described.sentences().is_empty(),
                "{:?}",
                described.sentences()
            );
        }
    }

    /// The plan says what was remembered, with the day, and the failure line names the scope. The
    /// regressions it rejects: a policy that carries a row the plan never showed, and a profile
    /// line that says `none` for a scope the policy added.
    #[test]
    fn the_plan_and_the_failure_line_name_a_remembered_scope() {
        let confined = confinement(&["/work/project"]).with_grants(vec![remembered(
            "/usr/bin/make",
            None,
            a_scope("remote"),
            false,
        )]);
        let make = step("/usr/bin/make", &[]);

        let described = confined.describe(&[&make]);
        let sentences = described.sentences();
        let profile = confined.profile(&[&make]);

        assert_eq!(sentences.len(), 1, "{sentences:?}");
        assert!(sentences[0].contains("2026-10-07"), "{sentences:?}");
        assert!(sentences[0].contains("make"), "{sentences:?}");
        assert!(profile.contains("credential scopes: remote"), "{profile}");
        let ls = step("/bin/ls", &[]);
        assert!(confined.describe(&[&ls]).sentences().is_empty());
        assert!(confined.profile(&[&ls]).contains("credential scopes: none"));
    }

    /// `gh` opens the directory its environment names, so a stage that carries the remote scope
    /// reads that one, for `gh` alone, and not where an assignment in front of it removed the scope.
    #[test]
    fn a_gh_stage_reads_the_configuration_directory_its_environment_names() {
        let confined = confinement(&["/work/project"]);
        let second = format!("{HOME}/.config/gh-second");
        let environment = vec![("GH_CONFIG_DIR".to_string(), second.clone())];
        let hosts = second.clone();

        let issue = confined.policy(
            &step("/usr/local/bin/gh", &["issue", "create"]),
            Path::new("/work"),
            &environment,
        );
        let version = confined.policy(
            &step("/usr/local/bin/gh", &["version"]),
            Path::new("/work"),
            &environment,
        );
        let push = confined.policy(
            &step("/usr/bin/git", &["push"]),
            Path::new("/work"),
            &environment,
        );
        let mut assigned = step("/usr/local/bin/gh", &["issue", "list"]);
        assigned.environment = vec![("GH_CONFIG_DIR".to_string(), second)];
        let assigned = confined.policy(&assigned, Path::new("/work"), &environment);
        let unset = confined.policy(
            &step("/usr/local/bin/gh", &["issue", "list"]),
            Path::new("/work"),
            &[],
        );

        assert!(reads(&issue, &hosts));
        assert!(!reads(
            &issue,
            &format!("{HOME}/.config/another-program/token")
        ));
        assert!(!reads(&version, &hosts));
        assert!(!reads(&push, &hosts));
        assert!(!reads(&assigned, &hosts));
        assert!(reads(&unset, &format!("{HOME}/.config/gh")));
        assert!(!reads(&unset, &hosts));
    }

    /// A tool whose configuration a variable moves reads it where the variable says, for the tool
    /// that reads the variable and no other, and not where an assignment in front of it removed the
    /// scope.
    #[test]
    fn a_stage_reads_the_configuration_its_variable_moves() {
        let confined = confinement(&["/work/project"]);
        let moved = |name: &str, path: &str| vec![(name.to_string(), path.to_string())];
        for (program, args, name, path, default) in [
            (
                "/usr/bin/aws",
                &["s3", "ls"][..],
                "AWS_CONFIG_FILE",
                "/elsewhere/aws-config",
                ".aws",
            ),
            (
                "/usr/bin/docker",
                &["ps"][..],
                "DOCKER_CONFIG",
                "/elsewhere/docker",
                ".docker",
            ),
            (
                "/usr/bin/kubectl",
                &["get", "pods"][..],
                "KUBECONFIG",
                "/elsewhere/kubeconfig",
                ".kube",
            ),
            (
                "/usr/bin/git",
                &["push"][..],
                "GIT_CONFIG_GLOBAL",
                "/elsewhere/gitconfig",
                ".gitconfig",
            ),
        ] {
            let environment = moved(name, path);
            let moved_to = confined.policy(&step(program, args), Path::new("/work"), &environment);
            let unset = confined.policy(&step(program, args), Path::new("/work"), &[]);
            assert!(reads(&moved_to, path), "{program} {name}");
            assert!(!reads(&unset, path), "{program} unset");
            assert!(
                reads(&unset, &format!("{HOME}/{default}")),
                "{program} keeps its fixed row"
            );
            let mut assigned = step(program, args);
            assigned.environment = vec![("GIT_SSH_COMMAND".to_string(), "ssh".to_string())];
            let assigned = confined.policy(&assigned, Path::new("/work"), &environment);
            assert!(!reads(&assigned, path), "{program} with an assignment");
        }
        let docker = moved("DOCKER_CONFIG", "/elsewhere/docker");
        let push = confined.policy(
            &step("/usr/bin/git", &["push"]),
            Path::new("/work"),
            &docker,
        );
        assert!(!reads(&push, "/elsewhere/docker"));
    }

    /// The location the environment moved a scope to is in the prompt with the variable that moved
    /// it, from the same reading as the profile, and a stage with the default location has none.
    #[test]
    fn the_prompt_names_a_location_the_environment_moved() {
        let confined = confinement(&["/work/project"]);
        let docker = step("/usr/bin/docker", &["ps"]);
        let environment = vec![("DOCKER_CONFIG".to_string(), "/elsewhere/docker".to_string())];

        let moved = confined.describe_in(&[&docker], &environment);
        let default = confined.describe_in(&[&docker], &[]);

        let reach = &moved.carried[0].reaches;
        assert_eq!(reach.len(), 1);
        assert_eq!(reach[0].variable, "DOCKER_CONFIG");
        assert_eq!(reach[0].path, PathBuf::from("/elsewhere/docker"));
        let sentences = moved.sentences();
        assert!(
            sentences
                .iter()
                .any(|line| line.contains("DOCKER_CONFIG") && line.contains("/elsewhere/docker")),
            "{sentences:?}"
        );
        assert!(default.carried[0].reaches.is_empty());
        assert_eq!(default.sentences().len(), 1);
    }

    /// A program installed at the top of the home is read as the file a person read, and the home
    /// is not opened for it.
    #[test]
    fn a_program_at_the_top_of_the_home_is_granted_as_a_file_and_not_as_the_home() {
        let confined = confinement(&["/work/project"]);
        let tool = format!("{HOME}/tool");

        let policy = confined.policy(&step(&tool, &[]), Path::new("/work"), &[]);

        assert!(reads(&policy, &tool));
        assert!(!reads(&policy, HOME));
    }

    /// The directories `PATH` names outside the home are read, with the installation beside a `bin`
    /// directory, and one inside the home is left to the toolchain's own list.
    #[test]
    fn the_path_outside_the_home_is_read_and_the_homes_own_bin_brings_no_parent() {
        let searched = vec![
            PathBuf::from("/opt/tools/bin"),
            PathBuf::from(format!("{HOME}/.cargo/bin")),
        ];

        let rows = program_reads(Path::new("/usr/bin/ls"), &searched, Some(Path::new(HOME)));

        assert!(rows.contains(&PathBuf::from("/opt/tools/bin")));
        assert!(rows.contains(&PathBuf::from("/opt/tools")));
        assert!(!rows.contains(&PathBuf::from(format!("{HOME}/.cargo"))));
    }

    /// A planner not told that its programs are confined reads `Operation not permitted` as a fault
    /// in the machine. The sentence has to name both spellings of the refusal and say who widens it.
    #[test]
    fn a_confining_turn_tells_the_planner_what_a_refusal_means() {
        let said = stated(
            true,
            Some(Prelude::Linux),
            Network::Open,
            SandboxMode::Standard,
        )
        .expect("a confining turn says something");

        assert!(said.contains("confined"), "{said}");
        assert!(said.contains("`Operation not permitted`"), "{said}");
        assert!(said.contains("`Permission denied`"), "{said}");
        assert!(
            said.contains("/add-dir") && said.contains("--add-dir"),
            "{said}"
        );
    }

    /// A planner that is not told the network is closed reads `Could not resolve host` as an
    /// outage and retries. Only a closed network says so, and a turn that does not confine does not.
    #[test]
    fn a_closed_network_is_told_to_the_planner_and_an_open_one_is_not() {
        let open = stated(
            true,
            Some(Prelude::MacOs),
            Network::Open,
            SandboxMode::Standard,
        )
        .expect("says something");
        let closed = stated(
            true,
            Some(Prelude::MacOs),
            Network::Closed,
            SandboxMode::Standard,
        )
        .expect("says something");
        assert!(!open.contains("network"), "{open}");
        assert!(closed.starts_with(open.as_str()), "{closed}");
        assert!(closed.contains("The network is closed"), "{closed}");
        assert_eq!(
            stated(
                false,
                Some(Prelude::MacOs),
                Network::Closed,
                SandboxMode::Standard
            ),
            None
        );
        let unconfined =
            stated(true, None, Network::Closed, SandboxMode::Standard).expect("says something");
        assert!(!unconfined.contains("network is closed"), "{unconfined}");
    }

    /// A planner told of a boundary its programs do not have would stop reaching for paths they
    /// can reach, so a turn that does not confine says nothing, whatever the platform.
    #[test]
    fn a_turn_that_does_not_confine_says_nothing_of_confinement() {
        assert_eq!(
            stated(
                false,
                Some(Prelude::Linux),
                Network::Open,
                SandboxMode::Standard
            ),
            None
        );
        assert_eq!(
            stated(
                false,
                Some(Prelude::MacOs),
                Network::Open,
                SandboxMode::Standard
            ),
            None
        );
        assert_eq!(
            stated(false, None, Network::Open, SandboxMode::Standard),
            None
        );
    }

    /// Windows has no base, so the same sentence would be false there. The regression it rejects
    /// is the confined sentence on a platform whose programs run with the person's own access.
    #[test]
    fn a_platform_with_no_base_says_its_programs_are_not_confined() {
        let said = stated(true, None, Network::Open, SandboxMode::Standard)
            .expect("a confining turn says something");

        assert!(said.contains("not confined"), "{said}");
        assert!(!said.contains("Operation not permitted"), "{said}");
    }

    /// The line a failed step carries names what the session owns, the lists by name and the
    /// scope its argv named, so a planner can tell a path outside them from a fault.
    #[test]
    fn the_profile_line_names_the_session_directories_the_lists_and_the_scope() {
        let confined = confinement(&["/work/project", "/work/added"]);
        let cargo = step("/usr/bin/cargo", &["build"]);
        let push = step("/usr/bin/git", &["push"]);

        let line = confined.profile(&[&cargo, &push]);

        for named in [
            "/work/project",
            "/work/added",
            "/var/scratch",
            "toolchain lists: cargo;",
            "credential scopes: remote)",
        ] {
            assert!(line.contains(named), "{named} is not in {line}");
        }
    }

    /// A step that brings no list and no scope says `none`, rather than leaving the planner to
    /// wonder whether the line left them out.
    #[test]
    fn a_step_with_no_list_and_no_scope_says_none_for_both() {
        let confined = confinement(&["/work/project"]);

        let line = confined.profile(&[&step("/bin/ls", &[]), &step("/usr/bin/git", &["status"])]);

        assert!(
            line.contains("toolchain lists: none; credential scopes: none)"),
            "{line}"
        );
    }

    /// The line is composed from the session and the compiled step. Two steps whose arguments name
    /// different paths a program chose get the same line, and neither path is in it.
    #[test]
    fn the_profile_line_is_the_same_whatever_paths_the_step_was_given() {
        let confined = confinement(&["/work/project"]);

        let one = confined.profile(&[&step("/bin/cat", &["/elsewhere/one"])]);
        let two = confined.profile(&[&step("/bin/cat", &["/elsewhere/two"])]);

        assert_eq!(one, two);
        assert!(!one.contains("/elsewhere"), "{one}");
    }

    /// The line and the policy read the same table: a step the policy gives the cargo cache and
    /// the remote scope is named as having both, and one it gives neither is not.
    #[test]
    fn the_profile_line_agrees_with_the_policy_on_the_lists_and_the_scope() {
        let confined = confinement(&["/work/project"]);
        let registry = format!("{HOME}/.cargo/registry");
        let known_hosts = format!("{HOME}/.ssh/known_hosts");

        for (program, args, granted) in [
            ("/usr/bin/cargo", vec!["build"], (true, false)),
            ("/usr/bin/git", vec!["push"], (false, true)),
            ("/usr/bin/make", vec!["build"], (false, false)),
        ] {
            let step = step(program, &args);
            let policy = confined.policy(&step, Path::new("/work"), &[]);
            let line = confined.profile(&[&step]);

            assert_eq!(writes(&policy, &registry), granted.0, "{program}");
            assert_eq!(line.contains("cargo;"), granted.0, "{program}: {line}");
            assert_eq!(reads(&policy, &known_hosts), granted.1, "{program}");
            assert_eq!(
                line.contains("scopes: remote)"),
                granted.1,
                "{program}: {line}"
            );
        }
    }

    /// With the network closed the policy, the line and the prompt's description are read from one
    /// decision: a stage keeps egress in the policy exactly where the line names a reason and the
    /// description marks it.
    #[test]
    fn the_policy_the_line_and_the_description_agree_on_which_stages_keep_the_network() {
        let closed = confinement(&["/work/project"]).with_network(Network::Closed);

        for (program, args, kept, reason) in [
            (
                "/usr/bin/cargo",
                vec!["build"],
                true,
                "a toolchain that fetches",
            ),
            (
                "/usr/bin/pip",
                vec!["install", "x"],
                true,
                "a toolchain that fetches",
            ),
            ("/usr/bin/git", vec!["push"], true, "a credential scope"),
            (
                "/usr/bin/curl",
                vec!["https://a.example"],
                true,
                "a program that talks",
            ),
            ("/usr/bin/ssh", vec!["host"], true, "a program that talks"),
            (
                "/usr/bin/docker",
                vec!["pull", "x"],
                true,
                "a credential scope",
            ),
            ("/usr/bin/python3", vec!["x.py"], false, ""),
            ("/usr/bin/node", vec!["x.js"], false, ""),
            ("/usr/bin/git", vec!["status"], false, ""),
            ("/usr/bin/make", vec!["test"], false, ""),
            ("/bin/cat", vec!["a"], false, ""),
        ] {
            let step = step(program, &args);
            let policy = closed.policy(&step, Path::new("/work"), &[]);
            let line = closed.profile(&[&step]);
            let described = closed.describe(&[&step]);

            assert_eq!(policy.allow_network, kept, "{program} {args:?}");
            assert_eq!(closed.egress(&step), kept, "{program} {args:?}");
            assert!(line.contains("Network: closed"), "{line}");
            if kept {
                assert!(line.contains(reason), "{program}: {line}");
                assert!(described.network == Network::Closed);
                assert!(
                    described.carried.iter().any(|carried| carried.network),
                    "{program}: the prompt does not mark the stage"
                );
            } else {
                assert!(line.contains("kept only by steps with: none"), "{line}");
            }
        }

        let open = confinement(&["/work/project"]);
        let make = step("/usr/bin/make", &[]);
        assert!(open.policy(&make, Path::new("/work"), &[]).allow_network);
        assert!(open.profile(&[&make]).ends_with("Network: open."));
    }

    /// A file the plan could have written, under a directory it may write to, gets no network by
    /// being named `curl` or `cargo`: the reasons are read from the file name.
    #[test]
    fn a_program_the_plan_could_have_written_keeps_no_network_by_its_name() {
        let closed = confinement(&["/work/project"]).with_network(Network::Closed);
        for (program, args) in [
            ("/work/project/bin/curl", vec!["https://a.example"]),
            ("/work/project/target/cargo", vec!["build"]),
            ("/var/scratch/ssh", vec!["host"]),
            ("/work/project/git", vec!["push"]),
            ("/tmp/curl", vec![]),
        ] {
            let step = step(program, &args);
            assert!(!closed.egress(&step), "{program}");
            assert!(
                !closed.policy(&step, Path::new("/work"), &[]).allow_network,
                "{program}"
            );
            assert!(
                closed
                    .network_for_the_trail(&[&step])
                    .unwrap()
                    .contains("every stage")
            );
        }
        let installed = step("/usr/bin/curl", &["https://a.example"]);
        assert!(closed.egress(&installed));
    }

    /// A stage with an assignment in front of it carries no remote scope, so it has no network
    /// under a closed setting; an unrelated `NAME=value` cannot be used to ask for one either way.
    #[test]
    fn a_closed_network_is_not_reopened_by_what_a_stage_is_started_with() {
        let closed = confinement(&["/work/project"]).with_network(Network::Closed);
        let mut assigned = step("/usr/bin/git", &["push"]);
        assigned.environment = vec![("GIT_SSH_COMMAND".to_string(), "ssh".to_string())];
        assert!(!closed.egress(&assigned));

        let mut arguments = step("/usr/bin/make", &["--network", "open", "curl"]);
        arguments.environment = vec![("BRAVEBOT_RUN_NETWORK".to_string(), "open".to_string())];
        assert!(!closed.egress(&arguments));
    }

    /// The trail names each stage that kept the network by its place and a fixed reason, and holds
    /// nothing a plan wrote: not the program, not an argument. An open network leaves no entry.
    #[test]
    fn the_trail_names_the_stages_that_kept_a_closed_network_by_place_and_reason() {
        let open = confinement(&["/work/project"]);
        let closed = open.clone().with_network(Network::Closed);
        let push = step("/usr/bin/git", &["push", "origin", "a-secret-branch"]);
        let cat = step("/bin/cat", &["notes"]);
        let build = step("/usr/bin/cargo", &["build"]);

        assert_eq!(open.network_for_the_trail(&[&push]), None);
        let kept = closed
            .network_for_the_trail(&[&cat, &push, &build])
            .expect("a closed network is recorded");
        assert_eq!(
            kept,
            "the network was closed for this run except for stage 2 (a credential scope), \
             stage 3 (a toolchain that fetches)"
        );
        assert!(!kept.contains("secret") && !kept.contains("git"), "{kept}");
        assert_eq!(
            closed.network_for_the_trail(&[&cat]).as_deref(),
            Some("the network was closed for every stage of this run")
        );
    }

    /// The pip bit is the file's: a `python3` resolved from the same installation gets none.
    #[test]
    fn the_fetch_bit_is_keyed_on_the_resolved_file() {
        let closed = confinement(&["/work/project"]).with_network(Network::Closed);
        let mut renamed = step("/usr/bin/python3", &["-m", "pip", "install", "x"]);
        renamed.program = "pip".to_string();
        assert!(
            !closed.egress(&renamed),
            "a name the plan chose granted egress"
        );
        assert!(closed.egress(&step("/usr/bin/pip3", &["install", "x"])));
    }

    /// A backend that cannot deny the network is refused rather than left to run the stage with it
    /// open, and the refusal names the setting. A backend that can, and a policy that keeps the
    /// network, are not.
    #[test]
    fn a_backend_that_cannot_deny_the_network_refuses_a_closed_stage() {
        use bravebot_sandbox::policy::Capabilities;
        let closed = confinement(&["/work/project"]).with_network(Network::Closed);
        let denied = closed.policy(&step("/bin/cat", &["a"]), Path::new("/work"), &[]);
        let kept = closed.policy(&step("/usr/bin/cargo", &["build"]), Path::new("/work"), &[]);
        let backend = |network_denial_enforced| Capabilities {
            level: bravebot_sandbox::policy::ConfinementLevel::Kernel,
            mechanisms: Vec::new(),
            network_denial_enforced,
            grants_paths_that_do_not_exist: true,
        };
        let (cannot, can) = (backend(false), backend(true));

        let refused = cannot_close_the_network(&denied, &cannot).expect("refused");
        assert!(refused.contains("run.network"), "{refused}");
        assert_eq!(cannot_close_the_network(&denied, &can), None);
        assert_eq!(cannot_close_the_network(&kept, &cannot), None);
    }

    /// A session directory under the build directory, which no row of the base reaches, and a
    /// confinement of this machine's own over it.
    fn a_session(name: &str) -> (PathBuf, Confinement) {
        let session = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join(format!("confine-unit-{name}"));
        let _ = std::fs::remove_dir_all(&session);
        std::fs::create_dir_all(&session).expect("session directory");
        let session = session.canonicalize().expect("canonical session");
        let confinement = Confinement::new(
            Prelude::current().expect("a platform with a base"),
            canonical(&temporary_directory()),
            None,
            vec![session.clone()],
            None,
        );
        (session, confinement)
    }

    /// The line a refusal is tested with: the first stage would write `late.txt` a second from
    /// now, and the second is the one the platform cannot confine.
    const A_SLOW_WRITER_INTO_A_PIPE: &str = "sh -c 'sleep 1; touch late.txt' | cat";

    fn can_confine() -> bool {
        Prelude::current().is_some() && bravebot_sandbox::confinement_works_here()
    }

    fn plan_of(line: &str, session: &Path) -> bravebot_core::command::Plan {
        crate::cmdline::compile(line, session, None, &mut |_, _| Ok(()))
            .unwrap_or_else(|error| panic!("`{line}` should compile: {error}"))
    }

    /// A stage the platform cannot confine is refused and nothing is started in its place. The
    /// regression it rejects is a fall back to the plain command when confining fails, which is
    /// every program run without its profile on exactly the machines that cannot apply one: the
    /// earlier stage is then left running and writes, or the refused stage runs and the call
    /// returns a result.
    #[test]
    fn a_stage_that_cannot_be_confined_is_refused_with_no_stage_left_running() {
        if !can_confine() {
            return;
        }
        let (session, confinement) = a_session("foreground-refusal");
        let confinement = confinement.failing_for("cat");
        let plan = plan_of(A_SLOW_WRITER_INTO_A_PIPE, &session);

        let refused = crate::exec::run_plan_observed(
            &plan,
            &bravebot_core::cancel::Cancel::new(),
            crate::exec::LIMIT,
            None,
            None,
            Some(&confinement),
            &mut |_| Ok(()),
        );

        assert!(
            matches!(&refused, Err(ExecError::NotConfined { program, .. }) if program == "cat"),
            "{refused:?}"
        );
        std::thread::sleep(std::time::Duration::from_secs(2));
        assert!(
            !session.join("late.txt").exists(),
            "the stage before the refused one was left running"
        );
    }

    /// The same refusal for a job left running, which is a second place a stage is started.
    #[test]
    fn a_job_with_a_stage_that_cannot_be_confined_is_refused_with_no_stage_left_running() {
        if !can_confine() {
            return;
        }
        let (session, confinement) = a_session("background-refusal");
        let confinement = confinement.failing_for("cat");
        let plan = plan_of(A_SLOW_WRITER_INTO_A_PIPE, &session);
        let steps = plan.steps.unrouted_pipeline().expect("one pipeline");

        let refused = crate::exec::start_steps(steps, &session, None, Some(&confinement));

        assert!(
            matches!(&refused, Err(ExecError::NotConfined { program, .. }) if program == "cat"),
            "{:?}",
            refused.err()
        );
        std::thread::sleep(std::time::Duration::from_secs(2));
        assert!(
            !session.join("late.txt").exists(),
            "the stage before the refused one was left running"
        );
    }

    /// The `\\?\` form `canonicalize` gives on Windows names a drive path no row can be compared
    /// with, so it is dropped there; a UNC path keeps it, since without it the path names
    /// something else.
    #[test]
    fn a_verbatim_drive_path_loses_its_prefix_and_nothing_else_does() {
        let long_verbatim = format!(r"\\?\C:\{}", "a".repeat(MAX_PATH));
        for (given, expected) in [
            (r"\\?\C:\Users\a", r"C:\Users\a"),
            (r"\\?\d:\", r"d:\"),
            (r"\\?\UNC\server\share\a", r"\\?\UNC\server\share\a"),
            (r"\\?\C:", r"\\?\C:"),
            (&long_verbatim, &long_verbatim),
            (r"C:\Users\a", r"C:\Users\a"),
            ("/home/a", "/home/a"),
        ] {
            assert_eq!(
                without_verbatim_prefix(PathBuf::from(given)),
                PathBuf::from(expected),
                "{given}"
            );
        }
    }

    /// Windows reads `Path` and `PATH` as one variable, so a step setting one must replace the
    /// other rather than start with both, and a lookup of `PATH` must find `Path`.
    #[test]
    fn variable_names_match_without_case_only_when_folding() {
        use std::ffi::{OsStr, OsString};
        let pair = |name: &str, value: &str| (OsString::from(name), OsString::from(value));
        let set =
            |name: &'static str, value: &'static str| (OsStr::new(name), Some(OsStr::new(value)));

        let folded = overlay(
            vec![pair("Path", "inherited"), pair("TEMP", "t")],
            [set("PATH", "chosen")].into_iter(),
            true,
        );
        assert_eq!(folded, vec![pair("TEMP", "t"), pair("PATH", "chosen")]);

        let exact = overlay(
            vec![pair("Path", "inherited"), pair("TEMP", "t")],
            [set("PATH", "chosen")].into_iter(),
            false,
        );
        assert_eq!(
            exact,
            vec![
                pair("Path", "inherited"),
                pair("TEMP", "t"),
                pair("PATH", "chosen")
            ]
        );

        let removed = overlay(
            vec![pair("Path", "inherited")],
            [(OsStr::new("PATH"), None)].into_iter(),
            true,
        );
        assert!(removed.is_empty());

        let held = vec![("Path".to_string(), r"C:\bin".to_string())];
        assert_eq!(lookup(&held, "PATH", true), Some(r"C:\bin".to_string()));
        assert_eq!(lookup(&held, "PATH", false), None);
        assert_eq!(lookup(&held, "TEMP", true), None);
    }
}
