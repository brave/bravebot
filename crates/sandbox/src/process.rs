//! The process a backend starts, and the decisions it is started with.
//!
//! A backend starts the process and hands back a [`ConfinedChild`] rather than handing
//! back a [`Command`] for the caller to spawn. Confinement can be a property of process
//! creation rather than of a command line: where the mechanism is carried by an argument
//! to the call that creates the process, there is no command a caller could spawn itself
//! and still be confined. A `Command` also exposes no getter for the stdio set on it, so
//! a backend that wraps the program in another one cannot carry over what a caller set.
//!
//! The stdio and the environment are therefore stated here, in terms this crate owns,
//! and the pipe ends come back as types this crate owns: a backend that creates the
//! pipes itself hands back what one letting the standard library create them hands back.

#[cfg(unix)]
use crate::SandboxError;
use std::ffi::OsString;
use std::fmt;
use std::fs::File;
use std::io::{self, Read, Write};
#[cfg(not(windows))]
use std::process::Child;
#[cfg(unix)]
use std::process::Command;
use std::process::{ExitStatus, Stdio};

/// Every start of a program and every creation of a control channel takes this, one at a time.
static GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Runs `spawn`, the start of a program, with no other start and no creation of a control channel
/// under way in this process.
///
/// **Why.** macOS creates a socket pair, or a pipe, and then marks it close-on-exec, so a process
/// another thread starts between the two steps holds both ends and the bridge hands them on to its
/// tools. Linux creates them already marked, so the race is macOS's, but the lock is taken
/// everywhere so every platform runs the same code. The pipes the agent makes for a stage's standard streams, and
/// a socket pair, are made under [`creating_a_channel`] for the same reason. A start creates the pipes of its own standard
/// streams inside `spawn`, so a start is also an fd creation and a fork: two starts must not
/// overlap either, or each could inherit the other's pipe ends and keep them open. Every start of a
/// program by a crate the bridge links goes through here, and every creation of a control channel
/// goes through [`creating_a_channel`]. A test in `crates/ui-bridge/tests/layering.rs` reads their
/// source and fails for a start outside the gate, which is what keeps a new start from skipping it.
/// See RPCVIEW-8 in `docs/specs/session-view.md`.
///
/// **What it costs.** Starts run one at a time. The lock is held until `spawn` returns, which for
/// `Command::spawn` is after the program has started: the standard library offers no point between
/// the fork that copies the descriptors and the exec that follows it, and the fork is the step that
/// has to be kept apart from creating a descriptor. A start stuck in its exec, which includes the
/// Landlock rule setup on Linux, therefore holds up every start behind it. A start that waits for
/// its program to finish, rather than to start, would hold the lock that long, so a caller spawns
/// here and waits outside. The gate is not reentrant: `spawn` must not enter it again, and
/// entering it again panics instead of waiting for itself. That guard sees only the same thread: a
/// `spawn` that waits on another thread which itself needs the gate would still hang, so `spawn`
/// starts the program and nothing else. Start latency under it has not been measured.
///
/// **When to reduce it.** If measured latency shows contention, take it only on macOS, create
/// channels before any worker thread exists, or start programs through a helper that holds none of
/// the host's descriptors, in that order. Letting starts overlap each other is not on the list:
/// that is the race above. The reasoning is in decision 10 of
/// `docs/design/mobile/decisions-and-tradeoffs.md`.
pub fn starting_a_program<T>(spawn: impl FnOnce() -> T) -> T {
    let _inside = Inside::enter();
    let _held = waiting(|| GATE.lock().unwrap_or_else(|p| p.into_inner()));
    spawn()
}

/// Runs `create`, which makes a descriptor pair a process could inherit, under the same gate as a
/// start of a program. A launcher that creates the pair and starts the program that holds its far
/// end in one step does both here.
pub fn creating_a_channel<T>(create: impl FnOnce() -> T) -> T {
    let _inside = Inside::enter();
    let _held = waiting(|| GATE.lock().unwrap_or_else(|p| p.into_inner()));
    create()
}

/// The threads that have asked for the gate and are waiting for it, so a test can see that a start
/// has reached the gate and is held there, which a timeout cannot tell from a start that is slow.
static WAITING: std::sync::Mutex<Vec<std::thread::ThreadId>> = std::sync::Mutex::new(Vec::new());

