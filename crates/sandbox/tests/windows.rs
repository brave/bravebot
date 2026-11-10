//! The AppContainer backend with a real process: what a container is refused, what it is granted,
//! and which programs start under the empty Windows base.
//!
//! Compiled on Windows only. The decisions the backend makes before a process starts are pinned on
//! every platform by the tests in `src/windows.rs`; these are the calls that apply them.
//!
//! A machine that cannot create a container profile fails every test here and does not skip, since
//! a job that reported green having confined nothing would be the false confidence this crate is
//! meant to remove.
#![cfg(windows)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use bravebot_sandbox::base::{Prelude, base};
use bravebot_sandbox::policy::SandboxPolicy;
use bravebot_sandbox::windows::AppContainerSandbox;
use bravebot_sandbox::{
    ConfinedChild, ConfinedStdin, Environment, Sandbox, SandboxError, Stream, Streams,
};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, MutexGuard, mpsc};
use std::time::Duration;

fn sandbox() -> AppContainerSandbox {
    AppContainerSandbox::new().expect("a container profile can be created on this machine")
}

/// Held while this binary starts a process through the standard library, and while a test holds
/// a handle any process could inherit across the start of a stage.
///
/// The standard library lets a process it starts inherit every inheritable handle this process
/// holds at that moment. The crate's gate keeps such a start apart from a stage being created, but
/// creating a stage takes that gate itself, so a test that holds an inheritable handle across one
/// keeps the other starts out with this instead.
static SPAWNING: Mutex<()> = Mutex::new(());

fn spawning() -> MutexGuard<'static, ()> {
    SPAWNING.lock().unwrap_or_else(|held| held.into_inner())
}

/// Start `command` apart from a test's inheritable handle and from a stage being created.
fn spawned(command: &mut Command) -> std::process::Child {
    let _spawning = spawning();
    bravebot_sandbox::process::starting_a_program(|| command.spawn()).expect("the program starts")
}

/// An empty directory under the workspace `target/test-scratch/`, spelled the ordinary way and
/// without `..`, which is how a session's directory reaches the backend.
fn scratch(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path.push("target");
    path.push("test-scratch");
    path.push(format!("windows-{name}"));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("the scratch directory is creatable");
    path
}

/// The Windows base, with a temporary directory of the test's own so nothing here depends on the
/// account's.
fn the_base(name: &str) -> SandboxPolicy {
    let temporary = scratch(&format!("{name}-tmp"));
    base(Prelude::Windows, &temporary, None, None).starting_in(&temporary)
}

/// What a program ended with and what it printed, or the refusal.
fn run(
    sandbox: &AppContainerSandbox,
    program: &str,
    args: &[&str],
    policy: &SandboxPolicy,
) -> Result<(Option<i32>, String), SandboxError> {
    run_with(sandbox, program, args, policy, Environment::Inherited)
}

/// [`run`] with the environment the stage starts with named.
fn run_with(
    sandbox: &AppContainerSandbox,
    program: &str,
    args: &[&str],
    policy: &SandboxPolicy,
    environment: Environment,
) -> Result<(Option<i32>, String), SandboxError> {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let streams = Streams {
        stdin: Stream::Null,
        stdout: Stream::Piped,
        stderr: Stream::Piped,
    };
    let mut child = sandbox.spawn(program.as_ref(), &args, policy, streams, environment)?;
    let mut printed = Vec::new();
    child
        .take_stdout()
        .expect("a stdout that was piped")
        .read_to_end(&mut printed)
        .expect("what the program printed");
    let mut complained = Vec::new();
    child
        .take_stderr()
        .expect("a stderr that was piped")
        .read_to_end(&mut complained)
        .expect("what the program complained of");
    let status = child.wait().expect("the program ends");
    let mut text = String::from_utf8_lossy(&printed).trim().to_owned();
    let complaint = String::from_utf8_lossy(&complained);
    if !complaint.trim().is_empty() {
        text.push_str(&format!(" [stderr: {}]", complaint.trim()));
    }
    Ok((status.code(), text))
}

/// A `cmd.exe /c` line, which `cmd.exe` receives as one argument.
fn cmd(
    sandbox: &AppContainerSandbox,
    line: &str,
    policy: &SandboxPolicy,
) -> Result<(Option<i32>, String), SandboxError> {
    run(sandbox, "cmd.exe", &["/c", line], policy)
}

/// How long a program in a container is given to run the first line it is handed.
const STARTING: Duration = Duration::from_secs(60);

/// How long a pipe is given to reach its end once nothing should be left holding it.
const ENDING: Duration = Duration::from_secs(30);

/// What the thread reading a program's output has seen.
#[derive(Debug, PartialEq, Eq)]
enum Seen {
    /// `started` was printed.
    Started,
    /// The pipe reached its end, which is every process holding its write end gone.
    Ended,
}

/// Read `stdout` on a thread of its own, saying once when `started` has been printed and once when
/// the pipe reaches its end.
fn watch(mut stdout: impl Read + Send + 'static) -> mpsc::Receiver<Seen> {
    let (seen, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut printed = Vec::new();
        let mut chunk = [0u8; 512];
        let mut started = false;
        while let Ok(read) = stdout.read(&mut chunk) {
            if read == 0 {
                break;
            }
            printed.extend_from_slice(&chunk[..read]);
            if !started && String::from_utf8_lossy(&printed).contains("started") {
                started = true;
                let _ = seen.send(Seen::Started);
            }
        }
        let _ = seen.send(Seen::Ended);
    });
    receiver
}

