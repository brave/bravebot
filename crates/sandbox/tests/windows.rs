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
use bravebot_sandbox::{Environment, Sandbox, SandboxError, Stream, Streams};
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};

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
            &format!("echo wrote> {}", directory.join("made.txt").display()),
            &policy,
        )
        .expect("the program starts")
    };

    let (code, printed) = write(&granted);
    assert_eq!(code, Some(0), "a granted write failed: {printed}");
    assert_eq!(
        std::fs::read_to_string(granted.join("made.txt"))
            .expect("the file the program made")
            .trim(),
        "wrote"
    );

    let (code, _) = write(&withheld);
    assert_ne!(code, Some(0), "a write outside the grants succeeded");
    assert!(!withheld.join("made.txt").exists());
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

/// The regression it rejects: an empty base that leaves `git` unable to start. It does not start
/// today: `git` opens `/dev/null` as `NUL` before it reads its arguments, and a container is
/// denied that device (`cmd.exe /c "type nul"` ends with "Access is denied" in the same
/// container), so every stage that runs `git` ends with "could not open '/dev/null'".
#[test]
#[ignore = "exposes that a container is denied the NUL device, which git opens before it reads its arguments"]
fn git_starts_under_the_empty_base() {
    let sandbox = sandbox();
    let policy = the_base("git");

    let (code, printed) = run(&sandbox, "git", &["--version"], &policy).expect("git starts");

    assert_eq!(code, Some(0), "{printed}");
    assert!(printed.starts_with("git version"), "{printed}");
}

/// The regression it rejects: a refusal of a read or a write that this backend cannot apply being
/// dropped, so the program starts and reaches the path the person meant to hold back. The write
/// is the one observable from outside: the file must not be there afterwards.
#[test]
fn a_policy_that_refuses_a_read_or_a_write_starts_no_program() {
    let sandbox = sandbox();
    let directory = scratch("refusals");
    let marker = directory.join("marker.txt");
    let policy = the_base("refusals").allow_write(&directory);
    let line = format!("echo ran> {}", marker.display());

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
