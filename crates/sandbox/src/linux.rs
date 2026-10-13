//! Linux confinement via Landlock and a network namespace.
//!
//! Filesystem restrictions are applied with Landlock, which needs kernel 5.19 or
//! newer. Availability is probed at runtime rather than assumed: an older kernel, or a
//! container that masks the syscall, means confinement is unavailable and the process
//! is refused instead of run unconfined.
//!
//! Landlock governs the filesystem, and with it the connect to a socket that has a path and
//! to an abstract one. The network is governed by a user and network namespace the stage is
//! started in ([`crate::netns`]), which leaves it a loopback and nothing else, and a forwarder
//! in it relays one port to the allowed-hosts proxy's unix socket. Landlock's own network
//! rules are not used: they filter by port and not by address, and leave UDP open. Where the
//! kernel will not make the namespace, [`Capabilities::network_denial_enforced`] and
//! [`Capabilities::egress_limited_to_a_port`] are `false` and a policy needing either is
//! refused. Subprocess denial needs a separate mechanism (e.g. seccomp filtering), which is
//! not implemented yet, so the reported level is [`ConfinementLevel::Partial`] and a policy
//! requiring it is refused.

use crate::netns::Namespace;
use crate::policy::{Capabilities, ConfinementLevel, SandboxPolicy};
use crate::process::{ConfinedChild, Environment, Streams};
use crate::{Sandbox, SandboxError};
use landlock::{
    ABI, Access, AccessFs, BitFlags, CompatLevel, Compatible, PathBeneath, PathFd, PathFdError,
    RulesetAttr, RulesetCreatedAttr, RulesetError, RulesetStatus, Scope, path_beneath_rules,
};
use std::ffi::{OsStr, OsString};
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

/// Opens every row of `policy` once. Returns the policy without the listed rows whose names are
/// gone, or `None` where every row opened, or the error for the first row that cannot be.
///
/// A listed row is an entry a directory had when it was listed. When its name is gone by the time
/// the row is opened there is nothing to grant, so the row is left out where any other row
/// refuses the policy. A name is gone when it is not found, is under something that is no longer
/// a directory, or is a stale handle on a network file system. The same open decides it, so a
/// name present at that open and removed later is the installer's to skip. A listed row that is
/// there and cannot be opened, and a path the caller named, are refused (SANDBOX-6).
fn opened_rows(policy: &SandboxPolicy) -> Result<Option<SandboxPolicy>, PathFdError> {
    for path in &policy.readable {
        PathFd::new(path)?;
    }
    let mut gone = Vec::new();
    for (index, row) in policy.writable.iter().enumerate() {
        match PathFd::new(&row.path) {
            Ok(_) => {}
            Err(PathFdError::OpenCall { source, .. })
                if row.listed
                    && matches!(
                        source.kind(),
                        std::io::ErrorKind::NotFound
                            | std::io::ErrorKind::NotADirectory
                            | std::io::ErrorKind::StaleNetworkFileHandle
                    ) =>
            {
                gone.push(index);
            }
            Err(error) => return Err(error),
        }
    }
    if gone.is_empty() {
        return Ok(None);
    }
    let mut kept = policy.clone();
    let mut index = 0;
    kept.writable.retain(|_| {
        index += 1;
        !gone.contains(&(index - 1))
    });
    Ok(Some(kept))
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
        let namespaces = crate::netns::available().is_ok();
        Capabilities {
            // Filesystem restrictions and the network are kernel-enforced, but subprocess
            // denial is not implemented yet, so this is deliberately not reported
            // as full kernel confinement.
            level: ConfinementLevel::Partial,
            mechanisms: if namespaces {
                vec!["landlock", "network-namespace"]
            } else {
                vec!["landlock"]
            },
            // Both are the namespace's, so both are reported only where one can be made.
            network_denial_enforced: namespaces,
            egress_limited_to_a_port: namespaces,
            // A rule is a right attached to an open descriptor, so a path nothing can
            // open cannot be named in one and a policy naming one is refused. A row that
            // stands for an entry a listing produced is left out instead when its name is gone.
            grants_paths_that_do_not_exist: false,
            subtracts_from_a_grant: true,
        }
    }

    fn spawn(
        &self,
        program: &OsStr,
        args: &[OsString],
        policy: &SandboxPolicy,
        streams: Streams,
        environment: Environment,
    ) -> Result<ConfinedChild, SandboxError> {
        let command = Self::confining(program, args, policy)?;
        crate::process::start(command, streams, &environment)
    }

    fn command(
        &self,
        program: &OsStr,
        args: &[OsString],
        policy: &SandboxPolicy,
        environment: &Environment,
    ) -> Result<Command, SandboxError> {
        let mut command = Self::confining(program, args, policy)?;
        crate::process::apply_environment(&mut command, environment);
        Ok(command)
    }
}

/// The namespace a stage is started in, or the refusal where this kernel will not make one.
fn namespace_for(forward: Option<(u16, &std::path::Path)>) -> Result<Namespace, SandboxError> {
    namespace_given(crate::netns::available(), forward)
}

/// [`namespace_for`] given what this kernel answered, so the refusal can be held to without a
/// kernel that gives it.
fn namespace_given(
    available: &Result<(), String>,
    forward: Option<(u16, &std::path::Path)>,
) -> Result<Namespace, SandboxError> {
    if let Err(why) = available {
        return Err(SandboxError::SetupFailed {
            mechanism: "network namespace",
            detail: format!(
                "this kernel does not let an unprivileged user make a user and network \
                 namespace ({why}), so a stage cannot be given no network or held to a proxy; \
                 refusing rather than reporting confinement that is not applied"
            ),
        });
    }
    Namespace::prepare(forward).map_err(|detail| SandboxError::SetupFailed {
        mechanism: "network namespace",
        detail,
    })
}

/// Why a stage cannot be put in a network namespace of its own here, or `None` where it can.
/// `doctor` says it, since it decides whether a closed network or an allowed-hosts list can be
/// applied on this machine.
pub fn network_namespace_unavailable() -> Option<String> {
    crate::netns::available().as_ref().err().cloned()
}

