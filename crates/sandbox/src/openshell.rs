//! A backend that starts a program inside an NVIDIA OpenShell sandbox.
//!
//! OpenShell runs a workload in a container or a microVM it manages through a gateway, with a
//! supervisor beside it that applies a filesystem policy (Landlock) and a network policy (a proxy
//! that decides per binary). This backend drives the `openshell` program rather than a library:
//! the SDK is published on git only, which the dependency policy refuses.
//!
//! One value is one sandbox. It is created at the first [`Sandbox::spawn`], from the image the
//! caller named, with a policy that reaches no host; the program's real path is then read inside
//! the image and the hosts the caller allowed are granted to that binary alone, with the policy
//! confirmed loaded before anything is started. Dropping the value deletes the sandbox.
//!
//! The program is started with `openshell sandbox exec`, whose standard streams are relayed to
//! the process this crate hands back, so a caller speaks to it as it would to a native child.
//!
//! Prototype: no variable is passed into the sandbox. `exec` takes a variable as an argument, which
//! other processes of the account can read, so a value never travels that way; a later step passes
//! credentials as OpenShell providers instead.

use crate::policy::{Capabilities, ConfinementLevel, SandboxPolicy};
use crate::process::{self, ConfinedChild, Environment, Streams};
use crate::programs::{Inherits, Resolved, SearchPath};
use crate::{Sandbox, SandboxError};
use std::ffi::{OsStr, OsString};
use std::process::Output;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// What the name of every sandbox [`next_name`] makes begins with.
const PREFIX: &str = "bb-";

/// The name the `openshell` program is looked up by.
const PROGRAM: &str = "openshell";

/// The longest sandbox name OpenShell 0.1.7 accepts: a longer one is refused with "maximum length".
const MAX_NAME: usize = 19;

/// How long a sandbox has to become ready after it is created.
const READY_WITHIN: Duration = Duration::from_secs(60);

/// How long the supervisor has to load a policy that was set, in seconds.
const POLICY_LOADED_WITHIN_SECS: u32 = 30;

/// How long creating a sandbox may take, which includes pulling its image the first time.
const CREATED_WITHIN: Duration = Duration::from_secs(300);

/// How long confirming a policy may take: OpenShell's own wait, and time for it to say so.
const POLICY_SET_WITHIN: Duration = Duration::from_secs(POLICY_LOADED_WITHIN_SECS as u64 + 30);

/// How long any other `openshell` command may take: reading a state, listing, deleting, asking a
/// version, or finding a program in the image.
const ANSWERED_WITHIN: Duration = Duration::from_secs(30);

/// The variables the `openshell` program itself is given: it finds its gateway and its client
/// certificates under the home directory.
const CLI_INHERITS: &[&str] = &["HOME"];

/// The variables a caller may hand a program without this backend refusing: `PATH` and `HOME` name
/// places on this machine, which mean nothing inside the image, so they are left out rather than
/// passed on.
const DROPPED_VARIABLES: &[&str] = &["PATH", "HOME"];

/// Run `command` to its end, or kill it once `within` has passed, and take what it printed. It is
/// started under the gate every start of a program goes through ([`process::starting_a_program`])
/// and waited for outside it, so a channel being made on another thread is not inherited and the
/// gate is not held while `openshell` runs. A gateway that never answers holds nobody past `within`.
fn output_of(mut command: std::process::Command, within: Duration) -> Result<Output, SandboxError> {
    use std::io::Read;
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child =
        process::starting_a_program(|| command.spawn()).map_err(SandboxError::SpawnFailed)?;
    // Read on threads of their own, so a program that fills a pipe is not stopped by it, and
    // collected within the same deadline: a pipe held open by something the program left running is
    // given up on, its reader left to finish alone, rather than waited for.
    let (sent, received) = std::sync::mpsc::channel::<(bool, Vec<u8>)>();
    let read = |pipe: Option<Box<dyn Read + Send>>, out: bool| {
        let sent = sent.clone();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                pipe.read_to_end(&mut bytes).ok();
            }
            sent.send((out, bytes)).ok();
        });
    };
    read(
        child
            .stdout
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
        true,
    );
    read(
        child
            .stderr
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
        false,
    );
    drop(sent);
    let deadline = Instant::now() + within;
    let late = || {
        setup(format!(
            "`{PROGRAM}` did not finish within {} seconds",
            within.as_secs()
        ))
    };
    let status = loop {
        match child.try_wait().map_err(SandboxError::SpawnFailed)? {
            Some(status) => break status,
            None if Instant::now() >= deadline => {
                // Killed and reaped as far as the system allows; the refusal below is the answer.
                child.kill().ok();
                child.wait().ok();
                return Err(late());
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    };
    // A program that ended just before the bound still has its output read: what it printed is
    // given a moment past the deadline, and no more.
    let read_by = deadline.max(Instant::now() + Duration::from_secs(1));
    let (mut stdout, mut stderr) = (None, None);
    while stdout.is_none() || stderr.is_none() {
        match received.recv_timeout(read_by.saturating_duration_since(Instant::now())) {
            Ok((true, bytes)) => stdout = Some(bytes),
            Ok((false, bytes)) => stderr = Some(bytes),
            Err(_) => return Err(late()),
        }
    }
    Ok(Output {
        status,
        stdout: stdout.unwrap_or_default(),
        stderr: stderr.unwrap_or_default(),
    })
}

/// The `openshell` program, found in a directory SANDBOX-30 allows.
fn find_cli() -> Result<Resolved, SandboxError> {
    Resolved::find(OsStr::new(PROGRAM)).ok_or_else(|| SandboxError::Unavailable {
        platform: "openshell",
        detail: format!("`{PROGRAM}` was not found in a directory a confined program cannot write"),
    })
}

/// Run `openshell` with `args` and `HOME` alone, to its end or for `within` at most.
fn run_cli<I, S>(cli: &Resolved, args: I, within: Duration) -> Result<Output, SandboxError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command =
        crate::programs::start(cli, Inherits::Only(CLI_INHERITS), SearchPath::Reduced);
    command.args(args);
    output_of(command, within)
}