/// A stage whose program, `cmd.exe /c cmd`, starts a second `cmd.exe` that reads commands from the
/// input this process holds.
///
/// The second is the only process that prints, so `started` arriving is that child running, and it
/// holds the write end of the stage's output, so the pipe reaching its end is that child gone.
fn a_stage_reading_its_input(
    sandbox: &AppContainerSandbox,
    name: &str,
) -> (ConfinedChild, ConfinedStdin, mpsc::Receiver<Seen>) {
    let streams = Streams {
        stdin: Stream::Piped,
        stdout: Stream::Piped,
        stderr: Stream::Null,
    };
    let mut child = sandbox
        .spawn(
            "cmd.exe".as_ref(),
            &[OsString::from("/c"), OsString::from("cmd")],
            &the_base(name),
            streams,
            Environment::Inherited,
        )
        .expect("the stage starts");
    let input = child.take_stdin().expect("a stdin that was piped");
    let seen = watch(child.take_stdout().expect("a stdout that was piped"));
    (child, input, seen)
}

/// Hand the stage's child a line and wait until it has run it, so it is waiting for the next one.
fn hand_it_a_line(input: &mut impl Write, seen: &mpsc::Receiver<Seen>) {
    input
        .write_all(b"echo started\r\n")
        .expect("the child is handed a line");
    assert_eq!(
        seen.recv_timeout(STARTING),
        Ok(Seen::Started),
        "the stage's child never ran the line it was handed"
    );
}

/// [`a_stage_reading_its_input`], once its child has run a line and is waiting for the next one.
fn a_stage_with_a_child_waiting_on_its_input(
    sandbox: &AppContainerSandbox,
    name: &str,
) -> (ConfinedChild, ConfinedStdin, mpsc::Receiver<Seen>) {
    let (child, mut input, seen) = a_stage_reading_its_input(sandbox, name);
    hand_it_a_line(&mut input, &seen);
    (child, input, seen)
}

/// The first `name.exe` on this process's `PATH`.
fn on_the_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(format!("{name}.exe")))
        .find(|candidate| candidate.is_file())
}

/// The regression it rejects: a container granted more than the policy named, so a program reads
/// a file in a directory it was never given. Both halves read a file the same way, so a program
/// that could read nothing at all is not mistaken for one that was refused.
#[test]
fn a_program_is_refused_a_file_outside_its_grants_and_reads_one_inside() {
    let sandbox = sandbox();
    let granted = scratch("read-granted");
    let withheld = scratch("read-withheld");
    std::fs::write(granted.join("note.txt"), "inside\n").expect("a file");
    std::fs::write(withheld.join("note.txt"), "outside\n").expect("a file");
    let policy = the_base("read").allow_read(&granted);
    let read = |directory: &Path| {
        cmd(
            &sandbox,
            &format!("type {}", directory.join("note.txt").display()),
            &policy,
        )
        .expect("the program starts")
    };

    let (code, printed) = read(&granted);
    assert_eq!(code, Some(0), "a granted read failed: {printed}");
    assert_eq!(printed, "inside");

    let (code, printed) = read(&withheld);
    assert_ne!(code, Some(0), "a file outside the grants was read");
    assert!(!printed.contains("outside"), "{printed}");
}

/// The regression it rejects: a grant whose entry reaches through a junction inside the granted
/// directory onto the directory the junction leads to. The entry is one the directory's contents
/// inherit, and writing it rewrites the access list of everything beneath, so a junction that a
/// person's tools or an earlier program left there would hand the container whatever it leads to.
/// The junction is made the way `mklink /J` makes one, by this process and outside any container.
#[test]
fn a_grant_does_not_reach_through_a_junction_to_where_it_leads() {
    let sandbox = sandbox();
    let granted = scratch("junction-granted");
    let withheld = scratch("junction-withheld");
    std::fs::write(granted.join("note.txt"), "inside\n").expect("a file");
    std::fs::write(withheld.join("note.txt"), "outside\n").expect("a file");
    let link = granted.join("link");
    let made = spawned(
        Command::new("cmd.exe")
            .arg("/c")
            .arg("mklink")
            .arg("/J")
            .arg(&link)
            .arg(&withheld)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )
    .wait_with_output()
    .expect("cmd.exe ends");
    assert!(
        made.status.success(),
        "the junction was not made: {}",
        String::from_utf8_lossy(&made.stderr)
    );
    let policy = the_base("junction").allow_write(&granted);
    let read = |path: &Path| {
        cmd(&sandbox, &format!("type {}", path.display()), &policy).expect("the program starts")
    };

    let (code, printed) = read(&granted.join("note.txt"));
    assert_eq!(code, Some(0), "a granted read failed: {printed}");
    assert_eq!(printed, "inside");

    for path in [withheld.join("note.txt"), link.join("note.txt")] {
        let (code, printed) = read(&path);
        assert_ne!(code, Some(0), "{} was read", path.display());
        assert!(!printed.contains("outside"), "{printed}");
    }
}

/// The access list of `path` as `icacls` prints it, which names every account with an entry, so a
/// list that gained one compares unequal to the list before.
fn access_list_of(path: &Path) -> String {
    let listed = spawned(
        Command::new("icacls")
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )
    .wait_with_output()
    .expect("icacls ends");
    assert!(
        listed.status.success(),
        "the access list was not read: {}",
        String::from_utf8_lossy(&listed.stderr)
    );
    String::from_utf8_lossy(&listed.stdout).into_owned()
}

