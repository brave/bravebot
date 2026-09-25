//! Linux confinement via Landlock.
//!
//! Filesystem restrictions are applied with Landlock, which needs kernel 5.19 or
//! newer. Availability is probed at runtime rather than assumed: an older kernel, or a
//! container that masks the syscall, means confinement is unavailable and the process
//! is refused instead of run unconfined.
//!
//! Landlock governs the filesystem only. Network denial and subprocess denial need
//! separate mechanisms (e.g. an empty network namespace, or seccomp filtering),
//! which are not implemented yet, so [`Capabilities::network_denial_enforced`] is
//! `false` and the reported level is [`ConfinementLevel::Partial`]. Claiming
//! kernel-level network or subprocess denial here would misreport the guarantee;
//! policies requiring either are refused.

use crate::policy::{Capabilities, ConfinementLevel, SandboxPolicy};
use crate::process::{ConfinedChild, Environment, Streams};
use crate::{Sandbox, SandboxError};
use landlock::{
    ABI, Access, AccessFs, BitFlags, CompatLevel, Compatible, PathBeneath, PathFd, RulesetAttr,
    RulesetCreatedAttr, RulesetError, RulesetStatus, path_beneath_rules,
};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

/// The right set the ruleset handles, which is every right this crate knows of.
///
/// A right left out of a ruleset's handled set is not restricted at all: the kernel's hooks
/// for it check nothing for that domain, which is Landlock's compatibility contract. So the
/// handled set is what decides how much of the filesystem a confinement covers, and naming
/// the oldest ABI here leaves every right added after it outside the boundary altogether. A
/// confined process then empties any file the account owns, by path, anywhere outside its
/// grants, while the policy reads as applied.
///
/// This is not the kernel floor, which is [`MINIMUM_ABI_VERSION`]. `CompatLevel::BestEffort`
/// narrows the handled set to the rights the running kernel carries, so naming the newest
/// ABI here costs an older kernel nothing: it is confined under every right it has rather
/// than under only the rights the oldest ABI had. The rules the grants are built from widen
/// with it, or a right becomes handled and granted nowhere and an ordinary write inside a
/// granted directory starts failing.
///
/// Every right this widens by is one Landlock counts as a write, so a write grant carries
/// it and a read grant does not. That is the grant a caller has to name for a program to
/// empty a file it may rewrite, to drive a device rather than only read it, and, on a
/// kernel carrying the ninth version, to reach a socket by its path. The alternative is a
/// read grant that carries a write, which is a policy's two lists saying one thing, and
/// leaving the rights unhandled is the third: every ioctl on every device a grant can open,
/// and every such socket on the machine, reachable from inside the boundary.
///
/// The newest ABI this crate knows rather than one this backend has been tested against,
/// because the cost of the second is a right that restricts nothing at all. Landlock's own
/// advice is to request rights that have been tried on a kernel carrying them and on one
/// that does not, and what makes that possible here is that an upgrade of the `landlock`
/// crate teaching it a newer ABI fails a test naming this constant: widening it is then a
/// line somebody writes with the new rights in front of them.
const HANDLED_ABI: ABI = ABI::V9;

/// The Landlock ABI version this backend refuses a kernel below.
///
/// ABI v2 is the first carrying the right that governs moving a file between two
/// directories, and a ruleset that cannot restrict that right denies the operation wherever
/// it appears, so a kernel below it confines a program to less than its grants name. What
/// v2 costs is the kernels between 5.13 and 5.19, which have Landlock without that right.
const MINIMUM_ABI_VERSION: libc::c_long = 2;

/// `landlock_create_ruleset`, stable since Linux 5.13.
const SYS_LANDLOCK_CREATE_RULESET: libc::c_long = 444;

/// Ask the kernel which Landlock ABI it supports.
///
/// Passing a null attribute pointer with `LANDLOCK_CREATE_RULESET_VERSION` returns the
/// version without creating a ruleset. A negative result means Landlock is absent.
// The exemption sits on the function because the function is the syscall.
#[allow(unsafe_code)]
fn landlock_abi_version() -> libc::c_long {
    const LANDLOCK_CREATE_RULESET_VERSION: libc::c_ulong = 1;
    // Landlock has no libc wrapper, so the syscall is issued directly.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    unsafe {
        libc::syscall(
            SYS_LANDLOCK_CREATE_RULESET,
            std::ptr::null::<libc::c_void>(),
            0usize,
            LANDLOCK_CREATE_RULESET_VERSION,
        )
    }
}

/// Whether a kernel reporting `version` carries [`MINIMUM_ABI_VERSION`].
///
/// A version rather than the syscall, so every answer is decided by something a test can
/// call: the kernel a suite runs on reports one of them and no test can make it report
/// another.
///
/// A kernel below the floor is refused rather than confined under the rights it does
/// carry. `BestEffort` compatibility is what lets a grant for a regular file drop the
/// rights only a directory can hold, and it would drop the right that governs a move just
/// as quietly on a kernel that is merely old, which is silent degradation rather than a
/// platform difference a caller can work with.
///
/// The two refusals are separate because the answer to them differs: a kernel with no
/// Landlock at all is a machine to enable the LSM on, and an old one is a machine to
/// upgrade.
fn abi_meets_minimum(version: libc::c_long) -> Result<(), SandboxError> {
    if version < 1 {
        return Err(SandboxError::Unavailable {
            platform: "linux",
            detail: "the landlock syscall is not implemented on this kernel \
                     (needs 5.19+ with the LSM enabled)"
                .into(),
        });
    }
    if version < MINIMUM_ABI_VERSION {
        return Err(SandboxError::Unavailable {
            platform: "linux",
            detail: format!(
                "this kernel implements landlock abi {version}, which has no right governing \
                 the move of a file between two directories, so a ruleset built on it denies \
                 every such move inside the paths a policy grants; refusing rather than \
                 confining to less than a policy asks for (needs abi {MINIMUM_ABI_VERSION}, \
                 kernel 5.19+)"
            ),
        });
    }
    Ok(())
}