/// A program started inside one OpenShell sandbox.
#[derive(Debug)]
pub struct OpenShell {
    cli: Resolved,
    name: String,
    image: String,
    hosts: Vec<String>,
    /// How far the sandbox got: made, so it is deleted when this is dropped, and ready, so a start
    /// may use it.
    state: Mutex<Prepared>,
}

/// How far an [`OpenShell`] sandbox got.
#[derive(Debug, Default, Clone, Copy)]
struct Prepared {
    /// It exists, so it is deleted when its backend is dropped.
    created: bool,
    /// It reported `Ready` and its policy was confirmed, so a program may be started in it.
    ready: bool,
}

impl OpenShell {
    /// A sandbox named `name`, made from `image`, whose program may reach `hosts` and nothing else.
    ///
    /// Nothing is created yet. `None` where the `openshell` program is not found in a directory
    /// SANDBOX-30 allows, which a caller reports as the backend being unavailable.
    pub fn new(name: &str, image: &str, hosts: &[String]) -> Result<Self, SandboxError> {
        let cli = find_cli()?;
        Self::with_program(cli, name, image, hosts)
    }

    /// [`OpenShell::new`] with the `openshell` program already resolved.
    pub fn with_program(
        cli: Resolved,
        name: &str,
        image: &str,
        hosts: &[String],
    ) -> Result<Self, SandboxError> {
        if !is_sandbox_name(name) {
            return Err(setup(format!(
                "`{name}` is not a sandbox name: up to {MAX_NAME} lower-case letters, digits and `-`, starting with a letter"
            )));
        }
        if image.is_empty() || image.chars().any(|c| c.is_control() || c.is_whitespace()) {
            return Err(setup("the image is not one image reference".into()));
        }
        if let Some(host) = hosts.iter().find(|host| !is_host_rule(host)) {
            return Err(setup(format!(
                "`{host}` is not a host name or a `*.` domain"
            )));
        }
        Ok(Self {
            cli,
            name: name.to_string(),
            image: image.to_string(),
            hosts: hosts.to_vec(),
            state: Mutex::new(Prepared::default()),
        })
    }

    /// The sandbox's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Run `openshell` with `args`, waiting for it to finish.
    fn run(&self, args: &[&OsStr], within: Duration) -> Result<Output, SandboxError> {
        run_cli(
            &self.cli,
            args.iter()
                .copied()
                .chain([OsStr::new("--color"), OsStr::new("never")]),
            within,
        )
    }

