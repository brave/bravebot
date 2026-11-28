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

use crate::confirm::{Carried, Confined, KeyChoice, Offered, Remembered};
use crate::exec::ExecError;
use crate::reach::{ChosenKey, Grant, Reached};
use crate::scratch::ScratchDirectory;
use bravebot_config::git_identities::Identities;
use bravebot_config::sandbox_network::Hosts;
use bravebot_core::command::Step;
use bravebot_core::permissions::{WrapperQuestion, is_wrapper};
use bravebot_core::policy::Containment;
use bravebot_i18n::t;
use bravebot_sandbox::SandboxMode;
use bravebot_sandbox::Variables;
use bravebot_sandbox::base::{Prelude, base, credential_locations, run_base, with_security_cache};
use bravebot_sandbox::hosts::{HostRule, Refusal, Verdict};
use bravebot_sandbox::network::{Network, program_talks_to_a_remote};
use bravebot_sandbox::policy::SandboxPolicy;
use bravebot_sandbox::programs::program_name;
use bravebot_sandbox::proxy::Proxy;
use bravebot_sandbox::rules::{
    Lists, Rules, persistence_locations, without_verbatim_prefix, writable_without_a_request,
};
use bravebot_sandbox::scope::{
    Reach, Requested, Scope, candidate_public_keys, environment_reach, identity_key, judged_socket,
    refused_identity_files, requested_reach, ssh_address, ssh_command,
};
use bravebot_sandbox::signing;
use bravebot_sandbox::toolchain::Toolchain;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

mod statements;

pub(crate) use statements::LOOPBACK_CONNECT_RULE;
#[cfg(test)]
pub use statements::stated_to_the_planner;
pub use statements::{
    RunStatement, change_notice_for, stated_in_the_description_for, stated_to_the_planner_for,
};
use statements::{
    commit_sentence, credential_locations_sentence, loopback_reach, macos_git_sentence,
    requests_menu_sentence,
};

/// Whether the turn's tool list holds `request_path` (TOOL-6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequestPath {
    Listed,
    Unlisted,
}

/// Whether anybody can be asked in this run (CLI-1). Where nobody can, no sentence names a route
/// only a person takes, such as trusting the workspace or `/add-dir` (SANDBOX-19).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Askable {
    Somebody,
    Nobody,
}

impl Askable {
    /// `Nobody` where `nobody_to_ask` is true, as [`crate::PermissionMode`]'s holder reports it.
    #[must_use]
    pub fn nobody_when(nobody_to_ask: bool) -> Self {
        match nobody_to_ask {
            true => Self::Nobody,
            false => Self::Somebody,
        }
    }
}

/// Whose directory stages are given to write temporary files in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TemporaryOwner {
    /// The account's temporary directory.
    Account,
    /// A directory of the session's own inside its scratch directory ([`stage_temporary`]).
    Session,
}

/// What the proxies decided for one foreground run (SANDBOX-24).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostsSeen {
    /// What the trail records: the decisions by host, entry and stage.
    pub detail: String,
    /// The hosts refused only because no entry covered them, each a valid host name a program
    /// chose, in the order first asked for. Not for the planner.
    pub unlisted: Vec<String>,
}

/// The most hosts one line puts to the person, so a program that tries many names cannot fill a
/// prompt. The rest stay refused and are asked about when a later line meets them.
pub const MOST_HOSTS_ASKED: usize = 8;

/// What a `run` program may reach in this session, before any step is read.
#[derive(Debug, Clone)]
pub struct Confinement {
    prelude: Prelude,
    temporary: PathBuf,
    /// Whether `temporary` is a directory of the session's own inside its scratch directory and not
    /// the account's temporary directory, which is so on Windows where the session has a scratch
    /// directory ([`stage_temporary`]). Then every stage is told it as `TEMP` and `TMP`, and no
    /// stage is granted the account's directory.
    temporary_owner: TemporaryOwner,
    home: Option<PathBuf>,
    /// The state directory in force where `BRAVEBOT_HOME` puts it elsewhere than `~/.bravebot`
    /// ([`Confinement::with_state_directory`]), refused to every stage as `~/.bravebot` is.
    state: Option<PathBuf>,
    roots: Vec<PathBuf>,
    /// The session directories as the session recorded them when it opened or added them
    /// ([`Confinement::with_session_directories`]), empty where the roots came from paths alone.
    /// A directory is a row in `roots` only while its name leads to the directory recorded, and
    /// each stage is refused as it starts once the name leads to a link or a file (SANDBOX-18).
    roots_made: Vec<ScratchDirectory>,
    /// The directories the session opened or added and has since left or closed
    /// ([`Confinement::with_left_directories`]). They are no row of any stage now, but a stage wrote
    /// in them while they were open, and closing one does not remove what was written (SANDBOX-20).
    left: Vec<PathBuf>,
    scratch: Option<PathBuf>,
    /// The directory `scratch` was made as. `scratch` is a row only while its name leads to this
    /// directory, and each stage is refused as it starts once the name leads to a link or a file
    /// (SANDBOX-18). None where the confinement was built from a path alone.
    scratch_made: Option<ScratchDirectory>,
    /// Where the run is, for the `[includeIf]` blocks of the person's git configuration: the
    /// session's first directory, and every directory the session may write.
    place: signing::Place,
    /// The per-user cache directory the platform names, resolved by the host on macOS, where the
    /// keychain lookup of `gh` and `git` writes under it.
    user_cache: Option<PathBuf>,
    /// The developer directory `xcode-select -p` names, resolved as the session opened, which the
    /// keyed base reads on macOS because the `/usr/bin` developer shims run out of it.
    developer: Option<PathBuf>,
    /// What the session decided about the network for the stages it starts.
    network: Network,
    /// The host list the session's stages with egress are held to, or none for no filtering and
    /// no proxy (SANDBOX-24).
    hosts: Option<Hosts>,
    /// How much of the machine a step reads (SANDBOX-22). `Off` never reaches a confinement: a turn
    /// in that mode builds none ([`Confinement::here`] is not asked), so the rows below are the
    /// two modes that confine.
    mode: SandboxMode,
    /// Whether the person asked for the temporary directory to be read and not written by every
    /// stage (`sandbox.holdBackTemporaryDirectory`, SANDBOX-22).
    temporary_held_back: bool,
    /// The reach a person remembered for commands, which attaches to the steps its shape names.
    grants: Vec<Grant>,
    /// The person's own filesystem lists as written ([`Confinement::with_filesystem`]), kept so
    /// that `filesystem` is resolved again when the state directory is set after them.
    lists: Lists,
    /// The person's own filesystem lists, resolved against this session's directories.
    filesystem: Rules,
    /// The paths the person let programs reach for this session, as they were approved
    /// (SANDBOX-28). Judged again each time a policy is built: see [`Confinement::reach_held`].
    path_reach: Vec<crate::workspace::PathReach>,
    /// Whether the session gives the stages it starts every path a `request_path` for writing would
    /// be granted, without the planner asking (SANDBOX-28). Set only for the lead session in bypass.
    unasked_writes: bool,
    /// Directories no stage of this confinement is given a row for, which a delegate in a checkout
    /// holds: the working directory and every other checkout (CHECKOUT-7, SANDBOX-28).
    withheld_writes: Vec<PathBuf>,
    /// The settings file the person named on the command line, as typed made absolute and where it
    /// lands after links. The next launch reads allow rules and sandbox settings from it wherever
    /// it sits, so no stage may write, replace or link it (SANDBOX-14). Held on macOS, where a row
    /// for the directory holding it would otherwise grant it.
    named_settings: Vec<PathBuf>,
    /// The hosts the person let programs reach for this session beyond `allowedHosts`
    /// (SANDBOX-24), added to the allowed set of every stage that is held to a proxy.
    host_grants: Vec<String>,
    /// What the planner asked this one line to add to every stage that has no assignment in front
    /// of it, from the fixed menu. Empty for every line that asked for nothing.
    requested: Vec<Requested>,
    /// Whether the session would accept a `scopes` argument and a `request_path` call, which the
    /// failure sentence names only where it would (SANDBOX-19). Off unless the caller says the
    /// session accepts them, so a sentence is never worded for a route the session refuses. A
    /// confinement exists only where runs are confined in a mode other than `off`, so the cause
    /// left for it to be off is a workspace that is not trusted.
    requests_offered: bool,
    /// Whether the turn's tool list holds `request_path`, which decides whether a failure sentence
    /// names it (TOOL-6). On unless the caller says the list lacks it, so the sentence is the one
    /// written for a turn offered every tool. A `--tools run` turn, a delegate and a definition
    /// whose tools line leaves it out are not offered it.
    request_path: RequestPath,
    /// Whether nobody can be asked in this run, as in a one-shot command or a pipeline (CLI-1).
    /// A failure sentence then names no route only a person can take, such as trusting the
    /// workspace or `/add-dir` (SANDBOX-19). Off unless the caller says so.
    askable: Askable,
    /// The key the person chose for each owner's repositories (`git.identities`), offered by a
    /// `git` stage whose argv names an ssh address of that owner (SANDBOX-16).
    identities: Identities,
    /// The key the person chose at a run prompt for an owner `identities` names nothing for.
    chosen_keys: Vec<ChosenKey>,
    /// The program a test has the platform fail to confine, which no machine's real mechanism does
    /// on demand.
    #[cfg(test)]
    unconfinable: Option<String>,
}

impl Confinement {
    /// Whether a failure sentence names `request_path`, which the turn's tool list decides.
    fn names_request_path(&self) -> bool {
        self.request_path == RequestPath::Listed
    }

    /// This confinement with the session's directories as the session recorded them when it opened
    /// the session on them or added them (SANDBOX-18).
    ///
    /// `directories` replace the paths [`Confinement::here`] was given. A directory is a row only
    /// while its name still leads to the directory recorded: the name is resolved first and the
    /// resolved path compared with the record, so the row is the path that was checked and not a
    /// second reading of a name a stage may have changed in between. A name that now leads to
    /// another directory, or to nothing, gives no row, and one that leads to a link or a file
    /// stops every stage as it starts ([`Confinement::session_linked`]), as the scratch
    /// directory's does. A directory not listed here is given no row.
    ///
    /// The first directory is the working directory. Where it gives no row, the run has no
    /// directory for the `[includeIf]` blocks of the person's git configuration to match, and the
    /// next directory is not taken for it.
    #[must_use]
    pub fn with_session_directories(mut self, directories: &[ScratchDirectory]) -> Self {
        let rows: Vec<Option<PathBuf>> = directories
            .iter()
            .map(|made| {
                let named = canonical(made.path());
                made.stands_as_made_at(&named).then_some(named)
            })
            .collect();
        let standing: Vec<PathBuf> = rows.iter().flatten().cloned().collect();
        self.place = signing::Place {
            directory: rows.first().cloned().flatten(),
            sessions: standing
                .iter()
                .chain(self.scratch.iter())
                .cloned()
                .collect(),
            agent: None,
        };
        self.roots = standing;
        self.roots_made = directories.to_vec();
        self
    }

    /// This confinement with the directories the session has left or closed, which a file is the
    /// plan's under as it is under one still open (SANDBOX-20).
    #[must_use]
    pub fn with_left_directories(mut self, directories: &[PathBuf]) -> Self {
        self.left = directories.iter().map(|left| canonical(left)).collect();
        self
    }

    /// The directories beyond the session's rows under which a file is the plan's (SANDBOX-14,
    /// SANDBOX-16, SANDBOX-20): the checkout holding each session directory, and each directory the
    /// session left with the checkout that held it.
    ///
    /// A file in the `bin` of a checkout, above the directory the session opened in, is as much a
    /// contributor's or a stage's as one inside it, and can be first on `PATH`. The checkout is the
    /// nearest ancestor with a `.git` ([`bravebot_sandbox::programs::with_checkout`]), the finder
    /// that records the directories SANDBOX-30 keeps out of the search for bravebot's own programs.
    /// A checkout that is the home directory or holds it is not counted: it is a dotfiles
    /// repository or the whole machine, and counting it would take the reason from every program
    /// the account installed, `~/.local/bin` and `/opt/homebrew` among them. Read from the file
    /// system as it is asked, so a `.git` made since the session opened is counted.
    fn beyond_the_session(&self) -> Vec<PathBuf> {
        let mut held: Vec<PathBuf> = Vec::new();
        for directory in self.roots.iter().chain(&self.left) {
            for found in bravebot_sandbox::programs::with_checkout(directory) {
                let a_checkout = found != *directory;
                if a_checkout
                    && self
                        .home
                        .as_deref()
                        .is_some_and(|home| home.starts_with(&found))
                {
                    continue;
                }
                let found = canonical(&found);
                if !held.contains(&found) {
                    held.push(found);
                }
            }
        }
        held
    }

    /// The temporary directory, where it is the session's own.
    fn sessions_temporary(&self) -> Option<&Path> {
        (self.temporary_owner == TemporaryOwner::Session).then_some(self.temporary.as_path())
    }