/// The threads waiting at the gate now. For tests; nothing else needs it.
#[doc(hidden)]
pub fn threads_waiting_at_the_gate() -> Vec<std::thread::ThreadId> {
    WAITING.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

/// Takes the gate with `acquire`, listing this thread as waiting until it has it.
fn waiting<G>(acquire: impl FnOnce() -> G) -> G {
    let me = std::thread::current().id();
    WAITING.lock().unwrap_or_else(|p| p.into_inner()).push(me);
    let held = acquire();
    let mut list = WAITING.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(at) = list.iter().position(|id| *id == me) {
        list.swap_remove(at);
    }
    held
}

std::thread_local! {
    static INSIDE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// This thread is inside the gate. Entering again would wait for itself, so it panics first.
struct Inside;

impl Inside {
    fn enter() -> Self {
        assert!(
            !INSIDE.with(|inside| inside.replace(true)),
            "the spawn gate was entered again from inside it"
        );
        Self
    }
}

impl Drop for Inside {
    fn drop(&mut self) {
        INSIDE.with(|inside| inside.set(false));
    }
}

/// What one of a confined process's standard streams is attached to.
///
/// This crate's own description of the three rather than [`Stdio`], which a backend
/// cannot read a decision back out of and which is consumed by the command it is set on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    /// This process's own, so the confined process reaches whatever we are attached to.
    Inherited,
    /// A pipe, whose other end comes back on the [`ConfinedChild`].
    Piped,
    /// Discarded. A read of it is at its end immediately.
    Null,
}

impl From<Stream> for Stdio {
    fn from(stream: Stream) -> Self {
        match stream {
            Stream::Inherited => Self::inherit(),
            Stream::Piped => Self::piped(),
            Stream::Null => Self::null(),
        }
    }
}

/// The three standard streams of a confined process.
///
/// No default, so that every caller answers for all three: an inherited stream hands the
/// confined process a descriptor into whatever this process is attached to, which is not
/// something to arrive at by leaving a field out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Streams {
    pub stdin: Stream,
    pub stdout: Stream,
    pub stderr: Stream,
}

/// The environment a confined process starts with.
///
/// A variable holds what no grant over paths can withhold or hand over: a credential and
/// the agent socket a signature is made through are both named by one. So which
/// variables a program is trusted with is a decision of its own, the caller makes it, and
/// there is no default here to make it by omission. This crate applies the answer, so it
/// is the same answer on every platform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Environment {
    /// This process's own.
    Inherited,
    /// Empty.
    Empty,
    /// These variables and no others.
    Only(Variables),
}

/// Variables a caller hands a confined process, each a name and its value.
///
/// Its `Debug` names the variables and never shows a value, since a value handed over this
/// way is typically a credential and a log line is readable by more than the process it
/// was meant for.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Variables(Vec<(OsString, OsString)>);

impl Variables {
    pub fn new() -> Self {
        Self::default()
    }

    /// Hand over `name` holding `value`, in place of any earlier value for that name.
    pub fn with(mut self, name: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        let name = name.into();
        self.0.retain(|(held, _)| *held != name);
        self.0.push((name, value.into()));
        self
    }

    pub fn iter(&self) -> impl Iterator<Item = &(OsString, OsString)> {
        self.0.iter()
    }

    pub fn names(&self) -> impl Iterator<Item = &OsString> {
        self.0.iter().map(|(name, _)| name)
    }
}

impl fmt::Debug for Variables {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.names()).finish()
    }
}

/// The three handles a confined process is started with, made by the caller.
///
/// For a caller running a pipeline, which wires one stage's output to the next stage's input and
/// cannot say that with [`Streams`]. Each is the process's own once handed over: this process's
/// copy is closed when the process has started, so a pipe reaches its end when the stage writing
/// to it exits rather than when the caller does.
#[cfg(windows)]
#[derive(Debug)]
pub struct Attached {
    pub stdin: std::os::windows::io::OwnedHandle,
    pub stdout: std::os::windows::io::OwnedHandle,
    pub stderr: std::os::windows::io::OwnedHandle,
}

/// The write end of a confined process's standard input.
#[derive(Debug)]
pub struct ConfinedStdin(File);