    /// Run `openshell` with `args`, and refuse unless it succeeded, naming `what` it was doing.
    fn run_ok(
        &self,
        what: &str,
        args: &[&OsStr],
        within: Duration,
    ) -> Result<String, SandboxError> {
        let output = self.run(args, within)?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Err(setup(format!(
                "{what} failed: {}",
                last_line(&output.stderr).unwrap_or("no reason given")
            )))
        }
    }

    /// Create the sandbox, wait for it, find the program's real path and grant it the hosts.
    ///
    /// Each step that fails refuses: a sandbox whose policy cannot be confirmed loaded is never
    /// used, and one already created is deleted when this value is dropped.
    fn prepare(&self, program: &OsStr) -> Result<(), SandboxError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| setup("the sandbox's state was poisoned".into()))?;
        if state.ready {
            return Ok(());
        }
        // Made before and never made ready: that attempt refused, and so does this one, rather
        // than starting a program in a sandbox whose policy was never confirmed.
        if state.created {
            return Err(setup(format!(
                "the sandbox `{}` was made and never became ready",
                self.name
            )));
        }
        let first = PolicyFile::write(&policy_document(None, &[]))?;
        // Taken as made from the moment it is asked for: a create that is killed at its bound or
        // fails may still have been made by the gateway, so drop deletes it either way.
        state.created = true;
        let created = self.run_ok(
            "creating the sandbox",
            &[
                OsStr::new("sandbox"),
                OsStr::new("create"),
                OsStr::new("--name"),
                OsStr::new(&self.name),
                OsStr::new("--from"),
                OsStr::new(&self.image),
                OsStr::new("--policy"),
                first.path().as_os_str(),
                OsStr::new("--detach"),
                OsStr::new("--no-tty"),
                OsStr::new("--no-auto-providers"),
            ],
            CREATED_WITHIN,
        );
        if let Err(error) = created {
            // A create that never started reached no gateway, so nothing was made.
            if matches!(error, SandboxError::SpawnFailed(_)) {
                state.created = false;
            }
            return Err(error);
        }
        self.wait_until_ready()?;
        if !self.hosts.is_empty() {
            let binary = self.real_path(program)?;
            let granted = PolicyFile::write(&policy_document(Some(&binary), &self.hosts))?;
            self.run_ok(
                "setting the sandbox's policy",
                &[
                    OsStr::new("policy"),
                    OsStr::new("set"),
                    OsStr::new(&self.name),
                    OsStr::new("--policy"),
                    granted.path().as_os_str(),
                    OsStr::new("--wait"),
                    OsStr::new("--timeout"),
                    OsStr::new(&POLICY_LOADED_WITHIN_SECS.to_string()),
                ],
                POLICY_SET_WITHIN,
            )?;
        }
        state.ready = true;
        Ok(())
    }

    /// Wait for the sandbox to report itself ready, or refuse once it errs or the time is up.
    fn wait_until_ready(&self) -> Result<(), SandboxError> {
        let deadline = Instant::now() + READY_WITHIN;
        loop {
            let shown = self.run_ok(
                "reading the sandbox's state",
                &[
                    OsStr::new("sandbox"),
                    OsStr::new("get"),
                    OsStr::new(&self.name),
                ],
                ANSWERED_WITHIN,
            )?;
            match phase(&shown) {
                Some("Ready") => return Ok(()),
                Some("Error") => {
                    return Err(setup(format!(
                        "the sandbox `{}` failed to start; `openshell logs {}` says why",
                        self.name, self.name
                    )));
                }
                _ if Instant::now() >= deadline => {
                    return Err(setup(format!(
                        "the sandbox `{}` was not ready within {} seconds",
                        self.name,
                        READY_WITHIN.as_secs()
                    )));
                }
                _ => std::thread::sleep(Duration::from_millis(500)),
            }
        }
    }

    /// The real path of `program` inside the image, since a network rule names the executable the
    /// kernel reports and not a link to it.
    fn real_path(&self, program: &OsStr) -> Result<String, SandboxError> {
        let resolved = self.run_ok(
            "finding the program inside the sandbox",
            &[
                OsStr::new("sandbox"),
                OsStr::new("exec"),
                OsStr::new("-n"),
                OsStr::new(&self.name),
                OsStr::new("--no-tty"),
                OsStr::new("--"),
                OsStr::new("sh"),
                OsStr::new("-c"),
                OsStr::new(r#"readlink -f "$(command -v "$1")""#),
                OsStr::new("sh"),
                program,
            ],
            ANSWERED_WITHIN,
        )?;
        let path = resolved.trim();
        if is_real_path(path) {
            Ok(path.to_string())
        } else {
            Err(setup(format!(
                "`{}` was not found in the image `{}`",
                program.to_string_lossy(),
                self.image
            )))
        }
    }
}

impl Drop for OpenShell {
    #[expect(
        clippy::let_underscore_must_use,
        reason = "a sandbox that cannot be deleted is left for `openshell sandbox list` to show, and dropping has nobody to tell"
    )]
    fn drop(&mut self) {
        if self
            .state
            .get_mut()
            .map(|state| state.created)
            .unwrap_or(true)
        {
            let _ = self.run(
                &[
                    OsStr::new("sandbox"),
                    OsStr::new("delete"),
                    OsStr::new(&self.name),
                ],
                ANSWERED_WITHIN,
            );
        }
    }
}

impl Sandbox for OpenShell {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            level: ConfinementLevel::Kernel,
            mechanisms: vec!["openshell"],
            network_denial_enforced: true,
            egress_limited_to_a_port: false,
            grants_paths_that_do_not_exist: false,
            subtracts_from_a_grant: false,
            governs_sockets_with_a_path: true,
        }
    }

    /// Start `program` inside the sandbox, creating the sandbox first.
    ///
    /// `policy` describes paths on this machine, which the image does not have, so it is not
    /// applied: the image is the program's filesystem and the hosts given at construction are its
    /// network.
    fn spawn(
        &self,
        program: &OsStr,
        args: &[OsString],
        _policy: &SandboxPolicy,
        streams: Streams,
        environment: Environment,
    ) -> Result<ConfinedChild, SandboxError> {
        if let Environment::Only(variables) = &environment
            && let Some((name, _)) = variables.iter().find(|(name, _)| {
                !DROPPED_VARIABLES
                    .iter()
                    .any(|dropped| name.as_os_str() == OsStr::new(dropped))
            })
        {
            return Err(setup(format!(
                "`{}` would be passed into an OpenShell sandbox, which takes no variables yet",
                name.to_string_lossy()
            )));
        }
        self.prepare(program)?;
        let mut command =
            crate::programs::start(&self.cli, Inherits::Only(CLI_INHERITS), SearchPath::Reduced);
        command
            .args([
                "sandbox", "exec", "-n", &self.name, "--no-tty", "--color", "never", "--",
            ])
            .arg(program)
            .args(args);
        // The command's environment was settled by `programs::start`; the program inside the
        // sandbox receives none of it.
        process::start(command, streams, &Environment::Inherited)
    }

    /// Refused: this backend starts a server for its caller to speak to, not a stage a pipeline
    /// wires to the next, so there is no command it could hand back that would be confined.
    fn command(
        &self,
        _program: &OsStr,
        _args: &[OsString],
        _policy: &SandboxPolicy,
        _environment: &Environment,
    ) -> Result<std::process::Command, SandboxError> {
        Err(setup(
            "an OpenShell sandbox starts a server, not a stage of a pipeline".into(),
        ))
    }
}

