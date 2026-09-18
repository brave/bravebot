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
use crate::{Sandbox, SandboxError};
use landlock::{
    ABI, Access, AccessFs, BitFlags, CompatLevel, Compatible, PathBeneath, PathFd, RulesetAttr,
    RulesetCreatedAttr, RulesetError, RulesetStatus, path_beneath_rules,
};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

/// The Landlock ABI this backend targets. ABI v2 is the first carrying the right that
/// governs moving a file between two directories, and a ruleset that does not handle
/// that right denies the operation wherever it appears, so targeting v1 confines a
/// program to less than its grants name. What v2 costs is the kernels between 5.13 and
/// 5.19, which have Landlock without that right.
const TARGET_ABI: ABI = ABI::V2;

/// The Landlock ABI version [`TARGET_ABI`] needs from the kernel.
const TARGET_ABI_VERSION: libc::c_long = 2;

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

/// Whether a kernel reporting `version` can enforce [`TARGET_ABI`].
///
/// A version rather than the syscall, so every answer is decided by something a test can
/// call: the kernel a suite runs on reports one of them and no test can make it report
/// another.
///
/// A kernel too old for the target ABI is refused rather than confined under the rights
/// it does carry. `BestEffort` compatibility is what lets a grant for a regular file drop
/// the rights only a directory can hold, and it drops the whole target ABI just as
/// quietly on a kernel that is merely old, which is silent degradation rather than a
/// platform difference a caller can work with.
///
/// The two refusals are separate because the answer to them differs: a kernel with no
/// Landlock at all is a machine to enable the LSM on, and an old one is a machine to
/// upgrade.
fn abi_supports_target(version: libc::c_long) -> Result<(), SandboxError> {
    if version < 1 {
        return Err(SandboxError::Unavailable {
            platform: "linux",
            detail: "the landlock syscall is not implemented on this kernel \
                     (needs 5.19+ with the LSM enabled)"
                .into(),
        });
    }
    if version < TARGET_ABI_VERSION {
        return Err(SandboxError::Unavailable {
            platform: "linux",
            detail: format!(
                "this kernel implements landlock abi {version}, which has no right governing \
                 the move of a file between two directories, so a ruleset built on it denies \
                 every such move inside the paths a policy grants; refusing rather than \
                 confining to less than a policy asks for (needs abi {TARGET_ABI_VERSION}, \
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
    /// A kernel that has it but reports less than [`TARGET_ABI_VERSION`] is refused for
    /// the same reason, since `BestEffort` would run it under the rights it does carry.
    pub fn new() -> Result<Self, SandboxError> {
        abi_supports_target(landlock_abi_version())?;

        landlock::Ruleset::default()
            .set_compatibility(CompatLevel::BestEffort)
            .handle_access(AccessFs::from_all(TARGET_ABI))
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

    fn command(
        &self,
        program: &str,
        args: &[String],
        policy: &SandboxPolicy,
    ) -> Result<Command, SandboxError> {
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
            .chain(policy.writable.iter())
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

        let readable: Vec<_> = policy.readable.clone();
        let writable: Vec<_> = policy.writable.clone();

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
                    .handle_access(AccessFs::from_all(TARGET_ABI))
                    .and_then(|r| r.create())
                    .map_err(|e| Error::other(format!("landlock: {e}")))?;

                if !readable.is_empty() {
                    ruleset = ruleset
                        .add_rules(rules_for_every_path(
                            &readable,
                            AccessFs::from_read(TARGET_ABI),
                        )?)
                        .map_err(|e| Error::other(format!("landlock read rules: {e}")))?;
                }

                if !writable.is_empty() {
                    ruleset = ruleset
                        .add_rules(rules_for_every_path(
                            &writable,
                            AccessFs::from_all(TARGET_ABI),
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

        Ok(command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::fs::MetadataExt;
    use std::process::Stdio;

    /// cat reporting that the file it was asked for could not be read. Any other code
    /// means it stopped before opening the file, which says nothing about a read grant.
    const CAT_FAILED: i32 = 1;

    /// touch reporting that the write it was asked for failed. Any other code means it
    /// stopped before the write, which says nothing about a write grant.
    const TOUCH_FAILED: i32 = 1;

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
            .command("/bin/true", &[], &SandboxPolicy::strict())
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
            .command(
                "/bin/true",
                &[],
                &SandboxPolicy::strict().allow_network_egress(),
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
            .command("/bin/true", &[], &policy)
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
            .command("/bin/true", &[], &policy)
            .expect("command builds")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
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
            .command("/usr/bin/touch", &[target.display().to_string()], &policy)
            .expect("command builds")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
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
            .command("/usr/bin/touch", &[target.display().to_string()], &policy)
            .expect("command builds")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
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
                .command("/usr/bin/cat", &[target.display().to_string()], policy)
                .expect("command builds")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
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

    /// The refusal before the spawn leaves a window: a path can go away between the check
    /// and the exec, and the rules are built on the far side of a fork where no error can
    /// reach the caller as anything but a failure to spawn. A dropped rule has to stop the
    /// exec there too, or the window is a process running under a policy nobody applied in
    /// full.
    #[test]
    fn a_ruleset_is_not_built_with_a_path_missing_from_it() {
        let err = rules_for_every_path(
            &[PathBuf::from("/bravebot-no-such-path")],
            AccessFs::from_read(TARGET_ABI),
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
                .command("/bin/true", &[], &policy)
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

    /// A caller reads this to decide whether an absent path in a policy needs creating
    /// first or the directory holding it named instead, and a report disagreeing with the
    /// backend costs it one of those on a platform where neither was necessary. Both
    /// directions of disagreement cost that, which is why the assertion ties the report to
    /// what the backend does rather than pinning a value.
    #[test]
    fn a_path_that_does_not_exist_is_granted_exactly_where_the_capability_says_so() {
        let dir = crate::testutil::scratch_dir("bravebot-landlock-absent-path-capability");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the scratch directory is creatable");
        let absent = dir.join("not-created-yet");

        // The same grant over a path that is there, so a backend refusing every policy
        // handed to it does not read as one refusing this path.
        LandlockSandbox
            .command("/bin/true", &[], &loadable_policy().allow_write(&dir))
            .expect("a grant naming a path that is there is one this backend installs");

        let granted = LandlockSandbox
            .command("/bin/true", &[], &loadable_policy().allow_write(&absent))
            .is_ok();
        assert_eq!(
            granted,
            LandlockSandbox
                .capabilities()
                .grants_paths_that_do_not_exist,
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
            abi_supports_target(-1).expect_err("a kernel with no landlock cannot confine anything");
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

        let old = abi_supports_target(TARGET_ABI_VERSION - 1)
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

        abi_supports_target(TARGET_ABI_VERSION).expect("the abi this backend targets is enough");
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
            .command(
                "/usr/bin/mv",
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
        let Some(sandbox) = sandbox_or_fail() else {
            return;
        };
        let held = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets this for a test");

        let printed = sandbox
            .command("/usr/bin/env", &[], &loadable_policy())
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