/// A rule for every path in `paths`, or an error saying how many produced none.
///
/// A Landlock rule is a right attached to an open descriptor, so a path nothing can open
/// cannot be named in one, and the rule builder skips such a path rather than reporting
/// it. A ruleset built straight from that builder therefore confines the process to fewer
/// paths than the policy listed while the policy reads as applied in full.
fn rules_for_every_path(
    paths: &[PathBuf],
    access: BitFlags<AccessFs>,
) -> std::io::Result<Vec<Result<PathBeneath<PathFd>, RulesetError>>> {
    let rules: Vec<_> = path_beneath_rules(paths, access).collect();
    if rules.len() != paths.len() {
        // Which paths, found by asking again, since the builder reports only the rules it
        // produced. The second pass is on the refusal path alone, and it names the grant
        // that is missing rather than leaving a count for somebody to work back from.
        let missing: Vec<_> = paths
            .iter()
            .filter(|path| PathFd::new(path).is_err())
            .map(|path| path.display().to_string())
            .collect();
        return Err(std::io::Error::other(format!(
            "landlock: {} of the {} paths the policy names have no rule ({}); refusing \
             rather than confining to less than the policy asked for",
            paths.len() - rules.len(),
            paths.len(),
            missing.join(", ")
        )));
    }
    Ok(rules)
}

/// Landlock-based confinement.
#[derive(Debug, Default)]
pub struct LandlockSandbox;

impl LandlockSandbox {
    /// Probes whether Landlock is actually enforceable here.
    ///
    /// Building a ruleset is not a sufficient test: under `BestEffort` the crate will
    /// happily construct one on a kernel that implements nothing, and the failure only
    /// surfaces later as `EINVAL` from inside `pre_exec`, where it looks like a spawn
    /// error rather than absent confinement.
    ///
    /// So the ABI is queried directly. `ENOSYS` means the syscall does not exist:
    /// the case on Docker Desktop's linuxkit kernel, which does not enable the LSM.
    /// A kernel that has it but reports less than [`MINIMUM_ABI_VERSION`] is refused for
    /// the same reason, since `BestEffort` would run it under the rights it does carry.
    pub fn new() -> Result<Self, SandboxError> {
        abi_meets_minimum(landlock_abi_version())?;

        landlock::Ruleset::default()
            .set_compatibility(CompatLevel::BestEffort)
            .handle_access(AccessFs::from_all(HANDLED_ABI))
            .and_then(|r| r.create())
            .map_err(|e| SandboxError::Unavailable {
                platform: "linux",
                detail: format!("landlock ruleset could not be created: {e}"),
            })?;

        Ok(Self)
    }
}

