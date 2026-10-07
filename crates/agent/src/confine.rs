//! The profile a program a person asked for runs under.
//!
//! `run` starts the programs a plan names. Each step is held to the platform's base, the list its
//! resolved binary brings, the credential scope its argv names, the places its binary is installed,
//! and the directories the session was opened on, and to nothing else a person's account can
//! reach. `docs/specs/sandboxing.md` decides every row; this composes them for one step.
//!
//! Nothing a program printed, and no value the model supplied, reaches a row. The inputs are the
//! compiled [`Step`], which a person read, and the session's own directories.

use crate::confirm::{Carried, Confined};
use crate::exec::ExecError;
use bravebot_core::command::Step;
use bravebot_sandbox::Variables;
use bravebot_sandbox::base::{Prelude, base};
use bravebot_sandbox::policy::SandboxPolicy;
use bravebot_sandbox::scope::{Scope, gh_configuration};
use bravebot_sandbox::toolchain::Toolchain;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

/// What a `run` program may reach in this session, before any step is read.
#[derive(Debug, Clone)]
pub struct Confinement {
    prelude: Prelude,
    temporary: PathBuf,
    developer: Option<PathBuf>,
    home: Option<PathBuf>,
    roots: Vec<PathBuf>,
    scratch: Option<PathBuf>,
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
            developer_directory(),
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
        developer: Option<PathBuf>,
        home: Option<&Path>,
        roots: Vec<PathBuf>,
        scratch: Option<&Path>,
    ) -> Self {
        Self {
            prelude,
            temporary,
            developer,
            home: home.map(canonical),
            roots: roots.iter().map(|root| canonical(root)).collect(),
            scratch: scratch.map(canonical),
            #[cfg(test)]
            unconfinable: None,
        }
    }

    /// This confinement, failing for the step that starts `program` as the platform would for one
    /// it cannot confine.
    #[cfg(test)]
    pub(crate) fn failing_for(mut self, program: &str) -> Self {
        self.unconfinable = Some(program.to_string());
        self
    }

    /// The toolchain list and the credential scope a step brings to its profile.
    ///
    /// The one place either is decided, read by [`Confinement::policy`] to build the rows and by
    /// [`Confinement::describe`] and [`Confinement::profile`] to say which rows there are. Neither
    /// is brought where the session names no home directory, since both are rows under it.
    fn carries(&self, step: &Step) -> (Option<Toolchain>, Option<Scope>) {
        if self.home.is_none() {
            return (None, None);
        }
        (
            Toolchain::of(&step.resolved),
            Scope::of(&step.resolved, &step.args, &step.environment),
        )
    }

    /// What the profile of each step of `steps` holds beyond the base and the places its programs
    /// are installed, for the prompt a person approves from.
    pub fn describe(&self, steps: &[&Step]) -> Confined {
        Confined {
            directories: self
                .roots
                .iter()
                .chain(self.scratch.iter())
                .cloned()
                .collect(),
            carried: steps
                .iter()
                .filter_map(|step| {
                    let (toolchain, scope) = self.carries(step);
                    (toolchain.is_some() || scope.is_some()).then(|| Carried {
                        program: step.program.clone(),
                        toolchain,
                        scope,
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
        let mut policy = base(
            self.prelude,
            &self.temporary,
            self.developer.as_deref(),
            self.home.as_deref(),
        )
        .allow_git_directory_writes();

        if let Some(home) = self.home.as_deref() {
            let (toolchain, scope) = self.carries(step);
            if let Some(toolchain) = toolchain {
                policy = toolchain.grant(policy, self.prelude, home);
            }
            if let Some(scope) = scope {
                policy = scope.grant(policy, home);
                if scope == Scope::Remote
                    && resolved.file_name().is_some_and(|name| name == "gh")
                    && let Some(directory) = gh_configuration(home, environment)
                {
                    policy = policy.allow_read(directory);
                }
                if scope == Scope::Remote
                    && let Some(socket) = variable(environment, "SSH_AUTH_SOCK")
                {
                    policy = policy.allow_write(socket);
                }
            }
        }

        let searched: Vec<PathBuf> = variable(environment, "PATH")
            .map(|path| std::env::split_paths(&path).collect())
            .unwrap_or_default();
        for path in program_reads(resolved, &searched, self.home.as_deref()) {
            policy = policy.allow_read(path);
        }
        // The two files the step starts, as files: a program installed inside the home is in no
        // directory row, and a person read exactly these two.
        policy = policy.allow_read(canonical(resolved));
        if started_as != resolved {
            policy = policy.allow_read(started_as);
        }

        for root in &self.roots {
            policy = policy.allow_read(root).allow_write(root);
        }
        if let Some(scratch) = &self.scratch {
            policy = policy.allow_read(scratch).allow_write(scratch);
        }
        policy.starting_in(directory)
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
        for step in steps {
            let (toolchain, scope) = self.carries(step);
            toolchains.extend(toolchain.map(Toolchain::name));
            scopes.extend(scope.map(Scope::name));
        }
        let named = |names: std::collections::BTreeSet<&str>| match names.is_empty() {
            true => "none".to_string(),
            false => names.into_iter().collect::<Vec<_>>().join(", "),
        };
        format!(
            "Confinement: programs could read and write {} and the temporary directory, and read \
             the system and program directories and git's configuration files; beyond those \
             they reached only what a toolchain list or credential scope added for the steps \
             that named one (toolchain lists: {}; credential scopes: {}). Any other path is \
             refused by the operating system as `Operation not permitted` or `Permission \
             denied`.",
            directories.join(", "),
            named(toolchains),
            named(scopes),
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
        let wanted = self.policy(step, directory, &readable);
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
pub fn stated_to_the_planner(confine_runs: bool) -> Option<&'static str> {
    stated(confine_runs, Prelude::current())
}

fn stated(confine_runs: bool, prelude: Option<Prelude>) -> Option<&'static str> {
    if !confine_runs {
        return None;
    }
    Some(match prelude {
        Some(_) => {
            "Programs this tool starts are confined. Each may reach only the directories the \
             session was opened on, the scratch directory and the temporary directory, all read \
             and written, the system and program directories and git's configuration files, \
             read, the caches of the toolchain it belongs to, and the credential scope its \
             command names. A path outside those is refused by the operating system as \
             `Operation not permitted` or `Permission denied`, so a program that reports either \
             for such a path was stopped by the sandbox and not by a fault in the machine. Only \
             the person widens it (`/add-dir`, `--add-dir`); a command cannot ask for more."
        }
        None => {
            "Programs this tool starts are not confined on this platform: they run with the \
             access of the person's own account."
        }
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

/// What `xcode-select -p` names, resolved once, where this is macOS.
///
/// The `/usr/bin` developer shims run the real program out of it. Resolved from the machine and
/// never from a step's own `DEVELOPER_DIR=`, so a line cannot choose which directory is granted.
fn developer_directory() -> Option<PathBuf> {
    static SELECTED: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    SELECTED
        .get_or_init(|| {
            if !cfg!(target_os = "macos") {
                return None;
            }
            let selected = Command::new("/usr/bin/xcode-select")
                .arg("-p")
                .output()
                .ok()
                .filter(|output| output.status.success())?;
            let named = String::from_utf8_lossy(&selected.stdout).trim().to_string();
            (!named.is_empty())
                .then(|| canonical(Path::new(&named)))
                .filter(|path| path.is_dir())
        })
        .clone()
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

    fn confinement(roots: &[&str]) -> Confinement {
        Confinement::new(
            Prelude::Linux,
            PathBuf::from("/tmp"),
            None,
            Some(Path::new(HOME)),
            roots.iter().map(PathBuf::from).collect(),
            Some(Path::new("/var/scratch")),
        )
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

    /// A session that names no home directory grants no row under it, so it describes none.
    #[test]
    fn a_session_with_no_home_describes_no_toolchain_and_no_scope() {
        let confined = Confinement::new(
            Prelude::Linux,
            PathBuf::from("/tmp"),
            None,
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
        let said = stated(true, Some(Prelude::Linux)).expect("a confining turn says something");

        assert!(said.contains("confined"), "{said}");
        assert!(said.contains("`Operation not permitted`"), "{said}");
        assert!(said.contains("`Permission denied`"), "{said}");
        assert!(
            said.contains("/add-dir") && said.contains("--add-dir"),
            "{said}"
        );
    }

    /// A planner told of a boundary its programs do not have would stop reaching for paths they
    /// can reach, so a turn that does not confine says nothing, whatever the platform.
    #[test]
    fn a_turn_that_does_not_confine_says_nothing_of_confinement() {
        assert_eq!(stated(false, Some(Prelude::Linux)), None);
        assert_eq!(stated(false, Some(Prelude::MacOs)), None);
        assert_eq!(stated(false, None), None);
    }

    /// Windows has no base, so the same sentence would be false there. The regression it rejects
    /// is the confined sentence on a platform whose programs run with the person's own access.
    #[test]
    fn a_platform_with_no_base_says_its_programs_are_not_confined() {
        let said = stated(true, None).expect("a confining turn says something");

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
            developer_directory(),
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
