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
use bravebot_config::sandbox_network::Hosts;
use bravebot_core::command::Step;
use bravebot_sandbox::SandboxMode;
use bravebot_sandbox::Variables;
use bravebot_sandbox::base::{Prelude, base, run_base, with_security_cache};
use bravebot_sandbox::network::{Network, program_talks_to_a_remote};
use bravebot_sandbox::policy::SandboxPolicy;
use bravebot_sandbox::rules::{Lists, Rules};
use bravebot_sandbox::scope::{Reach, Requested, Scope, environment_reach, requested_reach};
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
    /// The per-user cache directory the platform names, resolved by the host on macOS, where the
    /// keychain lookup of `gh` and `git` writes under it.
    user_cache: Option<PathBuf>,
    /// What the session decided about the network for the stages it starts.
    network: Network,
    /// The host list the session's stages with egress are held to, or none for no filtering and
    /// no proxy (SANDBOX-24).
    hosts: Option<Hosts>,
    /// How much of the machine a step reads (SANDBOX-22). `Off` never reaches a confinement: a turn
    /// in that mode builds none ([`Confinement::here`] is not asked), so the rows below are the
    /// two modes that confine.
    mode: SandboxMode,
    /// The reach a person remembered for commands, which attaches to the steps its shape names.
    grants: Vec<Grant>,
    /// The person's own filesystem lists, resolved against this session's directories.
    filesystem: Rules,
    /// The paths the person let programs reach for this session (SANDBOX-28). Applied before
    /// `filesystem`, so a denial of the person's still removes them.
    path_reach: Rules,
    /// What the planner asked this one line to add to every stage that has no assignment in front
    /// of it, from the fixed menu. Empty for every line that asked for nothing.
    requested: Vec<Requested>,
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
        let confinement = Self::new(
            prelude,
            canonical(&temporary_directory()),
            home,
            roots,
            scratch,
        );
        #[cfg(target_os = "macos")]
        let confinement = Self {
            user_cache: bravebot_sandbox::macos::user_cache_directory(),
            ..confinement
        };
        Some(confinement)
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
            user_cache: None,
            network: Network::Open,
            hosts: None,
            mode: SandboxMode::Standard,
            grants: Vec::new(),
            filesystem: Rules::none(),
            path_reach: Rules::none(),
            requested: Vec::new(),
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

    /// This confinement with the session's host list. A list that is set, even an empty one,
    /// filters every stage that has egress through a proxy; no `allowedHosts` leaves the stages
    /// as they were (SANDBOX-24).
    pub fn with_hosts(mut self, hosts: Option<&Hosts>) -> Self {
        self.hosts = hosts.filter(|hosts| hosts.allowed.is_some()).cloned();
        self
    }

    /// The proxy variables the stage `step` is started with, or none where it is not filtered.
    ///
    /// Applied after the person's environment and the step's own assignments, so an assignment a
    /// model wrote cannot point the stage at another proxy. A stage with no egress is given none:
    /// it reaches nothing. Where the proxy cannot be started the stage is refused, since starting
    /// it without the filter is the unfiltered reach the list exists to remove.
    fn host_environment(&self, step: &Step) -> Result<Vec<(String, String)>, String> {
        let Some(hosts) = &self.hosts else {
            return Ok(Vec::new());
        };
        if !self.egress(step) {
            return Ok(Vec::new());
        }
        let spelled = |entries: &[bravebot_config::sandbox_network::HostEntry]| {
            entries
                .iter()
                .map(|host| host.entry.clone())
                .collect::<Vec<_>>()
        };
        let requested = self.requested_for(step);
        let toolchains: Vec<Toolchain> = Toolchain::of(&step.resolved)
            .into_iter()
            .chain(requested.iter().filter_map(|request| match request {
                Requested::Toolchain(toolchain) => Some(*toolchain),
                _ => None,
            }))
            .collect();
        let remote = Scope::of(&step.resolved, &step.args, &step.environment)
            == Some(Scope::Remote)
            || requested
                .iter()
                .any(|request| matches!(request, Requested::Scope(Scope::Remote)))
            || self
                .granted(step)
                .any(|grant| matches!(grant.reached, Reached::Scope(Scope::Remote)));
        let list = crate::host_proxy::list_for(
            &spelled(hosts.allowed.as_deref().unwrap_or_default()),
            &spelled(&hosts.denied),
            remote,
            &toolchains,
        )?;
        let proxy = crate::host_proxy::proxy_for(&list)
            .map_err(|error| format!("the allowed-hosts proxy could not start: {error}"))?;
        Ok(proxy.environment())
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

    /// This confinement with the paths the person let programs reach for the session, each judged
    /// when it was approved (SANDBOX-28).
    pub fn with_path_reach(mut self, reach: &[crate::workspace::PathReach]) -> Self {
        let read: Vec<PathBuf> = reach
            .iter()
            .filter(|row| !row.write)
            .map(|row| row.path.clone())
            .collect();
        let write: Vec<PathBuf> = reach
            .iter()
            .filter(|row| row.write)
            .map(|row| row.path.clone())
            .collect();
        self.path_reach = Rules::granted(&read, &write);
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
            || self
                .requested_for(step)
                .iter()
                .any(|request| matches!(request, Requested::Toolchain(_)))
        {
            Some("a toolchain that fetches")
        } else if Scope::of(&step.resolved, &step.args, &step.environment).is_some()
            || self
                .requested_for(step)
                .iter()
                .any(|request| matches!(request, Requested::Scope(_)))
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

    /// This confinement with what the planner asked one line to add to its stages, deduplicated and
    /// kept in the menu's order so the prompt, the trail and the failure sentence name them alike.
    #[must_use]
    pub fn with_requested(mut self, requested: &[Requested]) -> Self {
        let mut kept: Vec<Requested> = Vec::new();
        for request in requested {
            if !kept.contains(request) {
                kept.push(*request);
            }
        }
        kept.sort_by_key(|request| {
            Requested::MENU
                .iter()
                .position(|name| *name == request.name())
        });
        self.requested = kept;
        self
    }

    /// Whether a request can be applied: the rows of both kinds are under the account's home.
    pub fn accepts_requests(&self) -> bool {
        self.home.is_some()
    }

    /// What the planner asked for, in the menu's order.
    pub fn requested(&self) -> &[Requested] {
        &self.requested
    }

    /// What the planner's request adds to `step`: nothing for a step with an assignment in front of
    /// it, for the reason an assignment removes a scope ([`Scope::of`]), and nothing where the
    /// session names no home, since both kinds of row are under it.
    fn requested_for(&self, step: &Step) -> &[Requested] {
        match self.home.is_some() && step.environment.is_empty() {
            true => &self.requested,
            false => &[],
        }
    }

    /// What the trail records of a request for one run: each stage by its place in the line and the
    /// names added to it, which are menu words and nothing the plan chose. `None` where the line
    /// asked for nothing.
    pub fn requested_for_the_trail(&self, steps: &[&Step]) -> Option<String> {
        if self.requested.is_empty() {
            return None;
        }
        let named: Vec<String> = steps
            .iter()
            .enumerate()
            .map(|(at, step)| {
                let added = self.requested_for(step);
                match added.is_empty() {
                    true => format!("stage {} (nothing)", at + 1),
                    false => format!(
                        "stage {} ({})",
                        at + 1,
                        added
                            .iter()
                            .map(|r| r.name())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                }
            })
            .collect();
        Some(format!(
            "the planner asked for {} for this run, added to {}",
            self.requested
                .iter()
                .map(|r| r.name())
                .collect::<Vec<_>>()
                .join(", "),
            named.join(", ")
        ))
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
            requested: steps
                .iter()
                .flat_map(|step| {
                    self.requested_for(step)
                        .iter()
                        .map(|request| (step.program.clone(), *request))
                })
                .collect(),
            requested_reaches: steps
                .iter()
                .flat_map(|step| {
                    let held = self.stage_reaches(step, environment);
                    let home = self.home.as_deref();
                    self.requested_for(step)
                        .iter()
                        .filter_map(|request| match (request, home) {
                            (Requested::Scope(scope), Some(home)) => {
                                Some(requested_reach(*scope, home, environment))
                            }
                            _ => None,
                        })
                        .flatten()
                        .filter(move |reach| !held.iter().any(|kept| kept.path == reach.path))
                        .map(|reach| (step.program.clone(), reach))
                        .collect::<Vec<_>>()
                })
                .collect(),
            carried: steps
                .iter()
                .filter_map(|step| {
                    let (toolchain, scope) = self.carries(step);
                    let reaches = self.stage_reaches(step, environment);
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

    /// Where `environment` moves the scope the step's own argv names. The process's own
    /// environment is what the executor starts a step with, and a step with an assignment in front
    /// of it carries no scope to move.
    fn stage_reaches(&self, step: &Step, environment: &[(String, String)]) -> Vec<Reach> {
        match (self.carries(step).1, self.home.as_deref()) {
            (Some(scope), Some(home)) => reaches(&step.resolved, scope, home, environment),
            _ => Vec::new(),
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
        if self.prelude == Prelude::MacOs
            && self.reads_the_machine()
            && let Some(cache) = &self.user_cache
        {
            policy = with_security_cache(policy, cache);
        }
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
            // The places a variable names are those of every program the scope has, since a script
            // wrapping `gh` is not `gh` and there is no program to key them on.
            for request in self.requested_for(step) {
                policy = match *request {
                    Requested::Toolchain(toolchain) => toolchain.grant(policy, self.prelude, home),
                    Requested::Scope(scope) => {
                        let mut policy = scope.grant(policy, home);
                        if !self.reads_the_machine() {
                            for reach in requested_reach(scope, home, environment) {
                                policy = policy.allow_read(reach.path);
                            }
                        }
                        match variable(environment, "SSH_AUTH_SOCK") {
                            Some(socket) if scope == Scope::Remote => policy.allow_write(socket),
                            _ => policy,
                        }
                    }
                };
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
        let policy = self.path_reach.apply(policy);
        self.filesystem.apply(policy).starting_in(directory)
    }

    /// The one sentence that says what the programs of `steps` ran under, for a result whose
    /// steps did not exit zero.
    ///
    /// Composed from this confinement and the compiled steps, which a person read, and from
    /// nothing a program printed or exited with: the same line follows a refusal, a failing test
    /// and a typo. It names the directories the session owns and the lists by name, and no path a
    /// program chose, since a program's error text is not where the profile is decided. It ends
    /// with the fixed sentence about credential locations.
    pub fn profile(&self, steps: &[&Step]) -> String {
        format!("{}{CREDENTIAL_LOCATIONS_SENTENCE}", self.reach(steps))
    }

    /// What the programs of `steps` could reach, by name, without the closing fixed sentence.
    fn reach(&self, steps: &[&Step]) -> String {
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
            for request in self.requested_for(step) {
                match request {
                    Requested::Scope(scope) => scopes.insert(scope.name()),
                    Requested::Toolchain(toolchain) => toolchains.insert(toolchain.name()),
                };
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
        let requested_paths = self.path_reach.counts();
        let rules = match requested_paths.allow_read + requested_paths.allow_write {
            0 => rules,
            asked => format!(
                "{rules} The person also let programs reach {asked} path{} through request_path \
                 this session.",
                if asked == 1 { "" } else { "s" },
            ),
        };
        if self.reads_the_machine() {
            return format!(
                "Confinement: programs could read this machine except the places that hold a \
                 credential, and write {} and the temporary directory and the toolchain caches; \
                 a place that holds a credential was read only where a credential scope added it \
                 for the steps that named one (credential scopes: {}). Any other path is refused \
                 by the operating system as `Operation not permitted` or `Permission denied`. \
                 Network: {}.{}{}",
                directories.join(", "),
                named(scopes),
                network,
                rules,
                self.menu_sentence(),
            );
        }
        format!(
            "Confinement: programs could read and write {} and the temporary directory, and read \
             the system and program directories and git's configuration files; beyond those \
             they reached only what a toolchain list or credential scope added for the steps \
             that named one (toolchain lists: {}; credential scopes: {}). Any other path is \
             refused by the operating system as `Operation not permitted` or `Permission \
             denied`. Network: {}.{}{}",
            directories.join(", "),
            named(toolchains),
            named(scopes),
            network,
            rules,
            self.menu_sentence(),
        )
    }

    /// The sentence naming what a line may ask for, or nothing where this session names no home to
    /// apply a request under.
    ///
    /// The same words follow every failure of the line, the exit code not among its inputs.
    fn menu_sentence(&self) -> String {
        match self.accepts_requests() {
            true => format!(" {}", requests_menu_sentence()),
            false => String::new(),
        }
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
        let not_confined = |detail: String| ExecError::NotConfined {
            program: step.program.clone(),
            detail,
        };
        let proxied = self.host_environment(step).map_err(not_confined)?;
        let environment = overlay(
            effective_environment(command),
            proxied
                .iter()
                .map(|(name, value)| (OsStr::new(name), Some(OsStr::new(value)))),
            FOLD_CASE,
        );
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
            program: command.get_program().to_os_string(),
            args: command.get_args().map(OsStr::to_os_string).collect(),
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

/// Fixed text that ends every profile line, so the line is the same for every failure.
const CREDENTIAL_LOCATIONS_SENTENCE: &str = " A credential location such as `~/.ssh`, `~/.aws` \
     or `~/.kube` cannot be added with `/add-dir` or `--add-dir`. Where the mode accepts a \
     request, a credential scope is how a line reaches one. Any other path a command needs is \
     asked for with the `request_path` tool: the person is asked, and a yes lasts for this \
     session.";

/// The sentence that names the menu, for the planner, and the person is asked about every time.
fn requests_menu_sentence() -> String {
    format!(
        "A line that needs more can ask for it with `run`'s `scopes` argument, by name from this \
         list: {}. The person is asked about the line every time, and no answer to it is \
         remembered. A path outside the workspace that a command needs is asked for with the \
         `request_path` tool instead, and a yes lasts for the session.",
        bravebot_sandbox::scope::Requested::MENU.join(", ")
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
        Some(Prelude::Linux | Prelude::MacOs) if mode == SandboxMode::Strict => {
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
             was stopped by the sandbox and not by a fault in the machine. The person widens it \
             (`/add-dir`, `--add-dir`)."
        }
        None => {
            "Programs this tool starts are not confined on this platform: they run with the \
             access of the person's own account."
        }
    });
    if prelude.is_some() {
        said.push(' ');
        said.push_str(&requests_menu_sentence());
    }
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
    program: std::ffi::OsString,
    args: Vec<std::ffi::OsString>,
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
                format!("{HOME}/.bravebot/gateway-keys.json"),
                format!("{HOME}/.bravebot/leo-premium.json"),
                format!("{HOME}/.bravebot/mcp.json"),
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
                format!("{HOME}/.bravebotx/state"),
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

    /// SANDBOX-26: a requested scope lifts its own directory on a stage that names nothing, and no
    /// other. The control is the same stage with no request, which refuses all three; the
    /// regression it rejects is a request that lifts every credential, or none.
    #[test]
    fn a_requested_scope_lifts_its_own_directory_for_a_stage_that_names_none() {
        let script = step("/bin/sh", &["-c", "aws s3 ls"]);
        let base = confinement(&["/work/project"]).with_mode(SandboxMode::Strict);
        let control = base.policy(&script, Path::new("/work/project"), &[]);
        for directory in [".aws", ".kube", ".docker"] {
            assert!(!reads(&control, &format!("{HOME}/{directory}")));
        }
        for (scope, own) in [
            (Scope::Aws, ".aws"),
            (Scope::Kubernetes, ".kube"),
            (Scope::Docker, ".docker"),
        ] {
            let asked = base
                .clone()
                .with_requested(&[Requested::Scope(scope)])
                .policy(&script, Path::new("/work/project"), &[]);
            for directory in [".aws", ".kube", ".docker"] {
                assert_eq!(
                    reads(&asked, &format!("{HOME}/{directory}")),
                    directory == own,
                    "{scope:?} and {directory}"
                );
            }
        }
    }

    /// SANDBOX-26: under `standard` a requested scope lifts the refusal of its own credential
    /// directory on a stage that names nothing, and no other. The control is the same stage with
    /// no request, which is refused all three. The regression it rejects is a request that the
    /// mode accepts and the profile then ignores, or one that lifts every credential.
    #[test]
    fn a_requested_scope_lifts_its_own_refused_directory_under_standard() {
        for prelude in [Prelude::Linux, Prelude::MacOs] {
            let script = step("/bin/sh", &["-c", "aws s3 ls"]);
            let base = reading_confinement(prelude, &["/work/project"]);
            assert_eq!(base.mode, SandboxMode::Standard);
            let control = base.policy(&script, Path::new("/work/project"), &[]);
            for directory in [".aws", ".kube", ".docker"] {
                let path = format!("{HOME}/{directory}/credentials");
                assert!(refuses(&control, &path), "{prelude:?} {directory} control");
            }
            for (scope, own) in [
                (Scope::Aws, ".aws"),
                (Scope::Kubernetes, ".kube"),
                (Scope::Docker, ".docker"),
            ] {
                let asked = base
                    .clone()
                    .with_requested(&[Requested::Scope(scope)])
                    .policy(&script, Path::new("/work/project"), &[]);
                for directory in [".aws", ".kube", ".docker"] {
                    let path = format!("{HOME}/{directory}/credentials");
                    assert_eq!(
                        !refuses(&asked, &path),
                        directory == own,
                        "{prelude:?} {scope:?} and {directory}"
                    );
                }
            }
        }
    }

    /// SANDBOX-26: under `standard` a requested remote scope lends the ssh agent socket to a
    /// script that names no `git`, and still refuses the private keys in `~/.ssh`. The controls are
    /// the same stage with no request, and a stage that asked for another scope.
    #[test]
    fn a_requested_remote_scope_lends_the_agent_socket_under_standard() {
        let socket = "/run/agent.sock";
        let environment = vec![("SSH_AUTH_SOCK".to_string(), socket.to_string())];
        for prelude in [Prelude::Linux, Prelude::MacOs] {
            let script = step("/usr/bin/python3", &["pr-fix.py", "start", "1"]);
            let base = reading_confinement(prelude, &["/work/project"]);
            let ask = |scope| {
                base.clone()
                    .with_requested(&[Requested::Scope(scope)])
                    .policy(&script, Path::new("/work/project"), &environment)
            };
            let control = base.policy(&script, Path::new("/work/project"), &environment);
            let remote = ask(Scope::Remote);
            let other = ask(Scope::Aws);

            assert!(writes(&remote, socket), "{prelude:?} remote");
            assert!(!writes(&control, socket), "{prelude:?} control");
            assert!(!writes(&other, socket), "{prelude:?} another scope");
            for policy in [&control, &remote] {
                assert!(refuses(policy, &format!("{HOME}/.ssh/id_ed25519")));
            }
        }
    }

    /// SANDBOX-16, SANDBOX-26: a requested scope reads the place the process environment moves its
    /// tool's configuration to, for a stage whose argv names no tool, and only that scope's
    /// variables. The control is the same stage asked for the scope with the variable unset, and a
    /// stage that asked for another scope. The regression it rejects is a request that lends the
    /// fixed `~/.aws` and fails for a person whose `AWS_CONFIG_FILE` lives elsewhere.
    #[test]
    fn a_requested_scope_reads_the_configuration_its_variable_moves() {
        let script = step("/bin/sh", &["-c", "run-the-deploy"]);
        let base = confinement(&["/work/project"]).with_mode(SandboxMode::Strict);
        let environment = |name: &str, path: &str| vec![(name.to_string(), path.to_string())];
        for (scope, name, path) in [
            (Scope::Aws, "AWS_CONFIG_FILE", "/elsewhere/aws-config"),
            (Scope::Docker, "DOCKER_CONFIG", "/elsewhere/docker"),
            (Scope::Kubernetes, "KUBECONFIG", "/elsewhere/kubeconfig"),
            (Scope::Remote, "GIT_CONFIG_GLOBAL", "/elsewhere/gitconfig"),
            (Scope::Remote, "GH_CONFIG_DIR", "/elsewhere/gh"),
        ] {
            let asked = base.clone().with_requested(&[Requested::Scope(scope)]);
            let moved = asked.policy(&script, Path::new("/work"), &environment(name, path));
            let unset = asked.policy(&script, Path::new("/work"), &[]);
            let other = base
                .clone()
                .with_requested(&[Requested::Scope(match scope {
                    Scope::Aws => Scope::Docker,
                    _ => Scope::Aws,
                })])
                .policy(&script, Path::new("/work"), &environment(name, path));
            let none = base.policy(&script, Path::new("/work"), &environment(name, path));
            assert!(reads(&moved, path), "{scope:?} {name}");
            assert!(!reads(&unset, path), "{scope:?} unset");
            assert!(!reads(&other, path), "{scope:?} asked as another scope");
            assert!(!reads(&none, path), "{scope:?} not asked for");
            let mut assigned = script.clone();
            assigned.environment = vec![("GIT_SSH_COMMAND".to_string(), "ssh".to_string())];
            let assigned = asked.policy(&assigned, Path::new("/work"), &environment(name, path));
            assert!(!reads(&assigned, path), "{scope:?} with an assignment");
        }
    }

    /// SANDBOX-16, SANDBOX-26: the prompt names a place a requested scope was moved to with the
    /// variable, once, and says nothing for a stage whose argv already carried it or whose
    /// variable is unset.
    #[test]
    fn the_prompt_names_a_location_the_environment_moved_for_a_requested_scope() {
        let base = confinement(&["/work/project"])
            .with_mode(SandboxMode::Strict)
            .with_requested(&[Requested::Scope(Scope::Docker)]);
        let script = step("/bin/sh", &["-c", "run-the-deploy"]);
        let docker = step("/usr/bin/docker", &["ps"]);
        let environment = vec![("DOCKER_CONFIG".to_string(), "/elsewhere/docker".to_string())];

        let moved = base.describe_in(&[&script], &environment);
        assert_eq!(moved.requested_reaches.len(), 1);
        assert_eq!(moved.requested_reaches[0].0, "sh");
        assert_eq!(moved.requested_reaches[0].1.variable, "DOCKER_CONFIG");
        assert!(
            moved
                .sentences()
                .iter()
                .any(|line| line.contains("sh also reads /elsewhere/docker")
                    && line.contains("DOCKER_CONFIG")),
            "{:?}",
            moved.sentences()
        );
        assert!(
            base.describe_in(&[&script], &[])
                .requested_reaches
                .is_empty()
        );

        let both = base.describe_in(&[&docker], &environment);
        let said = both
            .sentences()
            .iter()
            .filter(|line| line.contains("/elsewhere/docker"))
            .count();
        assert_eq!(said, 1, "{:?}", both.sentences());
    }

    /// SANDBOX-26: a requested toolchain brings its caches, written, which the stage did not have.
    #[test]
    fn a_requested_toolchain_brings_its_caches() {
        let script = step("/bin/sh", &["-c", "cargo build"]);
        let registry = format!("{HOME}/.cargo/registry");
        let base = confinement(&["/work/project"]).with_mode(SandboxMode::Strict);
        assert!(!writes(
            &base.policy(&script, Path::new("/work/project"), &[]),
            &registry
        ));
        let asked = base
            .with_requested(&[Requested::Toolchain(Toolchain::Cargo)])
            .policy(&script, Path::new("/work/project"), &[]);
        assert!(writes(&asked, &registry));
    }

    /// SANDBOX-26: a stage with a `NAME=value` in front of it gets no requested scope, as it gets
    /// no carried one (SANDBOX-16): a value written there can name a file the scope would then
    /// open. The other stage of the same line gets it, so the refusal is per stage.
    #[test]
    fn a_stage_with_an_assignment_gets_no_requested_scope() {
        let mut assigned = step("/bin/sh", &["-c", "aws s3 ls"]);
        assigned.environment = vec![("A".to_string(), "b".to_string())];
        let plain = step("/bin/sh", &["-c", "aws s3 ls"]);
        let asked = confinement(&["/work/project"])
            .with_mode(SandboxMode::Strict)
            .with_requested(&[Requested::Scope(Scope::Aws)]);
        let credentials = format!("{HOME}/.aws");

        assert!(!reads(
            &asked.policy(&assigned, Path::new("/work/project"), &[]),
            &credentials
        ));
        assert!(reads(
            &asked.policy(&plain, Path::new("/work/project"), &[]),
            &credentials
        ));
        assert_eq!(
            asked.requested_for_the_trail(&[&plain, &assigned]),
            Some(
                "the planner asked for aws for this run, added to stage 1 (aws), \
                 stage 2 (nothing)"
                    .to_string()
            )
        );
    }

    /// SANDBOX-26: a requested scope keeps a closed network for its stage, as a carried one does,
    /// a requested toolchain keeps it as one that fetches, and the trail names the stage and the
    /// fixed reason. The control is the same stage with no request, which has no network.
    #[test]
    fn a_requested_scope_or_toolchain_keeps_a_closed_network_for_its_stage() {
        let script = step("/bin/sh", &["-c", "gh pr list"]);
        let other = step("/bin/ls", &[]);
        let closed = confinement(&["/work/project"])
            .with_mode(SandboxMode::Strict)
            .with_network(Network::Closed);
        assert!(!closed.egress(&script));

        let scoped = closed
            .clone()
            .with_requested(&[Requested::Scope(Scope::Remote)]);
        assert!(scoped.egress(&script));
        assert_eq!(
            scoped.network_for_the_trail(&[&script]),
            Some(
                "the network was closed for this run except for stage 1 (a credential scope)"
                    .to_string()
            )
        );

        let built = closed.with_requested(&[Requested::Toolchain(Toolchain::Cargo)]);
        assert!(built.egress(&script));
        assert_eq!(
            built.network_for_the_trail(&[&script, &other]),
            Some(
                "the network was closed for this run except for stage 1 (a toolchain that fetches), \
                 stage 2 (a toolchain that fetches)"
                    .to_string()
            )
        );
    }

    /// SANDBOX-26: names are kept once and in the menu's order whatever order the planner wrote
    /// them, so the prompt and the trail read the same for the same request.
    #[test]
    fn a_request_is_kept_once_in_the_menus_order() {
        let asked = confinement(&["/work/project"]).with_requested(&[
            Requested::Toolchain(Toolchain::Cargo),
            Requested::Scope(Scope::Aws),
            Requested::Toolchain(Toolchain::Cargo),
            Requested::Scope(Scope::Remote),
        ]);
        let names: Vec<_> = asked.requested().iter().map(|r| r.name()).collect();
        assert_eq!(names, ["remote", "aws", "cargo"]);
    }

    /// SANDBOX-26: the prompt says what each stage is lent, and by whom it was asked for.
    #[test]
    fn the_description_names_each_requested_scope_and_the_stage_it_is_for() {
        let script = step("/bin/sh", &["-c", "aws s3 ls"]);
        let described = confinement(&["/work/project"])
            .with_mode(SandboxMode::Strict)
            .with_requested(&[Requested::Scope(Scope::Aws)])
            .describe(&[&script]);
        assert_eq!(
            described.requested,
            [("sh".to_string(), Requested::Scope(Scope::Aws))]
        );
        assert!(
            described
                .sentences()
                .iter()
                .any(|sentence| sentence.contains("aws") && sentence.contains("sh")),
            "{:?}",
            described.sentences()
        );
    }

    /// SANDBOX-19, SANDBOX-26: the failure sentence names the menu in `strict` and in `standard`,
    /// and is built from the confinement and the steps alone, so it is the same words for exit 1
    /// and exit 2. A session that names no home to apply a request under does not name one.
    #[test]
    fn the_failure_sentence_names_the_menu_where_a_request_is_accepted() {
        let script = step("/bin/sh", &["-c", "false"]);
        let strict = confinement(&["/work/project"]).with_mode(SandboxMode::Strict);
        let line = strict.profile(&[&script]);
        for name in Requested::MENU {
            assert!(line.contains(name), "{name} missing from {line}");
        }
        assert_eq!(line, strict.profile(&[&script]));

        let standard = reading_confinement(Prelude::MacOs, &["/work/project"]);
        let said = standard.profile(&[&script]);
        assert!(said.contains("scopes` argument"), "{said}");
        for name in Requested::MENU {
            assert!(said.contains(name), "{name} missing from {said}");
        }
        assert_eq!(said, standard.profile(&[&script]));
    }

    /// SANDBOX-26: the planner is told of the argument and its menu where it will be honoured, in
    /// `strict` and in `standard`, and not in `off`.
    #[test]
    fn the_planner_is_told_of_the_menu_in_strict_and_standard() {
        let told = |mode| stated(true, Some(Prelude::MacOs), Network::Open, mode);
        for mode in [SandboxMode::Strict, SandboxMode::Standard] {
            let said = told(mode).expect("says something");
            assert!(said.contains("`scopes`"), "{said}");
            for name in Requested::MENU {
                assert!(said.contains(name), "{name} missing from {said}");
            }
        }
        let standard = told(SandboxMode::Standard).unwrap();
        assert!(!standard.contains("cannot ask for more"), "{standard}");
        assert_eq!(told(SandboxMode::Off), None);
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

    /// SANDBOX-28: a path the person let programs reach is read, or read and written, by every
    /// stage; the control is the same confinement without it. The person's own deny for a path they
    /// also allowed is still in the policy, so it is not lifted by the grant, and the profile says
    /// how many paths were granted and not which.
    #[test]
    fn a_path_the_person_let_programs_reach_is_held_and_their_own_deny_still_applies() {
        let granted = [
            ("/data/in", false),
            ("/data/out", true),
            ("/data/kept", true),
        ]
        .map(|(path, write)| crate::workspace::PathReach {
            path: PathBuf::from(path),
            write,
            why: "the build needs it".to_string(),
        });
        let plain = reading_confinement(Prelude::Linux, &["/work/project"]);
        let held = plain
            .clone()
            .with_path_reach(&granted)
            .with_filesystem(&listed(&[], &[], &["/data/kept"]));
        let cat = step("/bin/cat", &["file"]);

        let before = plain.policy(&cat, Path::new("/work/project"), &[]);
        let after = held.policy(&cat, Path::new("/work/project"), &[]);

        assert!(!writes(&before, "/data/out") && !reads(&before, "/data/in"));
        assert!(reads(&after, "/data/in") && !writes(&after, "/data/in"));
        assert!(writes(&after, "/data/out") && reads(&after, "/data/out"));
        assert!(
            !writes(&after, "/data/kept"),
            "a grant lifted the person's own deny"
        );
        assert!(reads(&after, "/data/kept"));
        let said = held.profile(&[&cat]);
        assert!(said.contains("3 path"), "{said}");
        assert!(!said.contains("/data/out"), "{said}");
        assert!(!plain.profile(&[&cat]).contains("also let programs reach"));
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

    /// The regression it rejects: the Security framework's cache directory left unwritable on
    /// macOS, which is `gh` and `osxkeychain` failing with the keychain file readable, or the row
    /// reaching a Linux stage or a session that reads only what it was given.
    #[test]
    fn the_security_cache_is_written_by_a_macos_stage_that_reads_the_machine_only() {
        let cache = PathBuf::from("/private/var/folders/ab/cdef/C");
        let written = |prelude: Prelude, home: Option<&Path>, mode: SandboxMode| {
            let mut confined = Confinement::new(
                prelude,
                PathBuf::from("/tmp"),
                home,
                vec![PathBuf::from("/work/project")],
                None,
            )
            .with_mode(mode);
            confined.user_cache = Some(cache.clone());
            confined
                .policy(
                    &step("/usr/bin/gh", &["auth", "status"]),
                    Path::new("/work"),
                    &[],
                )
                .writable
                .iter()
                .any(|row| row.path == cache.join("mds"))
        };
        let home = Path::new("/Users/someone");

        assert!(written(Prelude::MacOs, Some(home), SandboxMode::Standard));
        assert!(!written(Prelude::Linux, Some(home), SandboxMode::Standard));
        assert!(!written(Prelude::MacOs, Some(home), SandboxMode::Strict));
        assert!(!written(Prelude::MacOs, None, SandboxMode::Standard));
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
            workspace: None,
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

    /// SANDBOX-19: every profile line ends with one fixed sentence saying a credential location
    /// cannot be added and a credential scope is the way to reach one. It is the same words in each
    /// mode, on each platform that confines, with or without a home to apply a request under, and
    /// for any step, and it names no mode, so it stays true whichever modes accept a request.
    #[test]
    fn the_profile_line_ends_with_the_fixed_sentence_about_credential_locations() {
        let roots = ["/work/project"];
        let no_home = Confinement::new(
            Prelude::Linux,
            PathBuf::from("/tmp"),
            None,
            roots.iter().map(PathBuf::from).collect(),
            Some(Path::new("/var/scratch")),
        );
        let confinements = [
            ("windows", confinement(&roots)),
            ("strict", confinement(&roots).with_mode(SandboxMode::Strict)),
            (
                "standard linux",
                reading_confinement(Prelude::Linux, &roots),
            ),
            (
                "standard macos",
                reading_confinement(Prelude::MacOs, &roots),
            ),
            (
                "strict macos",
                reading_confinement(Prelude::MacOs, &roots).with_mode(SandboxMode::Strict),
            ),
            ("no home", no_home),
        ];
        let failing = step("/bin/sh", &["-c", "false"]);
        let building = step("/usr/bin/make", &["build"]);

        for (name, confined) in &confinements {
            let lines = [
                confined.profile(&[&failing]),
                confined.profile(&[&building]),
                confined.profile(&[&failing, &building]),
            ];
            for line in lines {
                assert!(
                    line.ends_with(CREDENTIAL_LOCATIONS_SENTENCE),
                    "{name}: {line}"
                );
                assert_eq!(line.matches("cannot be added").count(), 1, "{name}: {line}");
            }
        }

        let sentence = CREDENTIAL_LOCATIONS_SENTENCE;
        for needle in [
            "`~/.ssh`",
            "`~/.aws`",
            "`~/.kube`",
            "`/add-dir`",
            "`--add-dir`",
            "credential scope",
        ] {
            assert!(
                sentence.contains(needle),
                "{needle} missing from {sentence}"
            );
        }
        for mode in ["strict", "standard", "off"] {
            assert!(!sentence.contains(mode), "{mode} named in {sentence}");
        }
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
        assert!(open.profile(&[&make]).contains("Network: open."));
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

    /// What a platform with a confinement of its own is started with: the program and then every
    /// argument, in the order the process receives them.
    #[cfg(unix)]
    fn what_the_process_receives(command: &Command) -> Vec<std::ffi::OsString> {
        std::iter::once(command.get_program())
            .chain(command.get_args())
            .map(OsStr::to_os_string)
            .collect()
    }

    /// RUN-8: the program a confined step is started by is the path the command held, byte for
    /// byte, so a file whose name is not text is not swapped for the file whose name spells the
    /// replacement of its bytes.
    ///
    /// The step is approved as `/usr/bin/git`, reached through a link in a directory whose name
    /// holds an invalid byte, and the checkout holds a second directory named with U+FFFD, which is
    /// what a lossy rendering of the first spells. The regression it rejects is a conversion to
    /// text between the command and the process: the wrapped command then names the second
    /// directory's file and runs it under the profile computed for git.
    #[cfg(unix)]
    #[test]
    fn a_program_whose_path_is_not_text_is_started_by_its_own_bytes() {
        use std::os::unix::ffi::OsStrExt;

        if Prelude::current().is_none() || bravebot_sandbox::for_current_platform().is_err() {
            return;
        }
        let (session, confinement) = a_session("program-path-bytes");
        let by_bytes = session.join(OsStr::from_bytes(b"dir-\xff")).join("runner");
        let lookalike = session.join("dir-\u{FFFD}").join("runner");
        assert_ne!(by_bytes, lookalike);
        let mut git = step("/usr/bin/git", &["--version"]);
        git.started_as = by_bytes.clone();

        let prepared = confinement
            .prepared(&Command::new(&by_bytes), &git, &session)
            .expect("the step is confined");

        assert_eq!(
            prepared.program, by_bytes,
            "the process is not started by the path the command held"
        );
        assert_ne!(
            prepared.program, lookalike,
            "the process is started by the file whose name spells the lossy rendering"
        );

        // A backend that writes its policy as text cannot name the path in it and refuses the
        // step; one that does not starts the program by its bytes. The lookalike is the one
        // answer neither gives.
        if let Ok(wrapped) = confinement.wrap(Command::new(&by_bytes), &git, &session) {
            let received = what_the_process_receives(&wrapped);
            assert!(
                received.iter().any(|part| part == by_bytes.as_os_str()),
                "{received:?}"
            );
            assert!(
                !received.iter().any(|part| part == lookalike.as_os_str()),
                "{received:?}"
            );
        }
    }

    /// The same of an argument, which is carried to the process as the bytes the command held.
    #[cfg(unix)]
    #[test]
    fn an_argument_that_is_not_text_is_carried_by_its_own_bytes() {
        use std::os::unix::ffi::OsStrExt;

        if Prelude::current().is_none() || bravebot_sandbox::for_current_platform().is_err() {
            return;
        }
        let (session, confinement) = a_session("argument-bytes");
        let by_bytes = OsStr::from_bytes(b"dir-\xff");
        let mut command = Command::new("/usr/bin/git");
        command.arg(by_bytes);

        let wrapped = confinement
            .wrap(command, &step("/usr/bin/git", &["status"]), &session)
            .expect("the step is confined");

        let received = what_the_process_receives(&wrapped);
        assert!(
            received.iter().any(|part| part == by_bytes),
            "the argument is not carried as the bytes the command held: {received:?}"
        );
        assert!(
            !received.iter().any(|part| part == "dir-\u{FFFD}"),
            "the argument is carried as its lossy rendering: {received:?}"
        );
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

    fn hosts_listing(allowed: Option<&[&str]>) -> Hosts {
        let entry = |name: &&str| bravebot_config::sandbox_network::HostEntry {
            entry: name.to_string(),
            by: None,
        };
        Hosts {
            allowed: allowed.map(|names| names.iter().map(entry).collect()),
            ..Hosts::default()
        }
    }

    /// A stage the closed setting left no egress is given no proxy to reach, and one that kept it
    /// is; the same stages under a session with no list are given nothing. The regression it
    /// rejects is the proxy variables applied to every stage, which hands a stage with no reason
    /// to reach the network a route there.
    #[test]
    fn only_a_stage_with_egress_is_pointed_at_the_proxy_and_only_under_a_list() {
        let closed = confinement(&["/work/project"]).with_network(Network::Closed);
        let filtered = closed
            .clone()
            .with_hosts(Some(&hosts_listing(Some(&["a.example"]))));
        let kept = step("/usr/bin/curl", &["https://a.example"]);
        let lost = step("/bin/cat", &["a"]);

        assert!(closed.egress(&kept) && !closed.egress(&lost));
        assert!(filtered.host_environment(&lost).unwrap().is_empty());
        let told = filtered.host_environment(&kept).unwrap();
        assert!(
            told.iter().any(|(name, _)| name == "HTTPS_PROXY"),
            "{told:?}"
        );
        assert!(closed.host_environment(&kept).unwrap().is_empty());
        let no_list = closed.with_hosts(Some(&hosts_listing(None)));
        assert!(no_list.host_environment(&kept).unwrap().is_empty());
    }
}
