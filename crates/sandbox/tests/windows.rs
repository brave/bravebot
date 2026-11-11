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
    run(sandbox, &cmd_exe(), &["/c", line], policy)
}

/// `cmd.exe` in the system directory, an absolute path as the backend requires.
fn cmd_exe() -> String {
    system32("cmd.exe")
}

/// A program in the system directory, an absolute path as the backend requires.
fn system32(name: &str) -> String {
    let root = std::env::var_os("SystemRoot").expect("the system directory is named");
    Path::new(&root)
        .join("System32")
        .join(name)
        .display()
        .to_string()
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
            cmd_exe().as_ref(),
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
/// A stage on the Windows runner is refused `mklink /J` through `cmd.exe` ("Access is denied"), so
/// two routes are tried in turn: `mklink /J` in a stage, then a probe in a stage that sets the
/// mount point reparse point itself with `FSCTL_SET_REPARSE_POINT`. When both are refused, the test
/// prints `junction not made by a stage by either route: ...` to stderr and returns, since with no
/// junction a stage made there is no grant to cross it. The route that made the junction is printed
/// as `junction made by a stage with mklink` or `junction made by a stage with a probe`.
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
        if code == Some(0) {
            eprintln!("junction made by a stage with mklink");
        } else {
            // Observed on the Windows runner: a container with write access to the directory is
            // refused `mklink /J` ("Access is denied"). The second route sets the reparse point
            // from a program in the stage, which does not go through `cmd.exe`. An empty directory
            // `mklink` left behind would make the probe's `create_dir` fail, so it is removed.
            let _ = std::fs::remove_dir(&link);
            let probed = probe_in_a_stage(
                &making,
                "stage-junction-making",
                policy.clone(),
                "probe_make_a_junction_in_this_stage",
                &[&link.display().to_string(), &withheld.display().to_string()],
            );
            let found = what_the_probe_printed(&probed, JUNCTION_PROBED);
            if found != "junction made" {
                // With no junction made by a stage there is no grant to cross it, which settles
                // nothing about a junction a stage could make some other way.
                eprintln!(
                    "junction not made by a stage by either route: mklink: {printed}; probe: {found}"
                );
                return;
            }
            eprintln!("junction made by a stage with a probe");
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
                cmd_exe().as_ref(),
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
    system32("curl.exe")
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
            system32("findstr.exe").as_ref(),
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
    let git = on_the_path("git").expect("git is on PATH, and this job installs it");
    let policy = the_base("git");

    let (code, printed) = run(
        &sandbox,
        &git.display().to_string(),
        &["--version"],
        &policy,
    )
    .expect("git starts");

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
/// registered, as a crash does. The scratch directory is named for this process, so two holders
/// started by different tests do not share one. Bounded, so a copy whose test died first lets go of
/// the stage and the profile.
fn hold_a_stage() -> ! {
    let sandbox = sandbox();
    let streams = Streams {
        stdin: Stream::Inherited,
        stdout: Stream::Inherited,
        stderr: Stream::Null,
    };
    let stage = sandbox
        .spawn(
            cmd_exe().as_ref(),
            &[OsString::from("/c"), OsString::from("cmd")],
            &the_base(&format!("held-{}", std::process::id())),
            streams,
            Environment::Inherited,
        )
        .expect("the stage starts");
    std::thread::sleep(STARTING + ENDING);
    drop(stage);
    drop(sandbox);
    std::process::exit(1)
}

/// Start this binary again as the process that holds a stage, and hand the stage's child a line.
///
/// The holder runs `killing_the_process_holding_a_stage_ends_every_program_in_it`, which becomes
/// [`hold_a_stage`] when [`HOLDING_A_STAGE`] is set. Returns the holder, the input it was handed
/// the line on, and what its output shows.
fn start_a_holder() -> (
    std::process::Child,
    std::process::ChildStdin,
    mpsc::Receiver<Seen>,
) {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

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
    (holder, input, seen)
}

/// The regression it rejects: a stage whose programs are ended by this process letting go of it
/// and not by the platform. A crash runs no code here, so what ends the programs then is the job
/// ending when its last handle is closed. The test that lets go of a stage passes either way.
#[test]
fn killing_the_process_holding_a_stage_ends_every_program_in_it() {
    if std::env::var_os(HOLDING_A_STAGE).is_some() {
        hold_a_stage();
    }
    let (mut holder, input, seen) = start_a_holder();
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

/// Delete the container profile `name` and return the `HRESULT` the platform answered with, which
/// is negative on failure.
fn delete_container_profile(name: &str) -> i32 {
    use windows_sys::Win32::Security::Isolation::DeleteAppContainerProfile;

    let name: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `name` is a terminated wide string that is alive for the call, which reads it and
    // nothing else.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    unsafe { DeleteAppContainerProfile(name.as_ptr()) }
}

/// SANDBOX-17 on Windows: a run that ends without reaching the drop of its backend leaves the
/// container profile it made registered. The regression it rejects: a profile that is deleted when
/// its holder is killed, which would need something outside the holder to delete it and is not what
/// the spec says. The spec says a run that ends without the drop leaves the profile and its
/// entries, and bounds what is left by a profile name that carries a per-backend nonce.
///
/// The holder is killed, so no code of its own runs. A profile is a folder under
/// `%LOCALAPPDATA%\Packages` named for it, and its name starts with `bravebot-` and the process
/// identifier of the process that made it, so the folders named for the holder are the profiles it
/// left. They are deleted at the end so the machine is left as it was found, and the folders
/// remaining afterwards are printed.
#[test]
fn a_holder_killed_while_it_holds_a_stage_leaves_its_container_profile_registered() {
    let (mut holder, input, seen) = start_a_holder();
    let started = seen.recv_timeout(STARTING);
    let holder_id = holder.id();
    holder.kill().expect("the holder is killed");
    let _ = holder.wait();
    let ended = seen.recv_timeout(ENDING);
    drop(input);
    assert_eq!(
        started,
        Ok(Seen::Started),
        "the held stage's child never ran the line it was handed"
    );
    assert_eq!(
        ended,
        Ok(Seen::Ended),
        "a program the stage started outlived the process holding it"
    );

    let packages = PathBuf::from(std::env::var_os("LOCALAPPDATA").expect("the profile folder"))
        .join("Packages");
    let prefix = format!("bravebot-{holder_id}-");
    let named_for_the_holder = || -> Vec<String> {
        std::fs::read_dir(&packages)
            .expect("the packages folder is readable")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(&prefix))
            .collect()
    };
    let left = named_for_the_holder();
    let deleted: Vec<(String, i32)> = left
        .iter()
        .map(|name| (name.clone(), delete_container_profile(name)))
        .collect();
    eprintln!(
        "profiles of holder {holder_id}: left {left:?}; deleted {deleted:?}; remaining after the \
         delete {:?}",
        named_for_the_holder()
    );

    assert!(
        !left.is_empty(),
        "no folder named {prefix}* under {}: the killed holder's profile is not registered",
        packages.display()
    );
    assert!(
        deleted.iter().all(|(_, result)| *result >= 0),
        "a leftover profile was not deleted: {deleted:?}"
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

/// A stage started with a named environment prints the variable it was given wherever it starts.
/// The regression it rejects: a block that is dropped and replaced by the inherited environment,
/// which starts the program and prints nothing for the variable.
///
/// Whether a start succeeds is not asserted. The platform refuses a start with only `SystemRoot`
/// and one more variable with `SpawnFailed` and OS error 203 ("The system could not find the
/// environment option that was entered"), and which variable it looks for is not established
/// (Known costs in the sandboxing spec). The test starts the program with five environments, each
/// naming more than the one before, and prints the outcome of each, so the log shows which of
/// them the platform refuses. An environment that does start the program must print the variable.
///
/// The program is `cmd.exe /c set BRAVEBOT_NAMED`, which prints `BRAVEBOT_NAMED=named`. The line
/// holds no `%`: the backend refuses a `cmd.exe` argument holding one, because `cmd.exe` reads it
/// as its own syntax (SANDBOX-4).
#[test]
fn a_stage_that_starts_with_a_named_environment_prints_the_variable_it_was_given() {
    let sandbox = sandbox();
    let policy = the_base("named-environment");
    let system_root = std::path::PathBuf::from(
        std::env::var_os("SystemRoot").expect("the system directory is named"),
    );
    let temporary = system_root.join("Temp");
    let shell = system_root.join("System32").join("cmd.exe");
    let named = || {
        bravebot_sandbox::Variables::new()
            .with("SystemRoot", system_root.clone())
            .with("BRAVEBOT_NAMED", "named")
    };
    let environments = [
        ("SystemRoot and one variable", named()),
        ("plus windir", named().with("windir", system_root.clone())),
        (
            "plus windir, TEMP and TMP",
            named()
                .with("windir", system_root.clone())
                .with("TEMP", temporary.clone())
                .with("TMP", temporary.clone()),
        ),
        (
            "plus windir, TEMP, TMP, PATH and ComSpec",
            named()
                .with("windir", system_root.clone())
                .with("TEMP", temporary.clone())
                .with("TMP", temporary.clone())
                .with("PATH", system_root.join("System32"))
                .with("ComSpec", shell.clone()),
        ),
        (
            "plus windir, TEMP, TMP, PATH, ComSpec and the profile and machine variables",
            [
                "USERPROFILE",
                "LOCALAPPDATA",
                "APPDATA",
                "ProgramData",
                "SystemDrive",
                "ProgramFiles",
                "ProgramFiles(x86)",
                "ProgramW6432",
                "PROCESSOR_ARCHITECTURE",
                "NUMBER_OF_PROCESSORS",
                "OS",
            ]
            .into_iter()
            .filter_map(|name| std::env::var_os(name).map(|value| (name, value)))
            .fold(
                named()
                    .with("windir", system_root.clone())
                    .with("TEMP", temporary.clone())
                    .with("TMP", temporary.clone())
                    .with("PATH", system_root.join("System32"))
                    .with("ComSpec", shell),
                |held, (name, value)| held.with(name, value),
            ),
        ),
    ];

    for (label, variables) in environments {
        let outcome = run_with(
            &sandbox,
            "cmd.exe",
            &["/c", "set BRAVEBOT_NAMED"],
            &policy,
            Environment::Only(variables),
        );
        eprintln!("named-environment {label}: {outcome:?}");
        if let Ok((_, printed)) = &outcome {
            assert_eq!(
                printed.trim_end_matches('"'),
                "BRAVEBOT_NAMED=named",
                "{label}: the program started and did not print the variable"
            );
        }
    }
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

/// The name this binary is copied to when it is started again inside a stage to run one of the
/// probes below. The stage is started with this process's environment, since a start with only a
/// named `SystemRoot` and one more variable is refused with error 203 (#3883).
const PROBE_FILE: &str = "bravebot-job-probe.exe";

/// The word a probe prints before what it found, so the test that started it picks the line out of
/// what the test harness prints around it.
const PROBED: &str = "job-probe";

/// The word the junction probe prints before `junction made` or `junction not made: <os error>`.
const JUNCTION_PROBED: &str = "junction-probe";

/// The word the connection probe prints before `connected` or `refused: <os error>`.
const CONNECT_PROBED: &str = "connect-probe";

/// Whether this process is the copy of the test binary that a stage was started with, which is the
/// only place a probe test does anything. Started any other way a probe returns, so the ordinary
/// run of this binary passes it without acting on the terminal or runner it is in.
fn this_is_a_probe_copy() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.file_name().map(|name| name == PROBE_FILE))
        .unwrap_or(false)
}

/// Make a junction at `link` that leads to `target`, the way `mklink /J` does and without
/// `cmd.exe`.
///
/// `link` is made as an empty directory first, and the mount point reparse point is set on it with
/// `FSCTL_SET_REPARSE_POINT`. The directory is removed again when the reparse point is refused.
fn make_a_junction(link: &Path, target: &Path) -> Result<(), std::io::Error> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_WRITE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    // The call is in the `Win32_System_IO` module of the bindings, a feature this crate's
    // manifest does not enable and the account that writes these tests may not add. It is
    // declared here with the platform's signature instead.
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn DeviceIoControl(
            device: *mut core::ffi::c_void,
            control_code: u32,
            input: *const core::ffi::c_void,
            input_size: u32,
            output: *mut core::ffi::c_void,
            output_size: u32,
            returned: *mut u32,
            overlapped: *mut core::ffi::c_void,
        ) -> i32;
    }

    /// `FSCTL_SET_REPARSE_POINT`.
    const SET_REPARSE_POINT: u32 = 0x0009_00A4;
    /// `IO_REPARSE_TAG_MOUNT_POINT`, the tag of a junction.
    const MOUNT_POINT: u32 = 0xA000_0003;

    let too_long = || {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "the target is too long for a reparse point",
        )
    };
    let substitute: Vec<u16> = OsStr::new(r"\??\")
        .encode_wide()
        .chain(target.as_os_str().encode_wide())
        .collect();
    let print: Vec<u16> = target.as_os_str().encode_wide().collect();
    let substitute_bytes = u16::try_from(substitute.len() * 2).map_err(|_| too_long())?;
    let print_bytes = u16::try_from(print.len() * 2).map_err(|_| too_long())?;
    // The four u16 fields of a mount point buffer, then both names and a terminator after each.
    let data_length = 8u16
        .checked_add(substitute_bytes)
        .and_then(|length| length.checked_add(print_bytes))
        .and_then(|length| length.checked_add(4))
        .ok_or_else(too_long)?;
    // REPARSE_DATA_BUFFER for a mount point, little-endian: the tag, the length of the data after
    // the first eight bytes, a reserved field, the offset and length of the substitute name, the
    // offset and length of the print name, and the names.
    let mut buffer: Vec<u8> = Vec::new();
    buffer.extend_from_slice(&MOUNT_POINT.to_le_bytes());
    buffer.extend_from_slice(&data_length.to_le_bytes());
    buffer.extend_from_slice(&0u16.to_le_bytes());
    buffer.extend_from_slice(&0u16.to_le_bytes());
    buffer.extend_from_slice(&substitute_bytes.to_le_bytes());
    buffer.extend_from_slice(&(substitute_bytes + 2).to_le_bytes());
    buffer.extend_from_slice(&print_bytes.to_le_bytes());
    for unit in substitute
        .iter()
        .copied()
        .chain([0])
        .chain(print.iter().copied())
        .chain([0])
    {
        buffer.extend_from_slice(&unit.to_le_bytes());
    }
    let size = u32::try_from(buffer.len()).map_err(|_| too_long())?;

    std::fs::create_dir(link)?;
    let wide: Vec<u16> = link.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: `wide` is a terminated wide string alive for the call, the security attributes and
    // the template are null, and the call reads nothing else.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let directory = unsafe {
        CreateFileW(
            wide.as_ptr(),
            GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if directory == INVALID_HANDLE_VALUE {
        let refused = std::io::Error::last_os_error();
        let _ = std::fs::remove_dir(link);
        return Err(refused);
    }
    let mut returned = 0u32;
    // SAFETY: `directory` is the open handle above. `buffer` holds `size` bytes and is alive for
    // the call, which only reads it. The call has no output buffer, `returned` is a local it
    // writes, and the overlapped pointer is null because the handle was not opened for it.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let set = unsafe {
        DeviceIoControl(
            directory,
            SET_REPARSE_POINT,
            buffer.as_ptr().cast(),
            size,
            std::ptr::null_mut(),
            0,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    let refused = (set == 0).then(std::io::Error::last_os_error);
    // SAFETY: `directory` is the handle opened above and is not used again.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    unsafe { CloseHandle(directory) };
    if let Some(refused) = refused {
        let _ = std::fs::remove_dir(link);
        return Err(refused);
    }
    Ok(())
}

/// Run only inside a stage that `a_grant_does_not_reach_through_a_junction_a_stage_made` started,
/// where it makes a junction at the first of the last two arguments leading to the second, and
/// prints
/// `junction-probe junction made` or `junction-probe junction not made: <os error>`. Started any
/// other way it does nothing.
#[test]
fn probe_make_a_junction_in_this_stage() {
    if !this_is_a_probe_copy() {
        return;
    }
    let arguments: Vec<String> = std::env::args().collect();
    let [.., link, target] = arguments.as_slice() else {
        println!("{JUNCTION_PROBED} junction not made: the probe was given no link and target");
        return;
    };
    match make_a_junction(Path::new(link), Path::new(target)) {
        Ok(()) => println!("{JUNCTION_PROBED} junction made"),
        Err(error) => println!("{JUNCTION_PROBED} junction not made: {error}"),
    }
}

/// Run only inside a stage that `a_container_granted_the_network_cannot_connect_to_this_hosts_loopback`
/// started, where it connects to the address in the last argument and prints `connect-probe connected` or
/// `connect-probe refused: <error>`, or `connect-probe not run: <reason>` when it has no address to
/// connect to. Started any other way it does nothing.
#[test]
fn probe_connect_from_this_stage() {
    if !this_is_a_probe_copy() {
        return;
    }
    let Some(address) = std::env::args()
        .next_back()
        .and_then(|text| text.parse::<SocketAddr>().ok())
    else {
        println!("{CONNECT_PROBED} not run: the probe was given no address");
        return;
    };
    match std::net::TcpStream::connect_timeout(&address, Duration::from_secs(10)) {
        Ok(_) => println!("{CONNECT_PROBED} connected"),
        Err(error) => println!("{CONNECT_PROBED} refused: {error}"),
    }
}

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
    if !this_is_a_probe_copy() {
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

/// Copy this test binary into a directory of its own that the stage is granted, and run its test
/// `test` as a stage under `policy` with `arguments` after the test's name. Returns what the probe
/// printed.
///
/// The directory is not the one the binary was built in, since that holds every other artefact of
/// the build and a grant is written onto each file beneath it. The stage is started with this
/// process's environment, so the arguments are how a probe is told what to act on.
fn probe_in_a_stage(
    sandbox: &AppContainerSandbox,
    name: &str,
    policy: SandboxPolicy,
    test: &str,
    arguments: &[&str],
) -> String {
    let directory = scratch(&format!("{name}-probe"));
    let probe = directory.join(PROBE_FILE);
    std::fs::copy(std::env::current_exe().expect("this test binary"), &probe)
        .expect("the test binary is copied");
    let policy = policy.allow_read(&directory);
    let mut args = vec![
        "--exact",
        test,
        "--nocapture",
        "--quiet",
        "--test-threads=1",
    ];
    args.extend_from_slice(arguments);

    let (code, printed) =
        run(sandbox, &probe.display().to_string(), &args, &policy).expect("the probe starts");

    assert_eq!(code, Some(0), "the probe failed: {printed}");
    printed
}

/// What follows `marker` on the line of `printed` that starts with it, which is the line a probe
/// printed among what the test harness prints around it.
fn what_the_probe_printed<'a>(printed: &'a str, marker: &str) -> &'a str {
    printed
        .lines()
        .find_map(|line| line.trim().strip_prefix(marker))
        .unwrap_or_else(|| panic!("the probe printed no {marker} line: {printed}"))
        .trim()
}

/// Start this test binary as a stage under `policy` and read its job from inside it.
///
/// The binary is copied into a directory of its own that the stage is granted, since the
/// directory it was built in holds every other artefact of the build and a grant is written onto
/// each file beneath it.
fn the_job_a_stage_is_in(name: &str, policy: SandboxPolicy) -> JobSeen {
    let sandbox = sandbox();
    let printed = probe_in_a_stage(
        &sandbox,
        name,
        policy,
        "probe_the_job_this_stage_is_in",
        &[],
    );
    let found = what_the_probe_printed(&printed, PROBED);
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

/// Set when this binary is started again to run its stages from a process that is in a job.
const IN_A_JOB: &str = "BRAVEBOT_TEST_IN_A_JOB";

/// What the copy of this binary that runs its stages from inside a job prints once they passed.
const NESTED: &str = "nested-job-limits-held";

/// Whether the platform says this process is in a job.
fn this_process_is_in_a_job() -> bool {
    use windows_sys::Win32::System::JobObjects::IsProcessInJob;
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let mut in_a_job = 0;
    // SAFETY: `GetCurrentProcess` returns the pseudo handle of this process, a null job names any
    // job, and `in_a_job` is a local the call writes.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let asked = unsafe { IsProcessInJob(GetCurrentProcess(), std::ptr::null_mut(), &mut in_a_job) };
    assert_ne!(asked, 0, "{}", std::io::Error::last_os_error());
    in_a_job != 0
}

/// Put this process in a job of its own with no limits, left open until the process ends.
///
/// Where this process is already in a job, the new one is nested under it.
fn put_this_process_in_a_job() {
    use windows_sys::Win32::System::JobObjects::{AssignProcessToJobObject, CreateJobObjectW};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    // SAFETY: both arguments are null, so the job is unnamed and has default security. The call
    // reads no memory.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    assert!(!job.is_null(), "{}", std::io::Error::last_os_error());
    // SAFETY: `job` is the handle created above and is never closed, and `GetCurrentProcess`
    // returns the pseudo handle of this process.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let assigned = unsafe { AssignProcessToJobObject(job, GetCurrentProcess()) };
    assert_ne!(
        assigned,
        0,
        "this process was not put in a job: {}",
        std::io::Error::last_os_error()
    );
}

/// Assert that stages started from this process are held to the limits of their policy: the UI
/// limits and the end of the job with its handle for every policy, a process limit of one only
/// where the policy denies subprocesses, and a child refused where it does.
fn assert_stages_are_limited(name: &str) {
    use windows_sys::Win32::System::JobObjects::{
        JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOB_OBJECT_UILIMIT_DESKTOP, JOB_OBJECT_UILIMIT_GLOBALATOMS, JOB_OBJECT_UILIMIT_HANDLES,
        JOB_OBJECT_UILIMIT_READCLIPBOARD, JOB_OBJECT_UILIMIT_WRITECLIPBOARD,
    };

    let every_ui_limit = JOB_OBJECT_UILIMIT_READCLIPBOARD
        | JOB_OBJECT_UILIMIT_WRITECLIPBOARD
        | JOB_OBJECT_UILIMIT_GLOBALATOMS
        | JOB_OBJECT_UILIMIT_DESKTOP
        | JOB_OBJECT_UILIMIT_HANDLES;
    let denying = SandboxPolicy {
        allow_subprocesses: false,
        ..the_base(&format!("{name}-denying"))
    };

    let denied = the_job_a_stage_is_in(&format!("{name}-denying"), denying.clone());
    let allowed = the_job_a_stage_is_in(
        &format!("{name}-allowing"),
        the_base(&format!("{name}-allowing")),
    );

    for (which, seen) in [("denying", &denied), ("allowing", &allowed)] {
        assert_eq!(
            seen.interface & every_ui_limit,
            every_ui_limit,
            "a UI limit is missing from the job of a {which} stage started with {name}"
        );
        assert_eq!(
            seen.flags & JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            "the job of a {which} stage started with {name} does not end its programs when closed"
        );
    }
    assert_eq!(
        denied.flags & JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
        JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
        "the job of a stage denied subprocesses has no process limit when started with {name}"
    );
    assert_eq!(denied.processes, 1);
    assert_eq!(
        allowed.flags & JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
        0,
        "the job of a stage allowed subprocesses has a process limit when started with {name}"
    );

    let (code, printed) = cmd(&sandbox(), STARTING_A_CHILD, &denying)
        .expect("a policy denying subprocesses is applied by the job and not refused");
    assert_ne!(code, Some(0), "the stage started a child: {printed}");
    assert!(!printed.contains("child-ran"), "{printed}");
}

/// SANDBOX-17 on Windows: a stage is held to the limits of its policy when this process is itself
/// in a job, as it is under a terminal or a CI runner that puts its children in one. The
/// regressions it rejects: a stage refused, or started without its limits, because the platform
/// would not nest the job it is created in, and a nested job that keeps only some limits.
///
/// Two halves. This process is asked whether it is in a job, which is the fact about the machine
/// that runs the suite and is printed, and its stages are checked as they are. A copy of this
/// binary then puts itself in a job of its own, so the nested case is made whatever the machine
/// does, and checks the same things from inside it. The copy is a process of its own because
/// joining a job cannot be undone and the other tests here run beside this one.
#[test]
fn a_stage_is_limited_whether_or_not_this_process_is_in_a_job() {
    if std::env::var_os(IN_A_JOB).is_some() {
        put_this_process_in_a_job();
        assert!(
            this_process_is_in_a_job(),
            "this process is in no job after joining one"
        );
        assert_stages_are_limited("nested");
        println!("{NESTED}");
        return;
    }

    eprintln!("this process is in a job: {}", this_process_is_in_a_job());
    assert_stages_are_limited("direct");

    let nested = spawned(
        Command::new(std::env::current_exe().expect("this test binary"))
            .args([
                "--exact",
                "a_stage_is_limited_whether_or_not_this_process_is_in_a_job",
                "--nocapture",
                "--quiet",
                "--test-threads=1",
            ])
            .env(IN_A_JOB, "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )
    .wait_with_output()
    .expect("the copy ends");
    let printed = String::from_utf8_lossy(&nested.stdout);
    let complained = String::from_utf8_lossy(&nested.stderr);
    assert!(
        nested.status.success() && printed.contains(NESTED),
        "stages inside a job were not held to their limits: {printed} [stderr: {complained}]"
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

/// A program that is not an absolute path is refused before a container is configured, so the
/// directory is not made by whichever `cmd.exe` a search of the caller's `PATH` finds. The
/// regression it rejects is the backend starting a bare name, which the platform resolves outside
/// anything the policy named.
#[test]
fn a_program_that_is_not_an_absolute_path_is_refused_before_a_container_exists() {
    let sandbox = sandbox();
    let directory = scratch("bare-program");
    let marker = directory.join("marker");
    let policy = the_base("bare-program").allow_write(&directory);
    let made = format!("mkdir {}", marker.display());

    for program in [
        "cmd.exe",
        "cmd",
        r".\cmd.exe",
        r"System32\cmd.exe",
        r"\cmd.exe",
        "C:cmd.exe",
    ] {
        let refused = run(&sandbox, program, &["/c", &made], &policy);
        assert!(
            matches!(refused, Err(SandboxError::SetupFailed { .. })),
            "{program:?} started: {refused:?}"
        );
    }
    assert!(
        !marker.exists(),
        "a program ran from a name left to a search"
    );
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

    let interpreter = cmd_exe();
    let refused = run(
        &sandbox,
        &interpreter,
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

/// SANDBOX-4 with a real process: `cmd.exe` spelled with a trailing dot, a trailing space or a
/// stream suffix is refused before anything starts, so the directory is not made, whether or not
/// `CreateProcessW` would have started the interpreter for that name. The regression it rejects is
/// the refusals reading the name as written, which finds no `cmd.exe` in these spellings and starts
/// them with arguments `cmd.exe` reads as its own.
#[test]
fn cmd_spelled_with_a_trailing_dot_a_space_or_a_stream_starts_nothing() {
    let sandbox = sandbox();
    let directory = scratch("cmd-spellings");
    let policy = the_base("cmd-spellings").allow_write(&directory);
    let named = directory.join("R&D");
    let named = named.display().to_string();
    let system = r"C:\Windows\System32";

    for program in [
        format!(r"{system}\cmd.exe."),
        format!(r"{system}\cmd.exe "),
        format!(r"{system}\cmd.exe::$DATA"),
        format!(r"{system}\cmd.exe:stream"),
    ] {
        let refused = run(&sandbox, &program, &["/c", "mkdir", &named], &policy);

        assert!(
            matches!(refused, Err(SandboxError::SetupFailed { .. })),
            "{program} started: {refused:?}"
        );
    }
    let made: Vec<_> = std::fs::read_dir(&directory)
        .expect("the scratch directory")
        .collect();
    assert!(made.is_empty(), "a spelling of cmd.exe made {made:?}");
}

/// SANDBOX-4 with a real process: a batch file spelled with a trailing dot is refused before
/// anything starts, so the file it would write is not there. The regression it rejects is the
/// extension test reading `build.bat.` as having none.
#[test]
fn a_batch_file_spelled_with_a_trailing_dot_starts_nothing() {
    let sandbox = sandbox();
    let directory = scratch("batch-dot");
    let policy = the_base("batch-dot").allow_write(&directory);
    let batch = directory.join("build.bat");
    let marker = directory.join("ran");
    std::fs::write(&batch, format!("@echo ran> {}\r\n", marker.display()))
        .expect("the batch file is written");
    let spelled = format!("{}.", batch.display());

    let refused = run(&sandbox, &spelled, &[], &policy);

    assert!(
        matches!(refused, Err(SandboxError::SetupFailed { .. })),
        "{spelled} started: {refused:?}"
    );
    assert!(!marker.exists(), "the batch file ran");
}

/// SANDBOX-4 with a real process, pinning the cost the spec records: a copy of `cmd.exe` called
/// `tool.exe` is recognised by no test of its file, so it is not refused as `cmd.exe` is and the
/// backend starts it. The regression it rejects is a change to `is_cmd` that refuses or accepts
/// differently without the spec's account changing with it. Which of the two the copy then does
/// with `R&D` is not asserted, since it has not been observed on Windows.
#[test]
fn a_copy_of_cmd_under_another_name_is_started_as_a_program() {
    let sandbox = sandbox();
    let directory = scratch("cmd-copy");
    let policy = the_base("cmd-copy")
        .allow_read(&directory)
        .allow_write(&directory);
    let copy = directory.join("tool.exe");
    std::fs::copy(r"C:\Windows\System32\cmd.exe", &copy).expect("a copy of cmd.exe");
    let named = directory.join("R&D");

    let started = run(
        &sandbox,
        &copy.display().to_string(),
        &["/c", "mkdir", &named.display().to_string()],
        &policy,
    );

    assert!(
        started.is_ok(),
        "the copy was refused, so SANDBOX-4's account of what it costs is out of date: {started:?}"
    );
}

/// The long file name of the copy of `whoami.exe` the spelling tests start by its 8.3 short name.
const LONG_PROGRAM: &str = "a_long_program_name.exe";

/// Start `program` with no arguments, outside any container, and say how it ended and what it
/// printed, or that it did not start.
fn started_as(program: &Path) -> std::io::Result<(Option<i32>, String)> {
    let child = {
        let _spawning = spawning();
        bravebot_sandbox::process::starting_a_program(|| {
            Command::new(program)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        })?
    };
    let ended = child.wait_with_output()?;
    Ok((
        ended.status.code(),
        String::from_utf8_lossy(&ended.stdout).trim().to_owned(),
    ))
}

/// `base` with `suffix` added to the end of its name, without any normalisation of the result.
fn spelled(base: &Path, suffix: &str) -> PathBuf {
    let mut name = base.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// A directory holding a copy of `whoami.exe` as `tool.exe` and as `LONG_PROGRAM`, and a `t.cmd`
/// and a `t.bat` that end with status 0. Returns the directory and what `whoami.exe` prints, which
/// a copy prints too.
fn a_directory_of_programs(name: &str) -> (PathBuf, String) {
    let directory = scratch(name);
    let root = std::env::var_os("SystemRoot").expect("the system directory is named");
    let whoami = Path::new(&root).join("System32").join("whoami.exe");
    std::fs::copy(&whoami, directory.join("tool.exe")).expect("a copy of whoami.exe");
    std::fs::copy(&whoami, directory.join(LONG_PROGRAM)).expect("a copy of whoami.exe");
    std::fs::write(directory.join("t.cmd"), "@exit /b 0\r\n").expect("a .cmd file");
    std::fs::write(directory.join("t.bat"), "@exit /b 0\r\n").expect("a .bat file");
    let (code, printed) = started_as(&whoami).expect("whoami.exe starts under its own name");
    assert_eq!(code, Some(0), "whoami.exe failed: {printed}");
    assert!(!printed.is_empty(), "whoami.exe printed nothing");
    (directory, printed)
}

/// The 8.3 short name of `path`, as `cmd.exe` prints it with `%~s`, or `None` where the volume made
/// none and `cmd.exe` printed the long name back.
fn short_name_of(path: &Path) -> Option<PathBuf> {
    use std::os::windows::process::CommandExt;

    let line = format!("for %I in (\"{}\") do @echo %~sI", path.display());
    let ended = spawned(
        Command::new("cmd.exe")
            .arg("/c")
            .raw_arg(line)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )
    .wait_with_output()
    .expect("cmd.exe ends");
    assert!(
        ended.status.success(),
        "the short name was not read: {}",
        String::from_utf8_lossy(&ended.stderr)
    );
    let printed = String::from_utf8_lossy(&ended.stdout).trim().to_owned();
    assert!(!printed.is_empty(), "cmd.exe printed no short name");
    let short = PathBuf::from(printed);
    (!short.as_os_str().eq_ignore_ascii_case(path.as_os_str())).then_some(short)
}

/// #3926, PERM-5 and PERM-20: the spellings a `deny` or `ask` command rule reads as the program
/// they name do start that program. This checks the platform assumption the fold relies on and
/// never calls the fold, so a regression in the fold cannot fail it; the tests in
/// `bravebot-core` pin the fold. The fault it rejects is a spelling the fold treats as the program
/// that Windows does not start as that program, which would refuse or misattribute a line. Each
/// spelling is started by this process through the standard library, outside any container, and a
/// failure names the spelling. The failure of this test was not demonstrated, since it runs only
/// on Windows.
#[test]
fn the_spellings_a_command_rule_folds_start_the_program_they_name() {
    let (directory, expected) = a_directory_of_programs("spellings-fold");
    let tool = directory.join("tool.exe");
    let mut spellings: Vec<(String, PathBuf, bool)> = vec![
        (
            "tool.exe. (a trailing dot)".to_owned(),
            spelled(&tool, "."),
            true,
        ),
        (
            "TOOL.EXE (another case)".to_owned(),
            directory.join("TOOL.EXE"),
            true,
        ),
        ("t.cmd".to_owned(), directory.join("t.cmd"), false),
        ("t.bat".to_owned(), directory.join("t.bat"), false),
    ];
    match short_name_of(&directory.join(LONG_PROGRAM)) {
        Some(short) => spellings.push((
            format!("{} (an 8.3 short name)", short.display()),
            short,
            true,
        )),
        None => eprintln!(
            "no 8.3 short name for {LONG_PROGRAM} on this volume; that spelling was not tried"
        ),
    }

    let mut failures = Vec::new();
    for (spelling, program, prints) in &spellings {
        match started_as(program) {
            Ok((Some(0), printed)) if !*prints || printed == expected => {
                eprintln!("{spelling}: started the program and it ended with 0");
            }
            other => failures.push(format!("{spelling}: {other:?} (expected {expected:?})")),
        }
    }
    let _ = std::fs::remove_dir_all(&directory);
    assert!(
        failures.is_empty(),
        "a spelling the fold reads as the program did not start it:\n{}",
        failures.join("\n")
    );
}

/// #3926: for the spellings whose result is not asserted, a trailing space and a `::$DATA` stream
/// suffix, only that Windows never starts a different program under them. The fold reads both as
/// the program they name, so the claim a rule needs is that a spelling either starts the program
/// named or starts nothing. Which of the two it is gets printed for each; an outcome that is
/// neither fails and names the spelling. Like the test above, this checks the platform assumption
/// and never calls the fold.
#[test]
fn a_trailing_space_or_a_stream_suffix_starts_the_program_named_or_nothing() {
    let (directory, expected) = a_directory_of_programs("spellings-observed");
    let tool = directory.join("tool.exe");
    let mut failures = Vec::new();
    for (spelling, program) in [
        ("tool.exe  (a trailing space)", spelled(&tool, " ")),
        (
            "tool.exe::$DATA (a stream suffix)",
            spelled(&tool, "::$DATA"),
        ),
    ] {
        match started_as(&program) {
            Ok((Some(0), printed)) if printed == expected => {
                eprintln!("{spelling}: started the program named");
            }
            Err(error) => eprintln!("{spelling}: did not start anything ({error})"),
            other => failures.push(format!("{spelling}: {other:?} (expected {expected:?})")),
        }
    }
    let _ = std::fs::remove_dir_all(&directory);
    assert!(
        failures.is_empty(),
        "a spelling started something other than the program named:\n{}",
        failures.join("\n")
    );
}

/// SANDBOX-3 on Windows: a container is refused the loopback interface of the machine it runs on
/// even when its policy grants the network. The regression it rejects: a container that reaches a
/// service listening on this host's loopback, such as a local credential or admin endpoint, because
/// the network capability was read as covering the loopback interface.
///
/// This process listens on an ephemeral loopback port and a probe in a stage granted the network
/// connects to it, printing `connect-probe connected` or `connect-probe refused: <error>`. The
/// listener does not accept, since a connection completes against its backlog. The test
/// asserts the connect itself failed, with an OS error or the timeout, and that the probe did not
/// stop before it tried. The reachability of the same listener from this
/// process is asserted first, so a listener nothing can reach is not mistaken for a refusal.
#[test]
fn a_container_granted_the_network_cannot_connect_to_this_hosts_loopback() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback listener");
    let address = listener.local_addr().expect("the listener's address");
    std::net::TcpStream::connect_timeout(&address, Duration::from_secs(10))
        .expect("this process reaches its own listener");
    let policy = the_base("loopback");
    assert!(policy.allow_network);

    let sandbox = sandbox();
    let printed = probe_in_a_stage(
        &sandbox,
        "loopback",
        policy,
        "probe_connect_from_this_stage",
        &[&address.to_string()],
    );
    let found = what_the_probe_printed(&printed, CONNECT_PROBED);
    eprintln!("loopback {address} from a container granted the network: {found}");

    let connect_failed = found
        .strip_prefix("refused: ")
        .is_some_and(|error| error == "connection timed out" || error.contains("(os error "));
    assert!(
        connect_failed,
        "a container granted the network was expected to be refused {address} on this host's loopback by the connect itself: {found}"
    );
}
