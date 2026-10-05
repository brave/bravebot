//! The profile a program a person asked for runs under.
//!
//! `run` starts the programs a plan names. Each step is held to the platform's base, the list its
//! resolved binary brings, the credential scope its argv names, the places its binary is installed,
//! and the directories the session was opened on, and to nothing else a person's account can
//! reach. `docs/specs/sandboxing.md` decides every row; this composes them for one step.
//!
//! Nothing a program printed, and no value the model supplied, reaches a row. The inputs are the
//! compiled [`Step`], which a person read, and the session's own directories.

use crate::exec::ExecError;
use bravebot_core::command::Step;
use bravebot_sandbox::Variables;
use bravebot_sandbox::base::{Prelude, base};
use bravebot_sandbox::policy::SandboxPolicy;
use bravebot_sandbox::scope::Scope;
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
    /// build one on. That is Windows, which the decision carves out, and a step there runs as it
    /// always has.
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
            args,
            environment: assigned,
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
            if let Some(toolchain) = Toolchain::of(resolved) {
                policy = toolchain.grant(policy, self.prelude, home);
            }
            if let Some(scope) = Scope::of(resolved, args, assigned) {
                policy = scope.grant(policy, home);
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

    /// `command`, confined to what its step may reach, or the reason it cannot be.
    ///
    /// Refused rather than started unconfined where the platform's mechanism is missing or will
    /// not apply the policy: a program a person approved runs under the profile or not at all.
    pub fn wrap(
        &self,
        command: Command,
        step: &Step,
        directory: &Path,
    ) -> Result<Command, ExecError> {
        let environment = effective_environment(&command);
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
        let args: Vec<String> = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect();
        #[cfg(unix)]
        {
            use bravebot_sandbox::Environment;
            sandbox
                .command(
                    &command.get_program().to_string_lossy(),
                    &args,
                    &policy,
                    &Environment::Only(variables),
                )
                .map_err(|error| not_confined(error.to_string()))
        }
        #[cfg(not(unix))]
        {
            let _ = (sandbox, args, policy, variables);
            Err(not_confined(
                "this platform hands back no command to confine".to_string(),
            ))
        }
    }
}

/// The variables `command` will start with: this process's, less the names removed and with the
/// ones set.
fn effective_environment(command: &Command) -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
    let mut held: Vec<(std::ffi::OsString, std::ffi::OsString)> = std::env::vars_os().collect();
    for (name, value) in command.get_envs() {
        held.retain(|(existing, _)| existing != name);
        if let Some(value) = value {
            held.push((name.to_os_string(), value.to_os_string()));
        }
    }
    held
}

fn variable(environment: &[(String, String)], name: &str) -> Option<String> {
    environment
        .iter()
        .find(|(held, _)| held == name)
        .map(|(_, value)| value.clone())
        .filter(|value| !value.is_empty())
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
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
}