impl Sandbox for LandlockSandbox {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            // Filesystem restrictions are kernel-enforced, but network and subprocess
            // denial are not implemented yet, so this is deliberately not reported
            // as full kernel confinement.
            level: ConfinementLevel::Partial,
            mechanisms: vec!["landlock"],
            network_denial_enforced: false,
            // A rule is a right attached to an open descriptor, so a path nothing can
            // open cannot be named in one and a policy naming one is refused.
            grants_paths_that_do_not_exist: false,
        }
    }

    fn spawn(
        &self,
        program: &str,
        args: &[String],
        policy: &SandboxPolicy,
        streams: Streams,
        environment: Environment,
    ) -> Result<ConfinedChild, SandboxError> {
        if !policy.is_meaningful() {
            return Err(SandboxError::PolicyTooPermissive);
        }

        // Network denial is not yet enforceable here, so a policy that requires it
        // must not be silently downgraded.
        if !policy.allow_network {
            return Err(SandboxError::SetupFailed {
                mechanism: "landlock",
                detail: "network denial is not implemented on Linux yet; refusing rather \
                         than reporting confinement that is not applied"
                    .into(),
            });
        }

        // Subprocess denial is not yet enforceable here, so a policy that requires it
        // must not be silently downgraded.
        if !policy.allow_subprocesses {
            return Err(SandboxError::SetupFailed {
                mechanism: "landlock",
                detail: "subprocess denial is not implemented on Linux yet; refusing rather \
                         than reporting confinement that is not applied"
                    .into(),
            });
        }

        // A path nothing can open is a grant this backend cannot install, and installing
        // the rest of the policy instead would hand the caller a narrower confinement
        // than the one it asked for with nothing saying so. The caller hears it here,
        // before a process exists, rather than from a program refused a path the policy
        // named.
        if let Some(e) = policy
            .readable
            .iter()
            .chain(policy.writable.iter().map(|row| &row.path))
            .find_map(|path| PathFd::new(path).err())
        {
            return Err(SandboxError::SetupFailed {
                mechanism: "landlock",
                detail: format!(
                    "the policy names a path no rule can be built for ({e}); refusing \
                     rather than confining to less than it asked for"
                ),
            });
        }

        let mut command = Command::new(program);
        command.args(args);
        if let Some(directory) = &policy.starting_in {
            command.current_dir(directory);
        }

        let readable: Vec<_> = policy.readable.clone();
        let writable: Vec<_> = policy.writable.iter().map(|row| row.path.clone()).collect();

        // Landlock applies to the calling thread and is inherited across exec, so the
        // ruleset is installed in the child between fork and exec.
        // `pre_exec` is unsafe by definition: its closure runs in the forked child, where
        // only async-signal-safe work is allowed.
        #[allow(unsafe_code)]
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        unsafe {
            command.pre_exec(move || {
                use std::io::Error;

                let mut ruleset = landlock::Ruleset::default()
                    .set_compatibility(CompatLevel::BestEffort)
                    .handle_access(AccessFs::from_all(HANDLED_ABI))
                    .and_then(|r| r.create())
                    .map_err(|e| Error::other(format!("landlock: {e}")))?;

                if !readable.is_empty() {
                    ruleset = ruleset
                        .add_rules(rules_for_every_path(
                            &readable,
                            AccessFs::from_read(HANDLED_ABI),
                        )?)
                        .map_err(|e| Error::other(format!("landlock read rules: {e}")))?;
                }

                if !writable.is_empty() {
                    ruleset = ruleset
                        .add_rules(rules_for_every_path(
                            &writable,
                            AccessFs::from_all(HANDLED_ABI),
                        )?)
                        .map_err(|e| Error::other(format!("landlock write rules: {e}")))?;
                }

                let status = ruleset
                    .restrict_self()
                    .map_err(|e| Error::other(format!("landlock: {e}")))?;

                // Fail closed: if the kernel did not actually enforce the ruleset, do
                // not continue to exec.
                if status.ruleset == RulesetStatus::NotEnforced {
                    return Err(Error::other(
                        "landlock reported the ruleset was not enforced",
                    ));
                }

                Ok(())
            });
        }

        crate::process::start(command, streams, &environment)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::{Prelude, base};
    use crate::testutil::{
        capturing_stdout, nothing_attached, printed_by, variable_names_received_by,
    };
    use std::io::Write;
    use std::os::unix::fs::MetadataExt;
    use std::path::Path;

    /// cat reporting that the file it was asked for could not be read. Any other code
    /// means it stopped before opening the file, which says nothing about a read grant.
    const CAT_FAILED: i32 = 1;

    /// touch reporting that the write it was asked for failed. Any other code means it
    /// stopped before the write, which says nothing about a write grant.
    const TOUCH_FAILED: i32 = 1;

    /// python reporting that the call it was asked to make raised. Any other code means it
    /// stopped before the call, which says nothing about a grant.
    const PYTHON_FAILED: i32 = 1;

    /// The Landlock ABI version carrying the right that governs emptying a file, and the
    /// version carrying the one that governs an ioctl on a device. Both are above the floor
    /// this backend refuses a kernel below.
    const TRUNCATE_ABI_VERSION: libc::c_long = 3;
    const IOCTL_ABI_VERSION: libc::c_long = 5;

    /// What a file a test reads back is filled with, so that an emptied one is a length
    /// rather than an absence.
    const CONTENTS: &[u8] = b"contents";

    /// Whether this kernel carries a right that arrived above the floor, announcing the
    /// skip where it does not.
    ///
    /// Such a kernel is one this backend still confines a process on
    /// ([`MINIMUM_ABI_VERSION`]) and one where no ruleset can hold that process to the
    /// right in question, so a test asserting that denial says it did not run rather than
    /// failing over the kernel or passing while asserting nothing.
    fn kernel_governs(operation: &str, arrived_in: libc::c_long, kernel: &str) -> bool {
        let version = landlock_abi_version();
        if version < arrived_in {
            // Straight at the descriptor, for the reason the missing-landlock skip is.
            let _ = writeln!(
                std::io::stderr(),
                "SKIPPED: landlock abi {version} carries no right governing {operation} \
                 (needs abi {arrived_in}, kernel {kernel})"
            );
            return false;
        }
        true
    }

    /// The narrowest policy a process can actually start under. The loader reads the
    /// binary and the libraries it links, and Landlock has no exemption for that, while
    /// withholding the network or subprocesses is refused outright because neither
    /// denial is enforceable here.
    ///
    /// Only the library directories this machine has: naming one it does not is a policy
    /// this backend refuses, and `/lib64` is absent on a distribution whose architecture
    /// never had it.
    fn loadable_policy() -> SandboxPolicy {
        ["/usr", "/lib", "/lib64", "/bin"]
            .into_iter()
            .filter(|path| std::path::Path::new(path).exists())
            .fold(
                SandboxPolicy::strict()
                    .allow_network_egress()
                    .allow_subprocesses(),
                SandboxPolicy::allow_read,
            )
    }

    /// Landlock is unusable here on kernels before 5.19 and in container runtimes that do
    /// not enable the LSM, notably Docker Desktop's linuxkit kernel.
    ///
    /// A kernel without it fails the tests that need real enforcement rather than
    /// skipping them. Those tests are the whole of what pins confinement on Linux, so a
    /// suite reporting green having never installed a ruleset is the false confidence
    /// this crate exists to avoid, and a kernel that cannot enforce one is worth hearing
    /// about from the test run rather than from the first process that escapes.
    /// `BRAVEBOT_ALLOW_MISSING_LANDLOCK=1` runs the rest of the suite on such a kernel
    /// and says in the output that it did.
    fn sandbox_or_fail() -> Option<LandlockSandbox> {
        match LandlockSandbox::new() {
            Ok(sandbox) => Some(sandbox),
            Err(e) => {
                if std::env::var("BRAVEBOT_ALLOW_MISSING_LANDLOCK").as_deref() == Ok("1") {
                    // Straight at the descriptor rather than through `eprintln!`, which the
                    // test harness captures and replays only for a test that failed. A skip
                    // announced that way is invisible in every run where it is the whole
                    // story, which is a silent skip again by another road.
                    let _ = writeln!(
                        std::io::stderr(),
                        "SKIPPED (BRAVEBOT_ALLOW_MISSING_LANDLOCK=1): {e}"
                    );
                    return None;
                }
                panic!(
                    "landlock is unavailable, so nothing here enforces confinement: {e}. \
                     Set BRAVEBOT_ALLOW_MISSING_LANDLOCK=1 to run the rest of the suite on \
                     a kernel that does not implement it."
                );
            }
        }
    }

    /// A right this crate knows and the ruleset does not handle is a right nothing
    /// restricts, so the boundary is only as wide as the newest ABI named here. Without
    /// this, an upgrade of the `landlock` crate is a widening nobody performs: the rights
    /// the version it learned governs sit outside the confinement, unrestricted on every
    /// kernel that carries them, and every other test here passes.
    ///
    /// `ABI::from` clamps to the greatest version the crate knows, and the enumeration is
    /// `non_exhaustive`, so this is the only way to ask it what that version is.
    #[test]
    fn the_ruleset_handles_every_right_this_crate_knows_of() {
        assert_eq!(
            HANDLED_ABI,
            ABI::from(i32::MAX),
            "the landlock crate knows a newer abi than this ruleset handles, so the rights \
             it added restrict nothing; widen HANDLED_ABI to it, having read what those \
             rights govern and what granting them inside a policy's paths means"
        );
        assert!(
            AccessFs::from_all(HANDLED_ABI).contains(AccessFs::Truncate),
            "the right that governs emptying a file has to be one the ruleset handles"
        );
    }

    #[test]
    fn capabilities_do_not_overstate_network_denial() {
        let caps = LandlockSandbox.capabilities();
        assert!(
            !caps.network_denial_enforced,
            "network denial is not implemented on Linux yet"
        );
        assert_eq!(caps.level, ConfinementLevel::Partial);
    }

    /// Until network denial is implemented, asking for it must be an error rather than
    /// a sandbox that quietly permits sockets.
    #[test]
    fn a_policy_requiring_network_denial_is_refused() {
        let err = LandlockSandbox
            .spawn(
                "/bin/true",
                &[],
                &SandboxPolicy::strict(),
                nothing_attached(),
                Environment::Inherited,
            )
            .expect_err("must refuse rather than under-enforce");
        match err {
            SandboxError::SetupFailed { mechanism, detail } => {
                assert_eq!(mechanism, "landlock");
                assert!(
                    detail.contains("network denial"),
                    "unexpected detail: {detail}"
                );
            }
            other => panic!("expected SetupFailed for network denial, got: {other:?}"),
        }
    }

    /// Until subprocess denial is implemented, asking for it must be an error rather than
    /// a sandbox that quietly permits fork/exec.
    #[test]
    fn a_policy_requiring_subprocess_denial_is_refused() {
        let err = LandlockSandbox
            .spawn(
                "/bin/true",
                &[],
                &SandboxPolicy::strict().allow_network_egress(),
                nothing_attached(),
                Environment::Inherited,
            )
            .expect_err("must refuse rather than under-enforce");
        match err {
            SandboxError::SetupFailed { mechanism, detail } => {
                assert_eq!(mechanism, "landlock");
                assert!(
                    detail.contains("subprocess denial"),
                    "unexpected detail: {detail}"
                );
            }
            other => panic!("expected SetupFailed for subprocess denial, got: {other:?}"),
        }
    }

    #[test]
    fn a_fully_permissive_policy_is_refused() {
        let policy = SandboxPolicy::strict()
            .allow_network_egress()
            .allow_subprocesses()
            .allow_write("/");
        let err = LandlockSandbox
            .spawn(
                "/bin/true",
                &[],
                &policy,
                nothing_attached(),
                Environment::Inherited,
            )
            .expect_err("must refuse a policy that confines nothing");
        assert!(matches!(err, SandboxError::PolicyTooPermissive));
    }

    #[test]
    fn a_confined_process_runs() {
        let Some(sandbox) = sandbox_or_fail() else {
            return;
        };
        let policy = loadable_policy();

        let mut child = sandbox
            .spawn(
                "/bin/true",
                &[],
                &policy,
                nothing_attached(),
                Environment::Inherited,
            )
            .expect("should spawn");
        assert!(child.wait().expect("should wait").success());
    }

    /// The property the backend exists for: writes outside the granted paths fail.
    #[test]
    fn a_confined_process_cannot_write_outside_its_grants() {
        let Some(sandbox) = sandbox_or_fail() else {
            return;
        };
        let policy = loadable_policy();

        // The parent has to be there and empty. Into a directory that does not exist touch
        // fails with ENOENT whatever the ruleset permits, which holds just as well against a
        // sandbox granting every write, and a file left behind by an earlier run would fail
        // every run after it.
        let dir = crate::testutil::scratch_dir("bravebot-landlock-denied-write");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the scratch directory is creatable");
        let target = dir.join("must-not-exist");

        let mut child = sandbox
            .spawn(
                "/usr/bin/touch",
                &[target.display().to_string()],
                &policy,
                nothing_attached(),
                Environment::Inherited,
            )
            .expect("should spawn");
        let status = child.wait().expect("should wait");

        // touch's own refusal, rather than any failure at all: a process that died before it
        // reached the write exits by signal or with some other code, and neither of those
        // says anything about a grant.
        assert_eq!(
            status.code(),
            Some(TOUCH_FAILED),
            "the write was not what failed"
        );
        assert!(!target.exists(), "file was created despite confinement");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The control that gives every denial here its meaning: without the write rules
    /// installed a write inside the grants fails too, and a test asserting only denials
    /// passes just as readily against a sandbox that permits nothing at all.
    #[test]
    fn a_confined_process_can_write_inside_its_grants() {
        let Some(sandbox) = sandbox_or_fail() else {
            return;
        };

        // The directory has to be there before the policy is built: a path that cannot be
        // opened is a grant this backend refuses, so a missing directory would produce no
        // command at all and the write below would never be reached.
        let dir = crate::testutil::scratch_dir("bravebot-landlock-granted-write");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the scratch directory is creatable");
        let target = dir.join("written");

        let policy = loadable_policy().allow_write(&dir);
        let mut child = sandbox
            .spawn(
                "/usr/bin/touch",
                &[target.display().to_string()],
                &policy,
                nothing_attached(),
                Environment::Inherited,
            )
            .expect("should spawn");
        assert!(
            child.wait().expect("should wait").success(),
            "a granted write was denied, so nothing else here means anything"
        );
        assert!(target.exists(), "the write succeeded and created nothing");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Reads are a grant list as much as writes are: a readable set widened to `/` leaves
    /// every other test here passing while the confined process can read the whole host.
    ///
    /// Both halves run against the same file, because a cat that failed for reasons of
    /// its own exits exactly as one refused the file does.
    #[test]
    fn a_confined_process_cannot_read_outside_its_grants() {
        let Some(sandbox) = sandbox_or_fail() else {
            return;
        };

        let dir = crate::testutil::scratch_dir("bravebot-landlock-denied-read");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the scratch directory is creatable");
        let target = dir.join("readable");
        std::fs::write(&target, b"contents").expect("the file is writable");

        let cat = |policy: &SandboxPolicy| {
            sandbox
                .spawn(
                    "/usr/bin/cat",
                    &[target.display().to_string()],
                    policy,
                    nothing_attached(),
                    Environment::Inherited,
                )
                .expect("should spawn")
        };

        let mut granted = cat(&loadable_policy().allow_read(&dir));
        assert_eq!(
            granted.wait().expect("should wait").code(),
            Some(0),
            "a granted read failed, so nothing below means anything"
        );

        let mut refused = cat(&loadable_policy());
        assert_eq!(
            refused.wait().expect("should wait").code(),
            Some(CAT_FAILED),
            "the read was not what failed"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A directory standing in for the one a session resolved as it opened. Under the workspace
    /// rather than under the machine's temporary directory, which is shared between users and
    /// so is not a place for a test to put a file it is about to assert on.
    fn a_temporary_directory(name: &str) -> PathBuf {
        let path = crate::testutil::scratch_dir(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("the scratch directory is creatable");
        path
    }

    /// The base is what a program a person asked for starts with, so a program has to be able
    /// to start under it on the machine in front of it: the rows it names have to be enough for
    /// the loader to read a binary and the libraries it links, and the rows this machine does
    /// not carry have to leave the rest installable rather than refusing the program.
    #[test]
    fn a_program_starts_under_the_base_this_machine_resolved() {
        let Some(sandbox) = sandbox_or_fail() else {
            return;
        };
        let temporary_directory = a_temporary_directory("bravebot-base-starts");
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let wanted = base(Prelude::Linux, &temporary_directory, home.as_deref());

        let resolved = wanted.nameable_under(&sandbox.capabilities());

        let mut child = sandbox
            .spawn(
                "/bin/true",
                &[],
                &resolved.policy,
                nothing_attached(),
                Environment::Inherited,
            )
            .expect("a program starts under the base");
        assert!(
            child.wait().expect("should wait").success(),
            "a program could not start under the base, so every run under one is a refusal"
        );

        let _ = std::fs::remove_dir_all(&temporary_directory);
    }

    /// A program that cannot put a name to the account it runs as is a program whose output is
    /// wrong rather than one that failed: `ls -l` prints numbers, and an archive unpacked under
    /// the base records them. The rows that answer a lookup are in the base for that reason, and
    /// a lookup is the only thing that shows whether all of them are.
    #[test]
    fn a_program_under_the_base_can_name_the_account_it_runs_as() {
        let Some(sandbox) = sandbox_or_fail() else {
            return;
        };
        let temporary_directory = a_temporary_directory("bravebot-base-account");
        let policy = base(Prelude::Linux, &temporary_directory, None)
            .nameable_under(&sandbox.capabilities())
            .policy;

        let mut child = sandbox
            .spawn(
                "/usr/bin/id",
                &["-gn".to_owned()],
                &policy,
                nothing_attached(),
                Environment::Inherited,
            )
            .expect("should spawn");

        assert_eq!(
            child.wait().expect("should wait").code(),
            Some(0),
            "a program under the base could not name the group it runs as"
        );

        let _ = std::fs::remove_dir_all(&temporary_directory);
    }

    /// What the base buys is what it leaves out, and a list is only what the kernel installs:
    /// a key no plan named is unreadable to a program running under the base. Both halves run
    /// under the same policy, because a program that could read nothing at all is refused the
    /// key as surely as one the base holds to what it names.
    #[test]
    fn a_program_under_the_base_reads_the_machine_and_not_a_private_key() {
        let Some(sandbox) = sandbox_or_fail() else {
            return;
        };

        let temporary_directory = a_temporary_directory("bravebot-base-private-key-tmp");
        let home = crate::testutil::scratch_dir("bravebot-base-private-key");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".ssh")).expect("the scratch directory is creatable");
        let key = home.join(".ssh").join("id_rsa");
        std::fs::write(&key, CONTENTS).expect("the key file is writable");

        let policy = base(Prelude::Linux, &temporary_directory, Some(&home))
            .nameable_under(&sandbox.capabilities())
            .policy;
        let cat = |path: &Path| {
            sandbox
                .spawn(
                    "/usr/bin/cat",
                    &[path.display().to_string()],
                    &policy,
                    nothing_attached(),
                    Environment::Inherited,
                )
                .expect("should spawn")
        };

        let mut granted = cat(Path::new("/etc/hosts"));
        assert_eq!(
            granted.wait().expect("should wait").code(),
            Some(0),
            "a file the base names could not be read, so the refusal below means nothing"
        );

        let mut refused = cat(&key);
        assert_eq!(
            refused.wait().expect("should wait").code(),
            Some(CAT_FAILED),
            "the key was readable, or the read failed for another reason"
        );

        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&temporary_directory);
    }

    /// The refusal before the spawn leaves a window: a path can go away between the check
    /// and the exec, and the rules are built on the far side of a fork where no error can
    /// reach the caller as anything but a failure to spawn. A dropped rule has to stop the
    /// exec there too, or the window is a process running under a policy nobody applied in
    /// full.
    #[test]
    fn a_ruleset_is_not_built_with_a_path_missing_from_it() {
        let err = rules_for_every_path(
            &[PathBuf::from("/bravebot-no-such-path")],
            AccessFs::from_read(HANDLED_ABI),
        )
        .expect_err("must refuse to build a ruleset short of a path");
        assert!(
            err.to_string().contains("/bravebot-no-such-path"),
            "the refusal has to name the path that has no rule: {err}"
        );
    }

    /// A policy is the list of paths a caller decided a program may reach, so a backend
    /// granting fewer of them than it was given leaves nobody able to read the guarantee
    /// off the policy: the process runs, the record says the policy was applied, and the
    /// one path the caller cared about is the one that was dropped.
    #[test]
    fn a_path_that_cannot_be_opened_is_refused_rather_than_dropped() {
        let dir = crate::testutil::scratch_dir("bravebot-landlock-absent-path");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the scratch directory is creatable");
        let absent = dir.join("not-created-yet");

        // Both lists, because a check over one of them leaves the other silently narrowing.
        for policy in [
            loadable_policy().allow_read(&absent),
            loadable_policy().allow_write(&absent),
        ] {
            let err = LandlockSandbox
                .spawn(
                    "/bin/true",
                    &[],
                    &policy,
                    nothing_attached(),
                    Environment::Inherited,
                )
                .expect_err("must refuse a grant it cannot install");
            match err {
                SandboxError::SetupFailed { mechanism, detail } => {
                    assert_eq!(mechanism, "landlock");
                    assert!(
                        detail.contains(&absent.display().to_string()),
                        "the refusal has to name the path: {detail}"
                    );
                }
                other => panic!("expected SetupFailed naming the path, got: {other:?}"),
            }
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The refusal above is the right answer to a caller that named the wrong path and the
    /// wrong answer to a machine that does not carry a toolchain some list knows, so what a
    /// caller does about it has to be measured against this backend rather than against a
    /// boolean: a resolution that still leaves the policy refused buys nothing, and one that
    /// resolves a policy this backend would have taken as written has thrown a grant away.
    #[test]
    fn a_policy_refused_over_an_absent_path_is_one_this_backend_installs_once_it_is_resolved() {
        let Some(sandbox) = sandbox_or_fail() else {
            return;
        };

        let dir = crate::testutil::scratch_dir("bravebot-landlock-resolved-policy");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the scratch directory is creatable");
        let absent = dir.join("not-created-yet");

        let wanted = loadable_policy().allow_write(&dir).allow_write(&absent);
        sandbox
            .spawn(
                "/bin/true",
                &[],
                &wanted,
                nothing_attached(),
                Environment::Inherited,
            )
            .expect_err("a grant over a path that is not there is one this backend refuses");

        let resolved = wanted.nameable_under(&sandbox.capabilities());
        assert_eq!(resolved.omitted, vec![absent]);
        assert!(
            resolved.policy.writable.iter().any(|row| row.path == dir),
            "the path that is there went with the one that is not"
        );
        let mut confined = sandbox
            .spawn(
                "/bin/true",
                &[],
                &resolved.policy,
                nothing_attached(),
                Environment::Inherited,
            )
            .expect("the resolved policy names only paths this backend can grant");
        assert!(
            confined.wait().expect("should wait").success(),
            "the resolved policy was installed and the process could not run under it"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A caller reads this to decide whether an absent path in a policy needs creating
    /// first or the directory holding it named instead, and a report disagreeing with the
    /// backend costs it one of those on a platform where neither was necessary. Both
    /// directions of disagreement cost that, which is why the assertion ties the report to
    /// what the backend does rather than pinning a value.
    #[test]
    fn a_path_that_does_not_exist_is_granted_exactly_where_the_capability_says_so() {
        let Some(sandbox) = sandbox_or_fail() else {
            return;
        };

        let dir = crate::testutil::scratch_dir("bravebot-landlock-absent-path-capability");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the scratch directory is creatable");
        let absent = dir.join("not-created-yet");

        // The same grant over a path that is there, so a backend refusing every policy
        // handed to it does not read as one refusing this path.
        let mut confined = sandbox
            .spawn(
                "/bin/true",
                &[],
                &loadable_policy().allow_write(&dir),
                nothing_attached(),
                Environment::Inherited,
            )
            .expect("a grant naming a path that is there is one this backend installs");
        assert!(confined.wait().expect("should wait").success());

        let started = sandbox.spawn(
            "/bin/true",
            &[],
            &loadable_policy().allow_write(&absent),
            nothing_attached(),
            Environment::Inherited,
        );
        // Whether the backend installed the grant, which is what the capability reports.
        // How the process then exited is a separate question, asked separately below, so
        // that a program failing for its own reasons cannot read as a refused policy.
        let granted = started.is_ok();
        if let Ok(mut confined) = started {
            assert!(
                confined.wait().expect("should wait").success(),
                "the confined process failed, so it says nothing about the grant"
            );
        }
        assert_eq!(
            granted,
            sandbox.capabilities().grants_paths_that_do_not_exist,
            "what this backend reports about a path that does not exist is not what it does"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A kernel with Landlock but without the right that governs moving a file is one this
    /// backend cannot hold a policy to, and `BestEffort` compatibility would run a process
    /// under it anyway, reporting a policy that was applied without the operation every
    /// build performs.
    #[test]
    fn a_kernel_that_cannot_govern_a_move_is_refused_rather_than_confining_without_it() {
        let absent =
            abi_meets_minimum(-1).expect_err("a kernel with no landlock cannot confine anything");
        match absent {
            SandboxError::Unavailable { platform, detail } => {
                assert_eq!(platform, "linux");
                assert!(
                    detail.contains("not implemented"),
                    "an absent syscall has to read as absent: {detail}"
                );
            }
            other => panic!("expected Unavailable for an absent syscall, got: {other:?}"),
        }

        let old = abi_meets_minimum(MINIMUM_ABI_VERSION - 1)
            .expect_err("an abi without the move right has to be refused");
        match old {
            SandboxError::Unavailable { platform, detail } => {
                assert_eq!(platform, "linux");
                assert!(
                    detail.contains("abi 1") && detail.contains("5.19"),
                    "the refusal has to name what the kernel has and what it needs: {detail}"
                );
            }
            other => panic!("expected Unavailable for an old abi, got: {other:?}"),
        }

        abi_meets_minimum(MINIMUM_ABI_VERSION).expect("the abi this backend needs is enough");
    }

    /// Writing a temporary file and renaming it into place is how a compiler, a package
    /// manager and an editor write anything at all, so a confinement that denies the move
    /// holds an ordinary build to less than the paths its policy granted.
    ///
    /// The inode is what the assertion is on, because `mv` answers a refused rename by
    /// copying the file and unlinking the original: the destination exists either way, and
    /// only a preserved inode says the move happened rather than a copy that is neither
    /// atomic nor cheap.
    #[test]
    fn a_confined_process_can_rename_a_file_between_two_granted_directories() {
        let Some(sandbox) = sandbox_or_fail() else {
            return;
        };

        // Both directories have to be there before the policy is built, since a path that
        // cannot be opened is a grant this backend refuses outright.
        let dir = crate::testutil::scratch_dir("bravebot-landlock-rename");
        let _ = std::fs::remove_dir_all(&dir);
        let source = dir.join("from").join("moved");
        let destination = dir.join("to").join("moved");
        std::fs::create_dir_all(dir.join("from")).expect("the scratch directory is creatable");
        std::fs::create_dir_all(dir.join("to")).expect("the scratch directory is creatable");
        std::fs::write(&source, b"contents").expect("the file is writable");
        let inode = std::fs::metadata(&source).expect("the file is there").ino();

        let policy = loadable_policy().allow_write(&dir);
        let mut child = sandbox
            .spawn(
                "/usr/bin/mv",
                &[
                    source.display().to_string(),
                    destination.display().to_string(),
                ],
                &policy,
                nothing_attached(),
                Environment::Inherited,
            )
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

    /// The right that governs emptying a file arrived after the ones that govern writing
    /// to it, so a ruleset handling only the older set leaves `truncate(2)` checked
    /// against nothing at all: that syscall names a path and opens no descriptor, so the
    /// write grant the ruleset does handle is never consulted, and a confined process
    /// empties a file no grant names while the record says the policy was applied. A
    /// private key, a shell profile and a database are each destroyed by an emptying as
    /// thoroughly as by a write.
    ///
    /// The refused half is granted the directory for reading, which is the narrower claim
    /// and the one worth making: emptying a file is a write, so a read grant carries no
    /// more right to it than no grant at all, and the paths every confined process here is
    /// granted for reading hold the loader and the system libraries. The half inside the
    /// write grant is the other end of the same right, and without it a ruleset that
    /// handles truncation and grants it nowhere reads as a confinement rather than as an
    /// ordinary write refused inside the paths a policy named.
    ///
    /// The witness is python because nothing else on the machine truncates by path:
    /// `truncate(1)` opens the file `O_WRONLY` and calls `ftruncate`, which the write
    /// grant already denies, so a test built on it passes against the gap it is written
    /// to catch. Its environment is empty, so a `PYTHON` variable a machine running this
    /// holds cannot send the interpreter to a path no grant names and fail it for a reason
    /// of its own.
    #[test]
    fn a_confined_process_cannot_truncate_a_file_outside_its_grants() {
        let Some(sandbox) = sandbox_or_fail() else {
            return;
        };
        if !kernel_governs("the emptying of a file", TRUNCATE_ABI_VERSION, "6.2+") {
            return;
        }

        let dir = crate::testutil::scratch_dir("bravebot-landlock-denied-truncate");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the scratch directory is creatable");
        let target = dir.join("kept");

        // Refilled before each half, so the second half reads the file the first left
        // rather than one it emptied.
        let emptied_by = |policy: &SandboxPolicy| {
            std::fs::write(&target, CONTENTS).expect("the file is writable");
            let mut child = sandbox
                .spawn(
                    "/usr/bin/python3",
                    &[
                        "-c".to_owned(),
                        "import os, sys; os.truncate(sys.argv[1], 0)".to_owned(),
                        target.display().to_string(),
                    ],
                    policy,
                    nothing_attached(),
                    Environment::Empty,
                )
                .expect("python is the one program here that truncates a path");
            child.wait().expect("should wait")
        };

        let granted = emptied_by(&loadable_policy().allow_write(&dir));
        assert_eq!(
            granted.code(),
            Some(0),
            "emptying a file inside a granted directory failed, so nothing below means \
             anything"
        );
        assert_eq!(
            std::fs::metadata(&target).expect("the file is there").len(),
            0,
            "the granted truncation reported success and emptied nothing"
        );

        let refused = emptied_by(&loadable_policy().allow_read(&dir));
        assert_eq!(
            std::fs::metadata(&target).expect("the file is there").len(),
            CONTENTS.len() as u64,
            "a file granted for reading and not for writing was emptied despite confinement"
        );
        // python's own refusal, rather than any failure at all: a process that died before
        // it reached the call exits with some other code, which says nothing about a grant.
        assert_eq!(
            refused.code(),
            Some(PYTHON_FAILED),
            "the truncation was not what failed"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The ruleset handles the right that governs an ioctl on a device, and Landlock counts
    /// that right as a write, so a device a policy named for reading can be opened and read
    /// and not driven. A caller that means a program to drive one names it for writing.
    ///
    /// This is the decision worth pinning rather than the denial alone: a policy's two
    /// lists are all a caller has to say what a program may do, so which list carries an
    /// ioctl is a fact a caller reads the grant against, and a diff moving it either way is
    /// a diff moving what every read grant over a device permits. Leaving the right
    /// unhandled is the third answer, and it permits every ioctl on every device a grant
    /// can open.
    ///
    /// Both halves run the same request against the same device, because an interpreter
    /// that failed for reasons of its own reports no result at all. The request is
    /// `RNDGETENTCNT`, which succeeds on a readable `/dev/urandom` and is not one of the
    /// commands the kernel permits a confined process whatever the ruleset says.
    #[test]
    fn a_confined_process_cannot_drive_a_device_it_was_granted_for_reading() {
        let Some(sandbox) = sandbox_or_fail() else {
            return;
        };
        if !kernel_governs("an ioctl on a device", IOCTL_ABI_VERSION, "6.10+") {
            return;
        }

        // _IOR('R', 0x00, int), spelled out because the value of a request is the
        // architecture's encoding of it rather than a number to take on trust.
        let request = "import array, fcntl, os\n\
                       RNDGETENTCNT = (2 << 30) | (4 << 16) | (ord('R') << 8) | 0x00\n\
                       entropy = array.array('i', [0])\n\
                       fcntl.ioctl(os.open('/dev/urandom', os.O_RDONLY), RNDGETENTCNT, entropy, True)\n";
        let driven_under = |policy: &SandboxPolicy| {
            sandbox
                .spawn(
                    "/usr/bin/python3",
                    &["-c".to_owned(), request.to_owned()],
                    policy,
                    nothing_attached(),
                    Environment::Empty,
                )
                .expect("python is what issues an ioctl here")
                .wait()
                .expect("should wait")
                .code()
        };

        assert_eq!(
            driven_under(&loadable_policy().allow_write("/dev/urandom")),
            Some(0),
            "driving a device granted for writing failed, so nothing below means anything"
        );
        assert_eq!(
            driven_under(&loadable_policy().allow_read("/dev/urandom")),
            Some(PYTHON_FAILED),
            "a device granted for reading was driven, so the right that governs an ioctl \
             is granted by a read or handled by nothing"
        );
    }

    /// Emptying a file and writing it again is what every `>` redirect, every
    /// `std::fs::write` over an existing file and every editor saving one does, and the
    /// kernel checks that against the same right as a truncation by path but through a
    /// different hook: the descriptor is already open, so a ruleset that handles the
    /// right without granting it holds an ordinary write inside a granted directory to
    /// creating a file and never rewriting it.
    ///
    /// `truncate(1)` rather than python, because this half is the one every program on the
    /// machine reaches through the file it has already opened.
    #[test]
    fn a_confined_process_can_truncate_a_file_inside_its_grants() {
        let Some(sandbox) = sandbox_or_fail() else {
            return;
        };

        let dir = crate::testutil::scratch_dir("bravebot-landlock-granted-truncate");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the scratch directory is creatable");
        let target = dir.join("emptied");
        std::fs::write(&target, CONTENTS).expect("the file is writable");

        let mut child = sandbox
            .spawn(
                "/usr/bin/truncate",
                &[
                    "-s".to_owned(),
                    "0".to_owned(),
                    target.display().to_string(),
                ],
                &loadable_policy().allow_write(&dir),
                nothing_attached(),
                Environment::Inherited,
            )
            .expect("should spawn");
        assert!(
            child.wait().expect("should wait").success(),
            "emptying a file inside a granted directory was denied"
        );
        assert_eq!(
            std::fs::metadata(&target).expect("the file is there").len(),
            0,
            "the write succeeded and emptied nothing"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// What a program may be trusted with in the environment is the caller's decision and
    /// not a backend's: a credential lives in a variable rather than in a file, so no
    /// grant over paths either withholds one or hands one over, and the agent socket a
    /// push signs through is named by a variable as well. A caller that asks for its own
    /// environment receives exactly that, on either platform, so the decision reads the
    /// same wherever it is made.
    ///
    /// `CARGO_MANIFEST_DIR` is the variable read back because cargo sets it in the
    /// environment of a test process, so it is one this process holds and nothing else
    /// invents.
    #[test]
    fn the_environment_a_confined_process_receives_is_the_callers() {
        let Some(sandbox) = sandbox_or_fail() else {
            return;
        };
        let held = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets this for a test");

        let mut child = sandbox
            .spawn(
                "/usr/bin/env",
                &[],
                &loadable_policy(),
                capturing_stdout(),
                Environment::Inherited,
            )
            .expect("the confined process runs");

        let environment = printed_by(&mut child);
        assert!(
            environment
                .lines()
                .any(|line| line == format!("CARGO_MANIFEST_DIR={held}")),
            "a variable this process holds did not reach the confined process: {environment}"
        );
    }

    /// A server declared with a directory runs there rather than wherever this process was
    /// started, since a relative path it opens is meant to be one inside it.
    #[test]
    fn a_confined_process_starts_in_the_directory_its_policy_names() {
        let Some(sandbox) = sandbox_or_fail() else {
            return;
        };
        let dir = crate::testutil::scratch_dir("bravebot-sandbox-starting-in");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the scratch directory is creatable");
        let dir = dir.canonicalize().expect("the scratch directory resolves");
        // Readable too, so a process started where this one was prints that rather than failing.
        let here = std::env::current_dir()
            .and_then(|here| here.canonicalize())
            .expect("this process has a directory");
        let policy = loadable_policy()
            .allow_read(&dir)
            .allow_read(&here)
            .starting_in(&dir);

        let mut child = sandbox
            .spawn(
                "/bin/pwd",
                &["-P".to_owned()],
                &policy,
                capturing_stdout(),
                Environment::Empty,
            )
            .expect("the confined process runs");

        assert_eq!(printed_by(&mut child).trim_end(), dir.to_string_lossy());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The other half of that decision, and the one a caller launching third-party code
    /// makes. Emptying it is this crate's to do rather than each caller's, so a program
    /// meant to hold none of this process's credentials holds none of them under either
    /// backend.
    ///
    /// Empty means empty rather than short of one named variable, which is the clause as
    /// written and the only form of it worth having: a process handed `PATH`, `HOME` or the
    /// socket a signature is made through has been handed a credential whatever else was
    /// withheld. A failure prints the names that arrived and not their values, so it says
    /// what leaked without publishing what a machine running this holds.
    #[test]
    fn a_confined_process_given_an_empty_environment_receives_none_of_this_processes_variables() {
        let Some(sandbox) = sandbox_or_fail() else {
            return;
        };
        assert!(
            std::env::var_os("CARGO_MANIFEST_DIR").is_some(),
            "cargo sets this for a test, and without it there is nothing here to withhold"
        );

        let mut child = sandbox
            .spawn(
                "/usr/bin/env",
                &[],
                &loadable_policy(),
                capturing_stdout(),
                Environment::Empty,
            )
            .expect("the confined process runs");

        let received = variable_names_received_by(&mut child);
        assert!(
            received.is_empty(),
            "a process asked to receive no variables received some: {received:?}"
        );
    }
}