impl Write for ConfinedStdin {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

/// The read end of a confined process's standard output.
#[derive(Debug)]
pub struct ConfinedStdout(File);

impl Read for ConfinedStdout {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

/// The read end of a confined process's standard error.
#[derive(Debug)]
pub struct ConfinedStderr(File);

impl Read for ConfinedStderr {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

/// The started process itself.
///
/// Two shapes, because confinement decides how the process is created. Where the
/// mechanism travels with the command, the standard library creates it and hands back a
/// [`Child`](std::process::Child). Where it is an argument to the call that creates the
/// process, this crate makes that call itself, and a `Child` cannot be built from what it
/// returns.
#[derive(Debug)]
enum Started {
    #[cfg(not(windows))]
    Spawned(Child),
    #[cfg(windows)]
    Created(crate::windows::CreatedProcess),
}

/// A process running under confinement.
///
/// Dropping this leaves the process running, as dropping a
/// [`Child`](std::process::Child) does: a caller that needs it gone calls
/// [`kill`](Self::kill) and then [`wait`](Self::wait).
#[derive(Debug)]
pub struct ConfinedChild {
    started: Started,
    stdin: Option<ConfinedStdin>,
    stdout: Option<ConfinedStdout>,
    stderr: Option<ConfinedStderr>,
}

impl ConfinedChild {
    /// The operating system's identifier for the process.
    pub fn id(&self) -> u32 {
        match &self.started {
            #[cfg(not(windows))]
            Started::Spawned(child) => child.id(),
            #[cfg(windows)]
            Started::Created(process) => process.id(),
        }
    }

    /// The pipe to the process's standard input, where one was asked for.
    ///
    /// Taken rather than borrowed, so writing to it does not borrow the child. Closing it
    /// then belongs to whoever took it: [`wait`](Self::wait) closes the one this still
    /// holds and cannot close one it has handed over.
    pub fn take_stdin(&mut self) -> Option<ConfinedStdin> {
        self.stdin.take()
    }

    /// The pipe from the process's standard output, where one was asked for.
    pub fn take_stdout(&mut self) -> Option<ConfinedStdout> {
        self.stdout.take()
    }

    /// The pipe from the process's standard error, where one was asked for.
    pub fn take_stderr(&mut self) -> Option<ConfinedStderr> {
        self.stderr.take()
    }

    /// Wait for the process to exit.
    ///
    /// The pipe to the process's standard input is closed first, where this still holds
    /// it: a process reading its input to the end never reaches the end while the write
    /// end is open, so waiting without closing it waits forever.
    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        drop(self.stdin.take());
        match &mut self.started {
            #[cfg(not(windows))]
            Started::Spawned(child) => child.wait(),
            #[cfg(windows)]
            Started::Created(process) => process.wait(),
        }
    }

    /// The process's exit status if it has exited, without waiting for it to.
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        match &mut self.started {
            #[cfg(not(windows))]
            Started::Spawned(child) => child.try_wait(),
            #[cfg(windows)]
            Started::Created(process) => process.try_wait(),
        }
    }