/// SANDBOX-6 for a junction a stage made. The regression it rejects: a later stage's grant being
/// written through a junction an earlier stage left inside the granted directory, which hands the
/// later container the directory the junction leads to. The earlier test makes its junction from
/// this process, outside any container; this one makes it from a stage that had write access to
/// the directory, which is what an attacker's program would do.
///
/// A grant is taken off when its stage ends, so a list read after the second stage has run shows
/// nothing whichever way the grant went. The target's access list is therefore read while a second
/// stage is still running with the directory granted, and compared whole with the list before, so
/// an entry for any account counts and not only one this test could name. The observation is
/// printed whichever way it goes, as `junction grant crossed` or `junction grant did not cross`,
/// so a log of the Windows job settles whether a grant reaches through a junction a stage made
/// (R15, R17) and does not depend on the assertion that follows. The assertion is the property the
/// spec promises: a later stage granted the directory cannot read the target, by either path.
#[test]
fn a_grant_does_not_reach_through_a_junction_a_stage_made() {
    let granted = scratch("stage-junction-granted");
    let withheld = scratch("stage-junction-withheld");
    std::fs::write(withheld.join("note.txt"), "outside\n").expect("a file");
    let link = granted.join("link");
    let before = access_list_of(&withheld);

    {
        let making = sandbox();
        let policy = the_base("stage-junction-making").allow_write(&granted);
        let line = format!("mklink /J {} {}", link.display(), withheld.display());
        let (code, printed) = cmd(&making, &line, &policy).expect("the first stage starts");
        if code != Some(0) {
            // Observed on the Windows runner: a container with write access to the directory is
            // refused `mklink /J` ("Access is denied"). With no junction made by a stage there is
            // no grant to cross it, which settles nothing about a junction a stage could make
            // some other way.
            eprintln!("junction not made by a stage: {printed}");
            return;
        }
    }
    assert_eq!(
        std::fs::read_to_string(link.join("note.txt")).expect("the junction leads to the target"),
        "outside\n",
        "the junction is not one that leads to the target"
    );

    let reading = sandbox();
    let policy = the_base("stage-junction-reading").allow_write(&granted);

    // The observation: a stage that stays running, so its grants are in place when the target's
    // list is read.
    let during = {
        let streams = Streams {
            stdin: Stream::Piped,
            stdout: Stream::Null,
            stderr: Stream::Null,
        };
        let mut waiting = reading
            .spawn(
                "cmd.exe".as_ref(),
                &[OsString::from("/c"), OsString::from("cmd")],
                &policy,
                streams,
                Environment::Inherited,
            )
            .expect("the second stage starts");
        let input = waiting.take_stdin().expect("a stdin that was piped");
        let during = access_list_of(&withheld);
        drop(input);
        let _ = waiting.kill();
        let _ = waiting.wait();
        during
    };
    let crossed = during != before;
    eprintln!(
        "{}: the access list of {} {} while a stage granted {} ran\n--- before ---\n{before}\n--- during ---\n{during}",
        if crossed {
            "junction grant crossed"
        } else {
            "junction grant did not cross"
        },
        withheld.display(),
        if crossed {
            "gained entries"
        } else {
            "was unchanged"
        },
        granted.display(),
    );

    for path in [withheld.join("note.txt"), link.join("note.txt")] {
        let (code, printed) = cmd(&reading, &format!("type {}", path.display()), &policy)
            .expect("the second stage starts");
        assert_ne!(
            code,
            Some(0),
            "{} was read; observed: junction grant {}",
            path.display(),
            if crossed { "crossed" } else { "did not cross" }
        );
        assert!(
            !printed.contains("outside"),
            "{printed}; observed: junction grant {}",
            if crossed { "crossed" } else { "did not cross" }
        );
    }
}

/// The regression it rejects: a write grant that is not applied, so a program cannot make a file
/// in the directory it was given, or one that reaches past the directory, so it makes a file
/// beside it.
#[test]
fn a_program_writes_inside_its_grants_and_not_outside() {
    let sandbox = sandbox();
    let granted = scratch("write-granted");
    let withheld = scratch("write-withheld");
    let policy = the_base("write").allow_write(&granted);
    let write = |directory: &Path| {
        cmd(
            &sandbox,
            &format!("mkdir {}", directory.join("made").display()),
            &policy,
        )
        .expect("the program starts")
    };

    let (code, printed) = write(&granted);
    assert_eq!(code, Some(0), "a granted write failed: {printed}");
    assert!(granted.join("made").is_dir(), "{printed}");

    let (code, _) = write(&withheld);
    assert_ne!(code, Some(0), "a write outside the grants succeeded");
    assert!(!withheld.join("made").exists());
}

/// `curl.exe` in the system directory, which every container reads without an entry of its own.
fn curl() -> String {
    let root = std::env::var_os("SystemRoot").expect("the system directory is named");
    Path::new(&root)
        .join("System32")
        .join("curl.exe")
        .display()
        .to_string()
}

/// What `curl` ends with when it could not resolve the host, and when it could not connect.
const CURL_COULD_NOT_RESOLVE: i32 = 6;
const CURL_COULD_NOT_CONNECT: i32 = 7;

/// The host the tests below reach, which CI reached to fetch this source. A machine that cannot
/// connect to it directly, without a proxy, fails them rather than skipping them, as a machine
/// that cannot make a container does.
const OUTSIDE: &str = "github.com";

/// What `curl` ends with fetching `url` under `policy`, and what it printed.
///
/// The proxy is refused in the argument vector and the configuration file left unread, so a
/// machine whose environment names a proxy has both halves of a test measure the same route.
fn fetched(
    sandbox: &AppContainerSandbox,
    url: &str,
    policy: &SandboxPolicy,
) -> (Option<i32>, String) {
    run(
        sandbox,
        &curl(),
        &[
            "-q",
            "-sS",
            "--noproxy",
            "*",
            "-m",
            "20",
            "-o",
            "fetched",
            url,
        ],
        policy,
    )
    .expect("curl starts")
}