/// A policy file written for one `openshell` call, removed when this is dropped.
struct PolicyFile(tempfile_path::TempPath);

impl PolicyFile {
    fn write(document: &str) -> Result<Self, SandboxError> {
        tempfile_path::TempPath::write(document)
            .map(Self)
            .map_err(|e| setup(format!("writing the sandbox's policy failed: {e}")))
    }

    fn path(&self) -> &std::path::Path {
        self.0.path()
    }
}

/// A file in the temporary directory, readable by this account alone, removed on drop.
mod tempfile_path {
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    pub(super) struct TempPath(PathBuf);

    impl TempPath {
        pub(super) fn write(text: &str) -> std::io::Result<Self> {
            let path = std::env::temp_dir().join(format!(
                "bravebot-openshell-{}-{}.yaml",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&path)?;
            file.write_all(text.as_bytes())?;
            Ok(Self(path))
        }

        pub(super) fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempPath {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a policy file left behind holds no secret, and dropping has nobody to tell"
        )]
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
}

/// The policy a sandbox is created or updated with.
///
/// The filesystem part is OpenShell's default baseline, with Landlock as a hard requirement so a
/// sandbox whose filesystem policy cannot be applied in full is refused rather than run degraded.
/// With no binary or no hosts there is no network rule, so nothing is reached. Every rule enforces:
/// OpenShell's default for an inspected endpoint is to log and allow.
pub fn policy_document(binary: Option<&str>, hosts: &[String]) -> String {
    let mut document = String::from(
        "version: 1\n\
         filesystem_policy:\n  \
           include_workdir: true\n  \
           read_only: [/bin, /usr, /lib, /proc, /dev/urandom, /etc, /var/log]\n  \
           read_write: [/tmp, /dev/null]\n\
         landlock:\n  \
           compatibility: hard_requirement\n",
    );
    if let Some(binary) = binary
        && !hosts.is_empty()
    {
        document.push_str("network_policies:\n  allowed_hosts:\n    endpoints:\n");
        for host in hosts {
            document.push_str(&format!(
                "      - host: {}\n        port: 443\n        enforcement: enforce\n",
                quoted(host)
            ));
        }
        document.push_str(&format!(
            "    binaries:\n      - path: {}\n",
            quoted(binary)
        ));
    }
    document
}

/// `text` as a double-quoted YAML scalar. Callers check that it holds no control character, so
/// only the quote and the backslash need escaping.
fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// A name for a new sandbox: `bb-` and twelve random lower-case letters and digits, short enough for
/// OpenShell and unlikely to be any other machine's on a gateway they share. Nothing about the name
/// says which process made it; the [`Ledger`] records that.
pub fn next_name() -> String {
    use std::hash::{BuildHasher, Hasher};
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let next = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    // RandomState's keys come from the operating system's randomness once per process, so two
    // processes, here or on another machine, draw different names.
    let draw = |round: u64| {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u64(next);
        hasher.write_u64(round);
        hasher.write_u32(std::process::id());
        hasher.finish()
    };
    let (high, low) = (draw(0), draw(1));
    let tail: String = (0..12)
        .map(|at| {
            let bits = if at < 6 {
                high >> (at * 10)
            } else {
                low >> ((at - 6) * 10)
            };
            char::from(ALPHABET[(bits % 36) as usize])
        })
        .collect();
    format!("{PREFIX}{tail}")
}

/// The oldest OpenShell this backend is written against: the release whose CLI it was tested with.
pub const MINIMUM_VERSION: &str = "0.1.7";

/// What `doctor` says of the OpenShell on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    /// The version `openshell --version` gives, where it gives one.
    pub version: Option<String>,
    /// Whether the gateway answers, and why not where it does not.
    pub gateway: Result<(), String>,
}

/// Ask the `openshell` program its version and whether its gateway is connected. `Err` where the
/// program is not found in a directory SANDBOX-30 allows.
pub fn probe() -> Result<Probe, SandboxError> {
    let cli = find_cli()?;
    let run = |args: &[&str]| run_cli(&cli, args, ANSWERED_WITHIN);
    let version = run(&["--version"])?;
    // `status` exits 0 whether or not it reached the gateway, so its words are what says which.
    let status = run(&["status", "--color", "never"])?;
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr)
    );
    Ok(Probe {
        version: version_in(&String::from_utf8_lossy(&version.stdout)),
        gateway: gateway_in(&said),
    })
}

