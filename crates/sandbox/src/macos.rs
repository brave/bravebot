//! macOS confinement via Seatbelt.
//!
//! Wraps the target command in `sandbox-exec` with a generated deny-by-default
//! profile. The mechanism is what Chrome and Firefox use for their renderer
//! processes; the `sandbox_init` C API is marked deprecated but has no supported
//! replacement for binaries distributed outside the App Store, and `sandbox-exec`
//! remains present on supported macOS versions.
//!
//! Verified empirically: with `(deny default)` a process cannot reach the network
//! (curl fails to resolve or connect) and cannot create files, while still being able
//! to exec and read permitted paths.

use crate::policy::{Capabilities, ConfinementLevel, SandboxPolicy};
use crate::{Sandbox, SandboxError};
use std::path::Path;
use std::process::Command;

const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

/// Seatbelt-based confinement.
#[derive(Debug, Default)]
pub struct SeatbeltSandbox;

impl SeatbeltSandbox {
    /// Fails if `sandbox-exec` is missing rather than degrading to no confinement.
    pub fn new() -> Result<Self, SandboxError> {
        if !Path::new(SANDBOX_EXEC).exists() {
            return Err(SandboxError::Unavailable {
                platform: "macos",
                detail: format!("{SANDBOX_EXEC} is not present"),
            });
        }
        Ok(Self)
    }

    /// Build the Seatbelt profile for a policy.
    ///
    /// Starts from `(deny default)` and adds only what the policy grants. Paths are
    /// written as subpath rules so a granted directory covers its contents.
    pub fn profile(policy: &SandboxPolicy) -> String {
        let mut out = String::from("(version 1)\n(deny default)\n");

        // Without these the process cannot start at all: the loader must exec the
        // binary and mach lookups are needed for basic runtime services. They grant no
        // filesystem or network reach of their own.
        out.push_str("(allow process-exec)\n");
        out.push_str("(allow sysctl-read)\n");
        out.push_str("(allow mach-lookup)\n");

        // The dynamic loader reads the root directory entry itself, which a subpath
        // grant for e.g. /usr does not cover. Without this the process dies with
        // SIGABRT before main runs, which looks like a mysterious crash rather than a
        // denied read. Reading `/` alone exposes no file contents.
        out.push_str("(allow file-read* (literal \"/\"))\n");

        for path in &policy.readable {
            out.push_str(&format!(
                "(allow file-read* (subpath {}))\n",
                quote(&path.to_string_lossy())
            ));
        }

        for path in &policy.writable {
            out.push_str(&format!(
                "(allow file-write* (subpath {}))\n",
                quote(&path.to_string_lossy())
            ));
        }

        if policy.allow_network {
            out.push_str("(allow network-outbound)\n");
        }

        if policy.allow_subprocesses {
            out.push_str("(allow process-fork)\n");
        }

        out
    }
}

