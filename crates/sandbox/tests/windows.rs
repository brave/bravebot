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

use bravebot_sandbox::base::{Prelude, base};
use bravebot_sandbox::policy::SandboxPolicy;
use bravebot_sandbox::windows::AppContainerSandbox;
use bravebot_sandbox::{
    ConfinedChild, ConfinedStdin, ConfinedStdout, Environment, Sandbox, SandboxError, Stream,
    Streams,
};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

fn sandbox() -> AppContainerSandbox {
    AppContainerSandbox::new().expect("a container profile can be created on this machine")
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
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let streams = Streams {
        stdin: Stream::Null,
        stdout: Stream::Piped,
        stderr: Stream::Piped,
    };
    let mut child = sandbox.spawn(
        program.as_ref(),
        &args,
        policy,
        streams,
        Environment::Inherited,
    )?;
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
fn watch(mut stdout: ConfinedStdout) -> mpsc::Receiver<Seen> {
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

/// A stage whose program, `cmd.exe /c cmd`, has started a second `cmd.exe` that reads commands from
/// the input this process holds, has run the line it was handed, and is waiting for the next one.
///
/// The second is the only process that prints, so `started` arriving is that child running, and it
/// holds the write end of the stage's output, so the pipe reaching its end is that child gone.
fn a_stage_with_a_child_waiting_on_its_input(
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
    let mut input = child.take_stdin().expect("a stdin that was piped");
    let seen = watch(child.take_stdout().expect("a stdout that was piped"));

    input
        .write_all(b"echo started\r\n")
        .expect("the child is handed a line");
    assert_eq!(
        seen.recv_timeout(STARTING),
        Ok(Seen::Started),
        "the stage's child never ran the line it was handed"
    );
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
    let made = std::process::Command::new("cmd.exe")
        .arg("/c")
        .arg("mklink")
        .arg("/J")
        .arg(&link)
        .arg(&withheld)
        .output()
        .expect("cmd.exe starts");
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

/// The processes attached to this process's console, which the console itself keeps, or none
/// where this process has no console.
fn on_this_console() -> Vec<u32> {
    use windows_sys::Win32::System::Console::GetConsoleProcessList;

    let mut attached = vec![0u32; 64];
    loop {
        // Sound because the buffer holds as many identifiers as the count says, and the call
        // writes none when it needs more.
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

/// The regression it rejects: a stage attached to the console this process runs in. Any process
/// attached to a console can write input into it, so a program on the terminal's console can
/// type an answer to the prompt a person is about to read.
///
/// A process with no console has nothing to share, and a stage started from it is on a console
/// of its own whatever the backend asks for, so this one is given a console first.
#[test]
fn a_stage_is_not_attached_to_the_console_this_process_runs_in() {
    if on_this_console().is_empty() {
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        unsafe { windows_sys::Win32::System::Console::AllocConsole() };
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
