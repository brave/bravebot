//! A confined stage must not recover its parent's exec-time environment.
#![cfg(target_os = "macos")]
#![forbid(unsafe_code)]

use bravebot_sandbox::{
    Environment, Sandbox, Stream, Streams, macos::SeatbeltSandbox, policy::SandboxPolicy,
};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const SECRET: &str = "BRAVEBOT_TEST_SECRET";
const MARKER: &str = "synthetic-exec-environment-marker";
const FIXTURE: &str = "BRAVEBOT_EXEC_ENVIRONMENT_FIXTURE";
const TEST: &str = "a_confined_child_cannot_read_its_parents_exec_time_environment";

// The only queried PID is the disposable test parent, re-exec'd with an environment containing
// only this fixture's variables. No environment bytes are printed or written to disk.
const SOURCE: &str = r#"
#include <sys/types.h>
#include <sys/sysctl.h>
#include <unistd.h>
#include <errno.h>
#include <stdlib.h>
#include <string.h>
int main(void) {
    if (getenv("BRAVEBOT_TEST_SECRET") != NULL) return 90;
    int mib[] = { CTL_KERN, KERN_PROCARGS2, getppid() };
    char data[262144];
    size_t len = sizeof(data);
    if (sysctl(mib, 3, data, &len, NULL, 0) != 0)
        return errno == EPERM || errno == EACCES ? 44 : 91;
    const char marker[] = "BRAVEBOT_TEST_SECRET=synthetic-exec-environment-marker";
    for (size_t i = 0; i + sizeof(marker) <= len; i++)
        if (memcmp(data + i, marker, sizeof(marker)) == 0) return 42;
    return 43;
}
"#;

fn probe_from_fixture_parent(program: &Path) {
    assert_eq!(std::env::var(SECRET).as_deref(), Ok(MARKER));
    assert_eq!(
        Command::new(program).env_clear().status().unwrap().code(),
        Some(42),
        "the unconfined control must recover the synthetic exec-time marker"
    );

    // Grant all reads so that filesystem denial cannot account for a passing test. Network
    // and subprocess grants must not lift the process-inspection restriction either.
    let policy = SandboxPolicy::strict()
        .allow_read("/")
        .allow_network_egress()
        .allow_subprocesses();
    let sandbox = SeatbeltSandbox::new().unwrap();
    let mut child = sandbox
        .spawn(
            program.as_os_str(),
            &[],
            &policy,
            Streams {
                stdin: Stream::Null,
                stdout: Stream::Null,
                stderr: Stream::Null,
            },
            Environment::Empty,
        )
        .unwrap();
    assert_eq!(
        child.wait().unwrap().code(),
        Some(44),
        "spawn must refuse KERN_PROCARGS2; 42 means exposed, 90 means inherited"
    );
    let status = sandbox
        .command(program.as_os_str(), &[], &policy, &Environment::Empty)
        .unwrap()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert_eq!(
        status.code(),
        Some(44),
        "command must refuse KERN_PROCARGS2; 42 means exposed, 90 means inherited"
    );
}

/// Emptying the child environment alone does not protect the parent's saved exec-time bytes.
/// Both production launch paths must deny the query, while an unconfined empty-environment
/// child must recover the marker. Only a re-exec'd synthetic parent is ever inspected.
#[test]
fn a_confined_child_cannot_read_its_parents_exec_time_environment() {
    if let Some(program) = std::env::var_os(FIXTURE) {
        probe_from_fixture_parent(Path::new(&program));
        return;
    }
    if !bravebot_sandbox::confinement_works_here() {
        eprintln!("skipped {TEST}: Seatbelt cannot be applied (including nested confinement)");
        return;
    }
    let dir =
        std::env::temp_dir().join(format!("bravebot-exec-environment-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let _scratch = Scratch(dir.clone());
    let source = dir.join("fixture.c");
    let program = dir.join("fixture");
    std::fs::write(&source, SOURCE).unwrap();
    assert!(
        Command::new("cc")
            .args(["-Wall", "-Wextra", "-Werror", "-o"])
            .arg(&program)
            .arg(source)
            .status()
            .unwrap()
            .success(),
        "the synthetic probe must compile"
    );
    assert!(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture"])
            .env_clear()
            .env(SECRET, MARKER)
            .env(FIXTURE, &program)
            .status()
            .unwrap()
            .success(),
        "the synthetic parent must observe both confined refusals and the unconfined exposure"
    );
}