/// Whether `version`, as `major.minor.patch`, is [`MINIMUM_VERSION`] or later.
pub fn is_supported(version: &str) -> bool {
    match (numbers(version), numbers(MINIMUM_VERSION)) {
        (Some(found), Some(minimum)) => found >= minimum,
        _ => false,
    }
}

fn numbers(version: &str) -> Option<(u32, u32, u32)> {
    let mut parts = version.split('.').map(|part| part.parse::<u32>().ok());
    let triple = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(triple)
}

/// The version in what `openshell --version` prints, `openshell 0.1.7`.
fn version_in(said: &str) -> Option<String> {
    let version = said.trim().strip_prefix("openshell ")?.trim();
    numbers(version).map(|_| version.to_string())
}

/// Whether what `openshell status` printed says the gateway is connected, and otherwise why not: the
/// status it gave, or the first line of the error it printed.
fn gateway_in(said: &str) -> Result<(), String> {
    let lines = || said.lines().map(str::trim);
    if let Some(status) = lines().find_map(|line| line.strip_prefix("Status:")) {
        return match status.trim() {
            "Connected" => Ok(()),
            other => Err(other.to_string()),
        };
    }
    let error = lines()
        .find_map(|line| line.strip_prefix("Error:"))
        .map(|line| {
            line.trim_start_matches(|c: char| c.is_whitespace() || c == '\u{d7}')
                .trim()
        })
        .filter(|line| !line.is_empty());
    Err(error
        .unwrap_or("no status was given")
        .chars()
        .filter(|c| !c.is_control())
        .collect())
}

/// Whether the process `pid` is running. One this process may not signal is running too, since a
/// sandbox in use is not to be deleted on a guess.
fn running(pid: u32) -> bool {
    i32::try_from(pid)
        .ok()
        .and_then(rustix::process::Pid::from_raw)
        .is_some_and(|pid| {
            !matches!(
                rustix::process::test_kill_process(pid),
                Err(rustix::io::Errno::SRCH)
            )
        })
}

/// The sandboxes this machine made, each with the process that made it, kept in one file so that a
/// later process can delete the ones a process ended by a signal left behind, and only those: a
/// sandbox another machine made on a shared gateway is in no file here.
pub struct Ledger {
    path: std::path::PathBuf,
}

impl Ledger {
    /// The record kept in `path`, which need not exist yet.
    pub fn at(path: std::path::PathBuf) -> Self {
        Self { path }
    }

    /// Record that this process makes the sandbox `name`, before it is created, so one left half
    /// made is recorded too.
    pub fn record(&self, name: &str) -> std::io::Result<()> {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        file.lock()?;
        writeln!(file, "{name} {} {}", std::process::id(), this_host())
    }

    /// Delete each recorded sandbox whose process is no longer running, drop it from the record,
    /// and return the names deleted. A sandbox whose process still runs, whatever process that now
    /// is, stays, as does one OpenShell would not delete. A sandbox already gone leaves the record.
    pub fn remove_left_behind(&self) -> Result<Vec<String>, SandboxError> {
        use std::io::{Read, Seek, Write};
        let mut file = match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(setup(format!("the sandbox record cannot be read: {error}"))),
        };
        file.lock()
            .map_err(|error| setup(format!("the sandbox record cannot be locked: {error}")))?;
        let mut recorded = String::new();
        file.read_to_string(&mut recorded)
            .map_err(|error| setup(format!("the sandbox record cannot be read: {error}")))?;
        let (ended, mut kept) = left_behind(&recorded, &this_host(), running);
        let mut removed = Vec::new();
        if !ended.is_empty() {
            let cli = find_cli()?;
            let run = |args: &[&str]| {
                run_cli(
                    &cli,
                    args.iter().chain(&["--color", "never"]),
                    ANSWERED_WITHIN,
                )
            };
            let listed = run(&["sandbox", "list"])?;
            if !listed.status.success() {
                return Err(setup(format!(
                    "listing sandboxes failed: {}",
                    last_line(&listed.stderr).unwrap_or("no reason given")
                )));
            }
            let listed = String::from_utf8_lossy(&listed.stdout).into_owned();
            let exists = |name: &str| {
                listed
                    .lines()
                    .any(|line| line.split_whitespace().next() == Some(name))
            };
            for (name, line) in ended {
                if !exists(&name) {
                    continue;
                }
                if run(&["sandbox", "delete", &name])?.status.success() {
                    removed.push(name);
                } else {
                    kept.push(line);
                }
            }
        }
        let rewritten: String = kept.iter().map(|line| format!("{line}\n")).collect();
        file.set_len(0)
            .and_then(|()| file.rewind())
            .and_then(|()| file.write_all(rewritten.as_bytes()))
            .map_err(|error| setup(format!("the sandbox record cannot be written: {error}")))?;
        Ok(removed)
    }
}