impl LandlockSandbox {
    /// The command for `program` with the ruleset installed between the fork and the exec, or
    /// a refusal of the policy.
    fn confining(
        program: &OsStr,
        args: &[OsString],
        policy: &SandboxPolicy,
    ) -> Result<Command, SandboxError> {
        if !policy.is_meaningful() {
            return Err(SandboxError::PolicyTooPermissive);
        }

        // A stage with no network, or held to a proxy's port, runs in a network namespace of its
        // own, where it has a loopback and nothing else. A kernel that will not make one refuses
        // the stage rather than run it with the host's network.
        let namespace = if !policy.allow_network {
            Some(namespace_for(None)?)
        } else if let Some(port) = policy.egress_only_to {
            let Some(socket) = policy.egress_through.as_deref() else {
                return Err(SandboxError::SetupFailed {
                    mechanism: "network namespace",
                    detail: "limiting egress to one port needs the unix socket the proxy listens \
                             on, and the policy names none; refusing rather than reporting \
                             confinement that is not applied"
                        .into(),
                });
            };
            Some(namespace_for(Some((port, socket)))?)
        } else {
            None
        };

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
        // named. A listed row whose name is gone is the exception: it is left out.
        let trimmed = opened_rows(policy).map_err(|e| SandboxError::SetupFailed {
            mechanism: "landlock",
            detail: format!(
                "the policy names a path no rule can be built for ({e}); refusing \
                 rather than confining to less than it asked for"
            ),
        })?;
        let policy = trimmed.as_ref().unwrap_or(policy);

        let mut command = Command::new(program);
        command.args(args);
        if let Some(directory) = &policy.starting_in {
            command.current_dir(directory);
        }

        // A refusal is made by listing: Landlock grants a directory with everything beneath it.
        let enumerated = !policy.unreadable.is_empty() || !policy.unwritable.is_empty();
        let readable = policy.readable_by_enumeration();
        let writable = policy.writable_by_enumeration();

        // Landlock applies to the calling thread and is inherited across exec, so the
        // ruleset is installed in the child between fork and exec.
        // `pre_exec` is unsafe by definition: its closure runs in the forked child, where
        // only async-signal-safe work is allowed.
        #[allow(unsafe_code)]
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        unsafe {
            command.pre_exec(move || {
                use std::io::Error;

                // The kernel refuses `TIOCSTI` to a process whose controlling terminal is not the
                // one it names, and a session of its own has none. Landlock governs an ioctl on a
                // device only from ABI 5 (kernel 6.10), so older kernels rely on this alone.
                // SAFETY: `setsid` takes no pointers and is async-signal-safe.
                if libc::setsid() == -1 {
                    return Err(Error::last_os_error());
                }

                // Before the ruleset, so the forwarder it leaves behind is not confined by the
                // stage's filesystem rules and the stage needs no grant to the proxy's socket.
                if let Some(namespace) = &namespace {
                    namespace.enter()?;
                }

                let mut ruleset = landlock::Ruleset::default()
                    .set_compatibility(CompatLevel::BestEffort)
                    .handle_access(AccessFs::from_all(HANDLED_ABI))
                    // A connect to an abstract socket made outside this domain. The socket has
                    // no path, so no filesystem right sees it, and `BestEffort` drops the scope
                    // on a kernel below Landlock ABI 6.
                    .and_then(|r| r.scope(Scope::AbstractUnixSocket))
                    .and_then(|r| r.create())
                    .map_err(|e| Error::other(format!("landlock: {e}")))?;

                // An entry that went away between the listing and here has nothing to grant, and
                // the rules are added as they are built so a large directory holds one
                // descriptor at a time and not one for each entry.
                if enumerated {
                    ruleset = ruleset
                        .add_rules(path_beneath_rules(
                            &readable,
                            AccessFs::from_read(HANDLED_ABI),
                        ))
                        .map_err(|e| Error::other(format!("landlock read rules: {e}")))?;
                } else if !readable.is_empty() {
                    ruleset = ruleset
                        .add_rules(rules_for_every_path(
                            &readable,
                            AccessFs::from_read(HANDLED_ABI),
                        )?)
                        .map_err(|e| Error::other(format!("landlock read rules: {e}")))?;
                }

                if enumerated {
                    ruleset = ruleset
                        .add_rules(path_beneath_rules(
                            &writable,
                            AccessFs::from_all(HANDLED_ABI),
                        ))
                        .map_err(|e| Error::other(format!("landlock write rules: {e}")))?;
                } else if !writable.is_empty() {
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

        Ok(command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::{Prelude, base};
    use crate::hosts::HostList;
    use crate::proxy::{Proxy, ProxyConfig};
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

    /// The version carrying the scopes, which is what bounds a connect to an abstract socket.
    const SCOPE_ABI_VERSION: libc::c_long = 6;

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

    /// RUN-8: the program and arguments the command holds are the bytes the caller gave, so a
    /// path that is not text is not replaced by the file whose name spells the replacement of its
    /// bytes. Decided before any process exists, so it runs on a kernel without Landlock.
    ///
    /// The regression it rejects is a conversion to text between the caller and the command: the
    /// command then names the lookalike, which runs under the profile computed for the other.
    #[test]
    fn a_program_and_an_argument_that_are_not_text_reach_the_command_as_their_bytes() {
        use std::os::unix::ffi::OsStrExt;

        let program = OsStr::from_bytes(b"/opt/dir-\xff/runner");
        let argument = OsString::from(OsStr::from_bytes(b"dir-\xff"));

        let command = LandlockSandbox::confining(
            program,
            std::slice::from_ref(&argument),
            &loadable_policy(),
        )
        .expect("a policy over directories that exist");

        assert_eq!(command.get_program(), program);
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [argument.as_os_str()]
        );
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

    /// The two capabilities are the namespace's, so they are reported together and only where a
    /// namespace can be made. The regression it rejects: either reported on a kernel that
    /// refuses the namespace, which hands the caller a stage with the host's network.
    #[test]
    fn capabilities_report_the_network_only_where_a_namespace_can_be_made() {
        let caps = LandlockSandbox.capabilities();
        let made = network_namespace_unavailable().is_none();
        assert_eq!(caps.network_denial_enforced, made);
        assert_eq!(caps.egress_limited_to_a_port, made);
        assert_eq!(caps.mechanisms.contains(&"network-namespace"), made);
        assert_eq!(caps.level, ConfinementLevel::Partial);
    }

    /// A kernel that refuses the namespace refuses the stage, with the namespace named. The
    /// regression it rejects: running the stage with the host's network after the refusal.
    #[test]
    fn a_kernel_that_refuses_the_namespace_refuses_the_stage() {
        let refusal = Err("Operation not permitted (os error 1)".to_string());
        for forward in [None, Some((8123, std::path::Path::new("/run/proxy.sock")))] {
            match namespace_given(&refusal, forward) {
                Err(SandboxError::SetupFailed { mechanism, detail }) => {
                    assert_eq!(mechanism, "network namespace");
                    assert!(detail.contains("Operation not permitted"), "{detail}");
                }
                Err(other) => panic!("expected SetupFailed, got: {other:?}"),
                Ok(_) => panic!("a refused namespace was made"),
            }
        }
    }

    /// A policy limiting egress to a port with no socket to relay it to is refused on any
    /// kernel, since the stage would have a port named and nothing behind it.
    #[test]
    fn a_policy_limiting_egress_to_one_port_without_the_proxys_socket_is_refused() {
        let err = LandlockSandbox
            .spawn(
                OsStr::new("/bin/true"),
                &[],
                &SandboxPolicy::strict()
                    .allow_network_egress_only_to(8123)
                    .allow_subprocesses(),
                nothing_attached(),
                Environment::Inherited,
            )
            .expect_err("must refuse rather than under-enforce");
        match err {
            SandboxError::SetupFailed { mechanism, detail } => {
                assert_eq!(mechanism, "network namespace");
                assert!(detail.contains("one port"), "unexpected detail: {detail}");
            }
            other => panic!("expected SetupFailed for the egress limit, got: {other:?}"),
        }
    }

    /// A policy that takes the network away is refused where this kernel cannot make a
    /// namespace, and not run with sockets.
    #[test]
    fn a_policy_requiring_network_denial_is_refused_where_no_namespace_can_be_made() {
        if network_namespace_unavailable().is_none() {
            let _ = writeln!(
                std::io::stderr(),
                "SKIPPED: this kernel lets an unprivileged user make a network namespace, so \
                 there is no refusal to observe"
            );
            return;
        }
        let err = LandlockSandbox
            .spawn(
                OsStr::new("/bin/true"),
                &[],
                &SandboxPolicy::strict().allow_subprocesses(),
                nothing_attached(),
                Environment::Inherited,
            )
            .expect_err("must refuse rather than under-enforce");
        match err {
            SandboxError::SetupFailed { mechanism, .. } => {
                assert_eq!(mechanism, "network namespace");
            }
            other => panic!("expected SetupFailed for network denial, got: {other:?}"),
        }
    }

    /// Whether this kernel lets a stage be put in a namespace of its own, announcing the skip
    /// where it does not. Ubuntu 24.04 restricts unprivileged user namespaces by default and
    /// many containers block them, and a test of what runs inside one cannot say anything there.
    fn can_make_a_namespace() -> bool {
        match network_namespace_unavailable() {
            None => true,
            Some(why) => {
                let _ = writeln!(
                    std::io::stderr(),
                    "SKIPPED: this kernel does not let an unprivileged user make a network \
                     namespace ({why})"
                );
                false
            }
        }
    }

    /// What `script` printed when run by bash in a stage under `policy`.
    fn printed_by_bash(sandbox: &LandlockSandbox, policy: &SandboxPolicy, script: &str) -> String {
        let mut child = sandbox
            .spawn(
                OsStr::new("/bin/bash"),
                &["-c".into(), script.into()],
                policy,
                capturing_stdout(),
                Environment::Inherited,
            )
            .expect("a stage in a namespace starts");
        printed_by(&mut child)
    }

    /// A shell function that says whether a connection to `address` was made within three
    /// seconds, over the transport `kind` (`tcp` or `udp`). The bound is what makes a stage
    /// that reached the host's network fail the test and not hang it.
    const PROBE: &str = "probe() { if timeout 3 bash -c \"exec 3<>/dev/$1/$2/$3\" 2>/dev/null; \
                         then echo \"$1 $2:$3 connected\"; else echo \"$1 $2:$3 refused\"; fi; }";

    fn a_listener_on_this_hosts_loopback() -> (std::net::TcpListener, u16) {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("a loopback port");
        let port = listener.local_addr().expect("a bound address").port();
        listener
            .set_nonblocking(true)
            .expect("a listener that can be polled");
        (listener, port)
    }

    /// A stage given no network has a namespace with nothing in it: a listener on the host's
    /// loopback is not there, and neither is any other address or protocol. The regression it
    /// rejects: the stage running in the host's network, where the listener accepts it.
    #[test]
    fn a_stage_with_no_network_cannot_connect_to_anything() {
        if !can_make_a_namespace() {
            return;
        }
        let sandbox = LandlockSandbox;
        let (listener, port) = a_listener_on_this_hosts_loopback();
        let policy = loadable_policy().allow_subprocesses();
        assert!(!policy.allow_network);

        let printed = printed_by_bash(
            &sandbox,
            &policy,
            &format!(
                "{PROBE}; probe tcp 127.0.0.1 {port}; probe tcp 192.0.2.1 80; \
                 probe udp 192.0.2.1 9; exit 0"
            ),
        );

        assert!(!printed.contains("connected"), "{printed}");
        assert!(
            listener.accept().is_err(),
            "the stage reached a listener on the host's loopback"
        );
    }

    /// A stage held to the proxy reaches it through the forwarder, and the request arrives at
    /// the proxy that was named. The regressions it rejects: a forwarder relaying to no socket or
    /// the wrong one, which the proxy's own record of the host asked for shows.
    #[test]
    fn a_stage_held_to_the_proxy_reaches_it_through_the_forwarder() {
        if !can_make_a_namespace() {
            return;
        }
        let sandbox = LandlockSandbox;
        let proxy = Proxy::start(ProxyConfig::new(HostList::default())).expect("a proxy");
        let socket = proxy.socket_path().expect("a unix socket").to_path_buf();
        let port = proxy.addr().port();
        let policy = loadable_policy()
            .allow_subprocesses()
            .allow_network_egress_only_to(port)
            .with_egress_through(socket);

        let printed = printed_by_bash(
            &sandbox,
            &policy,
            &format!(
                "exec 3<>/dev/tcp/127.0.0.1/{port} || {{ echo no-forwarder; exit 0; }}; \
                 printf 'CONNECT unlisted.example:443 HTTP/1.1\\r\\n\\r\\n' >&3; \
                 read -r line <&3; echo \"$line\"; exit 0"
            ),
        );

        assert!(printed.contains("403"), "{printed}");
        let asked: Vec<_> = proxy
            .decisions()
            .into_iter()
            .filter_map(|decision| decision.host)
            .collect();
        assert_eq!(asked, ["unlisted.example"]);
    }

    /// A stage held to the proxy has no other way out: not a host, not the proxy's port on
    /// another address, not UDP. The regression it rejects: a namespace that still has the
    /// host's interfaces, or one where only TCP is held.
    #[test]
    fn a_stage_held_to_the_proxy_cannot_connect_around_it() {
        if !can_make_a_namespace() {
            return;
        }
        let sandbox = LandlockSandbox;
        let proxy = Proxy::start(ProxyConfig::new(HostList::default())).expect("a proxy");
        let socket = proxy.socket_path().expect("a unix socket").to_path_buf();
        let port = proxy.addr().port();
        let (listener, other) = a_listener_on_this_hosts_loopback();
        let policy = loadable_policy()
            .allow_subprocesses()
            .allow_network_egress_only_to(port)
            .with_egress_through(socket);

        let printed = printed_by_bash(
            &sandbox,
            &policy,
            &format!(
                "{PROBE}; probe tcp 127.0.0.1 {port}; probe tcp 127.0.0.1 {other}; \
                 probe tcp 127.0.0.2 {port}; probe tcp 192.0.2.1 80; probe udp 192.0.2.1 9; \
                 exit 0"
            ),
        );

        assert!(
            printed.contains(&format!("tcp 127.0.0.1:{port} connected")),
            "the forwarder was not listening: {printed}"
        );
        assert_eq!(printed.matches("connected").count(), 1, "{printed}");
        assert!(
            listener.accept().is_err(),
            "the stage reached a listener on the host's loopback"
        );
    }

    /// Until subprocess denial is implemented, asking for it must be an error rather than
    /// a sandbox that quietly permits fork/exec.
    #[test]
    fn a_policy_requiring_subprocess_denial_is_refused() {
        let err = LandlockSandbox
            .spawn(
                OsStr::new("/bin/true"),
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
                OsStr::new("/bin/true"),
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
                OsStr::new("/bin/true"),
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
                OsStr::new("/usr/bin/touch"),
                &[target.display().to_string().into()],
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
                OsStr::new("/usr/bin/touch"),
                &[target.display().to_string().into()],
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
                    OsStr::new("/usr/bin/cat"),
                    &[target.display().to_string().into()],
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

    /// A command handed back for the caller to spawn is confined by the time it runs: the caller
    /// wires its own streams, as a pipeline must, and the ruleset and the environment it was
    /// given are still what the process gets. A `command` that handed back the bare program
    /// passes the granted half and fails the refused one.
    #[test]
    fn a_command_handed_back_is_confined_when_the_caller_spawns_it() {
        let Some(sandbox) = sandbox_or_fail() else {
            return;
        };
        let granted = crate::testutil::scratch_dir("bravebot-landlock-command-granted");
        let withheld = crate::testutil::scratch_dir("bravebot-landlock-command-withheld");
        for dir in [&granted, &withheld] {
            let _ = std::fs::remove_dir_all(dir);
            std::fs::create_dir_all(dir).expect("the scratch directory is creatable");
        }
        let policy = loadable_policy().allow_write(&granted);
        let environment =
            Environment::Only(crate::process::Variables::new().with("PATH", "/usr/bin"));
        let touch = |target: &std::path::Path| {
            sandbox
                .command(
                    OsStr::new("/usr/bin/touch"),
                    &[target.display().to_string().into()],
                    &policy,
                    &environment,
                )
                .expect("a command")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .expect("spawned")
                .code()
        };

        assert_eq!(touch(&granted.join("written")), Some(0));
        assert!(granted.join("written").exists());
        assert_eq!(touch(&withheld.join("written")), Some(TOUCH_FAILED));
        assert!(!withheld.join("written").exists());

        let printed = sandbox
            .command(OsStr::new("/usr/bin/env"), &[], &policy, &environment)
            .expect("a command")
            .output()
            .expect("spawned");
        assert_eq!(
            String::from_utf8_lossy(&printed.stdout).trim(),
            "PATH=/usr/bin"
        );

        for dir in [&granted, &withheld] {
            let _ = std::fs::remove_dir_all(dir);
        }
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
        let wanted = base(Prelude::Linux, &temporary_directory, None, home.as_deref());

        let resolved = wanted.nameable_under(&sandbox.capabilities());

        let mut child = sandbox
            .spawn(
                OsStr::new("/bin/true"),
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
        let policy = base(Prelude::Linux, &temporary_directory, None, None)
            .nameable_under(&sandbox.capabilities())
            .policy;

        let mut child = sandbox
            .spawn(
                OsStr::new("/usr/bin/id"),
                &["-gn".into()],
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

        let policy = base(Prelude::Linux, &temporary_directory, None, Some(&home))
            .nameable_under(&sandbox.capabilities())
            .policy;
        let cat = |path: &Path| {
            sandbox
                .spawn(
                    OsStr::new("/usr/bin/cat"),
                    &[path.display().to_string().into()],
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

    /// The run base under the kernel: each credential location is refused, each file a program
    /// reads by name is read, the three kinds of file in `~/.ssh` that hold no secret are read
    /// beside the private key that is not, a link to a credential is judged by where it leads,
    /// and a location absent when the stage starts is refused as well. The rows are spelled out
    /// here and not read from the base, so a row dropped from it fails this.
    #[test]
    fn a_stage_under_the_run_base_is_refused_each_credential_location_and_reads_the_rest() {
        let Some(sandbox) = sandbox_or_fail() else {
            return;
        };
        let temporary_directory = a_temporary_directory("bravebot-run-base-table-tmp");
        let home = crate::testutil::scratch_dir("bravebot-run-base-table");
        let _ = std::fs::remove_dir_all(&home);
        let held_back = [
            ".bravebot/gateway-keys.json",
            ".bravebot/leo-premium.json",
            ".bravebot/mcp.json",
            ".bravebot-browser/secret",
            ".ssh/id_ed25519",
            ".ssh/id_rsa",
            ".aws/credentials",
            ".kube/config",
            ".docker/config.json",
            ".azure/accessTokens.json",
            ".config/gcloud/credentials.db",
            ".gnupg/private-keys-v1.d/key",
            ".local/share/keyrings/login.keyring",
            ".password-store/site.gpg",
            ".config/BraveSoftware/Brave-Browser/Default/Cookies",
            ".config/google-chrome/Default/Cookies",
            ".config/chromium/Default/Cookies",
            ".mozilla/firefox/x/cookies.sqlite",
            ".config/sops/age/keys.txt",
            ".bash_history",
            ".zsh_history",
            ".zsh_sessions/A1B2.history",
            ".bash_sessions/A1B2.history",
            ".local/share/fish/fish_history",
            ".psql_history",
            ".mysql_history",
            ".node_repl_history",
            ".python_history",
            ".rediscli_history",
            ".sqlite_history",
            ".dbshell",
            ".mongodb/mongosh/mongosh_repl_history",
            ".claude/.credentials.json",
            ".claude.json",
            ".codex/auth.json",
            ".gemini/oauth_creds.json",
            ".local/share/opencode/auth.json",
            ".config/github-copilot/hosts.json",
            ".config/openconnect/vpn.conf",
            ".gitlab-runner/config.toml",
            ".pgadmin/pgadmin4.db",
            "Postman/files/environment.json",
            ".config/google-chrome-beta/Default/Cookies",
            ".config/google-chrome-unstable/Default/Cookies",
            ".config/microsoft-edge/Default/Cookies",
            ".config/microsoft-edge-beta/Default/Cookies",
            ".config/microsoft-edge-dev/Default/Cookies",
            ".config/vivaldi/Default/Cookies",
            "snap/firefox/common/.mozilla/firefox/x/cookies.sqlite",
            "snap/chromium/common/chromium/Default/Cookies",
            "snap/brave/current/.config/BraveSoftware/Brave-Browser/Default/Cookies",
            ".var/app/org.mozilla.firefox/.mozilla/firefox/x/cookies.sqlite",
            ".thunderbird/x/logins.json",
            ".local/share/kwalletd/kdewallet.kwl",
            ".Xauthority",
            ".config/Slack/Cookies",
            ".config/discord/Local Storage/leveldb/000003.log",
            ".config/Signal/config.json",
            ".config/1Password/1password.sqlite",
            ".config/Code/User/globalStorage/state.vscdb",
            ".config/Code - Insiders/User/globalStorage/state.vscdb",
            ".config/VSCodium/User/globalStorage/state.vscdb",
            ".config/Cursor/User/globalStorage/state.vscdb",
            ".config/JetBrains/IntelliJIdea2025.1/options/security.xml",
            ".config/Insomnia/insomnia.Environment.db",
            ".config/Bruno/preferences.json",
            ".config/Postman/Partitions/x/Cookies",
            ".local/share/DBeaverData/workspace6/General/.dbeaver/credentials-config.json",
        ];
        let read = [
            ".gitconfig",
            ".config/gh/hosts.yml",
            ".config/other/state",
            ".local/share/other/state",
            ".npmrc",
            ".cargo/credentials.toml",
            ".pypirc",
            ".netrc",
            ".git-credentials",
            ".ssh/config",
            ".ssh/known_hosts",
            ".ssh/id_ed25519.pub",
            ".bravebotx/state",
            "docs/notes.txt",
            ".vault-token",
            ".yarnrc.yml",
            "snap/firefox/common/other/state",
            ".local/share/fish/fish_variables",
            ".mongodb/mongosh/snippets/state",
        ];
        for row in held_back.iter().chain(&read) {
            let file = home.join(row);
            std::fs::create_dir_all(file.parent().expect("a row has a parent"))
                .expect("the scratch home is creatable");
            std::fs::write(&file, CONTENTS).expect("the scratch home is writable");
        }
        let door = home.join("docs").join("door");
        std::os::unix::fs::symlink(home.join(".aws"), &door).expect("a link");
        let state_door = home.join("docs").join("state-door");
        std::os::unix::fs::symlink(home.join(".bravebot"), &state_door).expect("a link");
        let policy = crate::base::run_base(Prelude::Linux, &temporary_directory, Some(&home))
            .nameable_under(&sandbox.capabilities())
            .policy;
        let cat = |path: &Path| {
            let mut child = sandbox
                .spawn(
                    OsStr::new("/usr/bin/cat"),
                    &[path.display().to_string().into()],
                    &policy,
                    nothing_attached(),
                    Environment::Inherited,
                )
                .expect("should spawn");
            child.wait().expect("should wait").code()
        };

        for row in read {
            assert_eq!(cat(&home.join(row)), Some(0), "{row} was refused");
        }
        for row in held_back {
            assert_eq!(cat(&home.join(row)), Some(CAT_FAILED), "{row} was read");
        }
        assert_eq!(
            cat(&door.join("credentials")),
            Some(CAT_FAILED),
            "a link to a credential directory was followed"
        );
        assert_eq!(
            cat(&state_door.join("gateway-keys.json")),
            Some(CAT_FAILED),
            "a link to the state directory was followed"
        );
        assert_eq!(
            cat(&home.join(".kube-not-there/config")),
            Some(CAT_FAILED),
            "the read failed for a reason that is not a refusal"
        );

        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&temporary_directory);
    }

    /// The regression it rejects: a refusal carried out by listing that leaves the stage able
    /// to write where it was never given a row, or to run a program from the directories the
    /// listing granted whole.
    #[test]
    fn a_stage_under_the_run_base_runs_programs_and_writes_only_where_it_was_given() {
        let Some(sandbox) = sandbox_or_fail() else {
            return;
        };
        let temporary_directory = a_temporary_directory("bravebot-run-base-writes-tmp");
        let home = crate::testutil::scratch_dir("bravebot-run-base-writes");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("docs")).expect("the scratch home is creatable");
        let policy = crate::base::run_base(Prelude::Linux, &temporary_directory, Some(&home))
            .nameable_under(&sandbox.capabilities())
            .policy;
        let touch = |path: &Path| {
            let mut child = sandbox
                .spawn(
                    OsStr::new("/usr/bin/touch"),
                    &[path.display().to_string().into()],
                    &policy,
                    nothing_attached(),
                    Environment::Inherited,
                )
                .expect("a program starts under the run base");
            child.wait().expect("should wait").code()
        };

        assert_eq!(touch(&temporary_directory.join("made")), Some(0));
        assert_eq!(
            touch(&home.join("docs").join("made")),
            Some(TOUCH_FAILED),
            "a file was made outside the temporary directory"
        );

        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&temporary_directory);
    }

    /// The regression it rejects: a session opened on the home directory itself, whose write row
    /// is a Landlock grant of everything beneath it, reads included, so the credential directories
    /// refused to the read rows are handed back by the write row.
    #[test]
    fn a_write_row_over_the_home_does_not_hand_back_a_credential_location() {
        let Some(sandbox) = sandbox_or_fail() else {
            return;
        };
        let temporary_directory = a_temporary_directory("bravebot-run-base-home-root-tmp");
        let home = crate::testutil::scratch_dir("bravebot-run-base-home-root");
        let _ = std::fs::remove_dir_all(&home);
        for row in [".aws/credentials", ".ssh/id_ed25519", "docs/notes.txt"] {
            let file = home.join(row);
            std::fs::create_dir_all(file.parent().expect("a row has a parent"))
                .expect("the scratch home is creatable");
            std::fs::write(&file, CONTENTS).expect("the scratch home is writable");
        }
        let policy = crate::base::run_base(Prelude::Linux, &temporary_directory, Some(&home))
            .allow_write(&home)
            .nameable_under(&sandbox.capabilities())
            .policy;
        let cat = |path: &Path| {
            let mut child = sandbox
                .spawn(
                    OsStr::new("/usr/bin/cat"),
                    &[path.display().to_string().into()],
                    &policy,
                    nothing_attached(),
                    Environment::Inherited,
                )
                .expect("should spawn");
            child.wait().expect("should wait").code()
        };

        assert_eq!(cat(&home.join("docs/notes.txt")), Some(0));
        for row in [".aws/credentials", ".ssh/id_ed25519"] {
            assert_eq!(cat(&home.join(row)), Some(CAT_FAILED), "{row} was read");
        }

        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&temporary_directory);
    }

    /// The regression it rejects: a refusal of writes that Landlock cannot express and the
    /// backend therefore drops, which leaves a file the person kept read-only writable inside a
    /// directory the session may write. The file is still read, and its neighbours are still
    /// written.
    #[test]
    fn a_stage_is_refused_a_write_the_policy_refuses_and_keeps_the_rest() {
        let Some(sandbox) = sandbox_or_fail() else {
            return;
        };
        let work = crate::testutil::scratch_dir("bravebot-landlock-unwritable");
        let _ = std::fs::remove_dir_all(&work);
        std::fs::create_dir_all(work.join("sub")).expect("the scratch directory is creatable");
        std::fs::write(work.join(".env"), CONTENTS).expect("the file is writable");
        std::fs::write(work.join("sub/other"), CONTENTS).expect("the file is writable");
        let policy = crate::base::run_base(
            Prelude::Linux,
            &a_temporary_directory("bravebot-landlock-unwritable-tmp"),
            None,
        )
        .allow_write(&work)
        .deny_write(work.join(".env"))
        .nameable_under(&sandbox.capabilities())
        .policy;
        let run = |program: &str, args: &[String]| {
            let args: Vec<OsString> = args.iter().map(OsString::from).collect();
            let mut child = sandbox
                .spawn(
                    OsStr::new(program),
                    &args,
                    &policy,
                    nothing_attached(),
                    Environment::Inherited,
                )
                .expect("should spawn");
            child.wait().expect("should wait").code()
        };
        // An append opens for writing, which is the right a refusal withholds; `touch` on a file
        // that is there changes only its times, which Landlock does not govern.
        let append = |path: &Path| {
            run(
                "/bin/sh",
                &[
                    "-c".to_string(),
                    "echo more >> \"$0\"".to_string(),
                    path.display().to_string(),
                ],
            )
        };

        assert_ne!(append(&work.join(".env")), Some(0));
        assert_eq!(append(&work.join("sub/other")), Some(0));
        assert_eq!(
            run("/usr/bin/cat", &[work.join(".env").display().to_string()]),
            Some(0),
            "a refusal of writes took the read with it"
        );
        let _ = std::fs::remove_dir_all(&work);
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
                    OsStr::new("/bin/true"),
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

    /// A policy with a refusal beneath `top`, which is what makes a caller list the directories
    /// above the refusal and pass each entry as a row of its own.
    fn a_policy_with_a_refusal_beneath(top: &Path) -> SandboxPolicy {
        loadable_policy()
            .allow_read(top)
            .deny_read(top.join("refused"))
    }

    /// A directory another process is changing has entries that are gone by the time a row for
    /// each is opened, and a stage refused for that is refused over a path nobody named. A name
    /// under something that has stopped being a directory is as gone as a name that was removed.
    /// A listed row that is still there is granted as before: leaving every listed row out would
    /// pass the first assertion and fail the write. The row whose name is not UTF-8 is there
    /// beside the row spelled with its replacement character, which is not, so an implementation
    /// that opened the lossy spelling would take the first for the second.
    #[test]
    fn a_listed_row_whose_name_went_away_is_left_out_and_the_ones_still_there_are_granted() {
        use std::os::unix::ffi::OsStrExt;

        let Some(sandbox) = sandbox_or_fail() else {
            return;
        };
        let top = a_temporary_directory("bravebot-landlock-listed-row-gone");
        std::fs::create_dir_all(top.join("refused")).expect("the refused directory is creatable");
        std::fs::create_dir_all(top.join("kept")).expect("the kept directory is creatable");
        let not_utf8 = top.join(OsStr::from_bytes(b"kept-\xff"));
        std::fs::create_dir_all(&not_utf8).expect("the not UTF-8 directory is creatable");
        std::fs::write(top.join("now-a-file"), "").expect("the file is writable");
        let policy = a_policy_with_a_refusal_beneath(&top)
            .allow_write_listed(top.join("kept"))
            .allow_write_listed(&not_utf8)
            .allow_write_listed(top.join("listed-then-removed"))
            .allow_write_listed(top.join("kept-\u{FFFD}"))
            .allow_write_listed(top.join("now-a-file").join("entry"));
        let written = [top.join("kept"), not_utf8].map(|directory| directory.join("written"));

        for written in &written {
            let mut child = sandbox
                .spawn(
                    OsStr::new("/usr/bin/touch"),
                    &[written.clone().into_os_string()],
                    &policy,
                    nothing_attached(),
                    Environment::Inherited,
                )
                .expect("a listed name that is gone leaves nothing to grant, so the stage starts");

            assert_eq!(
                child.wait().expect("should wait").code(),
                Some(0),
                "the listed row that is there was not granted: {written:?}"
            );
            assert!(written.exists(), "the write did not land: {written:?}");
        }

        let _ = std::fs::remove_dir_all(&top);
    }

    /// Only a row a listing produced may be left out. The same path named by the caller, in the
    /// same policy with the same refusal, is a grant the caller asked for and is refused
    /// (SANDBOX-6), so a backend that ignored every missing path whenever a policy lists would pass
    /// the test above and fail this one. The caller also names a path that a listing produced as
    /// well, as the whole-machine rows of a session can include a path it holds already, and the
    /// named row is still the one that refuses.
    #[test]
    fn a_row_the_caller_named_is_refused_when_it_went_away_in_a_policy_that_lists() {
        let top = a_temporary_directory("bravebot-landlock-named-row-gone");
        std::fs::create_dir_all(top.join("refused")).expect("the refused directory is creatable");
        let gone = top.join("named-then-removed");

        for policy in [
            a_policy_with_a_refusal_beneath(&top).allow_write(&gone),
            a_policy_with_a_refusal_beneath(&top)
                .allow_write(&gone)
                .allow_write_listed(&gone),
            a_policy_with_a_refusal_beneath(&top)
                .allow_write_listed(&gone)
                .allow_write(&gone),
        ] {
            let err = LandlockSandbox
                .spawn(
                    OsStr::new("/bin/true"),
                    &[],
                    &policy,
                    nothing_attached(),
                    Environment::Inherited,
                )
                .expect_err("a named row that is gone must refuse the policy");
            match err {
                SandboxError::SetupFailed { mechanism, detail } => {
                    assert_eq!(mechanism, "landlock");
                    assert!(
                        detail.contains(&gone.display().to_string()),
                        "the refusal has to name the path: {detail}"
                    );
                }
                other => panic!("expected SetupFailed naming the path, got: {other:?}"),
            }
        }

        let _ = std::fs::remove_dir_all(&top);
    }

    /// A listed row is left out because its name is not there, and for no other reason. One that
    /// is there and cannot be opened is a grant this backend cannot install, which is refused like
    /// any other. A pair of links that lead to each other cannot be opened by any account, so the
    /// row is there and unopenable without depending on a mode that root ignores.
    #[test]
    fn a_listed_row_that_is_there_and_cannot_be_opened_is_refused() {
        let top = a_temporary_directory("bravebot-landlock-listed-row-looping");
        std::fs::create_dir_all(top.join("refused")).expect("the refused directory is creatable");
        let looping = top.join("looping");
        std::os::unix::fs::symlink(top.join("other-end"), &looping).expect("the link is creatable");
        std::os::unix::fs::symlink(&looping, top.join("other-end")).expect("the link is creatable");

        let err = LandlockSandbox
            .spawn(
                OsStr::new("/bin/true"),
                &[],
                &a_policy_with_a_refusal_beneath(&top).allow_write_listed(&looping),
                nothing_attached(),
                Environment::Inherited,
            )
            .expect_err("a listed row that cannot be opened must refuse the policy");

        match err {
            SandboxError::SetupFailed { mechanism, detail } => {
                assert_eq!(mechanism, "landlock");
                assert!(
                    detail.contains(&looping.display().to_string()),
                    "the refusal has to name the path: {detail}"
                );
            }
            other => panic!("expected SetupFailed naming the path, got: {other:?}"),
        }

        let _ = std::fs::remove_dir_all(&top);
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
                OsStr::new("/bin/true"),
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
                OsStr::new("/bin/true"),
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
                OsStr::new("/bin/true"),
                &[],
                &loadable_policy().allow_write(&dir),
                nothing_attached(),
                Environment::Inherited,
            )
            .expect("a grant naming a path that is there is one this backend installs");
        assert!(confined.wait().expect("should wait").success());

        let started = sandbox.spawn(
            OsStr::new("/bin/true"),
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
                OsStr::new("/usr/bin/mv"),
                &[
                    source.display().to_string().into(),
                    destination.display().to_string().into(),
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
                    OsStr::new("/usr/bin/python3"),
                    &[
                        "-c".into(),
                        "import os, sys; os.truncate(sys.argv[1], 0)".into(),
                        target.display().to_string().into(),
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
                    OsStr::new("/usr/bin/python3"),
                    &["-c".into(), request.into()],
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

    /// An abstract socket has no path, so no filesystem grant names it and none can withhold
    /// it: the session bus, the X server and every daemon that listens in the abstract
    /// namespace are reachable from a confined process unless the ruleset scopes the connect.
    ///
    /// The listener belongs to this test process, which is outside the confined domain. Three
    /// connects run from one witness: an unconfined run reaches the listener, which shows the
    /// witness and the address are sound; the confined run first connects to a listener it
    /// made itself, which shows the scope leaves its own domain alone and that sockets as a
    /// whole were not refused; and then to the one outside, which is the denial. A ruleset
    /// that scopes nothing reaches the listener, and one that refuses every socket fails the
    /// control, so neither passes.
    #[test]
    fn a_confined_process_cannot_connect_to_an_abstract_socket_made_outside_it() {
        use std::os::linux::net::SocketAddrExt;
        use std::os::unix::net::{SocketAddr, UnixListener};

        /// The witness's exit codes. A script that stopped for a reason of its own exits with
        /// 1 or by signal, and neither says anything about a scope.
        const REACHED: i32 = 0;
        const REFUSED: i32 = 3;
        const OWN_SOCKET_UNREACHABLE: i32 = 4;
        const FAILED_FOR_ANOTHER_REASON: i32 = 5;

        let Some(sandbox) = sandbox_or_fail() else {
            return;
        };
        if !kernel_governs(
            "a connect to an abstract socket",
            SCOPE_ABI_VERSION,
            "6.12+",
        ) {
            return;
        }

        let name = format!("bravebot-landlock-scope-{}", std::process::id());
        let outside = UnixListener::bind_addr(
            &SocketAddr::from_abstract_name(name.as_bytes()).expect("an abstract address"),
        )
        .expect("this process can listen on an abstract address");

        let witness = "import socket, sys\n\
                       own = socket.socket(socket.AF_UNIX)\n\
                       own.bind(b'\\0' + sys.argv[1].encode() + b'-own')\n\
                       own.listen(1)\n\
                       try:\n\
                       \x20   socket.socket(socket.AF_UNIX).connect(b'\\0' + sys.argv[1].encode() + b'-own')\n\
                       except OSError:\n\
                       \x20   sys.exit(4)\n\
                       try:\n\
                       \x20   socket.socket(socket.AF_UNIX).connect(b'\\0' + sys.argv[1].encode())\n\
                       except PermissionError:\n\
                       \x20   sys.exit(3)\n\
                       except OSError:\n\
                       \x20   sys.exit(5)\n";
        let arguments =
            || -> Vec<OsString> { vec!["-c".into(), witness.into(), name.clone().into()] };

        let unconfined = Command::new("/usr/bin/python3")
            .args(arguments())
            .env_clear()
            .status()
            .expect("python runs")
            .code();
        assert_eq!(
            unconfined,
            Some(REACHED),
            "the witness did not reach a listener nothing confines, so the refusal below means \
             nothing"
        );

        let confined = sandbox
            .spawn(
                OsStr::new("/usr/bin/python3"),
                &arguments(),
                &loadable_policy(),
                nothing_attached(),
                Environment::Empty,
            )
            .expect("python starts under the policy")
            .wait()
            .expect("should wait")
            .code();
        assert_ne!(
            confined,
            Some(OWN_SOCKET_UNREACHABLE),
            "the confined process could not reach a socket it made itself, so the scope refuses \
             more than the sockets made outside it"
        );
        assert_ne!(
            confined,
            Some(FAILED_FOR_ANOTHER_REASON),
            "the connect to the outside socket failed, but not with a refusal"
        );
        assert_eq!(
            confined,
            Some(REFUSED),
            "the abstract socket made outside the confined process was reachable, or the \
             witness stopped for a reason of its own"
        );
        drop(outside);
    }

    /// A confined process leads a session of its own, so it has no controlling terminal to push
    /// keystrokes into on a kernel where Landlock does not govern an ioctl on a device.
    ///
    /// The regression it rejects: starting the program in the caller's session, where its session
    /// id is the caller's and not its own process id. Both ways of starting a program are read,
    /// since a stage is started through the command and a server through the spawn.
    #[test]
    fn a_confined_process_leads_a_session_of_its_own() {
        let Some(sandbox) = sandbox_or_fail() else {
            return;
        };
        let leads_its_session = "import os, sys\n\
                                 sys.exit(0 if os.getsid(0) == os.getpid() else 1)\n";
        let arguments = ["-c".into(), leads_its_session.into()];

        let through_the_command = sandbox
            .command(
                OsStr::new("/usr/bin/python3"),
                &arguments,
                &loadable_policy(),
                &Environment::Inherited,
            )
            .expect("a command")
            .stdin(std::process::Stdio::null())
            .status()
            .expect("spawned")
            .code();
        let through_the_spawn = sandbox
            .spawn(
                OsStr::new("/usr/bin/python3"),
                &arguments,
                &loadable_policy(),
                nothing_attached(),
                Environment::Inherited,
            )
            .expect("python is what reads the session id here")
            .wait()
            .expect("should wait")
            .code();

        assert_eq!(through_the_command, Some(0), "a command's program");
        assert_eq!(through_the_spawn, Some(0), "a spawned program");
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
                OsStr::new("/usr/bin/truncate"),
                &["-s".into(), "0".into(), target.display().to_string().into()],
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
                OsStr::new("/usr/bin/env"),
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
                OsStr::new("/bin/pwd"),
                &["-P".into()],
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
                OsStr::new("/usr/bin/env"),
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