    /// The confinement for a session on this machine, or `None` where the platform has no base to
    /// build one on. Every platform the agent runs on has one.
    ///
    /// `roots` are the directories the person opened the session on, each read and written, as
    /// paths alone. A session that recorded them holds them to the directories recorded with
    /// [`Confinement::with_session_directories`] (SANDBOX-18). `home` is the account's profile
    /// directory, which is what `~` means and what no row reaches.
    ///
    /// `scratch` is the session's own directory as the session made it. It is a row only while its
    /// name still leads to that directory (SANDBOX-18), since `canonical` would turn the row for a
    /// name that has become a link into wherever the link leads, and it is no row where it is or
    /// lies inside a credential location.
    pub fn here(
        roots: Vec<PathBuf>,
        scratch: Option<&ScratchDirectory>,
        home: Option<&Path>,
    ) -> Option<Self> {
        let prelude = Prelude::current()?;
        // Resolved first and compared second, so the row is the path that was checked and not a
        // second reading of a name a stage may have changed in between.
        let row = scratch.and_then(|made| {
            let named = canonical(made.path());
            made.stands_as_made_at(&named).then_some(named)
        });
        let confinement = Self::built(prelude, canonical(&temporary_directory()), home, roots, row);
        let confinement = Self {
            scratch_made: scratch.cloned(),
            ..confinement
        };
        #[cfg(target_os = "macos")]
        let confinement = Self {
            user_cache: bravebot_sandbox::macos::user_cache_directory(),
            developer: developer_directory(),
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
        Self::built(prelude, temporary, home, roots, scratch.map(canonical))
    }

    /// [`Confinement::new`] for a scratch row that is already resolved, so nothing reads the name a
    /// second time. A row that is or lies inside a credential location is left out, since it is a
    /// read and write one and is held to what a session directory is.
    fn built(
        prelude: Prelude,
        temporary: PathBuf,
        home: Option<&Path>,
        roots: Vec<PathBuf>,
        scratch: Option<PathBuf>,
    ) -> Self {
        let roots: Vec<PathBuf> = roots.iter().map(|root| canonical(root)).collect();
        let home = home.map(canonical);
        let scratch = scratch.filter(|row| {
            reaching_credentials(prelude, home.as_deref(), None, std::iter::once(row)).is_none()
        });
        let (temporary, temporary_owner) = stage_temporary(prelude, temporary, scratch.as_deref());
        if temporary_owner == TemporaryOwner::Session {
            // A directory that cannot be made leaves the row naming a path nothing can be written
            // in, which is a stage with no temporary directory and not one with the account's.
            std::fs::create_dir_all(&temporary).ok();
        }
        Self {
            prelude,
            temporary,
            temporary_owner,
            home,
            state: None,
            place: signing::Place {
                directory: roots.first().cloned(),
                sessions: roots.iter().chain(scratch.iter()).cloned().collect(),
                agent: None,
            },
            roots,
            roots_made: Vec::new(),
            left: Vec::new(),
            scratch,
            scratch_made: None,
            user_cache: None,
            developer: None,
            network: Network::Open,
            hosts: None,
            mode: SandboxMode::WorkspaceWrite,
            temporary_held_back: false,
            grants: Vec::new(),
            lists: Lists::default(),
            filesystem: Rules::none(),
            path_reach: Vec::new(),
            unasked_writes: false,
            withheld_writes: Vec::new(),
            named_settings: Vec::new(),
            host_grants: Vec::new(),
            requested: Vec::new(),
            requests_offered: false,
            request_path: RequestPath::Listed,
            askable: Askable::Somebody,
            identities: Identities::default(),
            chosen_keys: Vec::new(),
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

    /// Whether the session's network is closed, as opposed to a line's.
    pub fn network_is_closed(&self) -> bool {
        self.network().is_closed()
    }

    /// This confinement with the network closed, for one line that the person's trust of its
    /// directory let through under an open session (RUN-5, SANDBOX-20).
    ///
    /// The answer was given about a line the sandbox holds, and a line is held only where no stage
    /// keeps the network, so the stages of that line are started the way [`Confinement::holds`]
    /// judged them: with egress only for a reason a stage carries, which none does. The session's
    /// own setting is not changed and the next line is built from it again. Decided from the
    /// session's setting and nothing a program printed.
    pub fn closed_for_the_line(&self) -> Self {
        self.clone().with_network(Network::Closed)
    }

    /// This confinement with the session's host list. A list that is set, even an empty one,
    /// filters every stage that has egress through a proxy; no `allowedHosts` leaves the stages
    /// as they were (SANDBOX-24).
    pub fn with_hosts(mut self, hosts: Option<&Hosts>) -> Self {
        self.hosts = hosts.filter(|hosts| hosts.allowed.is_some()).cloned();
        self
    }

    /// This confinement with the hosts the person let programs reach for the session, added to the
    /// allowed set of the stages a list holds. A host the denied list names stays refused.
    #[must_use]
    pub fn with_host_grants(mut self, granted: &[String]) -> Self {
        self.host_grants = granted.to_vec();
        self
    }

    /// Whether an unlisted host is to be put to the person rather than refused outright, which is
    /// `onUnlisted: ask` under a list that is set.
    pub fn asks_about_unlisted_hosts(&self) -> bool {
        self.hosts.as_ref().is_some_and(|hosts| {
            hosts.on_unlisted == Some(bravebot_config::sandbox_network::OnUnlisted::Ask)
        })
    }

    /// The proxy the stage `step` is held to, or none where it is not filtered.
    ///
    /// Its variables are applied after the person's environment and the step's own assignments, so
    /// an assignment a model wrote cannot point the stage at another proxy, and its port is the
    /// only one the stage's policy reaches by IP. A stage with no egress is given none: it reaches
    /// nothing. Where the proxy cannot be started the stage is refused, since starting it without
    /// the filter is the unfiltered reach the list exists to remove.
    fn host_proxy(&self, step: &Step) -> Result<Option<Arc<Proxy>>, crate::host_proxy::Refused> {
        let Some(hosts) = &self.hosts else {
            return Ok(None);
        };
        if !self.egress(step) {
            return Ok(None);
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
        let remote = self.scope_of(step) == Some(Scope::Remote)
            || requested
                .iter()
                .any(|request| matches!(request, Requested::Scope(Scope::Remote)))
            || self
                .granted(step)
                .any(|grant| matches!(grant.reached, Reached::Scope(Scope::Remote)));
        let mut allowed = spelled(hosts.allowed.as_deref().unwrap_or_default());
        allowed.extend(self.host_grants.iter().cloned());
        let list =
            crate::host_proxy::list_for(&allowed, &spelled(&hosts.denied), remote, &toolchains)?;
        crate::host_proxy::proxy_for(&list)
            .map(Some)
            .map_err(|error| {
                crate::host_proxy::Refused::new(format!(
                    "the allowed-hosts proxy could not start: {error}"
                ))
            })
    }

    /// This confinement with the state directory in force, as [`crate::home::directory`] answers
    /// it, refused to every stage as `~/.bravebot` is, with the read of its `sessions` that
    /// `~/.bravebot` has (SANDBOX-12). The sandbox crate reads no environment, so the caller
    /// passes it. A value that adds nothing (the default, or a directory that holds the home
    /// directory or is a filesystem root) is passed over by
    /// [`bravebot_sandbox::base::moved_state_directory`].
    ///
    /// A scratch row that is or lies inside it is left out, as one inside `~/.bravebot` is.
    #[must_use]
    pub fn with_state_directory(mut self, state: Option<PathBuf>) -> Self {
        self.state = state;
        // The lists refuse an entry at or over the state directory, so they are judged again
        // against this one whichever of the two was given first.
        self.resolve_filesystem();
        self.scratch = self.scratch.take().filter(|row| {
            reaching_credentials(
                self.prelude,
                self.home.as_deref(),
                self.state.as_deref(),
                std::iter::once(row),
            )
            .is_none()
        });
        self
    }

    /// The state directory in force, for the functions of the sandbox crate that refuse it.
    fn state(&self) -> Option<&Path> {
        self.state.as_deref()
    }

    /// This confinement with the person's own filesystem lists (`sandbox.filesystem`), resolved
    /// now against the session's home and its first directory: globs are listed here, so a file a
    /// program makes afterwards is outside what one named, and a link is judged by where it leads.
    ///
    /// Every stage of every line this confinement starts gets them, a stage of a line left running
    /// included, because [`Confinement::policy`] is the one place a stage's rows are made.
    pub fn with_filesystem(mut self, lists: &Lists) -> Self {
        self.lists = lists.clone();
        self.resolve_filesystem();
        self
    }

    /// The lists are kept so that they are resolved again when the state directory is set after
    /// `with_filesystem`. Both setters go through this function, so the order of the builder calls
    /// does not change what is refused.
    fn resolve_filesystem(&mut self) {
        let base = self
            .roots
            .first()
            .cloned()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        self.filesystem = bravebot_sandbox::rules::resolve_for(
            Some(self.prelude),
            &self.lists,
            self.home.as_deref(),
            self.state(),
            &base,
        );
    }

    /// This confinement with the temporary directory written by no stage where `held_back`, which
    /// is read as before (SANDBOX-22). The scratch directory, a person's own `allowWrite` entries
    /// and the grants beneath the temporary directory are rows of their own and stay.
    pub fn with_temporary_held_back(mut self, held_back: bool) -> Self {
        self.temporary_held_back = held_back;
        self
    }

    /// This confinement with the paths the person let programs reach for the session
    /// (SANDBOX-28). Each is judged again whenever a policy is built, so a name that has been
    /// relinked since the approval is given no row.
    pub fn with_path_reach(mut self, reach: &[crate::workspace::PathReach]) -> Self {
        self.path_reach = reach.to_vec();
        self
    }

    /// The paths the person let programs reach whose names still lead to what was approved
    /// (SANDBOX-28). The name is not a link, none of its parents is one, it is the directory or
    /// file that was approved, and it is not at, inside or above a credential location, a
    /// persistence location, `~/.ssh` or the state directory. A name that fails is given no row
    /// and the stage still starts. Decided from file system metadata and the path alone, as each
    /// stage's policy is built.
    fn reach_held(&self) -> Vec<&crate::workspace::PathReach> {
        let home = self.home.as_deref();
        self.path_reach
            .iter()
            .filter(|row| {
                row.stands(home, self.state())
                    && reaching_credentials(
                        self.prelude,
                        home,
                        self.state(),
                        std::iter::once(&row.path),
                    )
                    .is_none()
            })
            .collect()
    }

    /// The paths the person let programs reach, as the mode holds them: under `read-only` a grant
    /// for writing gives its read and not its write, as a new request for it would (SANDBOX-22,
    /// SANDBOX-28). The grant is still held, so a move back gives the write again.
    fn reach_in_force(&self) -> Rules {
        let held = self.reach_held();
        let read_only = self.mode == SandboxMode::ReadOnly;
        let read: Vec<PathBuf> = held
            .iter()
            .filter(|row| read_only || !row.write)
            .map(|row| row.path.clone())
            .collect();
        let write: Vec<PathBuf> = held
            .iter()
            .filter(|row| !read_only && row.write)
            .map(|row| row.path.clone())
            .collect();
        Rules::granted(&read, &write)
    }

    /// This confinement given, where [`Confinement::writes_without_a_request`] allows it, every path
    /// a `request_path` for writing would be granted when a stage starts (SANDBOX-28).
    ///
    /// `bypass` is whether the permission mode answers a request in the person's place and the
    /// session accepts a request at all. A delegate is given them as the lead session is, since it
    /// is lent the mode and is offered no way to ask. A workspace the person has not trusted accepts none (PATHREQ-5), so its
    /// stages are given no row a request would have added. Nothing a program wrote or printed is
    /// among its inputs.
    #[must_use]
    pub fn with_unasked_writes(mut self, bypass: bool) -> Self {
        self.unasked_writes = bypass;
        self
    }

    /// This confinement with no row for `directories`, which the rows
    /// [`Confinement::with_unasked_writes`] gives would otherwise cover. A delegate in a checkout
    /// reaches the working directory and the other checkouts by no path (CHECKOUT-7), and its
    /// programs are given no write there. Taken from the workspace's own record of where the
    /// checkouts are, which no file or program output decides.
    #[must_use]
    pub fn with_withheld_writes(mut self, directories: Vec<PathBuf>) -> Self {
        self.withheld_writes = directories;
        self
    }

    /// This confinement holding the settings file the person named on the command line, under each
    /// of `paths` (the workspace's list, which the file tools read too). On macOS no stage writes
    /// the file wherever a row would otherwise grant it, whether the session was opened on the
    /// directory holding it or `--add-dir` named it, and none renames it, replaces it, links to it
    /// or replaces a directory above it, in every mode and for a `git` stage. A file that is not
    /// there yet is held at the same names.
    #[must_use]
    pub fn with_named_settings(mut self, paths: &[PathBuf]) -> Self {
        self.named_settings = paths.to_vec();
        self
    }

    /// Whether the stages this starts hold every path a request for writing would be granted.
    ///
    /// Only under `workspace-write`, since a person who chose `strict` asked for less reach than the
    /// default and `off` has no profile, and only where the machine is read, which is every
    /// platform with a home directory but Windows, where a grant is an access-control entry written
    /// onto each directory and so cannot be made for the entries of the home directory.
    pub fn writes_without_a_request(&self) -> bool {
        self.unasked_writes && self.mode == SandboxMode::WorkspaceWrite && self.reads_the_machine()
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
        !self.network().is_closed() || self.egress_reason(step).is_some()
    }

    /// The network the stages are held to: the session's decision, except under `read-only`, which
    /// closes it for every stage and gives none a reason to keep it, so that what the mode holds
    /// back is not reachable through a remote (SANDBOX-22).
    fn network(&self) -> Network {
        self.mode.network_in_force(self.network)
    }

    /// Whether `scope` is lent to a stage. The remote scope is not under `read-only`: it is how a
    /// stage pushes, and it writes `~/.ssh/known_hosts` (SANDBOX-22).
    fn lends(&self, scope: Scope) -> bool {
        !(self.mode == SandboxMode::ReadOnly && scope == Scope::Remote)
    }

    /// Whether a stage carrying or asked for `scope` is lent the agent socket the stage starts
    /// with: the remote scope where it is lent, and the signing scope for a person whose git signs
    /// with ssh (SANDBOX-16, SANDBOX-18). The profile's rows and the sentence the planner reads
    /// both ask this, so they cannot disagree.
    fn lends_the_agent(&self, scope: Scope) -> bool {
        self.lends(scope)
            && match scope {
                Scope::Remote => true,
                Scope::Signing => self
                    .home
                    .as_deref()
                    .is_some_and(|home| signing::read(home, self.state(), &self.place).enabled),
                _ => false,
            }
    }

    /// Why a stage keeps the network under a closed setting, in words fixed here so the line a
    /// person reads never carries a name the plan chose.
    fn egress_reason(&self, step: &Step) -> Option<&'static str> {
        // `network()` already closes it, but `egress()` and `network_for_the_trail` would hand a
        // read-only stage the network back through a reason (SANDBOX-22).
        if self.mode == SandboxMode::ReadOnly {
            return None;
        }
        // Every reason below is read from the file name, so a file the plan could have written
        // under a directory it may write to must not earn the network by being named `curl`.
        if self.writable_by_the_plan(&step.resolved) {
            return None;
        }
        if Toolchain::of(&step.resolved).is_some_and(|toolchain| toolchain.fetches(&step.resolved))
            || self
                .requested_for(step)
                .iter()
                .any(|request| {
                    matches!(request, Requested::Toolchain(toolchain) if toolchain.lends_the_network())
                })
        {
            Some("a toolchain that fetches")
        } else if self.scope_of(step).is_some()
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

    /// Whether `file` is under a directory the plan may write to, so that its name is one the plan
    /// could have chosen (SANDBOX-16, SANDBOX-20): the session's directories, the checkout holding
    /// each of them and each one the session left ([`Confinement::beyond_the_session`]), the
    /// scratch and temporary directories, and every row some stage of this session may be given
    /// for writing, whatever its program. A file another stage could have written is as much the
    /// plan's as one this stage could.
    fn writable_by_the_plan(&self, file: &Path) -> bool {
        let file = canonical(file);
        if self
            .roots
            .iter()
            .chain(self.scratch.iter())
            .any(|dir| file.starts_with(dir))
            || file.starts_with(&self.temporary)
            || file.starts_with(canonical(&self.temporary))
            || self
                .beyond_the_session()
                .iter()
                .any(|dir| file.starts_with(dir))
        {
            return true;
        }
        let policy = self.written_by_some_stage();
        let rows: Vec<PathBuf> = policy
            .writable
            .iter()
            .map(|row| canonical(&row.path))
            .collect();
        // A read refusal inside a write row is a write refusal too (SANDBOX-12, SANDBOX-18).
        let refused: Vec<PathBuf> = policy
            .unwritable
            .iter()
            .chain(&policy.unreadable)
            .map(|row| canonical(row))
            .collect();
        written_under(&rows, &refused, &file)
    }

    /// The write rows [`Confinement::composed`] can give some stage of this session from anything
    /// but its step, with the person's refusals.
    ///
    /// Every toolchain's caches are among them in every mode, since a stage may carry or ask for
    /// any toolchain's list, and a file one left there outlives a switch to `read-only`. A scope's
    /// write rows are not: they are the file `~/.ssh/known_hosts` and the agent socket the person's
    /// environment names, and the plan chose neither name. Nor are the rows bypass gives without a
    /// request (SANDBOX-28), which are every path the account can write, so counting them would
    /// take its reason from every program the account installed (SANDBOX-20).
    fn written_by_some_stage(&self) -> SandboxPolicy {
        let mut policy = self.base_policy();
        if let Some(home) = self.home.as_deref() {
            policy = Toolchain::grant_every_cache(policy, self.prelude, home, self.state());
            for grant in self.grants.iter().filter(|grant| grant.write) {
                if let Some(path) = grant.directory(home, self.state()) {
                    policy = policy.allow_write(path);
                }
            }
        }
        let write_reach: Vec<PathBuf> = self
            .reach_held()
            .iter()
            .filter(|row| row.write)
            .map(|row| row.path.clone())
            .collect();
        self.filesystem
            .apply(Rules::granted(&[], &write_reach).apply(policy))
    }

    /// Whether every stage of `steps`, started in `directory`, is held to a sandbox that leaves a
    /// plain line nothing a person would be asked about, which is what lets a person's trust of
    /// the directory answer for it (RUN-5).
    ///
    /// All of these hold, and each is read from the compiled steps and the session's own setting,
    /// never from what a program printed:
    ///
    /// - the mode confines, and the platform holds a `.git` back from every stage (SANDBOX-14),
    ///   which Linux and Windows do not, so no line there is held;
    /// - the network is closed for the session and no stage keeps it (SANDBOX-20). Asked of each
    ///   stage through [`Confinement::egress`], which is true for every stage while the network is
    ///   open;
    /// - no stage is git, whose profile lifts the hold-back, and none writes a `.git`;
    /// - no stage is lent anything a person remembered for it;
    /// - no stage starts a file under a directory the plan may write to, and no stage is handed
    ///   such a file as an argument where it is an executable file or the stage is an interpreter or
    ///   wrapper. A program a stage wrote is a line the person never saw, and under this answer the
    ///   next line would not ask either.
    ///
    /// An interpreter is known by the file name its program resolved to, so a script run by a
    /// program this list does not know and that is not executable, or through a module name, still
    /// runs unasked. It is held to the same sandbox as the line that wrote it, which is what this
    /// answer relies on.
    pub fn holds(&self, steps: &[&Step], directory: &Path) -> Containment {
        let held = !matches!(self.mode, SandboxMode::Off)
            && self.prelude == Prelude::MacOs
            && !steps.is_empty();
        if !held {
            return Containment::Unknown;
        }
        let environment = process_environment();
        let every_stage = steps.iter().all(|step| {
            !self.egress(step)
                && self.granted(step).next().is_none()
                && !self.writable_by_the_plan(&step.resolved)
                && !self.writable_by_the_plan(&step.started_as)
                && !self.composed(step, &environment).0.git_directories_writable
                && !self.hands_over_a_file_the_plan_could_write(step, directory)
        });
        match every_stage {
            true => Containment::Held,
            false => Containment::Unknown,
        }
    }

    /// Whether the profile of any stage of `steps` lifts the `.git` hold-back, which is the profile
    /// of a stage whose program is git (SANDBOX-18). Such a stage writes a repository's hooks and
    /// configuration, so the sandbox does not hold what the line does to the words it was shown,
    /// and the auto mode's classifier is not put the line (AUTO-3).
    ///
    /// Asked of the profile each stage is built with, the fact [`Confinement::holds`] reads for the
    /// same stage, on every platform: git is found by the file its program resolved to, and never
    /// by a name in the line or by what a program printed. The answer is the same on macOS, where
    /// the hold-back exists, and on Linux and Windows, where it does not and a stage that is not git
    /// may write a `.git` as well (SANDBOX-14).
    pub fn lifts_the_git_hold_back(&self, steps: &[&Step]) -> bool {
        let environment = process_environment();
        steps
            .iter()
            .any(|step| self.composed(step, &environment).0.git_directories_writable)
    }

    /// Whether an argument of `step`, or one of the words in it, names a regular file under a
    /// directory the plan may write to, where the step is an interpreter or a wrapper, or the file
    /// is executable.
    ///
    /// An argument is looked up whole, since a file name may hold a space or any other character
    /// the words are cut at, and as the words in it ([`any_name_in`]), so `sh -c "./gen.sh a"` and
    /// `node --require=./hook.js` name the file they run. A name is looked up as spelled, under
    /// `directory` where it is relative. That reads whether a file exists, what kind it is and
    /// whether it is executable, never what is in it, and a stage that creates or removes it can
    /// only change whether this asks about the line that names it.
    fn hands_over_a_file_the_plan_could_write(&self, step: &Step, directory: &Path) -> bool {
        let interpreter =
            program_name(&step.resolved).is_some_and(|name| runs_what_it_is_given(&name));
        step.args.iter().any(|arg| {
            any_name_in(arg, |name| {
                let file = directory.join(name);
                file.is_file()
                    && (interpreter || is_executable(&file))
                    && self.writable_by_the_plan(&file)
            })
        })
    }

    /// What the trail records about the network for the stages of one run, or `None` where the
    /// session left it open and there is nothing to say.
    ///
    /// Each stage that keeps it is named by its place in the line and one of the fixed reasons,
    /// never by a program name or an argument the plan chose.
    pub fn network_for_the_trail(&self, steps: &[&Step]) -> Option<String> {
        if !self.network().is_closed() {
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

    /// What the trail records of the hosts the programs of one foreground run asked for: what the
    /// host list decided and by which entry, from the proxies that serve its stages, and the hosts
    /// it refused for want of an entry. `None` where the session has no list or nothing was asked
    /// for.
    ///
    /// Takes the decisions, so each is recorded once. A proxy is shared by every stage with the
    /// same list, so a decision is named by the stages that proxy serves in this line, and one a
    /// job left running took is recorded with the next line that shares its proxy. A host a
    /// program asked for is carried as [`bravebot_sandbox::hosts::recordable`] allows and by no
    /// other bytes; the entries are the person's own and the defaults.
    pub fn hosts_seen(&self, steps: &[&Step]) -> Option<HostsSeen> {
        let mut served: Vec<(Arc<Proxy>, Vec<usize>)> = Vec::new();
        for (at, step) in steps.iter().enumerate() {
            let Ok(Some(proxy)) = self.host_proxy(step) else {
                continue;
            };
            match served
                .iter_mut()
                .find(|(known, _)| Arc::ptr_eq(known, &proxy))
            {
                Some((_, stages)) => stages.push(at + 1),
                None => served.push((proxy, vec![at + 1])),
            }
        }
        let mut unlisted: Vec<String> = Vec::new();
        let said: Vec<String> = served
            .iter()
            .filter_map(|(proxy, stages)| {
                let taken = proxy.take_decisions();
                if taken.decisions.is_empty() && taken.dropped == 0 {
                    return None;
                }
                for decision in &taken.decisions {
                    let Some(host) = &decision.host else { continue };
                    if decision.verdict == Verdict::Refused(Refusal::NotListed)
                        && HostRule::parse(host).is_some()
                        && !unlisted.contains(host)
                    {
                        unlisted.push(host.clone());
                    }
                }
                let mut counted: Vec<(String, usize)> = Vec::new();
                for decision in &taken.decisions {
                    let said = format!(
                        "{} {}",
                        decision
                            .host
                            .as_deref()
                            .unwrap_or("a name that is not a host name"),
                        match &decision.verdict {
                            Verdict::Allowed(rule) => format!("allowed by {}", rule.spelling()),
                            Verdict::Refused(Refusal::Denied(rule)) =>
                                format!("refused, denied by {}", rule.spelling()),
                            Verdict::Refused(Refusal::NotListed) => "refused, not listed".into(),
                            Verdict::Refused(Refusal::Port) => "refused, port not carried".into(),
                            Verdict::Refused(Refusal::NotPublic) => "refused, not public".into(),
                        }
                    );
                    match counted.iter_mut().find(|(known, _)| *known == said) {
                        Some((_, times)) => *times += 1,
                        None => counted.push((said, 1)),
                    }
                }
                let mut parts: Vec<String> = counted
                    .into_iter()
                    .map(|(said, times)| match times {
                        1 => said,
                        times => format!("{said} x{times}"),
                    })
                    .collect();
                if taken.dropped > 0 {
                    parts.push(format!("{} more not kept", taken.dropped));
                }
                Some(format!(
                    "the host list decided for the proxy of {}: {}",
                    stages
                        .iter()
                        .map(|stage| format!("stage {stage}"))
                        .collect::<Vec<_>>()
                        .join(", "),
                    parts.join("; ")
                ))
            })
            .collect();
        (!said.is_empty()).then(|| HostsSeen {
            detail: said.join("; "),
            unlisted,
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

    /// This confinement with whether the session would accept a `scopes` argument and a
    /// `request_path` call, which decides whether the failure sentence names either (SANDBOX-19).
    #[must_use]
    pub fn with_requests_offered(mut self, offered: bool) -> Self {
        self.requests_offered = offered;
        self
    }

    /// This confinement with whether the turn's tool list holds `request_path`, which decides
    /// whether a failure sentence names it (TOOL-6).
    #[must_use]
    pub fn with_request_path_listed(mut self, listed: bool) -> Self {
        self.request_path = if listed {
            RequestPath::Listed
        } else {
            RequestPath::Unlisted
        };
        self
    }

    /// This confinement with whether nobody can be asked in this run (CLI-1), which decides
    /// whether a failure sentence names a route only a person can take (SANDBOX-19).
    #[must_use]
    pub fn with_nobody_to_ask(mut self, nobody: bool) -> Self {
        self.askable = Askable::nobody_when(nobody);
        self
    }

    /// This confinement with the key the person chose for each owner's repositories, from the
    /// person's own settings (SANDBOX-16).
    #[must_use]
    pub fn with_identities(mut self, identities: Option<&Identities>) -> Self {
        self.identities = identities.cloned().unwrap_or_default();
        self
    }

    /// This confinement with the keys the person chose at earlier run prompts, from the record of
    /// reach (SANDBOX-16).
    #[must_use]
    pub fn with_chosen_keys(mut self, chosen: Vec<ChosenKey>) -> Self {
        self.chosen_keys = chosen;
        self
    }

    /// This confinement with `chosen` in place of whatever was chosen before for its host and
    /// owner, for the line the person chose it at.
    #[must_use]
    pub fn with_chosen_key(mut self, chosen: ChosenKey) -> Self {
        self.chosen_keys
            .retain(|held| !held.address.same_as(&chosen.address));
        self.chosen_keys.push(chosen);
        self
    }

    /// What `git.identities`, or failing an entry there a choice made at an earlier run prompt,
    /// decides for `step`, or `None` for a step that is not `git` carrying the remote scope by its
    /// argv.
    ///
    /// The address is read from the argv, which the planner wrote and the person approved, and
    /// never from the repository's configuration, which a clone or a stage may have written. An
    /// entry in the settings decides wherever there is one, a key it names that is refused
    /// included. A key is offered only where it is a public key file in the person's home outside
    /// every directory the plan may write ([`Confinement::offerable`]). A chosen key must also
    /// still be one ssh could offer, as the prompt that numbered it judged ([`candidate_public_keys`]),
    /// so that a choice lends no file the remote scope does not read; one that is no longer leaves
    /// the owner unchosen, and the prompt asks again.
    fn identified(&self, step: &Step) -> Option<Identified> {
        let home = self.home.as_deref()?;
        let is_git = program_name(&step.resolved).as_deref() == Some("git");
        if !is_git || self.carries(step).1 != Some(Scope::Remote) {
            return None;
        }
        let Some(address) = ssh_address(&step.args) else {
            return Some(Identified::NoAddress);
        };
        if let Some(entry) = self.identities.for_address(&address) {
            return Some(match self.offerable(&entry.key, home) {
                Some((command, key)) => Identified::Key { key, command },
                None => Identified::Unchosen,
            });
        }
        let Some(held) = self
            .chosen_keys
            .iter()
            .find(|held| held.address.same_as(&address))
        else {
            return Some(Identified::Unchosen);
        };
        let Some(written) = &held.key else {
            return Some(Identified::LeftToSsh(held.clone()));
        };
        Some(
            match written
                .to_str()
                .and_then(|written| self.offerable(written, home))
                .filter(|(_, key)| candidate_public_keys(home, self.state()).contains(key))
            {
                Some((command, key)) => Identified::Chosen {
                    chosen: ChosenKey {
                        key: Some(key),
                        ..held.clone()
                    },
                    command,
                },
                None => Identified::Unchosen,
            },
        )
    }

    /// The key `written` names, with the `GIT_SSH_COMMAND` that offers it, where a stage may be
    /// given it: a public key file in the person's home ([`identity_key`]) outside every directory
    /// the plan may write, since a key there would be one the plan chose.
    fn offerable(&self, written: &str, home: &Path) -> Option<(String, PathBuf)> {
        identity_key(written, home, self.state())
            .filter(|key| !self.writable_by_the_plan(key))
            .and_then(|key| Some((ssh_command(&key)?, key)))
    }

    /// The key a run prompt may ask the person to choose for `steps`, written down at `record`, or
    /// `None` where it asks nothing (SANDBOX-16).
    ///
    /// Asked where every `git` step of the line that carries the remote scope by its argv and is
    /// left unchosen, or was chosen at an earlier prompt and the planner asked for `ssh-key`, names
    /// one host and owner, no `git.identities` entry names that owner, and ssh
    /// could offer between two and [`KEY_CHOICES`] public keys: those at its default names and those
    /// an `IdentityFile` line of the person's ssh configuration names, judged as an entry's key is.
    /// The candidates come from the person's own files and the address from the argv, so nothing a
    /// program or a repository wrote is among the inputs.
    pub fn key_choice(&self, steps: &[&Step], record: &Path) -> Option<KeyChoice> {
        let asked_again = self.requested.contains(&Requested::SshKey);
        let (address, candidates) = self.open_key_choice(steps, asked_again)?;
        Some(KeyChoice {
            address,
            candidates,
            record: record.to_path_buf(),
        })
    }

    /// The owner [`Confinement::key_choice`] asks about and the keys it numbers, where `asked_again`
    /// says the planner's request names `ssh-key`. The failure sentence reads it with `asked_again`
    /// set, since that is the retry it points the planner at, so it promises a prompt only where
    /// the prompt would be shown.
    fn open_key_choice(
        &self,
        steps: &[&Step],
        asked_again: bool,
    ) -> Option<(bravebot_sandbox::scope::Address, Vec<PathBuf>)> {
        let home = self.home.as_deref()?;
        let mut asked: Option<bravebot_sandbox::scope::Address> = None;
        for step in steps {
            let open = match self.identified(step) {
                Some(Identified::Unchosen) => true,
                Some(Identified::Chosen { .. } | Identified::LeftToSsh(_)) => asked_again,
                _ => false,
            };
            if !open {
                continue;
            }
            let address = ssh_address(&step.args)?;
            if self.identities.for_address(&address).is_some() {
                continue;
            }
            match &asked {
                Some(held) if !held.same_as(&address) => return None,
                Some(_) => {}
                None => asked = Some(address),
            }
        }
        let address = asked?;
        let candidates = self.offerable_candidates(home);
        (2..=KEY_CHOICES)
            .contains(&candidates.len())
            .then_some((address, candidates))
    }

    /// The public keys ssh could offer that a stage may be given ([`Confinement::offerable`]).
    fn offerable_candidates(&self, home: &Path) -> Vec<PathBuf> {
        candidate_public_keys(home, self.state())
            .into_iter()
            .filter(|key| {
                key.to_str()
                    .is_some_and(|key| self.offerable(key, home).is_some())
            })
            .collect()
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
        match self.home.is_some() && !step.carries_an_assignment() {
            true => &self.requested,
            false => &[],
        }
    }

    /// Whether the planner asked for a name and [`Confinement::requested_for`] gives it to no
    /// stage of `steps`, because every stage has an assignment in front of its program
    /// (SANDBOX-26). Read from the compiled steps and the menu words alone. `false` where nothing
    /// was asked for and where the session names no home, which refuses a request for that reason.
    pub fn request_reaches_no_stage(&self, steps: &[&Step]) -> bool {
        !self.requested.is_empty()
            && self.home.is_some()
            && !steps.is_empty()
            && steps.iter().all(|step| self.requested_for(step).is_empty())
    }

    /// Whether the planner asked for a name and a stage of `steps` is given none of it because an
    /// assignment is in front of its program (SANDBOX-26). Read from the compiled steps and the
    /// menu words alone, for the failure sentence of a line that ran with the request.
    fn request_skips_a_stage(&self, steps: &[&Step]) -> bool {
        !self.requested.is_empty()
            && self.home.is_some()
            && steps.iter().any(|step| self.requested_for(step).is_empty())
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

    /// The credential scopes a person's remembered grants attach to the stages of `steps`, each
    /// once, in the order a stage first has one.
    ///
    /// A scope is listed only where [`Confinement::granted`] attaches its grant to a stage and the
    /// mode lends it ([`Confinement::lends`]), so a
    /// grant for another program, operation word or checkout, one for a stage with an assignment in
    /// front of it, and a global grant whose argv the scope table refuses add nothing. A directory a
    /// person named with `/reach <dir>` is not a credential scope and is not listed: it stays on
    /// the road [`Confinement::holds`] guards.
    ///
    /// Read from the grant record and the compiled steps, never from what a program printed. It
    /// does not change [`Confinement::holds`], which refuses a stage lent any grant.
    pub fn remembered_scopes(&self, steps: &[&Step]) -> Vec<Scope> {
        let mut scopes: Vec<Scope> = Vec::new();
        for step in steps {
            for scope in self.kept_on(step) {
                if !scopes.contains(&scope) {
                    scopes.push(scope);
                }
            }
        }
        scopes
    }

    /// The credential scopes the remembered grants lend `step`, each once, in the record's order.
    /// The one place that says which grants count, so the prompt, the policy and the trail agree.
    fn kept_on(&self, step: &Step) -> Vec<Scope> {
        let mut scopes: Vec<Scope> = Vec::new();
        for grant in self.granted(step) {
            if let Reached::Scope(scope) = grant.reached
                && self.lends(scope)
                && !scopes.contains(&scope)
            {
                scopes.push(scope);
            }
        }
        scopes
    }

    /// What the trail records of the scopes a person kept at an earlier prompt that a stage of this
    /// line is lent: each stage by its place in the line and the names lent to it, which are menu
    /// words and nothing the plan chose. `None` where no stage is lent one. Said apart from
    /// [`Confinement::requested_for_the_trail`] because the planner did not ask for these.
    pub fn kept_for_the_trail(&self, steps: &[&Step]) -> Option<String> {
        let kept = self.remembered_scopes(steps);
        if kept.is_empty() {
            return None;
        }
        let named: Vec<String> = steps
            .iter()
            .enumerate()
            .map(|(at, step)| {
                let lent = self.kept_on(step);
                match lent.is_empty() {
                    true => format!("stage {} (nothing)", at + 1),
                    false => format!(
                        "stage {} ({})",
                        at + 1,
                        lent.iter()
                            .map(|scope| scope.name())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                }
            })
            .collect();
        Some(format!(
            "a person kept {} at an earlier prompt, lent to {}",
            kept.iter()
                .map(|scope| scope.name())
                .collect::<Vec<_>>()
                .join(", "),
            named.join(", ")
        ))
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
            self.scope_of(step),
        )
    }

    /// The scope the argv of `step` names. A stage that signs carries one only where the person's
    /// own git configuration signs with ssh, so an account that does not sign sees no change, the
    /// network under a closed setting included.
    fn scope_of(&self, step: &Step) -> Option<Scope> {
        // A scope is chosen by the file name, so a file the plan could have written must not
        // earn `~/.aws` by being named `aws`.
        if self.writable_by_the_plan(&step.resolved) {
            return None;
        }
        // An assignment whose value is worked out at spawn is unknown here, so it carries no scope
        // for the reason a literal one does not.
        if !step.expansions.is_empty() {
            return None;
        }
        Scope::of(&step.resolved, &step.args, &step.environment).filter(|scope| {
            self.lends(*scope)
                && (*scope != Scope::Signing
                    || self
                        .home
                        .as_deref()
                        .is_some_and(|home| signing::read(home, self.state(), &self.place).enabled))
        })
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
            network: self.network(),
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
                                Some(requested_reach(*scope, home, self.state(), environment))
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
                    let network = self.network().is_closed() && self.egress(step);
                    let remembered: Vec<Remembered> = self
                        .granted(step)
                        .filter(|grant| match (&grant.reached, self.home.as_deref()) {
                            (Reached::Directory(_), Some(home)) => {
                                grant.directory(home, self.state()).is_some()
                            }
                            // A scope the mode does not lend is not on the stage, so it is not
                            // drawn as a row and does not count as one the line carries.
                            (Reached::Scope(scope), _) => self.lends(*scope),
                            _ => true,
                        })
                        .map(|grant| Remembered {
                            reached: match (&grant.reached, self.home.as_deref()) {
                                (Reached::Directory(_), Some(home)) => grant
                                    .directory(home, self.state())
                                    .map_or(grant.reached.clone(), Reached::Directory),
                                _ => grant.reached.clone(),
                            },
                            write: grant.write,
                            allowed: grant.allowed.clone(),
                        })
                        .collect();
                    let identity = match self.identified(step) {
                        Some(Identified::Key { key, .. }) => Some(Offered::Configured(key)),
                        Some(Identified::Chosen { chosen, .. } | Identified::LeftToSsh(chosen)) => {
                            Some(Offered::Chosen(chosen))
                        }
                        _ => None,
                    };
                    (toolchain.is_some() || scope.is_some() || network || !remembered.is_empty())
                        .then(|| Carried {
                            program: step.program.clone(),
                            toolchain,
                            scope,
                            reaches,
                            identity,
                            network,
                            remembered,
                        })
                })
                .collect(),
            writes_git_directories: self.writes_git_directories(steps, environment),
        }
    }

    /// Whether a stage of `steps` can write a `.git` on a platform that holds none back: its
    /// program is not git by the decision its profile is built with, and the profile writes beyond
    /// the temporary directory and the null device the base gives every stage (SANDBOX-18).
    fn writes_git_directories(&self, steps: &[&Step], environment: &[(String, String)]) -> bool {
        if self.prelude == Prelude::MacOs {
            return false;
        }
        let base = self.base_policy();
        steps.iter().any(|step| {
            let (policy, _) = self.composed(step, environment);
            !policy.git_directories_writable
                && policy
                    .writable
                    .iter()
                    .any(|row| !base.writable.iter().any(|kept| kept.path == row.path))
        })
    }

    /// Where `environment` moves the scope the step's own argv names. The process's own
    /// environment is what the executor starts a step with, and a step with an assignment in front
    /// of it carries no scope to move.
    fn stage_reaches(&self, step: &Step, environment: &[(String, String)]) -> Vec<Reach> {
        match (self.carries(step).1, self.home.as_deref()) {
            (Some(scope), Some(home)) => {
                reaches(&step.resolved, scope, home, self.state(), environment)
            }
            _ => Vec::new(),
        }
    }

    /// What `toolchain` adds to a stage: its list, and its caches written in every mode but
    /// `read-only`, which writes nothing a later program reads (SANDBOX-22).
    fn toolchain_rows(
        &self,
        toolchain: Toolchain,
        policy: SandboxPolicy,
        home: &Path,
    ) -> SandboxPolicy {
        match self.mode {
            SandboxMode::ReadOnly => toolchain.grant_reads(policy, home, self.state()),
            _ => toolchain.grant(policy, self.prelude, home, self.state()),
        }
    }

    /// `policy` with a read row for each of `reaches`.
    fn allow_reaches(
        mut policy: SandboxPolicy,
        reaches: impl IntoIterator<Item = Reach>,
    ) -> SandboxPolicy {
        for reach in reaches {
            policy = policy.allow_read(reach.path);
        }
        policy
    }

    /// What a requested `scope` adds to a stage: its grant, the places it reads where the stage
    /// does not read the machine, and the agent where the argv that signs would lend it.
    fn lend_requested_scope(
        &self,
        policy: SandboxPolicy,
        scope: Scope,
        home: &Path,
        place: &signing::Place,
        socket: &Option<PathBuf>,
        environment: &[(String, String)],
    ) -> SandboxPolicy {
        let mut policy = scope.grant(policy, home, self.state(), place);
        if !self.reads_the_machine() {
            policy = Self::allow_reaches(
                policy,
                requested_reach(scope, home, self.state(), environment),
            );
        }
        // A request lends the agent where the argv that signs would: only to a
        // person whose git signs with ssh.
        match socket.clone() {
            Some(socket) if self.lends_the_agent(scope) => policy.allow_write(socket),
            _ => policy,
        }
    }

    /// What the requests the plan carries for `step` add to a stage.
    fn grant_requested(
        &self,
        step: &Step,
        mut policy: SandboxPolicy,
        home: &Path,
        place: &signing::Place,
        socket: &Option<PathBuf>,
        environment: &[(String, String)],
    ) -> SandboxPolicy {
        // The places a variable names are those of every program the scope has, since a script
        // wrapping `gh` is not `gh` and there is no program to key them on.
        for request in self.requested_for(step) {
            policy = match *request {
                Requested::Scope(scope) if !self.lends(scope) => policy,
                Requested::Loopback => policy.allow_loopback(),
                // Lends nothing: it only has the prompt number the keys again.
                Requested::SshKey => policy,
                Requested::Toolchain(toolchain) => self.toolchain_rows(toolchain, policy, home),
                Requested::Scope(scope) => {
                    self.lend_requested_scope(policy, scope, home, place, socket, environment)
                }
            };
        }
        policy
    }

    /// What the reaches a person remembered for `step` add to a stage.
    fn grant_remembered(
        &self,
        step: &Step,
        mut policy: SandboxPolicy,
        home: &Path,
        place: &signing::Place,
    ) -> SandboxPolicy {
        for grant in self.granted(step) {
            policy = match &grant.reached {
                Reached::Scope(scope) if !self.lends(*scope) => policy,
                Reached::Scope(scope) => scope.grant(policy, home, self.state(), place),
                Reached::Directory(_) => match grant.directory(home, self.state()) {
                    Some(path) if grant.write => policy.allow_read(&path).allow_write(path),
                    Some(path) => policy.allow_read(path),
                    None => policy,
                },
            };
        }
        policy
    }

    /// `policy` with a write refusal for each read refusal inside one of its writable rows, where
    /// a row was given without a request (`unasked_is_empty` is false) or on macOS, and for each
    /// place under the home directory that runs code later, on macOS and Linux.
    fn deny_writes_beneath_read_refusals(
        &self,
        mut policy: SandboxPolicy,
        unasked_is_empty: bool,
    ) -> SandboxPolicy {
        // Seatbelt's refusal of a read does not stop a write, so a read refusal inside a writable
        // row is a write refusal too: a row given without a request, or on macOS any row, a
        // session directory at or above the home among them (SANDBOX-12, SANDBOX-18). The places
        // a write would run code later, or loosen this profile, are refused beside them, on Linux
        // as on macOS: a session on the home directory or above it holds a row over them, and a
        // launcher the person starts later reads what a stage wrote there (SANDBOX-30).
        let reads_refused_for_writes = !unasked_is_empty || self.prelude == Prelude::MacOs;
        let refuses_persistence = matches!(self.prelude, Prelude::MacOs | Prelude::Linux);
        if reads_refused_for_writes || refuses_persistence {
            let mut refused = if reads_refused_for_writes {
                policy.unreadable.clone()
            } else {
                Vec::new()
            };
            if let (true, Some(home)) = (refuses_persistence, self.home.as_deref()) {
                refused.extend(persistence_locations(home));
            }
            for refused in refused {
                let inside = policy
                    .writable
                    .iter()
                    .any(|row| refused.starts_with(&row.path) && row.path != refused);
                if inside && !policy.unwritable.contains(&refused) {
                    policy = policy.deny_write(refused);
                }
            }
        }
        policy
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
        let (policy, machine) = self.composed(step, environment);
        // A program bravebot starts itself must not be found in a place this stage can write. A
        // row that came with the whole machine is such a place only where the account can write.
        for row in &policy.writable {
            match machine.contains(&row.path) {
                true => bravebot_sandbox::programs::keep_out_where_writable(&row.path),
                false => bravebot_sandbox::programs::keep_out(&row.path),
            }
        }
        policy.starting_in(directory)
    }

    /// The base every stage of this confinement starts from, before anything its step brings.
    fn base_policy(&self) -> SandboxPolicy {
        let mut policy = if self.reads_the_machine() {
            run_base(
                self.prelude,
                &self.temporary,
                self.home.as_deref(),
                self.state(),
            )
        } else {
            base(
                self.prelude,
                &self.temporary,
                self.developer.as_deref(),
                self.home.as_deref(),
            )
        };
        if self.prelude == Prelude::MacOs
            && self.reads_the_machine()
            && let Some(cache) = &self.user_cache
        {
            policy = with_security_cache(policy, cache);
        }
        // Where the temporary directory is the session's own there is no row of the account's to
        // drop, and the session's stays like the scratch directory's.
        if self.temporary_held_back && self.temporary_owner == TemporaryOwner::Account {
            policy.writable.retain(|row| row.path != self.temporary);
        }
        policy
    }

    /// [`Confinement::policy`] before it is given a directory to start in, with the rows of it
    /// that came with the whole machine and no other grant. It records nothing, so a prompt can
    /// read it without the stage having run.
    fn composed(
        &self,
        step: &Step,
        environment: &[(String, String)],
    ) -> (SandboxPolicy, Vec<PathBuf>) {
        let Step {
            program: _,
            resolved,
            started_as,
            args: _,
            matched: _,
            environment: _,
            expansions: _,
            routes: _,
        } = step;
        let mut policy = self.base_policy();
        if !self.egress(step) {
            policy = policy.without_network_egress();
        }

        if let Some(home) = self.home.as_deref() {
            // The caches are written, which `read-only` does not do (SANDBOX-22).
            if self.reads_the_machine() && self.mode != SandboxMode::ReadOnly {
                policy = Toolchain::grant_every_cache(policy, self.prelude, home, self.state());
            }
            // A step with an assignment in front carries no scope, so this is the socket the
            // process started with, and never one the plan chose. It is judged here, as each stage
            // starts, and the row is the path the name leads to (SANDBOX-18).
            let socket = agent_socket(Some(home), self.state(), environment);
            let place = signing::Place {
                agent: socket.clone(),
                ..self.place.clone()
            };
            let (toolchain, scope) = self.carries(step);
            if let Some(toolchain) = toolchain {
                policy = self.toolchain_rows(toolchain, policy, home);
            }
            if let Some(scope) = scope {
                policy = scope.grant(policy, home, self.state(), &place);
                if !self.reads_the_machine() {
                    policy = Self::allow_reaches(
                        policy,
                        reaches(resolved, scope, home, self.state(), environment),
                    );
                }
                if let Some(key) = self.identified(step).as_ref().and_then(Identified::key) {
                    policy = policy.allow_read(key);
                }
                if self.lends_the_agent(scope)
                    && let Some(socket) = socket.clone()
                {
                    policy = policy.allow_write(socket);
                }
            }
            policy = self.grant_requested(step, policy, home, &place, &socket, environment);
            policy = self.grant_remembered(step, policy, home, &place);
        }

        if !self.reads_the_machine() {
            // This process's own `PATH`, never the step's: a `PATH=` assignment on the line is a
            // value the model supplied, and SANDBOX-18 lets no such value choose a read row.
            let searched: Vec<PathBuf> = std::env::var_os("PATH")
                .map(|path| std::env::split_paths(&path).collect())
                .unwrap_or_default();
            for path in program_reads(resolved, &searched, self.home.as_deref(), self.state()) {
                policy = policy.allow_read(path);
            }
            // The two files the step starts, as files: a program installed inside the home is in
            // no directory row, and a person read exactly these two.
            policy = policy.allow_read(canonical(resolved));
            if started_as != resolved {
                policy = policy.allow_read(started_as);
            }
        }

        // The session's directories are read in every mode and written in every mode but
        // `read-only` (SANDBOX-22). What a person's yes adds below is theirs to widen.
        let writes_the_session = self.mode != SandboxMode::ReadOnly;
        for root in &self.roots {
            policy = policy.allow_read(root);
            if writes_the_session {
                policy = policy.allow_write(root);
            }
        }
        if let Some(scratch) = &self.scratch {
            policy = policy.allow_read(scratch);
            if writes_the_session {
                policy = policy.allow_write(scratch);
            }
        }
        let mut policy = self.reach_in_force().apply(policy);
        let unasked: Vec<PathBuf> = match (self.writes_without_a_request(), self.home.as_deref()) {
            (true, Some(home)) => writable_without_a_request(home, self.state()),
            _ => Vec::new(),
        };
        let held: Vec<PathBuf> = policy.writable.iter().map(|row| row.path.clone()).collect();
        for row in &unasked {
            policy = policy.allow_write_listed(row);
        }
        if !unasked.is_empty() {
            for withheld in &self.withheld_writes {
                if !policy.unwritable.contains(withheld) {
                    policy = policy.deny_write(withheld.clone());
                }
            }
        }
        let mut policy = self.filesystem.apply(policy);
        // The rows bypass gives are every path the account can write, which the temporary
        // directory is one of, so holding it back there is a refusal inside a row and not a row
        // left out (SANDBOX-22).
        if self.temporary_held_back
            && self.temporary_owner == TemporaryOwner::Account
            && !unasked.is_empty()
            && !policy.unwritable.contains(&self.temporary)
        {
            policy = policy.deny_write(self.temporary.clone());
        }
        policy = self.deny_writes_beneath_read_refusals(policy, unasked.is_empty());
        // Not skipped for a stage that lifts the `.git` hold-back: these are refusals of their own
        // and not protected names (SANDBOX-14).
        if self.prelude == Prelude::MacOs {
            for named in &self.named_settings {
                if !policy.unwritable.contains(named) {
                    policy = policy.deny_write(named.clone());
                }
                policy = policy.keep_in_place(named.clone());
            }
        }
        let machine: Vec<PathBuf> = unasked
            .into_iter()
            .filter(|row| !held.contains(row))
            .collect();
        // The rows bypass gives without a request are the whole machine, so counting them would
        // leave no git anywhere; a git under one of them alone keeps the lift (SANDBOX-18).
        let beyond = self.beyond_the_session();
        let planted: Vec<&Path> = policy
            .writable
            .iter()
            .filter(|row| !machine.contains(&row.path))
            .map(|row| row.path.as_path())
            .chain(beyond.iter().map(PathBuf::as_path))
            .collect();
        let policy = match is_git(self.prelude, resolved, &planted) {
            true => policy.allow_git_directory_writes(),
            false => policy,
        };
        (policy, machine)
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
        self.profile_offering(steps, false)
    }

    /// [`Confinement::profile`], naming the one-line `unconfined` route to a step that signs where
    /// `unconfined_offered` says the session would accept the request, so a planner is never
    /// pointed at an argument it would be refused.
    pub fn profile_offering(&self, steps: &[&Step], unconfined_offered: bool) -> String {
        self.profile_offering_in(steps, unconfined_offered, &process_environment())
    }

    /// [`Confinement::profile_offering`] for a line that ran in the background, from the start or
    /// after it was moved there. Where the sentence names `'unconfined': true`, it adds
    /// [`BACKGROUND_UNCONFINED_IN_THE_FOREGROUND_FIRST`]: a line in the background earns no request
    /// (SANDBOX-29), so the planner is told to run the line in the foreground first. Whether the
    /// argument is named is decided by the same conditions that chose the sentences, never by
    /// reading the composed text.
    pub fn background_profile_offering(&self, steps: &[&Step], unconfined_offered: bool) -> String {
        let (said, named) = self.offering_in(steps, unconfined_offered, &process_environment());
        match named {
            true => format!("{said}{BACKGROUND_UNCONFINED_IN_THE_FOREGROUND_FIRST}"),
            false => said,
        }
    }

    /// [`Confinement::profile_offering`] for stages that start with `environment`.
    fn profile_offering_in(
        &self,
        steps: &[&Step],
        unconfined_offered: bool,
        environment: &[(String, String)],
    ) -> String {
        self.offering_in(steps, unconfined_offered, environment).0
    }

    /// [`Confinement::profile_offering_in`], and whether a sentence in it names
    /// `'unconfined': true`.
    fn offering_in(
        &self,
        steps: &[&Step],
        unconfined_offered: bool,
        environment: &[(String, String)],
    ) -> (String, bool) {
        let (signs, refused) = self.signing_state(steps);
        let source = match (signs, unconfined_offered) {
            (false, _) => String::new(),
            (true, false) => format!(
                "{SIGNING_KEY_SOURCE_SENTENCE}{SIGNING_AGENT_SENTENCE}{}.",
                self.signing_agent_sentence(steps, environment)
            ),
            (true, true) => format!(
                "{SIGNING_KEY_SOURCE_SENTENCE}{SIGNING_AGENT_SENTENCE}{}{SIGNING_UNCONFINED_WAY_OUT}.",
                self.signing_agent_sentence(steps, environment)
            ),
        };
        let refusal = if refused {
            SIGNING_KEY_REFUSED_SENTENCE
        } else {
            ""
        };
        let commit = commit_sentence(self.mode);
        let (git, git_names_it) = match self.prelude {
            Prelude::MacOs if !steps.is_empty() && steps.iter().all(|step| self.runs_git(step)) => {
                (String::new(), false)
            }
            Prelude::MacOs if unconfined_offered => (
                format!(
                    "{}{MACOS_GIT_UNCONFINED_SENTENCE}",
                    macos_git_sentence(self.mode)
                ),
                true,
            ),
            Prelude::MacOs => (macos_git_sentence(self.mode).to_string(), false),
            Prelude::Linux | Prelude::Windows => (String::new(), false),
        };
        let launch_names_it = self.prelude == Prelude::MacOs
            && unconfined_offered
            && steps
                .iter()
                .any(|step| launches_applications(&step.resolved));
        let launch = match launch_names_it {
            true => MACOS_LAUNCH_UNCONFINED_SENTENCE,
            false => "",
        };
        let listing = match self.prelude {
            Prelude::MacOs if steps.iter().any(|step| lists_processes(&step.resolved)) => {
                match self.names_requests() {
                    true => format!("{MACOS_PROCESS_LISTING_SENTENCE} {LOOPBACK_CONNECT_RULE}"),
                    false => MACOS_PROCESS_LISTING_SENTENCE.to_string(),
                }
            }
            _ => String::new(),
        };
        let shell = match steps.iter().any(|step| redirects_inside_a_shell(step)) {
            true => SHELL_REDIRECTION_SENTENCE,
            false => "",
        };
        let assigned = match self.request_skips_a_stage(steps) {
            true => REQUEST_SKIPPED_AN_ASSIGNED_STAGE_SENTENCE,
            false => "",
        };
        let (linked, linked_names_it) = self.linked_checkout_sentence(steps, unconfined_offered);
        let (identity, identity_names_it) =
            self.identity_sentence(steps, unconfined_offered, environment);
        // Each sentence that names the argument is said only where it is offered: the signing way
        // out, the macOS git and launch sentences, and the linked checkout and identity sentences.
        // Each term is the condition that chose its sentence, and no composed text is tested.
        let names_it = unconfined_offered
            && (signs || git_names_it || launch_names_it || linked_names_it || identity_names_it);
        let said = format!(
            "{linked}{}{}{source}{refusal}{identity}{commit}{git}{launch}{listing}{shell}{assigned}{}",
            self.cache_sentence(steps),
            self.reach(steps),
            credential_locations_sentence(
                self.mode,
                self.requests_offered,
                self.names_request_path(),
                self.askable,
            ),
        );
        (said, names_it)
    }

    /// The sentence that opens the failure sentence where a step of `steps` is git and the `.git`
    /// of the checkout holding a session directory is a file, as a linked worktree's or a
    /// submodule's is (SANDBOX-14, SANDBOX-19). Nothing otherwise, and nothing under `read-only`,
    /// where [`READ_ONLY_COMMIT_SENTENCE`] already says a commit fails.
    ///
    /// git keeps such a checkout's index and refs, and the repository's common `.git`, in the
    /// directory the file names, which is outside the session directory. The sentence names the
    /// `.git` file and never the directory it names: that line is content of the checkout, which
    /// nobody may have vouched for. It is decided from the file each step resolved to and whether
    /// the `.git` entry is a file, never from what the entry holds or what a program printed. It
    /// names a route only where the session would accept it: `request_path` with write where a
    /// request is offered and the list holds the tool, `'unconfined': true` where
    /// `unconfined_offered` says so, and otherwise that no route exists. The bool is whether the
    /// sentence names `'unconfined': true`.
    fn linked_checkout_sentence(
        &self,
        steps: &[&Step],
        unconfined_offered: bool,
    ) -> (String, bool) {
        if !matches!(self.mode, SandboxMode::Strict | SandboxMode::WorkspaceWrite)
            || !steps.iter().any(|step| self.runs_git(step))
        {
            return (String::new(), false);
        }
        let linked = self.linked_checkouts();
        if linked.is_empty() {
            return (String::new(), false);
        }
        let named = linked
            .iter()
            .map(|entry| format!("`{}`", entry.display()))
            .collect::<Vec<_>>()
            .join(", ");
        let is = match linked.len() {
            1 => "is a file",
            _ => "are files",
        };
        let mut routes = String::new();
        // Where nobody can be asked, the caller offers requests only in a mode that answers them
        // unasked (`LiveMode::answers_requests`), so `names_requests` already carries that.
        if self.names_requests() && self.names_request_path() {
            routes.push_str(LINKED_CHECKOUT_REQUEST_SENTENCE);
        }
        if unconfined_offered {
            routes.push_str(LINKED_CHECKOUT_UNCONFINED_SENTENCE);
        }
        if routes.is_empty() {
            routes.push_str(LINKED_CHECKOUT_NO_ROUTE_SENTENCE);
        }
        (
            format!("{named} {is}{LINKED_CHECKOUT_SENTENCE}{routes} "),
            unconfined_offered,
        )
    }

    /// Whether `step` runs git as the `.git` write lift judges it: a file named `git` that no
    /// stage of the plan could have written ([`is_git`], [`Confinement::writable_by_the_plan`]).
    fn runs_git(&self, step: &Step) -> bool {
        is_git(self.prelude, &step.resolved, &[]) && !self.writable_by_the_plan(&step.resolved)
    }

    /// The `.git` entries that are files, one for the checkout holding each session directory: the
    /// directory itself where it has a `.git`, or the nearest ancestor that has one
    /// ([`bravebot_sandbox::programs::with_checkout`]). Only the kind of the entry is read, never
    /// what it holds, and a link is not followed.
    fn linked_checkouts(&self) -> Vec<PathBuf> {
        let mut linked: Vec<PathBuf> = Vec::new();
        for directory in &self.roots {
            let Some(checkout) = bravebot_sandbox::programs::with_checkout(directory).pop() else {
                continue;
            };
            let entry = checkout.join(".git");
            let a_file = std::fs::symlink_metadata(&entry).is_ok_and(|kind| kind.is_file());
            if a_file && !linked.contains(&entry) {
                linked.push(entry);
            }
        }
        linked
    }

    /// The sentence that opens the failure sentence where a step of `steps` resolved to a
    /// toolchain whose cache this mode does not write, or to `gh` on macOS, whose cache is written
    /// only where `~/.cache` exists (SANDBOX-15, SANDBOX-19). Nothing otherwise.
    ///
    /// Chosen from the file each step resolved to, the platform, the mode and whether the session
    /// names a home and a scratch directory, and from nothing a program printed, so the same words
    /// follow a cache refusal, a failing test and a typo.
    fn cache_sentence(&self, steps: &[&Step]) -> String {
        // Each step's toolchain, and whether its own assignment names XDG_CACHE_HOME. A gh step
        // with that assignment keeps no cache under `~/.cache`. Read from the plan's argv, which
        // the planner wrote.
        let read: Vec<(Option<Toolchain>, bool)> = steps
            .iter()
            .map(|step| {
                let Step {
                    program: _,
                    resolved,
                    started_as: _,
                    args: _,
                    matched: _,
                    environment,
                    expansions: _,
                    routes: _,
                } = step;
                let moves_the_cache = environment.iter().any(|(name, _)| name == "XDG_CACHE_HOME");
                (Toolchain::of(resolved), moves_the_cache)
            })
            .collect();
        let toolchains: std::collections::BTreeSet<&str> = read
            .iter()
            .filter_map(|(toolchain, _)| *toolchain)
            .map(Toolchain::name)
            .collect();
        match self.mode {
            SandboxMode::ReadOnly if !toolchains.is_empty() => format!(
                "{READ_ONLY_CACHE_OPENING}{}{READ_ONLY_CACHE_CLOSING}",
                toolchains.into_iter().collect::<Vec<_>>().join(", "),
            ),
            SandboxMode::Strict | SandboxMode::WorkspaceWrite
                if self.prelude == Prelude::MacOs
                    && self.home.is_some()
                    && toolchains.contains("gh") =>
            {
                let moved = read
                    .iter()
                    .filter(|(toolchain, _)| matches!(toolchain, Some(Toolchain::Gh)))
                    .all(|(_, moves_the_cache)| *moves_the_cache);
                // The route is a requested scope on an `env` stage: an assignment in front of gh
                // itself takes its scope away (`Scope::of`), and a request is added only to a
                // stage with no assignment in front of it (SANDBOX-26).
                match (&self.scratch, self.names_requests()) {
                    _ if moved => String::new(),
                    (Some(scratch), true) => format!(
                        "{MACOS_GH_CACHE_SENTENCE}{MACOS_GH_CACHE_MOVED_SENTENCE}`env \
                         XDG_CACHE_HOME='{}' gh ...`{MACOS_GH_CACHE_MOVED_SCOPE}",
                        scratch.display()
                    ),
                    _ => format!("{MACOS_GH_CACHE_SENTENCE}{MACOS_GH_CACHE_PERSON_SENTENCE}"),
                }
            }
            _ => String::new(),
        }
    }

    /// What the failure sentence says of the key the `git` steps of `steps` offered over ssh, or
    /// nothing where no step is `git` carrying the remote scope by its argv.
    ///
    /// Fixed text chosen by what [`Confinement::identified`] decided for each step, which reads the
    /// argv, the person's settings and their choices at run prompts, and nothing a program printed.
    /// It names no host, owner or path. The bool is whether the sentence names
    /// `'unconfined': true`.
    fn identity_sentence(
        &self,
        steps: &[&Step],
        unconfined_offered: bool,
        environment: &[(String, String)],
    ) -> (String, bool) {
        let decided: Vec<Identified> = steps
            .iter()
            .filter_map(|step| self.identified(step))
            .collect();
        if decided.is_empty() {
            return (String::new(), false);
        }
        let mut said = String::new();
        for (holds, sentence) in [
            (
                decided
                    .iter()
                    .any(|held| matches!(held, Identified::Key { .. })),
                IDENTITY_CHOSEN_SENTENCE,
            ),
            (
                decided
                    .iter()
                    .any(|held| matches!(held, Identified::Chosen { .. })),
                IDENTITY_PICKED_SENTENCE,
            ),
            (
                decided
                    .iter()
                    .any(|held| matches!(held, Identified::LeftToSsh(_))),
                IDENTITY_PICKED_DEFAULT_SENTENCE,
            ),
            (
                decided.contains(&Identified::Unchosen),
                IDENTITY_UNCHOSEN_SENTENCE,
            ),
            (
                decided.contains(&Identified::NoAddress),
                IDENTITY_NO_ADDRESS_SENTENCE,
            ),
        ] {
            if holds {
                said.push_str(sentence);
            }
        }
        let home = self.home.as_deref();
        said.push_str(&self.push_reach_sentence(home, environment));
        said.push_str(IDENTITY_OPENING);
        if self.open_key_choice(steps, true).is_some() {
            said.push_str(IDENTITY_PROMPT_CLAUSE);
            said.push_str(IDENTITY_ENTRY_CLAUSE);
            said.push_str(IDENTITY_PROMPT_TAIL);
        } else {
            said.push_str(IDENTITY_ENTRY_CLAUSE);
            said.push_str(IDENTITY_NO_PROMPT_TAIL);
        }
        if unconfined_offered {
            said.push_str(IDENTITY_UNCONFINED_WAY_OUT);
        }
        (said, unconfined_offered)
    }

    /// What a stage that pushes over ssh had to authenticate with: whether the session's agent
    /// socket was lent, and how many public key files ssh could be given and how many its
    /// configuration names that could not be read.
    ///
    /// Counted from the environment the stage starts with and the person's own files, never from
    /// what a program printed.
    fn push_reach_sentence(&self, home: Option<&Path>, environment: &[(String, String)]) -> String {
        let agent = match agent_socket(home, self.state(), environment) {
            Some(_) => IDENTITY_AGENT_LENT_SENTENCE,
            None => IDENTITY_NO_AGENT_SENTENCE,
        };
        let Some(home) = home else {
            return agent.to_string();
        };
        let readable = self.offerable_candidates(home).len();
        let refused = refused_identity_files(home, self.state());
        let files = |count: usize| match count {
            1 => "1 public key file".to_string(),
            _ => format!("{count} public key files"),
        };
        let unreadable = match refused {
            0 => String::new(),
            _ => format!(
                ", and {} named by an `IdentityFile` line could not be read",
                files(refused)
            ),
        };
        format!(
            "{agent} The step could read {} among those ssh looks for by default and those its \
             configuration names{unreadable}.",
            match readable {
                0 => "no public key file".to_string(),
                count => files(count),
            }
        )
    }

    /// Whether a step of `steps` carries the signing scope or was asked to carry it, whether or
    /// not the person's git signs with ssh, and then whether the person's `user.signingkey` names
    /// a file no scope reads, so that the signature fails and the planner is told why.
    fn signing_state(&self, steps: &[&Step]) -> (bool, bool) {
        let Some(home) = self.home.as_deref() else {
            return (false, false);
        };
        let asked = steps.iter().any(|step| {
            self.scope_of(step) == Some(Scope::Signing)
                || self
                    .requested_for(step)
                    .contains(&Requested::Scope(Scope::Signing))
        });
        if !asked {
            return (false, false);
        }
        let signing = signing::read(home, self.state(), &self.place);
        (
            true,
            signing.enabled && signing.key == signing::Key::Refused,
        )
    }

    /// Whether the agent socket was lent to a step of `steps` that signs, and so what a signature
    /// that fails for want of a private key has to mean.
    ///
    /// Decided by [`Confinement::lends_the_agent`], which [`Confinement::policy`] decides the rows
    /// with: a step lends the socket the stage starts with where it carries or was asked for the
    /// remote scope, or the signing scope for a person whose git signs with ssh. Read from that and
    /// the person's own configuration, and never from what a program printed.
    fn signing_agent_sentence(&self, steps: &[&Step], environment: &[(String, String)]) -> String {
        let reaches_the_socket = steps.iter().any(|step| {
            self.scope_of(step)
                .is_some_and(|scope| self.lends_the_agent(scope))
                || self.requested_for(step).iter().any(|request| {
                    matches!(request, Requested::Scope(scope) if self.lends_the_agent(*scope))
                })
        });
        match (
            reaches_the_socket,
            agent_socket(self.home.as_deref(), self.state(), environment).is_some(),
        ) {
            (true, true) => SIGNING_AGENT_LENT_SENTENCE.to_string(),
            (true, false) => SIGNING_NO_AGENT_SENTENCE.to_string(),
            // `read-only` lends no remote scope, so it is not offered as the way to the socket.
            (false, _) if !self.lends(Scope::Remote) => SIGNING_NOT_SSH_SENTENCE.to_string(),
            (false, _) => format!("{SIGNING_NOT_SSH_SENTENCE}{SIGNING_REMOTE_CLAUSE}"),
        }
    }

    /// What the programs of `steps` could reach, by name, without the closing fixed sentence.
    fn reach(&self, steps: &[&Step]) -> String {
        // The scratch directory is made inside the temporary directory, which every mode writes,
        // so under `read-only` the directories said not to be written are `opened` alone.
        let mut opened: Vec<String> = Vec::new();
        for root in &self.roots {
            let shown = root.display().to_string();
            if !opened.contains(&shown) {
                opened.push(shown);
            }
        }
        let mut directories = opened.clone();
        if let Some(scratch) = &self.scratch {
            let shown = scratch.display().to_string();
            if !directories.contains(&shown) {
                directories.push(shown);
            }
        }
        let mut toolchains = std::collections::BTreeSet::new();
        let mut scopes = std::collections::BTreeSet::new();
        let mut reaching = std::collections::BTreeSet::new();
        let mut loopback = false;
        for step in steps {
            let (toolchain, scope) = self.carries(step);
            toolchains.extend(toolchain.map(Toolchain::name));
            scopes.extend(scope.map(Scope::name));
            if self.network().is_closed() {
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
                    Requested::Loopback => !std::mem::replace(&mut loopback, true),
                    Requested::SshKey => false,
                };
            }
        }
        let named = |names: std::collections::BTreeSet<&str>| match names.is_empty() {
            true => "none".to_string(),
            false => names.into_iter().collect::<Vec<_>>().join(", "),
        };
        let network = match self.network() {
            Network::Open => "open".to_string(),
            Network::Closed => format!("closed, kept only by steps with: {}", named(reaching),),
        };
        let network = match self.hosts {
            Some(_) => format!(
                "{network}; a step with network access is held to the hosts \
                 sandbox.network.allowedHosts names, and refused where the platform cannot hold it \
                 to them"
            ),
            None => network,
        };
        let network = match loopback
            .then(|| loopback_reach(self.prelude, self.network(), self.hosts.is_some()))
            .flatten()
        {
            Some(reach) => format!("{network}; {reach}"),
            None => network,
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
        let requested_paths = self.reach_in_force().counts();
        let rules = match requested_paths.allow_read + requested_paths.allow_write {
            0 => rules,
            asked => format!(
                "{rules} The person also let programs reach {asked} path{} through {} \
                 this session.",
                if asked == 1 { "" } else { "s" },
                if self.names_request_path() {
                    "request_path"
                } else {
                    "a request"
                },
            ),
        };
        if self.reads_the_machine() {
            let writes = match (self.writes_without_a_request(), self.mode) {
                (true, _) => format!(
                    "write every path {} would be granted, \
                     which is everything the account can write except the places that hold a \
                     credential and the person's own denyRead and denyWrite paths, with no new \
                     entry made directly in the home directory, a directory above it, or a \
                     directory that holds a credential location",
                    if self.names_request_path() {
                        "a request_path with write set"
                    } else {
                        "a request for write access"
                    },
                ),
                (false, SandboxMode::ReadOnly) => format!(
                    "write the temporary directory and the paths the person's own `allowWrite` \
                     entries and remembered write grants name, and nothing else of {} or the \
                     toolchain caches",
                    opened.join(", "),
                ),
                (false, _) => format!(
                    "write {} and the temporary directory and the toolchain caches",
                    directories.join(", "),
                ),
            };
            return format!(
                "Confinement: programs could read this machine except the places that hold a \
                 credential, and {writes}; \
                 a place that holds a credential was read only where a credential scope added it \
                 for the steps that named one (credential scopes: {}). Any other path is refused \
                 by the operating system as `Operation not permitted` or `Permission denied`. \
                 Network: {}.{}{}",
                named(scopes),
                network,
                rules,
                self.menu_sentence(),
            );
        }
        let session = match self.mode {
            SandboxMode::ReadOnly => format!(
                "read {}, write only the temporary directory and the paths the person's own \
                 `allowWrite` entries and remembered write grants name",
                directories.join(", "),
            ),
            _ => format!(
                "read and write {} and the temporary directory",
                directories.join(", "),
            ),
        };
        format!(
            "Confinement: programs could {session}, and read the system and program directories \
             and git's configuration files; beyond those they reached only what a toolchain list \
             or credential scope added for the steps that named one (toolchain lists: {}; \
             credential scopes: {}). Any other path is refused by the operating system as \
             `Operation not permitted` or `Permission denied`. Network: {}.{}{}",
            named(toolchains),
            named(scopes),
            network,
            rules,
            self.menu_sentence(),
        )
    }

    /// The sentence naming what a line may ask for, or nothing where this session would not accept
    /// a request or names no home to apply one under.
    ///
    /// The same words follow every failure of the line, the exit code not among its inputs.
    fn menu_sentence(&self) -> String {
        match self.names_requests() {
            true => format!(
                " {}",
                requests_menu_sentence(self.mode, self.names_request_path())
            ),
            false => String::new(),
        }
    }

    /// Whether a sentence may name a `scopes` argument or a `request_path` call: the session would
    /// accept one and names a home to apply it under.
    fn names_requests(&self) -> bool {
        self.requests_offered && self.accepts_requests()
    }

    /// The first session directory that is or lies inside a credential location, or on Windows
    /// holds one, and that location (SANDBOX-18).
    ///
    /// A Windows container cannot be refused a location inside a directory it is granted, so a
    /// directory that holds one is refused there too. Elsewhere the location is refused inside the
    /// grant, which holds for a directory above it but not for one at it: on macOS a row at a
    /// refused path lifts the refusal. Names are compared without regard to case on Windows and
    /// macOS, whose file systems fold it, and exactly on Linux.
    ///
    /// The roots are the names a session directory still leads from: a root that leads to another
    /// directory is dropped by [`Confinement::with_session_directories`], so this judges where
    /// each one leads now.
    fn root_reaching_credentials(&self) -> Option<(&Path, PathBuf)> {
        reaching_credentials(
            self.prelude,
            self.home.as_deref(),
            self.state(),
            self.roots.iter(),
        )
    }

    /// The scratch directory, if its name now leads to a link or a file.
    fn scratch_linked(&self) -> Option<&Path> {
        self.scratch_made
            .as_ref()
            .filter(|made| made.leads_elsewhere())
            .map(ScratchDirectory::path)
    }

    /// The first session directory, if its name now leads to a link or a file (SANDBOX-18). One
    /// that leads to another directory or nowhere is given no row when the confinement is built
    /// and is not this: nothing is reached through it, and the stages after it still start.
    fn session_linked(&self) -> Option<&Path> {
        self.roots_made
            .iter()
            .find(|made| made.leads_elsewhere())
            .map(ScratchDirectory::path)
    }

    /// The session's own temporary directory, if its name now leads to a link or a file.
    ///
    /// It is made inside the scratch directory, which every stage may write, so a stage can put a
    /// link at its name and the row written from it would then grant wherever it leads. Judged as
    /// [`Confinement::scratch_linked`] judges the scratch directory: the name is not followed, and
    /// a name that is gone or is a directory is not this. On Windows `symlink_metadata` reports a
    /// junction as a link as well; that case is reasoned from the standard library and has not
    /// been run there.
    fn temporary_linked(&self) -> Option<&Path> {
        let temporary = self.sessions_temporary()?;
        std::fs::symlink_metadata(temporary)
            .is_ok_and(|found| !found.is_dir())
            .then_some(temporary)
    }

    /// The named settings file, where it is a file that another name on this machine leads to as
    /// well. A refusal of writes is by path and a second link to the file is written through, so
    /// on macOS a stage cannot be kept from the file while one exists.
    #[cfg(unix)]
    fn named_settings_with_another_link(&self) -> Option<&Path> {
        use std::os::unix::fs::MetadataExt;
        if self.prelude != Prelude::MacOs {
            return None;
        }
        self.named_settings
            .iter()
            .map(PathBuf::as_path)
            .find(|named| {
                std::fs::metadata(named).is_ok_and(|found| found.is_file() && found.nlink() > 1)
            })
    }

    #[cfg(not(unix))]
    fn named_settings_with_another_link(&self) -> Option<&Path> {
        None
    }

    /// Why a stage cannot be started because of a session, scratch or temporary directory, if it
    /// cannot (SANDBOX-18).
    fn directory_refusal(&self) -> Option<String> {
        if let Some(path) = self.named_settings_with_another_link() {
            return Some(format!(
                "the settings file `{}` named on the command line has another hard link, which a \
                 stage is not kept from writing through; remove the link, or name another file",
                path.display()
            ));
        }
        if let Some((root, location)) = self.root_reaching_credentials() {
            return Some(match self.prelude {
                Prelude::Windows => format!(
                    "the session directory `{}` is or holds `{}`, which a Windows stage cannot be \
                     kept out of; open the session on a project directory instead",
                    root.display(),
                    location.display()
                ),
                _ => format!(
                    "the session directory `{}` is or lies inside `{}`, which a stage given that \
                     directory to write is not kept out of; open the session on a project \
                     directory instead",
                    root.display(),
                    location.display()
                ),
            });
        }
        if let Some(linked) = self.session_linked() {
            return Some(format!(
                "the session directory `{}` has been replaced by a link or a file, so a stage \
                 given it to write could reach wherever its name now leads; open the session on \
                 it again to be given it",
                linked.display()
            ));
        }
        if let Some(linked) = self.scratch_linked() {
            return Some(format!(
                "the session's scratch directory `{}` has been replaced by a link or a file, so a \
                 stage given it to write could reach wherever its name now leads; start a new \
                 session to be given another",
                linked.display()
            ));
        }
        self.temporary_linked().map(|linked| {
            format!(
                "the session's temporary directory `{}` has been replaced by a link or a file, \
                 so a stage given it to write could reach wherever its name now leads; start a \
                 new session to be given another",
                linked.display()
            )
        })
    }

    /// The sentence refusing a step because a `denyRead` or `denyWrite` entry cannot be applied,
    /// or `None` where every entry can.
    fn unapplied_denial_refusal(&self) -> Option<String> {
        let (item, reason) = self.filesystem.unapplied_denial()?;
        // An entry a checkout's layer wrote is text the person did not write, and this
        // sentence reaches the planner as the driver's own. It names the list, the reason (a
        // closed set the driver wrote) and the file, and leaves the spelling to the screens
        // that are the person's own (SANDBOX-25). Which of the two is decided from the layer
        // that wrote the entry, never from the entry.
        if item.entry.from_a_checkout {
            return Some(
                t!(
                    run_not_confined_checkout_entry,
                    list = item.list.key(),
                    file = item
                        .entry
                        .by
                        .as_deref()
                        .map(crate::confirm::shown_path)
                        .unwrap_or_default(),
                    reason = crate::permissions::filesystem_reason(reason)
                )
                .to_string(),
            );
        }
        Some(format!(
            "sandbox.filesystem.{} names `{}`, which cannot be applied, and a stage started \
             without it would reach the path it holds back",
            item.list.key(),
            item.entry.path
        ))
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
            before: crate::exec::StartedBefore::Nothing,
            network_closed: false,
            entry: None,
        };
        if let Some(detail) = self.directory_refusal() {
            return Err(not_confined(detail));
        }
        let proxy = self
            .host_proxy(step)
            .map_err(|refused| refused_as_not_confined(step, &refused))?;
        let proxied = proxy.as_deref().map(Proxy::environment).unwrap_or_default();
        let identified = self.identified(step);
        let chosen = identified.as_ref().and_then(Identified::command);
        let environment = overlay(
            temporary_defaults(
                std::env::vars_os().collect(),
                self.sessions_temporary(),
                command.get_envs(),
                FOLD_CASE,
            ),
            proxied
                .iter()
                .map(|(name, value)| (OsStr::new(name), Some(OsStr::new(value))))
                .chain(
                    chosen
                        .iter()
                        .map(|value| (OsStr::new(GIT_SSH_COMMAND), Some(OsStr::new(value)))),
                ),
            FOLD_CASE,
        );
        #[cfg(test)]
        if self.unconfinable.as_deref() == Some(step.program.as_str()) {
            return Err(not_confined("a test made the platform refuse".to_string()));
        }
        let sandbox = bravebot_sandbox::for_current_platform()
            .map_err(|error| not_confined(error.to_string()))?;
        let capabilities = sandbox.capabilities();
        // The strings the policy reads: the names it keys on are ASCII scope variables and
        // `SSH_AUTH_SOCK`. A value a substitution printed that is not text is left out, which reads
        // as the variable being unset and so lends nothing; the bytes are decided on by no one, and
        // the step still receives them as they are through `command`.
        let readable: Vec<(String, String)> = environment
            .iter()
            .filter_map(|(name, value)| {
                Some((name.to_str()?.to_string(), value.to_str()?.to_string()))
            })
            .collect();
        if let Some(detail) = self.unapplied_denial_refusal() {
            return Err(not_confined(detail));
        }
        let mut wanted = self.policy(step, directory, &readable);
        if let Some(proxy) = &proxy {
            wanted = wanted.allow_network_egress_only_to(proxy.addr().port());
            // A Linux stage runs in a network of its own, which reaches the proxy by this socket.
            #[cfg(target_os = "linux")]
            if let Some(socket) = proxy.socket_path() {
                wanted = wanted.with_egress_through(socket);
            }
        }
        if let Some(detail) = cannot_keep_from_sockets(self.mode, &capabilities) {
            return Err(not_confined(detail));
        }
        if let Some(detail) = cannot_close_the_network(
            &wanted,
            &capabilities,
            network_pinned_by_the_administrator(),
        ) {
            return Err(ExecError::NotConfined {
                program: step.program.clone(),
                detail,
                before: crate::exec::StartedBefore::Nothing,
                network_closed: true,
                entry: None,
            });
        }
        if let Some(detail) = cannot_hold_to_the_proxy(&wanted, &capabilities) {
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
                before: crate::exec::StartedBefore::Nothing,
                network_closed: false,
                entry: None,
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

/// `command`, started as the leader of a session of its own on Unix, which is a process group of
/// its own with no controlling terminal.
///
/// The group is what ending a stage addresses, so that the programs the stage started are ended
/// with it ([`crate::exec`], RUN-11). A stage that stayed in the session's group could not be
/// signalled without signalling the session. The missing terminal is what keeps a program that
/// opens or configures the terminal from being stopped by it (RUN-11).
fn leading_a_session(command: Command) -> Command {
    #[cfg(unix)]
    {
        let mut command = command;
        bravebot_sandbox::process::leading_a_session(&mut command);
        command
    }
    #[cfg(not(unix))]
    command
}

/// `command`, ready to start under `confinement`, or the reason it cannot be.
pub(crate) fn begin(
    confinement: Option<&Confinement>,
    command: Command,
    step: &Step,
    directory: &Path,
) -> Result<Launch, ExecError> {
    let Some(confinement) = confinement else {
        return Ok(Launch::Command(leading_a_session(command)));
    };
    #[cfg(unix)]
    {
        // A confined stage is a session leader once the backend has called `setsid`, and a session
        // leader leads a group of its own. Starting it as a leader here as well would make that
        // call fail, since it refuses a process that already leads a group.
        confinement
            .wrap(command, step, directory)
            .map(Launch::Command)
    }
    #[cfg(windows)]
    {
        use bravebot_sandbox::windows::{refusal_for_program, text_of};

        let args: Vec<std::ffi::OsString> = command.get_args().map(OsStr::to_os_string).collect();
        let refusal = match text_of(command.get_program(), &args) {
            Ok((program, args)) => refusal_for_program(&program, &args),
            Err(refusal) => Some(refusal),
        };
        if let Some(refusal) = refusal {
            return Err(ExecError::NotConfined {
                program: step.program.clone(),
                detail: refusal.to_string(),
                before: crate::exec::StartedBefore::Nothing,
                network_closed: false,
                entry: None,
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
                before: crate::exec::StartedBefore::Nothing,
            }),
            Err(other) => Err(ExecError::NotConfined {
                program: program_name,
                detail: other.to_string(),
                before: crate::exec::StartedBefore::Nothing,
                network_closed: false,
                entry: None,
            }),
        }
    }
}

/// Follows the `.git` files [`Confinement::linked_checkout_sentence`] names, after a failed git
/// step (SANDBOX-19).
const LINKED_CHECKOUT_SENTENCE: &str = ", so the checkout is a linked worktree or a submodule: \
     git keeps its index and refs, and the repository's common `.git`, in the directory such a \
     file names, outside the session directory, and a git step that writes them, as `git commit` \
     does, is refused that write wherever no directory this run may write holds them.";

/// The route [`LINKED_CHECKOUT_SENTENCE`] names where the session accepts a `request_path` call.
const LINKED_CHECKOUT_REQUEST_SENTENCE: &str = " A `request_path` call with write for the \
     directory that file names, and for the common `.git`, lets a later line write them: the \
     person is asked.";

/// The route [`LINKED_CHECKOUT_SENTENCE`] names where the session accepts an `unconfined` line.
const LINKED_CHECKOUT_UNCONFINED_SENTENCE: &str = " The git line can be run again with \
     'unconfined': true, saying in 'why' what the sandbox refused: the person is asked to approve \
     that one line, which runs with no sandbox.";

/// Said in place of a route where the session accepts neither a request nor an `unconfined` line.
const LINKED_CHECKOUT_NO_ROUTE_SENTENCE: &str =
    " Nothing this session accepts lets a step write them.";

/// Said after a failed step on macOS where the session accepts an `unconfined` request
/// (SANDBOX-29), for the script the sentence above cannot help: one whose own child process runs
/// git. Never said where the session would refuse the request.
const MACOS_GIT_UNCONFINED_SENTENCE: &str = " A script that has to run git itself, and was refused \
     that write, can be run again with 'unconfined': true, saying in 'why' what the sandbox \
     refused: the person is asked to approve that one line, which runs with no sandbox.";

/// Said after a failed step on macOS where the session accepts an `unconfined` request
/// (SANDBOX-29) and a step of the line runs `open` or `osascript`, whose requests to LaunchServices
/// and Apple Events the profile refuses (SANDBOX-31). Never said where the session would refuse the
/// request.
const MACOS_LAUNCH_UNCONFINED_SENTENCE: &str = " On macOS the default profile refuses \
     LaunchServices and Apple Events, so `open` and `osascript` cannot launch an application, open \
     a URL in one or script one. The line can be run again with 'unconfined': true, saying in \
     'why' what the sandbox refused: the person is asked to approve that one line, which runs \
     with no sandbox.";

/// Whether the file a step resolved to is `open` or `osascript`, the programs that ask
/// LaunchServices or Apple Events to act for the caller. Decided from the compiled step alone.
fn launches_applications(resolved: &Path) -> bool {
    [OsStr::new("open"), OsStr::new("osascript")]
        .contains(&resolved.file_name().unwrap_or_default())
}

/// Said after a failed step on macOS when a step runs a program that lists processes, which the
/// profile refuses (SANDBOX-8). The programs fail for different reasons, so it says that the
/// sandbox refuses listing processes and does not name the rule that does it.
const MACOS_PROCESS_LISTING_SENTENCE: &str = " On macOS the sandbox refuses listing processes, so \
     `pgrep`, `pkill`, `killall`, `lsof` and `ps` fail whether or not the program they look for \
     is running, and that failure is not an answer about the program. A local service is found by \
     connecting to its port instead, such as `curl http://127.0.0.1:9222/json/version` for a \
     browser started with --remote-debugging-port=9222.";

/// Said first after a failed step on macOS whose program resolved to `gh`, in a mode that writes
/// gh's cache. The profile writes `~/.cache/gh` and never `~/.cache`, so on an account with no
/// `~/.cache` gh cannot create its cache (SANDBOX-15).
const MACOS_GH_CACHE_SENTENCE: &str = "A `gh` step could write its cache `~/.cache/gh`, and on \
     macOS only where `~/.cache` already exists: where it does not, the sandbox refuses gh \
     creating it.";

/// Follows [`MACOS_GH_CACHE_SENTENCE`] where the session accepts a `scopes` argument, ahead of the
/// line written with the session's scratch directory, which every mode that writes gh's cache also
/// writes. gh keeps its cache in `$XDG_CACHE_HOME/gh` where that variable is set, so the line moves
/// the cache somewhere the stage can create it. The variable is set by `env`, since an assignment
/// in front of gh takes away its remote scope (SANDBOX-16), and the line asks for that scope by
/// name, since `env` carries none of its own (SANDBOX-19, SANDBOX-26).
const MACOS_GH_CACHE_MOVED_SENTENCE: &str = " A gh step can keep its cache in the session's \
     scratch directory instead, which it can write, by starting gh through `env` with \
     XDG_CACHE_HOME set to that directory and asking for gh's login by name: ";

/// Said after a failed step where the planner asked for a name in `scopes` and a stage of the line
/// was given none of it because an assignment is in front of its program (SANDBOX-19,
/// SANDBOX-26). Fixed words that quote nothing of the line. `env` in front of the program is not
/// named.
pub(crate) const REQUEST_SKIPPED_AN_ASSIGNED_STAGE_SENTENCE: &str = " A name in 'scopes' was not added to \
     a stage of this line that has a NAME=value assignment in front of its program. Pass git \
     configuration as `git -c name=value` instead of GIT_CONFIG_* assignments, or drop the \
     assignment.";

/// Follows the line [`MACOS_GH_CACHE_MOVED_SENTENCE`] introduces.
const MACOS_GH_CACHE_MOVED_SCOPE: &str = " with `scopes: [\"remote\"]`. An assignment written in \
     front of gh itself takes away gh's login and network, since a stage with an assignment in \
     front of it carries no credential scope. ";

/// Follows [`MACOS_GH_CACHE_SENTENCE`] where the session accepts no `scopes` argument or holds no
/// scratch directory, so no line run in it moves gh's cache and keeps gh's login.
const MACOS_GH_CACHE_PERSON_SENTENCE: &str = " Only the person can create `~/.cache`, so say in \
     the reply that gh needs it. ";

/// Opens the sentence that names the toolchains whose cache `read-only` leaves unwritten, ahead of
/// their names.
const READ_ONLY_CACHE_OPENING: &str = "Under `read-only` no toolchain cache is written, so a step \
     of ";

/// Closes the sentence [`READ_ONLY_CACHE_OPENING`] opens.
const READ_ONLY_CACHE_CLOSING: &str = " that writes its cache is refused that write. ";

/// Said after a failed step that runs a shell with a command string holding a `>` (CMDLINE-5,
/// SANDBOX-19). A
/// redirection inside the shell's command string is opened by the confined shell, so the sandbox
/// refuses one into a file outside the directories it writes, and the driver knows no destination
/// to name. A redirection written on the `run` line itself is opened by the driver, which checks
/// it and names what to do where it is refused. Chosen from the file the step resolved to; nothing
/// the shell printed is read.
const SHELL_REDIRECTION_SENTENCE: &str = " A redirection inside a shell's command string, as in \
     `sh -c '... >> file'`, is opened by the sandboxed shell, which cannot write a file outside the \
     directories above. Write the redirection on the run line itself, outside the quotes, where \
     bravebot opens the file and a refusal names it and how to reach it.";

/// Whether a step runs a shell with a command string that redirects, the steps
/// [`SHELL_REDIRECTION_SENTENCE`] is said for: the file it resolved to is a shell and an argument
/// the planner wrote holds a `>`. An argument a filename pattern produced, matched or passed through
/// as written, is not looked at (CMDLINE-4). Decided from the compiled step alone, as
/// [`launches_applications`] is; nothing the shell printed is read.
fn redirects_inside_a_shell(step: &Step) -> bool {
    let Step {
        program: _,
        resolved,
        started_as: _,
        args,
        matched: _,
        environment: _,
        expansions: _,
        routes: _,
    } = step;
    ["sh", "bash", "dash", "zsh", "ksh", "csh", "tcsh", "fish"]
        .map(OsStr::new)
        .contains(&resolved.file_name().unwrap_or_default())
        && args
            .iter()
            .enumerate()
            .any(|(at, argument)| !step.from_a_pattern(at) && argument.contains('>'))
}

/// Whether the file a step resolved to is one of the programs [`MACOS_PROCESS_LISTING_SENTENCE`]
/// names. Decided from the compiled step alone, as [`launches_applications`] is.
fn lists_processes(resolved: &Path) -> bool {
    ["pgrep", "pkill", "killall", "lsof", "ps"]
        .map(OsStr::new)
        .contains(&resolved.file_name().unwrap_or_default())
}

/// Said to the planner whenever a step carries the signing scope, so that a signature that fails
/// for want of a key can be told from one the scope never reached. Fixed text: it does not depend
/// on any configuration of the repository.
const SIGNING_KEY_SOURCE_SENTENCE: &str = " A step that signs a commit is lent the public key \
     that `user.signingkey` names in the person's `~/.gitconfig` or `~/.config/git/config`, or in \
     a file their `[includeIf \"gitdir:...\"]` includes for this directory, and every `*.pub` \
     file directly inside `~/.ssh` whose key the person's ssh agent holds. A `user.signingkey` \
     set in a repository's own configuration is not read, so git signing with a key that is \
     neither of these fails with \"Couldn't load public key\" until the person adds that key to \
     the agent or sets it in one of those places.";

/// Said after [`SIGNING_KEY_SOURCE_SENTENCE`], ahead of one of the three sentences that say
/// whether the agent socket was lent. Fixed text: it does not branch on what the program printed.
const SIGNING_AGENT_SENTENCE: &str =
    " A step that signs never reads a private key: it signs through the person's ssh agent.";

/// Said when the step was lent the agent socket, so that "No private key found" is told from a
/// socket the sandbox hid.
const SIGNING_AGENT_LENT_SENTENCE: &str = " The session's ssh agent socket was lent to that step, so \
     a signature that fails with \"No private key found\" means the configured public key was lent \
     but the agent does not hold its private key, not that the key is missing or that the sandbox \
     hides it, and it is solved by the person loading the key with `ssh-add`";

/// Said when the step asked for the socket and the session has none to lend.
const SIGNING_NO_AGENT_SENTENCE: &str = " No ssh agent socket was lent to that step, because the \
     session has none, so a signature that fails with \"No private key found\" means there was no \
     agent to sign through, and it is solved by the person starting an ssh agent that holds the \
     key before the session";

/// Said when no scope of the step lends the socket, because the person's own configuration does
/// not sign with ssh. It names the setting.
const SIGNING_NOT_SSH_SENTENCE: &str = " No ssh agent socket was lent to that step, because \
     `gpg.format` is not `ssh` in the person's `~/.gitconfig` or `~/.config/git/config`, and a \
     repository's own configuration is not read. A signature that fails with \"No private key \
     found\" there means the sandbox hid the agent, not that the agent lacks the key, and it is \
     solved by the person setting `gpg.format` to `ssh` in one of those files";

/// Added to [`SIGNING_NOT_SSH_SENTENCE`] where the mode lends the remote scope.
const SIGNING_REMOTE_CLAUSE: &str = ", or by a line that asks for the `remote` scope, which \
     lends the socket whatever those files say";

/// Said after the sentences of a failed line that ran in the background, where one of them names
/// `'unconfined': true`. A line in the background earns no request for no sandbox (SANDBOX-29), so
/// the request would be refused until a line fails in the foreground. The sentence is fixed when
/// the job starts, and a line may fail in the foreground before the job's end is read, so it says
/// what to do in both cases.
const BACKGROUND_UNCONFINED_IN_THE_FOREGROUND_FIRST: &str = " This line ran in the background, \
     which earns no request for 'unconfined': true. Unless a line has already failed in the \
     foreground with the sandbox this turn, first run this line again in the foreground with the \
     sandbox, without 'background' and with a 'deadline_seconds' long enough for it to end on its \
     own. If it fails there because the sandbox refused it access, ask for 'unconfined': true and \
     say in 'why' what was refused.";

/// The way through the sentences above names where the session accepts a request for a line
/// with no sandbox, ending without a full stop so the caller can end the sentence.
const SIGNING_UNCONFINED_WAY_OUT: &str = ", or, once a confined run of that one commit line \
     failed because the sandbox refused it access, by running it again with 'unconfined': true, \
     saying in 'why' what was refused, which the person is asked to approve and which runs with \
     no sandbox";

/// Said once to the planner when a step signs and the key `user.signingkey` names is one no scope
/// reads. It names the setting and never the value, which is the person's.
const SIGNING_KEY_REFUSED_SENTENCE: &str = " A step that signs a commit reads only a public key: \
     the `user.signingkey` in the person's own git configuration must be a key written out in \
     full or an existing `.pub` file inside the home directory, and a private key, a file \
     elsewhere and a relative path are not read, so signing fails until the person changes it.";

/// The variable git runs ssh by, which a `git.identities` entry sets for a stage.
const GIT_SSH_COMMAND: &str = "GIT_SSH_COMMAND";

/// What [`Confinement::identified`] decided for one `git` stage carrying the remote scope.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Identified {
    /// The argv names an ssh address, and this is the key the person's `git.identities` names for
    /// its owner, with the `GIT_SSH_COMMAND` that offers it.
    Key { key: PathBuf, command: String },
    /// The argv names an ssh address, no entry names its owner, and this is the key the person
    /// chose for it at an earlier run prompt, with the `GIT_SSH_COMMAND` that offers it.
    Chosen { chosen: ChosenKey, command: String },
    /// The argv names an ssh address, no entry names its owner, and the person chose at an earlier
    /// run prompt to leave the key to ssh.
    LeftToSsh(ChosenKey),
    /// The argv names an ssh address, and nothing names a key that can be read for its owner.
    Unchosen,
    /// The argv names no ssh address: a remote given by name, or another transport.
    NoAddress,
}

impl Identified {
    /// The key the stage reads and ssh offers first, where there is one.
    fn key(&self) -> Option<&Path> {
        match self {
            Self::Key { key, .. } => Some(key),
            Self::Chosen { chosen, .. } => chosen.key.as_deref(),
            _ => None,
        }
    }

    /// The `GIT_SSH_COMMAND` the stage starts with, where it is given a key.
    fn command(&self) -> Option<&str> {
        match self {
            Self::Key { command, .. } | Self::Chosen { command, .. } => Some(command),
            _ => None,
        }
    }
}

/// The most keys a run prompt numbers for the person to choose between, one digit each.
const KEY_CHOICES: usize = 9;

const IDENTITY_CHOSEN_SENTENCE: &str = " A git step whose argv named an ssh address had ssh \
     offer first the key the person's git.identities setting names for that address's owner, and \
     after it only the keys their ssh configuration names for the host.";

const IDENTITY_PICKED_SENTENCE: &str = " A git step whose argv named an ssh address had ssh offer \
     first the key the person chose for that address's owner at a run prompt, and after it only the \
     keys their ssh configuration names for the host.";

const IDENTITY_PICKED_DEFAULT_SENTENCE: &str = " A git step whose argv named an ssh address \
     offered whichever key ssh tried first, as the person chose for that address's owner at a run \
     prompt.";

const IDENTITY_UNCHOSEN_SENTENCE: &str = " A git step whose argv named an ssh address offered \
     whichever key ssh tried first, since neither the person's git.identities setting nor a choice \
     of theirs at a run prompt names a readable public key for that address's owner.";

const IDENTITY_NO_ADDRESS_SENTENCE: &str = " A git step that named its remote by name, such as \
     `origin`, or by an address that is not ssh, was given no key by git.identities, which applies \
     only to an ssh address written in the argv; where it connected over ssh, ssh offered whichever \
     key it tried first.";

/// Said of a push over ssh when the session's agent socket was lent to the stage.
const IDENTITY_AGENT_LENT_SENTENCE: &str = " The session's ssh agent socket was lent to that git \
     step.";

/// Said of a push over ssh when the session has no agent socket to lend.
const IDENTITY_NO_AGENT_SENTENCE: &str = " No ssh agent socket was lent to that git step, because \
     the session has none, and a stage never reads a private key, so ssh had none to offer.";

/// How the fixed text about the key ssh offers begins, and the marker that it was said.
const IDENTITY_OPENING: &str = " Where GitHub refused a push for permission, the key ssh offered \
     may belong to an account that cannot write to the repository, and where it refused the login, \
     no key offered is registered. ssh offers an agent's keys in order and GitHub accepts the first \
     one any account holds, so a retry offers the same key. Only the person chooses the key, for a \
     git step whose argv names the ssh address: ";

/// Said where a run prompt will list the keys for the owner a step names.
const IDENTITY_PROMPT_CLAUSE: &str = "at its run prompt in the terminal interface, which offers \
     the public keys ssh could use, or for one already chosen where the retry's `scopes` names \
     `ssh-key`, or ";

/// The way to choose a key that needs no prompt.
const IDENTITY_ENTRY_CLAUSE: &str = "with an entry \
     `\"git\": {\"identities\": {\"<host>/<owner>\": \"<public key file>\"}}` in \
     ~/.bravebot/settings.json.";

/// Ends the text where a run prompt will list keys.
const IDENTITY_PROMPT_TAIL: &str = " In the terminal interface `/reach keys` lists the keys \
     chosen at a prompt and `/reach keys remove <number>` forgets one, so the next prompt asks \
     again. In either case ask them rather than retrying.";

/// Ends the text where no run prompt will list keys, since the prompt offers from two to nine
/// public keys and asks only about an owner no setting or earlier choice names.
const IDENTITY_NO_PROMPT_TAIL: &str = " No run prompt will list keys for this step, so ask them \
     rather than retrying.";

/// Said of a push over ssh where the session would accept an `unconfined` request.
const IDENTITY_UNCONFINED_WAY_OUT: &str = " A push refused for permission can also be run again \
     as its own line with 'unconfined': true, saying in 'why' what the sandbox refused, which the \
     person is asked to approve and which runs with no sandbox.";

/// Why a stage cannot run under `mode`, where the mode is strict or read-only and the backend
/// cannot hold a connect to a unix socket that has a path to the stage's write rows, or `None`.
///
/// Such a socket reaches whatever serves it with that server's authority: `systemd-run --user`
/// through the user's D-Bus starts a program outside the sandbox, which writes wherever the person
/// can. Strict keeps a program from everything it was not granted and read-only keeps it from
/// writing the session's directories, so each is refused where that cannot hold. `workspace-write`
/// runs the stage, and `doctor`, `/status` and the opening screen say the sockets are reachable
/// (SANDBOX-3, SANDBOX-22).
fn cannot_keep_from_sockets(
    mode: SandboxMode,
    capabilities: &bravebot_sandbox::policy::Capabilities,
) -> Option<String> {
    (matches!(mode, SandboxMode::Strict | SandboxMode::ReadOnly)
        && !capabilities.governs_sockets_with_a_path)
        .then(|| {
            format!(
                "the sandbox mode is {mode} (sandbox.mode) and this kernel cannot keep a program \
                 from a unix socket that has a path, such as the user's D-Bus, the SSH agent or \
                 docker.sock; {mode} needs Landlock ABI 9, which is Linux 7.1 or newer"
            )
        })
}

/// What a local server's policy becomes under the session's decision about the network
/// (SANDBOX-20, SANDBOX-24), and the variables that point it at the proxy, where there is one.
///
/// A server is not a stage and carries none of a stage's reasons to keep the network, so a closed
/// session gives it none, a runner's fetch included. Under a host list it is held to the proxy
/// that applies the list, with the registry hosts of the toolchain its program is. The error says
/// why the platform cannot hold it, and the server is not started, as a stage is not.
pub fn held_to_the_session_network(
    policy: SandboxPolicy,
    program: &Path,
    network: Network,
    hosts: Option<&Hosts>,
    capabilities: &bravebot_sandbox::policy::Capabilities,
) -> Result<(SandboxPolicy, Vec<(String, String)>), String> {
    held_under_the_session_network(
        policy,
        program,
        network,
        hosts,
        capabilities,
        &|policy, list| {
            let proxy = crate::host_proxy::proxy_for(list)
                .map_err(|error| format!("the allowed-hosts proxy could not start: {error}"))?;
            let held = policy.allow_network_egress_only_to(proxy.addr().port());
            #[cfg(target_os = "linux")]
            let held = match proxy.socket_path() {
                Some(socket) => held.with_egress_through(socket),
                None => held,
            };
            Ok((held, proxy.environment()))
        },
    )
}

/// The hosts a local server is held to: the list its declaration carries, narrowed to what the
/// session's list also allows, and the session's denied entries and `onUnlisted` as they are
/// (SANDBOX-24, SERVERS-10).
///
/// A declaration can only narrow the session's list, so a server file written by someone else
/// cannot reach a host the person's own settings or the managed layer do not allow. Where the
/// session has no allowed list, the declaration's is the list. A declaration with no list leaves
/// the session's as it is.
pub fn hosts_for_a_server(session: Option<&Hosts>, declared: Option<&[String]>) -> Option<Hosts> {
    let Some(declared) = declared else {
        return session.cloned();
    };
    let kept = |entry: &String| {
        session
            .and_then(|hosts| hosts.allowed.as_deref())
            .is_none_or(|allowed| {
                let (list, _) = bravebot_sandbox::hosts::HostList::parse(
                    allowed.iter().map(|host| host.entry.as_str()),
                    [],
                );
                bravebot_sandbox::hosts::HostRule::parse(entry)
                    .is_some_and(|rule| list.allows_every_name_of(&rule))
            })
    };
    let entry = |entry: &String| bravebot_config::sandbox_network::HostEntry {
        entry: entry.clone(),
        by: None,
    };
    Some(Hosts {
        allowed: Some(declared.iter().filter(|e| kept(e)).map(entry).collect()),
        denied: session
            .map(|hosts| hosts.denied.clone())
            .unwrap_or_default(),
        on_unlisted: session.and_then(|hosts| hosts.on_unlisted),
    })
}

/// The policy [`held_to_the_session_network`] would give a server, for a caller that must start no
/// proxy: where a host list holds the server to one, the policy is limited to a port that is not
/// any proxy's, which is all a view of it reads. Every refusal is the one a launch would make.
pub fn foreseen_under_the_session_network(
    policy: SandboxPolicy,
    program: &Path,
    network: Network,
    hosts: Option<&Hosts>,
    capabilities: &bravebot_sandbox::policy::Capabilities,
) -> Result<SandboxPolicy, String> {
    held_under_the_session_network(
        policy,
        program,
        network,
        hosts,
        capabilities,
        &|policy, _| Ok((policy.allow_network_egress_only_to(0), Vec::new())),
    )
    .map(|(held, _)| held)
}

/// A server's policy under the session network, and the variables that point it at the proxy.
type Held = Result<(SandboxPolicy, Vec<(String, String)>), String>;

/// A refused allowed-hosts list as the error a step is stopped with. The sentence is fixed; the
/// entry travels beside it for the person alone.
fn refused_as_not_confined(step: &Step, refused: &crate::host_proxy::Refused) -> ExecError {
    ExecError::NotConfined {
        program: step.program.clone(),
        detail: refused.to_string(),
        before: crate::exec::StartedBefore::Nothing,
        network_closed: false,
        entry: refused.entry().map(str::to_string),
    }
}

/// The list a server's proxy decides with: the entries of `hosts` and the registry hosts of the
/// toolchain the server's program is (SANDBOX-24, SERVERS-10).
fn host_list_of_a_server(
    program: &Path,
    hosts: &Hosts,
) -> Result<bravebot_sandbox::hosts::HostList, crate::host_proxy::Refused> {
    let spelled = |entries: &[bravebot_config::sandbox_network::HostEntry]| {
        entries
            .iter()
            .map(|host| host.entry.clone())
            .collect::<Vec<_>>()
    };
    let toolchains: Vec<Toolchain> = Toolchain::of(&canonical(program)).into_iter().collect();
    crate::host_proxy::list_for(
        &spelled(hosts.allowed.as_deref().unwrap_or_default()),
        &spelled(&hosts.denied),
        false,
        &toolchains,
    )
}

/// The decision [`held_to_the_session_network`] makes, with the way a host list is applied left to
/// `limit`, so that a launch and a view of one cannot decide differently.
fn held_under_the_session_network(
    policy: SandboxPolicy,
    program: &Path,
    network: Network,
    hosts: Option<&Hosts>,
    capabilities: &bravebot_sandbox::policy::Capabilities,
    limit: &dyn Fn(SandboxPolicy, &bravebot_sandbox::hosts::HostList) -> Held,
) -> Held {
    if network.is_closed() {
        let closed = policy.without_network_egress();
        return match cannot_close_the_network(
            &closed,
            capabilities,
            network_pinned_by_the_administrator(),
        ) {
            Some(detail) => Err(detail),
            None => Ok((closed, Vec::new())),
        };
    }
    let Some(hosts) = hosts.filter(|hosts| hosts.allowed.is_some()) else {
        return Ok((policy, Vec::new()));
    };
    let list = host_list_of_a_server(program, hosts).map_err(|refused| refused.to_string())?;
    let (held, environment) = limit(policy, &list)?;
    match cannot_hold_to_the_proxy(&held, capabilities) {
        Some(detail) => Err(detail),
        None => Ok((held, environment)),
    }
}

/// Why a backend cannot apply `policy`, where the policy withholds the network and the backend does
/// not enforce that, or `None`.
///
/// Refused here with the setting named rather than left to the backend's own words: a person who
/// closed the network and is told only that confinement failed does not know which setting asked
/// for what the platform cannot do, and a backend that applied the rest and left the network open
/// would be the silent fall back to `open` the setting exists to forbid.
///
/// It names what lets the program start: `run.network` set to `open`, or `--run-network open`.
/// Where the administrator's managed file pinned the network closed (`pinned`), no setting of the
/// person's opens it, so it says that and names no setting to change (SANDBOX-20).
fn cannot_close_the_network(
    policy: &SandboxPolicy,
    capabilities: &bravebot_sandbox::policy::Capabilities,
    pinned: bool,
) -> Option<String> {
    (!policy.allow_network && !capabilities.network_denial_enforced).then(|| match pinned {
        true => "the administrator's managed settings close the network for this session \
                 (run.network), which no flag or settings file of the person's can open, and this \
                 platform cannot deny it to a program that does not need it"
            .to_string(),
        false => "the network is closed for this session (run.network) and this platform cannot \
                  deny it to a program that does not need it. The person can let it start by \
                  running bravebot with --run-network open, or by setting run.network to open in \
                  their own settings where no project settings file closes it"
            .to_string(),
    })
}

/// Whether the network this process settled was closed by the administrator's managed file.
fn network_pinned_by_the_administrator() -> bool {
    crate::tools::network_pinned_closed(bravebot_config::settled_run_network())
}

/// Why a backend cannot hold `policy` to the proxy that applies the host list, where the policy
/// limits egress to the proxy's port and the backend cannot, or `None`.
///
/// Refused with the setting named for the reason the closed network is: a backend that applied
/// the rest and left egress open would give a program the proxy variables and a way around them,
/// and a person who set a host list would be told only that confinement failed.
fn cannot_hold_to_the_proxy(
    policy: &SandboxPolicy,
    capabilities: &bravebot_sandbox::policy::Capabilities,
) -> Option<String> {
    (policy.egress_only_to.is_some() && !capabilities.egress_limited_to_a_port).then(|| {
        "the allowed hosts are set for this session (sandbox.network.allowedHosts) or this server \
         (allowed_hosts) and this platform cannot hold a program to the proxy that applies them"
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

/// The names Windows programs read for where to put a temporary file.
const TEMPORARY_NAMES: [&str; 2] = ["TEMP", "TMP"];

/// The variables a command starts with: `held`, less the names `changes` removes and with the ones
/// it sets, and `TEMP` and `TMP` naming `temporary` where there is one. They are set before
/// `changes`, so a stage's own assignment of either wins, as `TMPDIR` is left to the line on the
/// other platforms ([TRUST-14](../../../docs/specs/trust-map.md)). A name differing only in case is
/// replaced, not kept beside them, where `fold_case` says variable names are compared that way.
fn temporary_defaults<'a>(
    held: Vec<Pair>,
    temporary: Option<&'a Path>,
    changes: impl Iterator<Item = (&'a std::ffi::OsStr, Option<&'a std::ffi::OsStr>)>,
    fold_case: bool,
) -> Vec<Pair> {
    let held = match temporary {
        Some(temporary) => overlay(
            held,
            TEMPORARY_NAMES
                .iter()
                .map(|name| (OsStr::new(name), Some(temporary.as_os_str()))),
            fold_case,
        ),
        None => held,
    };
    overlay(held, changes, fold_case)
}

/// The directory stages are given to write temporary files in, and whose it is. On Windows with a
/// scratch directory it is a directory inside that, so the account's temporary directory, which
/// holds other programs' files and is not the session's, is a grant to no stage and no entry is
/// written onto what is in it (SANDBOX-12). Everywhere else, and with no scratch directory, it is
/// the account's `temporary`.
fn stage_temporary(
    prelude: Prelude,
    temporary: PathBuf,
    scratch: Option<&Path>,
) -> (PathBuf, TemporaryOwner) {
    match (prelude, scratch) {
        (Prelude::Windows, Some(scratch)) => (scratch.join("temp"), TemporaryOwner::Session),
        _ => (temporary, TemporaryOwner::Account),
    }
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

/// The macOS developer directory the keyed base reads, resolved once per process, and `None`
/// where the platform has none.
pub(crate) fn developer_directory() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        bravebot_sandbox::macos::developer_directory()
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// Where `environment` moves what the scope of the step that resolved to `resolved` reads.
fn reaches(
    resolved: &Path,
    scope: Scope,
    home: &Path,
    moved: Option<&Path>,
    environment: &[(String, String)],
) -> Vec<Reach> {
    let program = program_name(resolved).unwrap_or_default();
    environment_reach(scope, &program, home, moved, environment)
}

/// This process's variables, those whose name and value are text.
fn process_environment() -> Vec<(String, String)> {
    std::env::vars().collect()
}

fn variable(environment: &[(String, String)], name: &str) -> Option<String> {
    lookup(environment, name, FOLD_CASE)
}

/// The socket the process's own `SSH_AUTH_SOCK` names, as a stage that carries a scope that lends
/// the agent is granted it, or `None` where there is none or the row is refused
/// ([`judged_socket`]).
fn agent_socket(
    home: Option<&Path>,
    moved: Option<&Path>,
    environment: &[(String, String)],
) -> Option<PathBuf> {
    judged_socket(
        Path::new(&variable(environment, "SSH_AUTH_SOCK")?),
        home?,
        moved,
    )
}

fn lookup(environment: &[(String, String)], name: &str, fold_case: bool) -> Option<String> {
    environment
        .iter()
        .find(|(held, _)| same_variable(held.as_ref(), name.as_ref(), fold_case))
        .map(|(_, value)| value.clone())
        .filter(|value| !value.is_empty())
}

/// The first of `directories` that is or lies inside a credential location, or on Windows holds
/// one, and that location (SANDBOX-18). See [`Confinement::root_reaching_credentials`].
fn reaching_credentials<'a>(
    prelude: Prelude,
    home: Option<&Path>,
    moved: Option<&Path>,
    directories: impl Iterator<Item = &'a PathBuf>,
) -> Option<(&'a Path, PathBuf)> {
    // The locations under the home need one to name them; `/Library/Keychains` does not.
    let mut locations = home
        .map(|home| credential_locations(prelude, home, moved))
        .unwrap_or_default();
    if prelude == Prelude::MacOs {
        locations.push(PathBuf::from(
            bravebot_sandbox::base::MACOS_SYSTEM_KEYCHAINS,
        ));
    }
    let reaches = |root: &Path, candidate: &Path| match prelude {
        Prelude::Windows => overlaps(root, candidate),
        Prelude::MacOs => {
            overlaps(root, candidate) && root.components().count() >= candidate.components().count()
        }
        Prelude::Linux => root.starts_with(candidate),
    };
    directories.into_iter().find_map(|root| {
        locations.iter().find_map(|location| {
            let resolved = canonical(location);
            [location, &resolved]
                .into_iter()
                .any(|candidate| reaches(root, candidate))
                .then(|| (root.as_path(), location.clone()))
        })
    })
}

fn canonical(path: &Path) -> PathBuf {
    without_verbatim_prefix(path.canonicalize().unwrap_or_else(|_| path.to_path_buf()))
}

/// Whether a program, known by the file name it resolved to, starts a file or a program its
/// arguments name: a shell, a language's interpreter, or a wrapper that runs its operand.
fn runs_what_it_is_given(name: &str) -> bool {
    const INTERPRETERS: &[&str] = &[
        "sh",
        "bash",
        "dash",
        "zsh",
        "ksh",
        "csh",
        "tcsh",
        "fish",
        "node",
        "nodejs",
        "deno",
        "bun",
        "npx",
        "tsx",
        "ts-node",
        "ruby",
        "jruby",
        "perl",
        "php",
        "lua",
        "julia",
        "groovy",
        "swift",
        "pwsh",
        "osascript",
        "Rscript",
        "awk",
        "gawk",
        "mawk",
        "tclsh",
        "pypy",
        "pypy3",
    ];
    // `python`, `python3`, `python3.12`: the name is `python` and a version.
    let versioned = name
        .strip_prefix("python")
        .is_some_and(|rest| rest.chars().all(|c| c.is_ascii_digit() || c == '.'));
    versioned || is_wrapper(name, WrapperQuestion::RunsItsOperand) || INTERPRETERS.contains(&name)
}

/// Whether `found` accepts one of the names an argument could be a file under: the argument
/// whole, the pieces left after cutting it at white space, shell punctuation and `=`, and the words
/// a shell would read in it ([`shell_words`]). The argument and each word is also read as an
/// option, so what follows each `=` in it is a name, and so is each piece between two.
///
/// The whole argument is a name because a file's name may hold any character the others cut at.
/// The words read with quoting are names because a command string may quote a name that holds
/// one, as `sh -c "'run script.sh'"` does, and a cut at every quote would split it. A name that is
/// not a file is looked up and found nothing, so more names only ever ask about more lines. The
/// names are slices of the text and are offered one at a time, so a long argument is not copied
/// once for each `=` in it.
fn any_name_in(arg: &str, mut found: impl FnMut(&str) -> bool) -> bool {
    let mut found = |name: &str| !name.is_empty() && found(name);
    if arg
        .split(|c: char| c.is_whitespace() || ";&|()<>\"'`$=".contains(c))
        .any(&mut found)
    {
        return true;
    }
    std::iter::once(arg.to_string())
        .chain(shell_words(arg))
        .any(|text| {
            found(&text)
                || text
                    .match_indices('=')
                    .any(|(at, _)| found(&text[at + 1..]))
                || text.split('=').any(&mut found)
        })
}

/// The words a POSIX shell reads in `text`: cut at unquoted white space and `;&|()<>` and a
/// backtick, with single quotes holding every character, double quotes holding every character but
/// `\` before `"`, `\`, `$` or a backtick, and a `\` outside quotes holding the next character. A
/// `\` before a newline joins the lines. A quote left open runs to the end of the text. Nothing is
/// expanded, so `$HOME` stays the characters it is spelled with.
fn shell_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_word = true;
                word.extend(chars.by_ref().take_while(|&held| held != '\''));
            }
            '"' => {
                in_word = true;
                while let Some(held) = chars.next() {
                    match held {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some(escaped @ ('"' | '\\' | '$' | '`')) => word.push(escaped),
                            Some('\n') => {}
                            Some(other) => word.extend(['\\', other]),
                            None => word.push('\\'),
                        },
                        _ => word.push(held),
                    }
                }
            }
            '\\' => match chars.next() {
                Some('\n') => {}
                escaped => {
                    in_word = true;
                    word.extend(escaped);
                }
            },
            c if c.is_whitespace() || ";&|()<>`".contains(c) => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            c => {
                in_word = true;
                word.push(c);
            }
        }
    }
    if in_word {
        words.push(word);
    }
    words
}

/// Whether `file` has an execute bit set for anybody, which is how a program a stage wrote and made
/// runnable is told from a file it only wrote. False where the platform has no such bit.
fn is_executable(file: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(file).is_ok_and(|found| found.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        let _ = file;
        false
    }
}

/// Whether the file a step resolved to is git, the one program whose stage writes a `.git`
/// (SANDBOX-14, SANDBOX-18): a file named `git`, or `git.exe` in any case on Windows, beneath none
/// of `planted`, the rows a stage may write, so a file a stage could have put there is not git by
/// being named `git`.
fn is_git(prelude: Prelude, resolved: &Path, planted: &[&Path]) -> bool {
    let file = canonical(resolved);
    let named_git = match (prelude, resolved.file_name()) {
        (Prelude::Windows, Some(name)) => name.eq_ignore_ascii_case("git.exe"),
        (_, name) => name == Some(OsStr::new("git")),
    };
    named_git
        && !planted
            .iter()
            .any(|row| file.starts_with(row) || file.starts_with(canonical(row)))
}

/// Whether a stage holding `rows` may write `file`: the narrowest row holding it lies beneath the
/// narrowest of `refused` holding it, since a write row beneath a refusal stands and one equal to
/// it is withdrawn ([`SandboxPolicy::unwritable`]).
fn written_under(rows: &[PathBuf], refused: &[PathBuf], file: &Path) -> bool {
    let narrowest = |rows: &[PathBuf]| {
        rows.iter()
            .filter(|row| file.starts_with(row))
            .map(|row| row.components().count())
            .max()
    };
    match (narrowest(rows), narrowest(refused)) {
        (Some(row), Some(refusal)) => row > refusal,
        (row, None) => row.is_some(),
        (None, Some(_)) => false,
    }
}

/// Whether one of `a` and `b` is the other or lies inside it, comparing components without regard
/// to case, as a Windows file system does.
///
/// A component is folded as text only when both sides are text; otherwise the bytes are compared,
/// folding ASCII, so two names that differ only in what a lossy rendering replaces stay different.
fn overlaps(a: &Path, b: &Path) -> bool {
    let same_name = |left: &OsStr, right: &OsStr| match (left.to_str(), right.to_str()) {
        (Some(left), Some(right)) => left.to_lowercase() == right.to_lowercase(),
        _ => left.eq_ignore_ascii_case(right),
    };
    a.components()
        .zip(b.components())
        .all(|(left, right)| same_name(left.as_os_str(), right.as_os_str()))
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
/// token beside `~/.cargo/bin`. A program installed inside the home is read as the file a person
/// read and not as its directory, which the caller adds. A row that is, holds or lies inside a
/// credential location is left out whatever named it.
pub(crate) fn program_reads(
    program: &Path,
    searched: &[PathBuf],
    home: Option<&Path>,
    moved: Option<&Path>,
) -> Vec<PathBuf> {
    reads_of(program, searched, home, moved, false)
}

/// [`program_reads`] for a local server, which has no toolchain list to bring the installation a
/// version manager keeps in the home. The program's own `bin` directory brings its parent where it
/// sits deeper in the home than a directory directly inside it, as `nvm` installs one, since that
/// parent is the installation the program came from.
pub(crate) fn server_program_reads(
    program: &Path,
    searched: &[PathBuf],
    home: Option<&Path>,
    moved: Option<&Path>,
) -> Vec<PathBuf> {
    reads_of(program, searched, home, moved, true)
}

fn reads_of(
    program: &Path,
    searched: &[PathBuf],
    home: Option<&Path>,
    moved: Option<&Path>,
    with_installation: bool,
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
        .filter(|installation| with_installation && below_the_top_of_home(installation))
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
    match home.as_deref() {
        Some(home) => {
            bravebot_sandbox::rules::outside_every_credential_location(reads, home, moved)
        }
        None => reads,
    }
}

#[cfg(test)]
mod tests;