/// One of the addresses `OUTSIDE` names, looked up by this process rather than in a container.
fn an_outside_address() -> SocketAddr {
    (OUTSIDE, 80)
        .to_socket_addrs()
        .expect("this machine looks up the host")
        .find(SocketAddr::is_ipv4)
        .expect("the host has an IPv4 address")
}

/// Fetch `url` from a container granted the network, which has to succeed for the rest to mean
/// anything, and then from the same base without it, which has to end with `refused`.
fn reached_only_with_the_network(name: &str, url: &str, refused: i32) {
    let sandbox = sandbox();
    let allowed = the_base(name);
    let denied = allowed.clone().without_network_egress();
    assert!(allowed.allow_network && !denied.allow_network);

    let (code, printed) = fetched(&sandbox, url, &allowed);
    assert_eq!(
        code,
        Some(0),
        "a container granted the network did not reach {url}, so nothing below means anything: \
         {printed}"
    );

    let (code, printed) = fetched(&sandbox, url, &denied);
    assert_eq!(
        code,
        Some(refused),
        "a container denied the network did not end as curl ends when refused: {printed}"
    );
}

/// SANDBOX-3 on Windows: a container connects out only when its policy asks for the network. The
/// regressions it rejects: a token built with the internet client capability whatever the policy
/// asked for, which is egress no record shows, and one built without it, which is every stage
/// that needs the network failing. Both halves connect to the same address, so the refused half
/// is a connection the container was denied and not a host that was not there. A container is
/// refused the loopback interface whatever its capabilities, so the address is outside this
/// machine.
#[test]
fn a_container_connects_out_only_when_its_policy_asks_for_the_network() {
    let url = format!("http://{}/", an_outside_address());
    reached_only_with_the_network("egress", &url, CURL_COULD_NOT_CONNECT);
}

/// SANDBOX-3 on Windows: a container without the network is refused a host name lookup as well as
/// a connection. A lookup is a request the resolver sends out for the program, so a name that
/// carries data reaches whoever answers for that name. The regression it rejects: a container
/// that looks names up without the capability, which `curl` reports as a connection refused
/// rather than a host it could not resolve.
#[test]
fn a_container_without_the_network_cannot_look_up_a_name() {
    reached_only_with_the_network(
        "lookup",
        &format!("http://{OUTSIDE}/"),
        CURL_COULD_NOT_RESOLVE,
    );
}

/// The regression it rejects: an empty base that leaves a program unable to start. SANDBOX-3
/// holds that a container reads the system directories through an entry the platform already
/// wrote, so `cmd.exe` needs no row. A compiler is not in them on every machine, so its
/// directory is granted the way a plan grants a directory on `PATH`.
#[test]
fn cmd_and_a_compiler_start_under_the_empty_base() {
    let sandbox = sandbox();
    let policy = the_base("starts");

    let (code, printed) = cmd(&sandbox, "echo hello", &policy).expect("cmd.exe starts");
    assert_eq!(code, Some(0), "{printed}");
    assert_eq!(printed, "hello", "cmd.exe did not read /c");

    let compiler = ["gcc", "cc", "clang"]
        .into_iter()
        .find_map(on_the_path)
        .expect("a compiler is on PATH, and this job installs one");
    let policy = policy.allow_read(compiler.parent().expect("a directory holds the compiler"));
    let (code, printed) = run(
        &sandbox,
        &compiler.display().to_string(),
        &["--version"],
        &policy,
    )
    .expect("the compiler starts");
    assert_eq!(code, Some(0), "{printed}");
}

/// The regression it rejects: a stdio server's streams not reaching a program in the container, so
/// a line written to its input never comes back on its output. A local MCP server is a program of
/// this shape: it reads lines from its input, answers on its output, and ends when its input does.
/// `findstr` is one, answering each line with itself, and starts under the empty base like `cmd.exe`
/// does, with a directory of its own granted the way a server's home is.
#[test]
fn a_program_answering_lines_on_its_streams_starts_under_the_base_and_ends_with_its_input() {
    let sandbox = sandbox();
    let home = scratch("stdio-server-home");
    let policy = the_base("stdio-server")
        .allow_read(&home)
        .allow_write(&home)
        .starting_in(&home);
    let streams = Streams {
        stdin: Stream::Piped,
        stdout: Stream::Piped,
        stderr: Stream::Null,
    };
    let mut child = sandbox
        .spawn(
            "findstr.exe".as_ref(),
            &[OsString::from("^")],
            &policy,
            streams,
            Environment::Inherited,
        )
        .expect("the server starts");
    let mut input = child.take_stdin().expect("a stdin that was piped");
    let mut output = child.take_stdout().expect("a stdout that was piped");

    // `findstr` holds its output back while it is a pipe, so the answers are read once the input
    // ends rather than line by line.
    input
        .write_all(b"first\r\nsecond\r\n")
        .expect("the server is handed its lines");
    drop(input);
    let mut answered = String::new();
    output
        .read_to_string(&mut answered)
        .expect("the server answers");
    assert_eq!(
        answered.lines().collect::<Vec<_>>(),
        ["first", "second"],
        "the server did not answer each line with itself"
    );

    let status = child.wait().expect("the server ends");
    assert_eq!(
        status.code(),
        Some(0),
        "the server did not end with its input"
    );
}