/// This machine as the record writes it: an id that stays when the machine's network name changes,
/// `/etc/machine-id` on Linux and the hardware UUID on macOS, or its name where neither is read. One
/// word, so it stays one field of a line.
fn this_host() -> String {
    static MACHINE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    MACHINE
        .get_or_init(|| {
            machine_id().unwrap_or_else(|| {
                let name = rustix::system::uname()
                    .nodename()
                    .to_string_lossy()
                    .into_owned();
                match name.split_whitespace().collect::<Vec<_>>().join("-") {
                    joined if joined.is_empty() => "unnamed".to_string(),
                    joined => joined,
                }
            })
        })
        .clone()
}

/// The id the operating system keeps for this machine, where it keeps one this can read.
fn machine_id() -> Option<String> {
    let plain = |id: &str| {
        let id = id.trim();
        (!id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-'))
            .then(|| id.to_ascii_lowercase())
    };
    if cfg!(target_os = "linux") {
        return std::fs::read_to_string("/etc/machine-id")
            .ok()
            .and_then(|id| plain(&id));
    }
    if cfg!(target_os = "macos") {
        // Named by its path, since a minimal PATH, as cron or a launched application gives, would
        // otherwise leave this process naming the machine differently from the others.
        let sysctl = Resolved::written_in_source("/usr/sbin/sysctl");
        let mut command = crate::programs::start(&sysctl, Inherits::Only(&[]), SearchPath::Reduced);
        command.args(["-n", "kern.uuid"]);
        let output = output_of(command, Duration::from_secs(5)).ok()?;
        return output
            .status
            .success()
            .then(|| plain(&String::from_utf8_lossy(&output.stdout)))
            .flatten();
    }
    None
}

/// The recorded sandboxes of `host` whose process `running` says has ended, each with its line, and
/// the lines to keep. Another machine's line is kept untouched, since a process id means nothing
/// here, as where two machines share a state directory, and so is a line that names no machine. A
/// line that is not a sandbox name and a process id is dropped.
fn left_behind(
    recorded: &str,
    host: &str,
    running: impl Fn(u32) -> bool,
) -> (Vec<(String, String)>, Vec<String>) {
    let mut ended = Vec::new();
    let mut kept = Vec::new();
    for line in recorded.lines() {
        let mut words = line.split_whitespace();
        // A line of a name and a process alone names no machine, so it is kept and never swept: on
        // a state directory machines share, its process id may be another machine's.
        let (Some(name), Some(pid), made_on, None) =
            (words.next(), words.next(), words.next(), words.next())
        else {
            continue;
        };
        let Some(made_on) = made_on else {
            kept.push(line.to_string());
            continue;
        };
        let (true, Ok(pid)) = (
            is_sandbox_name(name) && name.starts_with(PREFIX),
            pid.parse::<u32>(),
        ) else {
            continue;
        };
        match made_on == host && !running(pid) {
            true => ended.push((name.to_string(), line.to_string())),
            false => kept.push(line.to_string()),
        }
    }
    (ended, kept)
}

/// Whether `name` is a name OpenShell accepts for a sandbox and nothing a command line could read
/// as anything else.
fn is_sandbox_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && name.len() <= MAX_NAME
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.ends_with('-')
}

/// Whether OpenShell takes `host` as a rule: a host name, or `*.` and a domain, in labels of letters,
/// digits and inner hyphens. A caller with a wider list keeps only what this accepts.
pub fn accepts_host(host: &str) -> bool {
    is_host_rule(host)
}

/// Whether `host` is a host name or a `*.` and a domain, in the shape `allowed_hosts` takes.
fn is_host_rule(host: &str) -> bool {
    let name = host.strip_prefix("*.").unwrap_or(host);
    !name.is_empty()
        && name.len() <= 253
        && name.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                && !label.starts_with('-')
                && !label.ends_with('-')
        })
}

/// Whether `path` is an absolute path with nothing a policy file or a reader could misread.
fn is_real_path(path: &str) -> bool {
    path.starts_with('/') && !path.chars().any(|c| c.is_control())
}

/// The phase `openshell sandbox get` reports, from its `Phase:` line.
fn phase(shown: &str) -> Option<&str> {
    shown
        .lines()
        .find_map(|line| line.trim().strip_prefix("Phase:"))
        .and_then(|rest| rest.split_whitespace().next())
}

/// The last line `openshell` wrote, for a refusal to quote what it said.
fn last_line(bytes: &[u8]) -> Option<&str> {
    std::str::from_utf8(bytes)
        .ok()?
        .lines()
        .map(str::trim)
        .rev()
        .find(|line| !line.is_empty())
}