/// Quote a path as a Seatbelt string literal.
///
/// Escapes backslashes and quotes so a path containing either cannot terminate the
/// literal early and inject profile syntax.
fn quote(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

impl Sandbox for SeatbeltSandbox {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            // Seatbelt is enforced by the kernel, though its policy language is
            // coarser than Landlock plus seccomp on Linux.
            level: ConfinementLevel::Kernel,
            mechanisms: vec!["seatbelt"],
            network_denial_enforced: true,
            // A profile is text the kernel reads as the process starts, so a path that is
            // not there yet is named in one and the file can be created afterwards.
            grants_paths_that_do_not_exist: true,
        }
    }

    fn command(
        &self,
        program: &str,
        args: &[String],
        policy: &SandboxPolicy,
    ) -> Result<Command, SandboxError> {
        if !policy.is_meaningful() {
            return Err(SandboxError::PolicyTooPermissive);
        }

        let mut wrapped = Command::new(SANDBOX_EXEC);
        wrapped.arg("-p").arg(Self::profile(policy));
        wrapped.arg(program);
        wrapped.args(args);

        Ok(wrapped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::os::unix::fs::MetadataExt;
    use std::process::Stdio;

    /// `CURLE_COULDNT_CONNECT`: curl reached the connection and was refused it. Any other code
    /// means it stopped before that, which is some other denial reported as this one.
    const CURL_COULDNT_CONNECT: i32 = 7;

    /// touch reporting that the operation it was asked for failed.
    const TOUCH_FAILED: i32 = 1;

    /// Answer one request, so a curl that was permitted a socket gets a reply and exits rather
    /// than waiting out its own timeout. Called only where a connection is expected to arrive.
    fn answer_one(listener: &TcpListener) {
        let (mut stream, _) = listener.accept().expect("the connection arrives");
        let _ = stream.read(&mut [0u8; 1024]);
        let _ = stream.write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n");
    }

    #[test]
    fn a_strict_profile_denies_by_default() {
        let profile = SeatbeltSandbox::profile(&SandboxPolicy::strict());
        assert!(profile.contains("(deny default)"));
        assert!(!profile.contains("network-outbound"));
        assert!(!profile.contains("file-write*"));
        assert!(!profile.contains("process-fork"));
    }

    #[test]
    fn granted_paths_appear_as_subpath_rules() {
        let policy = SandboxPolicy::strict()
            .allow_read("/workspace")
            .allow_write("/workspace/target");
        let profile = SeatbeltSandbox::profile(&policy);
        assert!(profile.contains(r#"(allow file-read* (subpath "/workspace"))"#));
        assert!(profile.contains(r#"(allow file-write* (subpath "/workspace/target"))"#));
    }

    /// A grant here is a name in a profile rather than a right on an open descriptor, so a
    /// path that does not exist yet is one this backend can grant, and what a policy names
    /// is what the profile carries. That is the half of the shared rule this backend
    /// answers: the other cannot name such a path at all and refuses the policy rather
    /// than granting less than it asked for, so a caller that meets a refusal there knows
    /// it is the platform and not the policy.
    ///
    /// The command is built as well as the profile, so validation added here later has to
    /// be decided rather than inherited from the other backend.
    #[test]
    fn a_path_that_is_not_there_yet_is_granted_as_named() {
        let absent = "/bravebot-no-such-path/known_hosts";
        assert!(
            !Path::new(absent).exists(),
            "the path has to be absent for this to say anything"
        );

        let policy = SandboxPolicy::strict()
            .allow_read("/usr")
            .allow_write(absent);
        let profile = SeatbeltSandbox::profile(&policy);
        assert!(
            profile.contains(&format!(r#"(allow file-write* (subpath "{absent}"))"#)),
            "the grant the policy named is not in the profile: {profile}"
        );
        SeatbeltSandbox
            .command("/usr/bin/true", &[], &policy)
            .expect("a path that is not there yet is a grant, not a refusal");
    }

    /// A caller reads this to decide whether an absent path in a policy needs creating
    /// first or the directory holding it named instead, and a report disagreeing with the
    /// backend costs it one of those on a platform where neither was necessary. Both
    /// directions of disagreement cost that, which is why the assertion ties the report to
    /// what the backend does rather than pinning a value.
    ///
    /// What the kernel installs rather than what the profile text holds: a rule the profile
    /// compiler refuses is a grant this backend cannot make however the policy named it,
    /// and a capability claiming otherwise is the overstatement SANDBOX-5 exists to stop.
    #[test]
    fn a_path_that_does_not_exist_is_granted_exactly_where_the_capability_says_so() {
        let sandbox = SeatbeltSandbox::new().expect("sandbox-exec is present on macOS");

        let dir = crate::testutil::scratch_dir("bravebot-sandbox-absent-path-capability");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the scratch directory is creatable");
        let absent = dir.join("not-created-yet");
        assert!(
            !absent.exists(),
            "the path has to be absent for this to say anything"
        );

        let policy = SandboxPolicy::strict()
            .allow_read("/usr")
            .allow_read("/bin")
            .allow_write(&absent);
        let granted = sandbox
            .command("/usr/bin/true", &[], &policy)
            .ok()
            .and_then(|mut command| {
                command
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .ok()
            })
            .is_some_and(|status| status.success());

        assert_eq!(
            granted,
            sandbox.capabilities().grants_paths_that_do_not_exist,
            "what this backend reports about a path that does not exist is not what it does"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn network_is_only_allowed_when_requested() {
        let denied = SeatbeltSandbox::profile(&SandboxPolicy::strict());
        assert!(!denied.contains("network-outbound"));

        let allowed = SeatbeltSandbox::profile(&SandboxPolicy::strict().allow_network_egress());
        assert!(allowed.contains("(allow network-outbound)"));
    }

    /// A path containing a quote must not close the string literal and let its
    /// remainder be parsed as profile directives. The payload text still appears, but it
    /// is part of the path, but every quote in it is escaped, so Seatbelt reads the
    /// whole thing as one string.
    #[test]
    fn paths_cannot_inject_profile_syntax() {
        let policy = SandboxPolicy::strict().allow_read(r#"/tmp/x") (allow network-outbound) ("#);
        let profile = SeatbeltSandbox::profile(&policy);

        assert!(
            profile.contains(r#"\""#),
            "the embedded quote was not escaped: {profile}"
        );
        // Unescaped, this would have been a directive of its own.
        assert!(
            !profile.contains("\n(allow network-outbound)"),
            "injected directive escaped the literal: {profile}"
        );
    }

    /// Seatbelt must accept a profile built from a hostile path rather than failing to
    /// parse, since a parse failure would be reported as confinement being unavailable.
    #[test]
    fn a_profile_containing_a_hostile_path_still_applies() {
        let sandbox = SeatbeltSandbox::new().expect("sandbox-exec is present on macOS");
        let policy = SandboxPolicy::strict()
            .allow_read("/usr")
            .allow_read("/bin")
            .allow_read(r#"/tmp/x") (allow network-outbound) ("#);

        let mut child = sandbox
            .command("/usr/bin/true", &[], &policy)
            .expect("command builds")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("should spawn");
        assert!(child.wait().expect("should wait").success());
    }

    #[test]
    fn a_fully_permissive_policy_is_refused() {
        let sandbox = SeatbeltSandbox;
        let policy = SandboxPolicy::strict()
            .allow_network_egress()
            .allow_subprocesses()
            .allow_write("/");
        let err = sandbox
            .spawn("/usr/bin/true", &[], &policy)
            .expect_err("must refuse a policy that confines nothing");
        assert!(matches!(err, SandboxError::PolicyTooPermissive));
    }

    #[test]
    fn capabilities_report_kernel_enforcement() {
        let caps = SeatbeltSandbox.capabilities();
        assert_eq!(caps.level, ConfinementLevel::Kernel);
        assert!(caps.network_denial_enforced);
        assert!(caps.mechanisms.contains(&"seatbelt"));
    }

    /// Confirms the sandbox actually runs a process, not just that a profile string
    /// was built.
    #[test]
    fn a_confined_process_runs() {
        let sandbox = SeatbeltSandbox::new().expect("sandbox-exec is present on macOS");
        let policy = SandboxPolicy::strict()
            .allow_read("/usr")
            .allow_read("/bin");
        let mut child = sandbox
            .command("/usr/bin/true", &[], &policy)
            .expect("command builds")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("should spawn");
        assert!(child.wait().expect("should wait").success());
    }

    /// The point of the sandbox: a confined process cannot write outside its grants.
    #[test]
    fn a_confined_process_cannot_write_outside_its_grants() {
        let sandbox = SeatbeltSandbox::new().expect("sandbox-exec is present on macOS");
        let policy = SandboxPolicy::strict()
            .allow_read("/usr")
            .allow_read("/bin");

        // The directory has to be there before touch runs, and empty. Into a parent that does
        // not exist, touch fails with ENOENT and creates nothing whatever the profile permits,
        // which holds just as well against a sandbox granting every write; and a file left by a
        // run that failed would fail every run after it.
        let dir = crate::testutil::scratch_dir("bravebot-sandbox-denied-write");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the scratch directory is creatable");
        let target = dir.join("must-not-exist");

        let mut child = sandbox
            .command("/usr/bin/touch", &[target.display().to_string()], &policy)
            .expect("command builds")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("should spawn");
        let status = child.wait().expect("should wait");

        // touch's own refusal, rather than any failure at all: sandbox-exec declining to exec it
        // exits 71 and a process dying before main exits by signal, and neither of those says
        // anything about a write.
        assert_eq!(
            status.code(),
            Some(TOUCH_FAILED),
            "the write was not what failed"
        );
        assert!(!target.exists(), "file was created despite confinement");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Network denial is the property that makes exfiltration structurally impossible,
    /// so it is asserted against a real process rather than only in the profile text.
    ///
    /// Both halves run against a socket this test is listening on. The permitted half is what
    /// makes the denied half mean anything: curl exits 7 against a port nothing is listening on
    /// just as readily as against a socket it was refused, so without establishing that a
    /// connection succeeds here, the denied half would pass against a sandbox that enforces
    /// nothing.
    #[test]
    fn a_confined_process_cannot_reach_the_network() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port to connect to");
        let port = listener.local_addr().expect("the bound address").port();

        let sandbox = SeatbeltSandbox::new().expect("sandbox-exec is present on macOS");
        let denied = SandboxPolicy::strict()
            .allow_read("/usr")
            .allow_read("/bin")
            // Resolved rather than /etc, which is a symlink to it. Seatbelt matches the resolved
            // path, so a grant for /etc reaches nothing, and curl exits over its unreadable
            // LibreSSL configuration before it opens a socket at all.
            .allow_read("/private/etc")
            .allow_read("/System")
            .allow_read("/Library");
        let permitted = denied.clone().allow_network_egress();

        // An address, so no resolver is involved, and stdout is discarded, so curl needs no file
        // to write the body to. The proxy is refused in the argument vector because the
        // environment here is this process's own: a machine whose shell exports `http_proxy`
        // would otherwise have both halves of this test measure a connection to somewhere else.
        let curl = |policy: &SandboxPolicy| {
            let args: Vec<String> = [
                "-s",
                "--noproxy",
                "*",
                "-m",
                "5",
                &format!("http://127.0.0.1:{port}/"),
            ]
            .iter()
            .map(|s| s.to_string())
            .collect();
            sandbox
                .command("/usr/bin/curl", &args, policy)
                .expect("command builds")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("should spawn")
        };

        let mut reaching = curl(&permitted);
        answer_one(&listener);
        assert_eq!(
            reaching.wait().expect("should wait").code(),
            Some(0),
            "a permitted process could not reach the listener, so nothing below means anything"
        );

        let mut refused = curl(&denied);
        assert_eq!(
            refused.wait().expect("should wait").code(),
            Some(CURL_COULDNT_CONNECT),
            "the connection was not what failed, so this says nothing about network denial"
        );
    }

    /// Writing a temporary file and renaming it into place is how a compiler, a package
    /// manager and an editor write anything at all, so a confinement that denies the move
    /// holds an ordinary build to less than the paths its policy granted. A write grant
    /// here is a subpath rule covering every operation on what is under it, so this
    /// backend needs nothing beyond the grant to permit the move; the other one needs the
    /// kernel right that governs it, and the two are held to one rule.
    ///
    /// The inode is what the assertion is on, because `mv` answers a refused rename by
    /// copying the file and unlinking the original: the destination exists either way, and
    /// only a preserved inode says the move happened rather than a copy that is neither
    /// atomic nor cheap.
    #[test]
    fn a_confined_process_can_rename_a_file_between_two_granted_directories() {
        let sandbox = SeatbeltSandbox::new().expect("sandbox-exec is present on macOS");

        let dir = crate::testutil::scratch_dir("bravebot-sandbox-rename");
        let _ = std::fs::remove_dir_all(&dir);
        let source = dir.join("from").join("moved");
        let destination = dir.join("to").join("moved");
        std::fs::create_dir_all(dir.join("from")).expect("the scratch directory is creatable");
        std::fs::create_dir_all(dir.join("to")).expect("the scratch directory is creatable");
        std::fs::write(&source, b"contents").expect("the file is writable");
        let inode = std::fs::metadata(&source).expect("the file is there").ino();

        // Read as well as write: mv stats both ends before it moves anything, so a profile
        // granting only the write fails over the stat and says nothing about the move.
        let policy = SandboxPolicy::strict()
            .allow_read("/usr")
            .allow_read("/bin")
            .allow_read(&dir)
            .allow_write(&dir);
        let mut child = sandbox
            .command(
                "/bin/mv",
                &[
                    source.display().to_string(),
                    destination.display().to_string(),
                ],
                &policy,
            )
            .expect("command builds")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("should spawn");
        assert!(
            child.wait().expect("should wait").success(),
            "a move inside one granted directory failed"
        );
        assert_eq!(
            std::fs::metadata(&destination)
                .expect("the destination is there")
                .ino(),
            inode,
            "the file was copied and unlinked rather than moved, so the move was denied"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// What a program may be trusted with in the environment is the caller's decision and
    /// not a backend's: a credential lives in a variable rather than in a file, so no
    /// grant over paths either withholds one or hands one over, and the agent socket a
    /// push signs through is named by a variable as well. A backend that emptied it would
    /// take that decision away from the caller here and leave it with the caller on the
    /// other platform, which is one policy meaning two things.
    ///
    /// `CARGO_MANIFEST_DIR` is the variable read back because cargo sets it in the
    /// environment of a test process, so it is one this process holds and nothing else
    /// invents.
    #[test]
    fn the_environment_a_confined_process_receives_is_the_callers() {
        let sandbox = SeatbeltSandbox::new().expect("sandbox-exec is present on macOS");
        let held = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets this for a test");
        let policy = SandboxPolicy::strict()
            .allow_read("/usr")
            .allow_read("/bin");

        let printed = sandbox
            .command("/usr/bin/env", &[], &policy)
            .expect("command builds")
            .output()
            .expect("the confined process runs");

        let environment = String::from_utf8_lossy(&printed.stdout);
        assert!(
            environment
                .lines()
                .any(|line| line == format!("CARGO_MANIFEST_DIR={held}")),
            "a variable this process holds did not reach the confined process: {environment}"
        );
    }
}