    /// Ask the operating system to end the process.
    pub fn kill(&mut self) -> io::Result<()> {
        match &mut self.started {
            #[cfg(not(windows))]
            Started::Spawned(child) => child.kill(),
            #[cfg(windows)]
            Started::Created(process) => process.kill(),
        }
    }
}

/// Hand back a process a backend created itself, with the pipe ends it kept.
///
/// The counterpart of [`start`] for a platform where confinement is an argument to
/// process creation: the backend has made the call, so what is left is to present the
/// result the same way.
#[cfg(windows)]
pub(crate) fn confined(
    process: crate::windows::CreatedProcess,
    stdin: Option<File>,
    stdout: Option<File>,
    stderr: Option<File>,
) -> ConfinedChild {
    ConfinedChild {
        started: Started::Created(process),
        stdin: stdin.map(ConfinedStdin),
        stdout: stdout.map(ConfinedStdout),
        stderr: stderr.map(ConfinedStderr),
    }
}

/// Give `command` the environment `environment` names and no other.
///
/// Matched rather than compared, so a fourth answer added here is a compile error
/// instead of a program quietly handed everything this process holds.
#[cfg(unix)]
pub(crate) fn apply_environment(command: &mut Command, environment: &Environment) {
    match environment {
        Environment::Empty => {
            command.env_clear();
        }
        Environment::Only(variables) => {
            command.env_clear();
            command.envs(variables.iter().map(|(name, value)| (name, value)));
        }
        Environment::Inherited => {}
    }
}

/// Start a command a backend has made confining, or refuse.
#[cfg(unix)]
pub(crate) fn start(
    mut command: Command,
    streams: Streams,
    environment: &Environment,
) -> Result<ConfinedChild, SandboxError> {
    apply_environment(&mut command, environment);
    command
        .stdin(Stdio::from(streams.stdin))
        .stdout(Stdio::from(streams.stdout))
        .stderr(Stdio::from(streams.stderr));

    let mut child = starting_a_program(|| command.spawn()).map_err(SandboxError::SpawnFailed)?;

    // The pipe ends are moved into files this crate owns. Neither of the standard
    // library's pipe types can be built from a descriptor, so a backend that creates the
    // pipes itself would have nothing to return.
    let stdin = child
        .stdin
        .take()
        .map(|pipe| ConfinedStdin(File::from(std::os::fd::OwnedFd::from(pipe))));
    let stdout = child
        .stdout
        .take()
        .map(|pipe| ConfinedStdout(File::from(std::os::fd::OwnedFd::from(pipe))));
    let stderr = child
        .stderr
        .take()
        .map(|pipe| ConfinedStderr(File::from(std::os::fd::OwnedFd::from(pipe))));

    Ok(ConfinedChild {
        started: Started::Spawned(child),
        stdin,
        stdout,
        stderr,
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::testutil::nothing_attached;

    const EVERY_STREAM_PIPED: Streams = Streams {
        stdin: Stream::Piped,
        stdout: Stream::Piped,
        stderr: Stream::Piped,
    };

    fn shell(script: &str) -> Command {
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg(script);
        command
    }

    fn read(mut pipe: impl Read) -> String {
        let mut text = String::new();
        pipe.read_to_string(&mut text).expect("a readable pipe");
        text
    }

    /// A confined start waits while a control channel is being created, so the program cannot hold
    /// a descriptor of the channel in the window before it is marked close-on-exec.
    #[test]
    fn a_start_waits_for_a_channel_being_created() {
        use std::sync::mpsc::{TryRecvError, channel};
        use std::time::{Duration, Instant};
        let (finished, done) = channel();
        let (named, who) = channel();
        let thread = creating_a_channel(|| {
            let thread = std::thread::spawn(move || {
                let _ = named.send(std::thread::current().id());
                let mut child = start(shell("true"), EVERY_STREAM_PIPED, &Environment::Inherited)
                    .expect("/bin/sh runs");
                child.wait().expect("should wait");
                let _ = finished.send(());
            });
            let id = who
                .recv_timeout(Duration::from_secs(30))
                .expect("the start thread runs");
            // The start has to be seen held at the gate, not merely unfinished after a while.
            let deadline = Instant::now() + Duration::from_secs(10);
            while !threads_waiting_at_the_gate().contains(&id) {
                match done.try_recv() {
                    Err(TryRecvError::Empty) => {}
                    Ok(()) => {
                        panic!("a program was started while a control channel was being created")
                    }
                    Err(TryRecvError::Disconnected) => {
                        panic!("the start thread failed instead of waiting")
                    }
                }
                assert!(
                    Instant::now() < deadline,
                    "the start never reached the gate, so it does not wait for it"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(matches!(done.try_recv(), Err(TryRecvError::Empty)));
            thread
        });
        done.recv_timeout(Duration::from_secs(30))
            .expect("the start never went ahead once the gate was released");
        thread.join().expect("the start thread ends");
    }

    /// A start creates the pipes of its standard streams inside `spawn`, and macOS marks those
    /// close-on-exec in a second step like a socket pair, so a start that overlapped another could
    /// inherit its pipe ends and keep them open. Starts exclude each other as well as channel
    /// creation.
    #[test]
    fn two_starts_do_not_overlap() {
        use std::sync::mpsc::channel;
        use std::time::{Duration, Instant};
        let (inside, hears) = channel::<()>();
        let (leave, leaves) = channel::<()>();
        let first = std::thread::spawn(move || {
            starting_a_program(|| {
                let _ = inside.send(());
                let _ = leaves.recv_timeout(Duration::from_secs(10));
            });
        });
        hears
            .recv_timeout(Duration::from_secs(10))
            .expect("the first start is inside the gate");
        let (entered, second_in) = channel::<()>();
        let second = std::thread::spawn(move || {
            starting_a_program(|| {
                let _ = entered.send(());
            });
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        let second_id = second.thread().id();
        while !threads_waiting_at_the_gate().contains(&second_id) {
            assert!(
                second_in.try_recv().is_err(),
                "a second start was inside the gate with the first"
            );
            assert!(
                Instant::now() < deadline,
                "the second start never reached the gate"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            second_in.try_recv().is_err(),
            "a second start was inside the gate with the first"
        );
        let _ = leave.send(());
        first.join().unwrap();
        second.join().unwrap();
        assert!(second_in.try_recv().is_ok(), "the second start never ran");
    }

    /// Entering the gate from inside it would wait for itself. That is a panic, which says where,
    /// rather than a process whose every start hangs.
    #[test]
    fn entering_the_gate_again_panics_rather_than_waiting_for_itself() {
        use std::time::Duration;
        let entered_again: [(&str, fn()); 3] = [
            ("a start inside a start", || {
                starting_a_program(|| starting_a_program(|| ()));
            }),
            ("a start inside a creation", || {
                creating_a_channel(|| starting_a_program(|| ()));
            }),
            ("a creation inside a start", || {
                starting_a_program(|| creating_a_channel(|| ()));
            }),
        ];
        for (what, enter) in entered_again {
            let (outcome, hears) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let panicked = std::panic::catch_unwind(enter).is_err();
                let _ = outcome.send(panicked);
            });
            assert_eq!(
                hears.recv_timeout(Duration::from_secs(3)),
                Ok(true),
                "{what} did not panic"
            );
        }
    }

    /// Each stream asked for a pipe is carried by a pipe of its own, so what a process said
    /// about itself on one is never read as part of what it produced on the other.
    #[test]
    fn a_stream_asked_for_a_pipe_carries_what_the_process_wrote_to_that_stream() {
        let mut child = start(
            shell("echo produced; echo diagnostic >&2"),
            EVERY_STREAM_PIPED,
            &Environment::Inherited,
        )
        .expect("/bin/sh runs");

        let produced = read(child.take_stdout().expect("stdout was asked for a pipe"));
        let diagnostic = read(child.take_stderr().expect("stderr was asked for a pipe"));

        assert!(child.wait().expect("should wait").success());
        assert_eq!(produced, "produced\n");
        assert_eq!(diagnostic, "diagnostic\n");
    }

    /// A process handed named variables receives exactly those: the values it was given,
    /// and none of this process's own, so naming `PATH` for a server does not also hand it
    /// every credential this process holds in a variable.
    #[test]
    fn a_process_handed_named_variables_receives_those_and_no_others() {
        assert!(
            std::env::var_os("CARGO_MANIFEST_DIR").is_some(),
            "cargo sets this for a test, and without it there is nothing here to withhold"
        );
        let mut child = start(
            Command::new("/usr/bin/env"),
            Streams {
                stdin: Stream::Null,
                stdout: Stream::Piped,
                stderr: Stream::Inherited,
            },
            &Environment::Only(
                Variables::new()
                    .with("PATH", "/usr/bin")
                    .with("WEATHER_TOKEN", "a value"),
            ),
        )
        .expect("/usr/bin/env runs");

        let printed = read(child.take_stdout().expect("stdout was asked for a pipe"));
        assert!(child.wait().expect("should wait").success());
        // Names only: a variable that leaked carries this process's own value, a token among them.
        let mut received: Vec<&str> = printed
            .lines()
            .filter_map(|line| line.split_once('=').map(|(name, _)| name))
            .collect();
        received.sort_unstable();
        assert_eq!(received, vec!["PATH", "WEATHER_TOKEN"]);
        assert!(printed.lines().any(|line| line == "WEATHER_TOKEN=a value"));
    }

    /// A value handed over this way is typically a credential, and a caller that logs what
    /// it launched with `{:?}` must not publish it.
    #[test]
    fn a_variables_value_is_not_in_its_debug_form() {
        let shown = format!(
            "{:?}",
            Environment::Only(Variables::new().with("WEATHER_TOKEN", "hunter2"))
        );
        assert!(shown.contains("WEATHER_TOKEN"), "{shown}");
        assert!(!shown.contains("hunter2"), "{shown}");
    }

    /// A stream nobody asked a pipe for produces none, so a caller is never handed a reader
    /// on a stream it decided to discard.
    #[test]
    fn a_process_asked_for_no_pipes_is_handed_none() {
        let mut child = start(shell("exit 0"), nothing_attached(), &Environment::Inherited)
            .expect("/bin/sh runs");

        assert!(child.take_stdin().is_none());
        assert!(child.take_stdout().is_none());
        assert!(child.take_stderr().is_none());
        assert!(child.wait().expect("should wait").success());
    }

    /// Waiting closes the input the caller left here, so a process reading its input to the
    /// end reaches the end.
    ///
    /// Bounded rather than simply waited on, because the failure this rejects is a process
    /// that never exits: a test that reports that is worth more than one that hangs with it.
    /// The bound is far longer than the exit it waits for, so a loaded machine does not
    /// decide the result.
    #[test]
    fn waiting_for_a_process_that_reads_its_input_to_the_end_closes_that_input() {
        let child = start(
            shell("cat > /dev/null"),
            Streams {
                stdin: Stream::Piped,
                stdout: Stream::Null,
                stderr: Stream::Null,
            },
            &Environment::Inherited,
        )
        .expect("/bin/sh runs");

        let (reported, exited) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut child = child;
            let _ = reported.send(child.wait());
        });

        match exited.recv_timeout(std::time::Duration::from_secs(30)) {
            Ok(status) => assert!(status.expect("should wait").success()),
            Err(_) => panic!("the process is still reading an input that waiting never closed"),
        }
    }
}