/// The regression it rejects: an empty base that leaves `git` unable to start. `git` opens
/// `/dev/null` as `NUL` before it reads its arguments, so the base starts it only where the device
/// lets a container open it. On Windows Server 2025 the device's access list names no ALL
/// APPLICATION PACKAGES entry, a container is denied it (`cmd.exe /c "type nul"` ends with "Access
/// is denied" in the same container), and every stage that runs `git` ends with "could not open
/// '/dev/null'" until an administrator adds the entry. The job adds it before this runs.
#[test]
fn git_starts_under_the_empty_base() {
    let sandbox = sandbox();
    let policy = the_base("git");

    let (code, printed) = run(&sandbox, "git", &["--version"], &policy).expect("git starts");

    assert_eq!(
        code,
        Some(0),
        "{printed} (a container can start git only where the NUL device names ALL APPLICATION \
         PACKAGES)"
    );
    assert!(printed.starts_with("git version"), "{printed}");
}

/// The regression it rejects: a kill that ends the stage's own process and nothing it started, so
/// a line that was cancelled or ran out of time goes on running in its children, with the
/// container's token, its network and the stage's pipes.
#[test]
fn killing_a_stage_ends_the_programs_it_started() {
    let sandbox = sandbox();
    let (mut child, input, seen) = a_stage_with_a_child_waiting_on_its_input(&sandbox, "kill");

    child.kill().expect("the stage is killed");
    let ended = seen.recv_timeout(ENDING);
    // A child still running reads the end of its input here and exits, so a failure below leaves
    // no process behind.
    drop(input);

    assert_eq!(
        ended,
        Ok(Seen::Ended),
        "a program the stage started outlived the kill"
    );
    assert_ne!(child.wait().expect("the stage ends").code(), Some(0));
}

/// The regression it rejects: a stage whose programs outlive every handle this process holds on
/// it. That is what is left when this process crashes or is ended before it kills the stage: the
/// platform closes its handles, and nothing else ends the programs.
#[test]
fn a_stage_let_go_of_while_running_leaves_no_program_behind() {
    let sandbox = sandbox();
    let (child, input, seen) = a_stage_with_a_child_waiting_on_its_input(&sandbox, "dropped");

    drop(child);
    let ended = seen.recv_timeout(ENDING);
    drop(input);

    assert_eq!(
        ended,
        Ok(Seen::Ended),
        "a program outlived the handle on its stage"
    );
}

/// Set when this binary is started again to hold a stage for the test that kills it.
const HOLDING_A_STAGE: &str = "BRAVEBOT_TEST_HOLDING_A_STAGE";

/// Start a stage that reads and writes this process's own input and output, and keep it until this
/// process is killed.
///
/// Its programs hold this process's output, so the test that started this one sees that output end
/// only when every one of them is gone. Killed, this process leaves its container profile
/// registered, as a crash does. Bounded, so a copy whose test died first lets go of the stage and
/// the profile.
fn hold_a_stage() -> ! {
    let sandbox = sandbox();
    let streams = Streams {
        stdin: Stream::Inherited,
        stdout: Stream::Inherited,
        stderr: Stream::Null,
    };
    let stage = sandbox
        .spawn(
            "cmd.exe".as_ref(),
            &[OsString::from("/c"), OsString::from("cmd")],
            &the_base("held"),
            streams,
            Environment::Inherited,
        )
        .expect("the stage starts");
    std::thread::sleep(STARTING + ENDING);
    drop(stage);
    drop(sandbox);
    std::process::exit(1)
}

/// The regression it rejects: a stage whose programs are ended by this process letting go of it
/// and not by the platform. A crash runs no code here, so what ends the programs then is the job
/// ending when its last handle is closed. The test that lets go of a stage passes either way.
#[test]
fn killing_the_process_holding_a_stage_ends_every_program_in_it() {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

    if std::env::var_os(HOLDING_A_STAGE).is_some() {
        hold_a_stage();
    }
    let mut holder = spawned(
        Command::new(std::env::current_exe().expect("this test binary"))
            .args([
                "--exact",
                "killing_the_process_holding_a_stage_ends_every_program_in_it",
                "--nocapture",
                "--quiet",
                "--test-threads=1",
            ])
            .env(HOLDING_A_STAGE, "1")
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit()),
    );
    let mut input = holder.stdin.take().expect("a stdin that was piped");
    let seen = watch(holder.stdout.take().expect("a stdout that was piped"));

    input
        .write_all(b"echo started\r\n")
        .expect("the child is handed a line");
    let started = seen.recv_timeout(STARTING);
    holder.kill().expect("the holder is killed");
    let _ = holder.wait();
    let ended = seen.recv_timeout(ENDING);
    drop(input);

    assert_eq!(
        started,
        Ok(Seen::Started),
        "the held stage's child never ran the line it was handed: a holder that failed said why \
         above, and one that said nothing ran no test, so its name above is not this test's"
    );
    assert_eq!(
        ended,
        Ok(Seen::Ended),
        "a program the stage started outlived the process holding it"
    );
}

/// The regression it rejects: a stage that inherits every inheritable handle this process holds
/// rather than the three streams it is given. One of those is the write end of a pipe another
/// process reads, which a stage holding it could write into, and a pipe whose write end a stage
/// holds does not end when its owner closes it.
#[test]
fn a_stage_inherits_no_handle_but_the_streams_it_is_given() {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::{HANDLE, HANDLE_FLAG_INHERIT, SetHandleInformation};

    let sandbox = sandbox();
    let spawning = spawning();
    let (reader, writer) = std::io::pipe().expect("a pipe");
    // SAFETY: `writer` is an open pipe handle for the call, and the call takes a handle and two
    // flag masks and reads no memory.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let marked = unsafe {
        SetHandleInformation(
            writer.as_raw_handle() as HANDLE,
            HANDLE_FLAG_INHERIT,
            HANDLE_FLAG_INHERIT,
        )
    };
    assert_ne!(marked, 0, "{}", std::io::Error::last_os_error());
    let (child, mut input, seen) = a_stage_reading_its_input(&sandbox, "handles");
    drop(writer);
    drop(spawning);
    hand_it_a_line(&mut input, &seen);

    let ended = watch(reader).recv_timeout(ENDING);
    drop(input);
    drop(child);

    assert_eq!(
        ended,
        Ok(Seen::Ended),
        "the stage holds a handle it was not given"
    );
}