fn setup(detail: String) -> SandboxError {
    SandboxError::SetupFailed {
        mechanism: "openshell",
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved() -> Resolved {
        Resolved::written_in_source("/opt/homebrew/bin/openshell")
    }

    #[test]
    fn a_policy_with_no_hosts_reaches_nothing_and_requires_landlock() {
        let document = policy_document(Some("/usr/local/bin/python3.13"), &[]);
        assert!(!document.contains("network_policies"), "{document}");
        assert!(
            document.contains("compatibility: hard_requirement"),
            "{document}"
        );
        assert!(!document.contains("best_effort"), "{document}");
        assert_eq!(
            policy_document(None, &["example.com".into()]),
            policy_document(None, &[])
        );
    }

    #[test]
    fn each_host_is_granted_to_the_one_binary_and_enforced() {
        let document = policy_document(
            Some("/usr/local/bin/python3.13"),
            &["example.com".into(), "*.githubusercontent.com".into()],
        );
        assert!(
            document.contains(
                "- host: \"example.com\"\n        port: 443\n        enforcement: enforce"
            ),
            "{document}"
        );
        assert!(
            document.contains("- host: \"*.githubusercontent.com\""),
            "{document}"
        );
        assert!(
            document.contains("binaries:\n      - path: \"/usr/local/bin/python3.13\""),
            "{document}"
        );
        assert!(!document.contains("audit"), "{document}");
    }

    #[test]
    fn a_quote_or_backslash_cannot_end_a_scalar_early() {
        assert_eq!(quoted(r#"a"b\c"#), r#""a\"b\\c""#);
    }

    #[test]
    fn a_host_that_is_not_a_name_or_a_domain_is_refused() {
        for bad in [
            "",
            "*.",
            "a b.com",
            "evil.com\n  - host: x",
            "-a.com",
            "a..com",
            "a.com:443",
            "http://a.com",
        ] {
            assert!(!is_host_rule(bad), "{bad:?} was accepted");
            assert!(
                OpenShell::with_program(resolved(), "spike", "python:3.13-slim", &[bad.into()])
                    .is_err()
            );
        }
        for good in [
            "example.com",
            "*.githubusercontent.com",
            "api.github.com",
            "a-b.example",
        ] {
            assert!(is_host_rule(good), "{good:?} was refused");
        }
    }

    #[test]
    fn a_name_or_image_a_command_line_could_misread_is_refused() {
        for bad in [
            "",
            "Spike",
            "-spike",
            "spike-",
            "spike name",
            "spike;rm",
            "1spike",
        ] {
            assert!(
                OpenShell::with_program(resolved(), bad, "python:3.13-slim", &[]).is_err(),
                "{bad:?}"
            );
        }
        for bad in ["", "python 3", "python:3\n--policy x"] {
            assert!(
                OpenShell::with_program(resolved(), "spike", bad, &[]).is_err(),
                "{bad:?}"
            );
        }
        assert!(
            OpenShell::with_program(
                resolved(),
                "bravebot-mcp-1",
                "python:3.13-slim@sha256:abc",
                &[]
            )
            .is_ok()
        );
        // OpenShell refuses a name past its maximum, so this refuses it first, with a reason.
        let longest = "abcdefghij-klmnopqr";
        assert_eq!(longest.len(), MAX_NAME);
        assert!(OpenShell::with_program(resolved(), longest, "python:3.13-slim", &[]).is_ok());
        let past = "abcdefghij-klmnopqrs";
        assert!(OpenShell::with_program(resolved(), past, "python:3.13-slim", &[]).is_err());
    }

    #[test]
    fn a_path_from_the_image_must_be_absolute_and_plain() {
        assert!(is_real_path("/usr/local/bin/python3.13"));
        for bad in ["", "python3", "/usr/bin/x\n- path: /bin/sh"] {
            assert!(!is_real_path(bad), "{bad:?}");
        }
    }

    #[test]
    fn the_version_and_the_gateway_are_read_from_what_openshell_prints() {
        assert_eq!(version_in("openshell 0.1.7\n"), Some("0.1.7".into()));
        assert_eq!(version_in("openshell 0.2\n"), None);
        assert_eq!(version_in("something else"), None);
        assert!(is_supported("0.1.7") && is_supported("0.1.10") && is_supported("1.0.0"));
        assert!(!is_supported("0.1.6") && !is_supported("0.0.9") && !is_supported("x"));

        let connected =
            "Server Status\n\n  Gateway: openshell\n  Status: Connected\n  Version: 0.1.7\n";
        assert_eq!(gateway_in(connected), Ok(()));
        assert_eq!(
            gateway_in("  Status: Disconnected\n"),
            Err("Disconnected".to_string())
        );
        let refused = "Error:   \u{d7} failed to read TLS CA from /x/ca.crt\n  \u{2570}\u{2500}\u{25b6} No such file\n";
        assert_eq!(
            gateway_in(refused),
            Err("failed to read TLS CA from /x/ca.crt".to_string())
        );
        assert_eq!(gateway_in(""), Err("no status was given".to_string()));
    }

    #[test]
    fn a_name_is_random_and_fits_openshell() {
        let names: std::collections::BTreeSet<String> = (0..200).map(|_| next_name()).collect();
        assert_eq!(names.len(), 200, "two names were the same");
        for name in &names {
            assert!(is_sandbox_name(name), "{name}");
            assert_eq!(name.len(), PREFIX.len() + 12, "{name}");
        }
    }

    /// Only a recorded sandbox whose process has ended is left behind: one whose process runs is
    /// kept, and a line that is not a name and a process id is dropped.
    #[test]
    fn only_a_recorded_sandbox_of_an_ended_process_is_left_behind() {
        let recorded = "bb-aaaaaaaaaaaa 100 here\nbb-bbbbbbbbbbbb 200 here\nbb-cccccccccccc 100 there\nnot a line\nbb-dddddddddddd x here\nbb-eeeeeeeeeeee 100\nmine 300 here\n";
        let (ended, kept) = left_behind(recorded, "here", |pid| pid == 200);
        assert_eq!(
            ended,
            [(
                "bb-aaaaaaaaaaaa".to_string(),
                "bb-aaaaaaaaaaaa 100 here".to_string()
            )]
        );
        assert_eq!(
            kept,
            [
                "bb-bbbbbbbbbbbb 200 here",
                "bb-cccccccccccc 100 there",
                "bb-eeeeeeeeeeee 100"
            ],
            "a running process's sandbox, another machine's and one naming no machine are kept"
        );
        assert_eq!(left_behind("", "here", |_| false), (Vec::new(), Vec::new()));
    }

    /// The record keeps what this process made, and a sweep of it with no ended process deletes
    /// nothing and asks OpenShell nothing.
    #[test]
    fn a_record_holds_this_process_and_a_sweep_keeps_a_running_one() {
        let path = std::env::temp_dir().join(format!(
            "bravebot-openshell-ledger-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.as_nanos())
        ));
        let ledger = Ledger::at(path.clone());
        assert_eq!(
            ledger.remove_left_behind().expect("no record yet"),
            Vec::<String>::new()
        );
        let name = next_name();
        ledger.record(&name).expect("recorded");
        assert_eq!(
            ledger.remove_left_behind().expect("swept"),
            Vec::<String>::new()
        );
        let kept = std::fs::read_to_string(&path).expect("the record");
        assert_eq!(
            kept,
            format!("{name} {} {}\n", std::process::id(), this_host())
        );
        let _ = std::fs::remove_file(&path);
    }

    /// A command that runs past its bound is killed and refused, and one that ends is read whole.
    #[test]
    fn a_command_past_its_bound_is_killed_and_one_that_ends_is_read() {
        let program = |name: &str| {
            crate::programs::start(
                &Resolved::find(OsStr::new(name)).expect(name),
                Inherits::Only(&[]),
                SearchPath::Reduced,
            )
        };
        let mut sleeping = program("sleep");
        sleeping.arg("5");
        let began = Instant::now();
        let refused = output_of(sleeping, Duration::from_millis(200));
        assert!(
            matches!(refused, Err(SandboxError::SetupFailed { .. })),
            "{refused:?}"
        );
        assert!(
            began.elapsed() < Duration::from_secs(3),
            "it waited for the program"
        );
        // The shell ends at once, and what it left running holds the pipe open past the bound.
        let mut holding = program("sh");
        holding.args(["-c", "sleep 5 & echo said"]);
        let began = Instant::now();
        let refused = output_of(holding, Duration::from_millis(500));
        assert!(
            matches!(refused, Err(SandboxError::SetupFailed { .. })),
            "{refused:?}"
        );
        assert!(
            began.elapsed() < Duration::from_secs(3),
            "it waited for the pipe"
        );
        let mut echoing = program("echo");
        echoing.arg("said");
        let said = output_of(echoing, Duration::from_secs(10)).expect("ran");
        assert!(said.status.success());
        assert_eq!(said.stdout, b"said\n");
    }

    #[test]
    fn this_process_is_running_and_a_process_that_ended_is_not() {
        assert!(running(std::process::id()));
        let mut child = crate::programs::start(
            &Resolved::find(OsStr::new("true")).expect("true"),
            Inherits::Only(&[]),
            SearchPath::Reduced,
        )
        .spawn()
        .expect("a child");
        let pid = child.id();
        child.wait().expect("waited");
        assert!(!running(pid));
    }

    #[test]
    fn the_phase_is_read_from_its_own_line() {
        assert_eq!(
            phase("Sandbox:\n\n  Id: 1\n  Name: x\n  Phase: Ready\n"),
            Some("Ready")
        );
        assert_eq!(
            phase("  Phase: Error\n  Configuration: ConfigurationPending: x"),
            Some("Error")
        );
        assert_eq!(phase("nothing here"), None);
    }

    #[test]
    fn a_variable_other_than_path_or_home_is_refused_before_anything_is_created() {
        let shell =
            OpenShell::with_program(resolved(), "spike", "python:3.13-slim", &[]).expect("valid");
        let variables = crate::Variables::new().with("GITHUB_TOKEN", "x");
        let refused = shell.spawn(
            OsStr::new("python3"),
            &[],
            &SandboxPolicy::strict(),
            crate::testutil::nothing_attached(),
            Environment::Only(variables),
        );
        assert!(
            matches!(refused, Err(SandboxError::SetupFailed { .. })),
            "{refused:?}"
        );
        assert!(
            !shell.state.lock().expect("lock").created,
            "a sandbox was created"
        );
    }
}