/// RPCVIEW-8 on Windows: a stage is created under the gate every start of a program takes. The
/// regression it rejects: a stage created outside it. The copies of the streams a stage is given
/// are inheritable while it is created, and a program the standard library starts on another
/// thread in that window inherits them as well, so an unconfined program would hold the stage's
/// input and keep its output from ending.
#[test]
fn a_stage_start_waits_for_a_channel_being_created() {
    bravebot_sandbox::process::start_waits_at_the_gate_within(
        || {
            let sandbox = sandbox();
            let (code, printed) =
                cmd(&sandbox, "echo started", &the_base("gate")).expect("cmd.exe starts");
            assert_eq!(code, Some(0), "{printed}");
        },
        STARTING,
    );
}

/// The processes attached to this process's console, which the console itself keeps, or none
/// where this process has no console.
fn on_this_console() -> Vec<u32> {
    use windows_sys::Win32::System::Console::GetConsoleProcessList;

    let mut attached = vec![0u32; 64];
    loop {
        // SAFETY: the buffer holds as many identifiers as the count says, and the call writes
        // none when it needs more.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let count =
            unsafe { GetConsoleProcessList(attached.as_mut_ptr(), attached.len() as u32) } as usize;
        if count <= attached.len() {
            attached.truncate(count);
            return attached;
        }
        attached.resize(count, 0);
    }
}

/// Attach this process to a new console, leaving its standard streams where they were.
///
/// The platform points the three at the new console, and the other tests in this binary, which
/// run beside this one, and the harness's own report write through them.
fn give_this_process_a_console() {
    use windows_sys::Win32::System::Console::{
        AllocConsole, GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
        SetStdHandle,
    };

    let streams = [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE];
    // SAFETY: `GetStdHandle` takes a constant naming a stream and reads no memory.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let held = streams.map(|stream| unsafe { GetStdHandle(stream) });
    // SAFETY: `AllocConsole` takes no arguments and reads no memory.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let allocated = unsafe { AllocConsole() };
    assert_ne!(allocated, 0, "{}", std::io::Error::last_os_error());
    for (stream, handle) in streams.into_iter().zip(held) {
        // SAFETY: `handle` is the value `GetStdHandle` returned for this stream above, and the
        // call reads no memory.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        unsafe { SetStdHandle(stream, handle) };
    }
}

/// The regression it rejects: a stage attached to the console this process runs in. Any process
/// attached to a console can write input into it, so a program on the terminal's console can
/// type an answer to the prompt a person is about to read.
///
/// A process with no console has nothing to share, and a stage started from it is on a console
/// of its own whatever the backend asks for, so this one is given a console first.
#[test]
fn a_stage_is_not_attached_to_the_console_this_process_runs_in() {
    if on_this_console().is_empty() {
        give_this_process_a_console();
    }
    let sandbox = sandbox();
    let (child, input, _seen) = a_stage_with_a_child_waiting_on_its_input(&sandbox, "console");

    let attached = on_this_console();
    let stage = child.id();
    drop(input);
    drop(child);

    assert!(
        attached.contains(&std::process::id()),
        "this process is not on a console, so nothing was compared: {attached:?}"
    );
    assert!(
        !attached.contains(&stage),
        "the stage {stage} is attached to this process's console: {attached:?}"
    );
}

/// The regression it rejects: a job whose process limit counts more than the stage itself, so a
/// stage under a policy that denies subprocesses, which starts no child, is refused its own start
/// or is ended with it (the console host a stage without a window is given is the process the
/// limit might count). The stage runs a `cmd.exe` built-in, which starts no process, so the only
/// processes in the job are the stage and what the platform adds for its console. Whether a child
/// is refused is pinned in the tests that follow it, not here.
#[test]
fn a_stage_starting_no_child_runs_under_a_policy_that_denies_subprocesses() {
    let sandbox = sandbox();
    let policy = the_base("no-children");
    let denied = SandboxPolicy {
        allow_subprocesses: false,
        ..policy
    };
    assert!(!denied.allow_subprocesses);

    let (code, printed) = cmd(&sandbox, "echo hello", &denied)
        .expect("a policy denying subprocesses is applied by the job and not refused");

    assert_eq!(code, Some(0), "{printed}");
    assert_eq!(printed, "hello");
}

/// The line the next two tests hand `cmd.exe`: it starts a second `cmd.exe`, which is a program
/// the first creates rather than a built-in it runs itself, and prints a word only that second
/// one can print.
const STARTING_A_CHILD: &str = "cmd /c echo child-ran";

/// SANDBOX-17 on Windows: a policy that denies subprocesses holds a stage to the program it
/// started. The regression it rejects: a job with no process limit, which is what a policy
/// denying subprocesses had before the limit existed and what a limit set but never applied
/// leaves, so the stage's `cmd.exe` starts the second one and prints its word. The paired test
/// below runs the same line under a policy that allows subprocesses, so a child that failed to
/// start for another reason (the program missing, a path not granted) is not mistaken for one the
/// limit refused.
#[test]
fn a_stage_cannot_start_a_child_where_its_policy_denies_subprocesses() {
    let sandbox = sandbox();
    let denied = SandboxPolicy {
        allow_subprocesses: false,
        ..the_base("child-denied")
    };

    let (code, printed) = cmd(&sandbox, STARTING_A_CHILD, &denied)
        .expect("a policy denying subprocesses is applied by the job and not refused");

    assert_ne!(code, Some(0), "the stage started a child: {printed}");
    assert!(
        !printed.contains("child-ran"),
        "the stage started a child: {printed}"
    );
}

/// The paired half of the test above. The regression it rejects: a process limit applied to every
/// policy, which would stop a stage starting the programs its policy allows (the base allows them,
/// so a build or a `git` run from a shell would fail).
#[test]
fn a_stage_starts_a_child_where_its_policy_allows_subprocesses() {
    let sandbox = sandbox();
    let allowed = the_base("child-allowed");
    assert!(allowed.allow_subprocesses);

    let (code, printed) = cmd(&sandbox, STARTING_A_CHILD, &allowed).expect("cmd.exe starts");

    assert_eq!(code, Some(0), "{printed}");
    // `cmd /c` leaves the closing quote of its own line on what the second `cmd` echoes.
    assert_eq!(printed.trim_end_matches('"'), "child-ran");
}

/// The name this binary is copied to when it is started again inside a stage to say what its job
/// limits. The stage is started with this process's environment. Starting it with only a named
/// `SystemRoot` and one more variable failed on the Windows runner with error 203, and why is
/// not known (#3883).
const PROBE_FILE: &str = "bravebot-job-probe.exe";

/// The word a probe prints before what it found, so the test that started it picks the line out of
/// what the test harness prints around it.
const PROBED: &str = "job-probe";

/// What the job the calling process is in limits, as the process itself reads it, or why it
/// cannot.
fn the_job_this_process_is_in() -> Result<(u32, u32, u32), std::io::Error> {
    use std::mem::size_of;
    use windows_sys::Win32::System::JobObjects::{
        JOBOBJECT_BASIC_UI_RESTRICTIONS, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectBasicUIRestrictions, JobObjectExtendedLimitInformation, QueryInformationJobObject,
    };

    let mut interface = JOBOBJECT_BASIC_UI_RESTRICTIONS::default();
    // SAFETY: a null job names the job of the calling process. `interface` is a local of exactly
    // the size passed, alive for the call, and the call writes at most that many bytes into it.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let queried = unsafe {
        QueryInformationJobObject(
            std::ptr::null_mut(),
            JobObjectBasicUIRestrictions,
            std::ptr::from_mut(&mut interface).cast(),
            size_of::<JOBOBJECT_BASIC_UI_RESTRICTIONS>() as u32,
            std::ptr::null_mut(),
        )
    };
    if queried == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let mut extended = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    // SAFETY: as above, with `extended` the local of the size passed.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let queried = unsafe {
        QueryInformationJobObject(
            std::ptr::null_mut(),
            JobObjectExtendedLimitInformation,
            std::ptr::from_mut(&mut extended).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            std::ptr::null_mut(),
        )
    };
    if queried == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok((
        interface.UIRestrictionsClass,
        extended.BasicLimitInformation.LimitFlags,
        extended.BasicLimitInformation.ActiveProcessLimit,
    ))
}

/// Run only inside a stage that [`the_job_a_stage_is_in`] started, where it prints what the stage's
/// job limits. Started any other way it does nothing, so the ordinary run of this binary passes
/// it without reading the job of the terminal or runner it is in.
#[test]
fn probe_the_job_this_stage_is_in() {
    let is_the_copy = std::env::current_exe()
        .ok()
        .and_then(|path| path.file_name().map(|name| name == PROBE_FILE))
        .unwrap_or(false);
    if !is_the_copy {
        return;
    }
    match the_job_this_process_is_in() {
        Ok((interface, flags, processes)) => {
            println!("{PROBED} interface={interface} flags={flags} processes={processes}");
        }
        Err(error) => println!("{PROBED} error={error}"),
    }
}

/// What a stage's job limits, as a program in the stage reads it.
struct JobSeen {
    /// The `JOB_OBJECT_UILIMIT_*` flags.
    interface: u32,
    /// The `JOB_OBJECT_LIMIT_*` flags.
    flags: u32,
    /// The active process limit, which means nothing unless `flags` holds the limit.
    processes: u32,
}

/// Start this test binary as a stage under `policy` and read its job from inside it.
///
/// The binary is copied into a directory of its own that the stage is granted, since the
/// directory it was built in holds every other artefact of the build and a grant is written onto
/// each file beneath it.
fn the_job_a_stage_is_in(name: &str, policy: SandboxPolicy) -> JobSeen {
    let sandbox = sandbox();
    let directory = scratch(&format!("{name}-probe"));
    let probe = directory.join(PROBE_FILE);
    std::fs::copy(std::env::current_exe().expect("this test binary"), &probe)
        .expect("the test binary is copied");
    let policy = policy.allow_read(&directory);

    let (code, printed) = run(
        &sandbox,
        &probe.display().to_string(),
        &[
            "--exact",
            "probe_the_job_this_stage_is_in",
            "--nocapture",
            "--quiet",
            "--test-threads=1",
        ],
        &policy,
    )
    .expect("the probe starts");

    assert_eq!(code, Some(0), "the probe failed: {printed}");
    let found = printed
        .lines()
        .find_map(|line| line.trim().strip_prefix(PROBED))
        .unwrap_or_else(|| panic!("the probe printed no {PROBED} line: {printed}"));
    assert!(!found.contains("error="), "the job was not read: {found}");
    let field = |key: &str| {
        found
            .split_whitespace()
            .find_map(|part| part.strip_prefix(key))
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or_else(|| panic!("the probe line has no {key}: {found}"))
    };
    JobSeen {
        interface: field("interface="),
        flags: field("flags="),
        processes: field("processes="),
    }
}

/// SANDBOX-17 on Windows: the job a stage is in carries the UI limits and ends its programs when
/// it is closed, whatever the policy. The regressions it rejects: a flag left out of the set (the
/// clipboard read or write, the atom table, desktops, USER handles), and a job made without them
/// that a request for them would have to be refused for. The flags are read by a program in the
/// stage from its own job, and compared with the platform's constants and not this crate's.
#[test]
fn a_stage_is_in_a_job_carrying_every_ui_limit() {
    use windows_sys::Win32::System::JobObjects::{
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_UILIMIT_DESKTOP,
        JOB_OBJECT_UILIMIT_GLOBALATOMS, JOB_OBJECT_UILIMIT_HANDLES,
        JOB_OBJECT_UILIMIT_READCLIPBOARD, JOB_OBJECT_UILIMIT_WRITECLIPBOARD,
    };

    for (name, policy) in [
        ("ui-allowing", the_base("ui-allowing")),
        (
            "ui-denying",
            SandboxPolicy {
                allow_subprocesses: false,
                ..the_base("ui-denying")
            },
        ),
    ] {
        let seen = the_job_a_stage_is_in(name, policy);

        for (limit, flag) in [
            ("clipboard read", JOB_OBJECT_UILIMIT_READCLIPBOARD),
            ("clipboard write", JOB_OBJECT_UILIMIT_WRITECLIPBOARD),
            ("global atoms", JOB_OBJECT_UILIMIT_GLOBALATOMS),
            ("desktop", JOB_OBJECT_UILIMIT_DESKTOP),
            ("USER handles", JOB_OBJECT_UILIMIT_HANDLES),
        ] {
            assert_eq!(
                seen.interface & flag,
                flag,
                "the {limit} limit is not in the job of the {name} stage"
            );
        }
        assert_eq!(
            seen.flags & JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            "the job of the {name} stage does not end its programs when closed"
        );
    }
}

/// SANDBOX-17 on Windows: the job's process limit is present for a policy that denies
/// subprocesses, is one, and is absent for a policy that allows them. The regressions it rejects:
/// no limit where the policy denies subprocesses, a limit above one, which lets a child start, and
/// a limit on every policy. Read by a program in the stage from its own job, so it is the limit in
/// force and not the one this crate meant to set.
#[test]
fn a_stage_is_in_a_job_bounding_processes_only_where_its_policy_denies_subprocesses() {
    use windows_sys::Win32::System::JobObjects::JOB_OBJECT_LIMIT_ACTIVE_PROCESS;

    let denied = the_job_a_stage_is_in(
        "processes-denying",
        SandboxPolicy {
            allow_subprocesses: false,
            ..the_base("processes-denying")
        },
    );
    let allowed = the_job_a_stage_is_in("processes-allowing", the_base("processes-allowing"));

    assert_eq!(
        denied.flags & JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
        JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
        "the job of a stage denied subprocesses has no process limit"
    );
    assert_eq!(denied.processes, 1);
    assert_eq!(
        allowed.flags & JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
        0,
        "the job of a stage allowed subprocesses has a process limit"
    );
}

/// The regression it rejects: a refusal of a read or a write that this backend cannot apply being
/// dropped, so the program starts and reaches the path the person meant to hold back. The write
/// is the one observable from outside: the file must not be there afterwards.
#[test]
fn a_policy_that_refuses_a_read_or_a_write_starts_no_program() {
    let sandbox = sandbox();
    let directory = scratch("refusals");
    let marker = directory.join("marker");
    let policy = the_base("refusals").allow_write(&directory);
    let line = format!("mkdir {}", marker.display());

    let refused_read = cmd(
        &sandbox,
        &line,
        &policy.clone().deny_read(directory.join("secret")),
    );
    let refused_write = cmd(&sandbox, &line, &policy.deny_write(&marker));

    for refused in [refused_read, refused_write] {
        assert!(
            matches!(
                refused,
                Err(SandboxError::SetupFailed { .. } | SandboxError::PolicyTooPermissive)
            ),
            "the stage started: {refused:?}"
        );
    }
    assert!(!marker.exists(), "a program ran under a refused policy");
}

/// SANDBOX-4 for `cmd.exe` with a real process: a name holding `&` is refused before anything
/// starts, so the directory is not made under any name. The regression it rejects is the backend
/// quoting the argument and starting `cmd.exe`, which reads the `&` as its own rather than as part
/// of the name.
#[test]
fn cmd_with_an_argument_holding_an_ampersand_starts_nothing() {
    let sandbox = sandbox();
    let directory = scratch("ampersand");
    let policy = the_base("ampersand").allow_write(&directory);
    let named = directory.join("R&D");

    let refused = run(
        &sandbox,
        "cmd.exe",
        &["/c", "mkdir", &named.display().to_string()],
        &policy,
    );

    assert!(
        matches!(refused, Err(SandboxError::SetupFailed { .. })),
        "cmd.exe started: {refused:?}"
    );
    let made: Vec<_> = std::fs::read_dir(&directory)
        .expect("the scratch directory")
        .collect();
    assert!(made.is_empty(), "cmd.exe made {made:?}");
}
