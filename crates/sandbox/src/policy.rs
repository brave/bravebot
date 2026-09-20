//! What a confined process is allowed to do, and how strongly that is enforced.

use std::fmt;
use std::path::{Path, PathBuf};

/// The confinement a process should run under.
///
/// Deny-by-default: [`SandboxPolicy::strict`] permits nothing, and each allowance is
/// added explicitly. The alternative, starting permissive and subtracting, means a
/// forgotten subtraction silently grants access.
#[derive(Debug, Clone)]
pub struct SandboxPolicy {
    /// Directories the process may read. Empty means no filesystem reads.
    pub readable: Vec<PathBuf>,
    /// Directories the process may write. Empty means no filesystem writes.
    pub writable: Vec<PathBuf>,
    /// Whether the process may open sockets.
    ///
    /// Normally false. Inference is brokered through the parent, so a confined
    /// process needs no network of its own, and without a socket, an instruction to
    /// exfiltrate data has nowhere to send it.
    pub allow_network: bool,
    /// Whether the process may spawn children. False stops a confined process from
    /// launching an unconfined helper.
    pub allow_subprocesses: bool,
}

impl SandboxPolicy {
    /// Permits nothing: no filesystem, no network, no children.
    pub fn strict() -> Self {
        Self {
            readable: Vec::new(),
            writable: Vec::new(),
            allow_network: false,
            allow_subprocesses: false,
        }
    }

    pub fn allow_read(mut self, path: impl Into<PathBuf>) -> Self {
        self.readable.push(path.into());
        self
    }

    pub fn allow_write(mut self, path: impl Into<PathBuf>) -> Self {
        self.writable.push(path.into());
        self
    }

    /// Permit network access. Named to be conspicuous in review, because granting it
    /// removes the property that makes exfiltration structurally impossible.
    pub fn allow_network_egress(mut self) -> Self {
        self.allow_network = true;
        self
    }

    pub fn allow_subprocesses(mut self) -> Self {
        self.allow_subprocesses = true;
        self
    }

    /// The part of this policy a backend can be asked to name, and what that left out.
    ///
    /// A caller assembling a policy out of paths whose existence is not its own to decide,
    /// a base every program gets and a list keyed on the tool a program turns out to be,
    /// names more paths than any one machine carries. A backend that cannot grant a path
    /// missing from the disk refuses the whole policy rather than confining a process to
    /// the rest of it, so on such a backend an unresolved policy is a program refused over
    /// a toolchain the machine never had.
    ///
    /// Left out rather than rescued: naming the directory holding an absent path grants
    /// over every other file in it, and creating the file writes where nothing asked for a
    /// write, having first guessed whether the row names a file or a directory.
    ///
    /// Nothing is added, nothing moves between the two lists, and the network and
    /// subprocess grants are carried as they were, so what comes back is a subset of what
    /// was wanted and never more.
    pub fn nameable_under(&self, capabilities: &Capabilities) -> Resolution {
        if capabilities.grants_paths_that_do_not_exist {
            return Resolution {
                policy: self.clone(),
                omitted: Vec::new(),
            };
        }

        let mut omitted = Vec::new();
        let readable = keep_the_paths_that_are_there(&self.readable, &mut omitted);
        let writable = keep_the_paths_that_are_there(&self.writable, &mut omitted);

        Resolution {
            policy: Self {
                readable,
                writable,
                allow_network: self.allow_network,
                allow_subprocesses: self.allow_subprocesses,
            },
            omitted,
        }
    }

    /// Whether this policy would confine anything at all.
    ///
    /// A policy granting network, subprocesses, and write access to `/` is not
    /// confinement; treating it as such would be the sort of accident that makes a
    /// sandbox decorative.
    pub fn is_meaningful(&self) -> bool {
        !self.allow_network
            || !self.allow_subprocesses
            || !self.writable.iter().any(|p| p.as_path() == Path::new("/"))
    }
}

/// What a wanted policy came to once the paths a backend cannot name were taken out.
#[derive(Debug, Clone)]
pub struct Resolution {
    /// The policy to hand to the backend.
    pub policy: SandboxPolicy,
    /// Every wanted path left out of it, each named once, readable rows first.
    ///
    /// Reported rather than dropped quietly: a program held to fewer paths than somebody
    /// granted it fails over a file it was meant to reach, and a caller that cannot say
    /// which path went has nothing to tell them.
    pub omitted: Vec<PathBuf>,
}

/// The rows of `paths` that are on disk, appending the rest to `omitted`.
///
/// Existence is read through the symlink, since a grant is installed on what a path opens
/// and a dangling one opens nothing.
fn keep_the_paths_that_are_there(paths: &[PathBuf], omitted: &mut Vec<PathBuf>) -> Vec<PathBuf> {
    let mut kept = Vec::with_capacity(paths.len());
    for path in paths {
        if path.exists() {
            kept.push(path.clone());
        } else if !omitted.contains(path) {
            omitted.push(path.clone());
        }
    }
    kept
}

/// How much confinement was actually achieved.
///
/// Reported rather than assumed: the guarantee genuinely differs across platforms, and
/// claiming a single uniform level would misrepresent the weakest one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfinementLevel {
    /// Kernel-enforced filesystem and network restrictions.
    Kernel,
    /// Restrictions enforced, but coarser than [`ConfinementLevel::Kernel`].
    Partial,
    /// No OS-level confinement available.
    None,
}

impl fmt::Display for ConfinementLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Kernel => f.write_str("kernel-enforced"),
            Self::Partial => f.write_str("partial"),
            Self::None => f.write_str("none"),
        }
    }
}

/// What a platform backend can actually deliver, so a caller can tell a user what they
/// got instead of implying every platform is equal.
#[derive(Debug, Clone)]
pub struct Capabilities {
    pub level: ConfinementLevel,
    /// Mechanisms in use, for the audit trail.
    pub mechanisms: Vec<&'static str>,
    /// Whether network denial is enforced by the kernel rather than by convention.
    pub network_denial_enforced: bool,
    /// Whether a grant may name a path that does not exist yet.
    ///
    /// A caller assembling a policy out of paths whose existence is not its own to decide
    /// has three answers for one that is absent, and each costs something: creating the
    /// file writes where nothing was asked for, naming the directory holding it grants
    /// wider than the path, and leaving the grant out refuses the program a path somebody
    /// meant it to have. Which of the three is necessary is what this reports, asked of the
    /// backend rather than of the platform the build targets: a caller reading the platform
    /// instead pays one of those costs on the platform where neither was necessary.
    pub grants_paths_that_do_not_exist: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_permits_nothing() {
        let policy = SandboxPolicy::strict();
        assert!(policy.readable.is_empty());
        assert!(policy.writable.is_empty());
        assert!(!policy.allow_network);
        assert!(!policy.allow_subprocesses);
    }

    #[test]
    fn allowances_accumulate() {
        let policy = SandboxPolicy::strict()
            .allow_read("/workspace")
            .allow_write("/workspace/out");
        assert_eq!(policy.readable, vec![PathBuf::from("/workspace")]);
        assert_eq!(policy.writable, vec![PathBuf::from("/workspace/out")]);
    }

    #[test]
    fn a_strict_policy_is_meaningful() {
        assert!(SandboxPolicy::strict().is_meaningful());
    }

    /// Granting everything is not confinement, and must not be mistaken for it.
    #[test]
    fn granting_everything_is_not_meaningful() {
        let policy = SandboxPolicy::strict()
            .allow_network_egress()
            .allow_subprocesses()
            .allow_write("/");
        assert!(!policy.is_meaningful());
    }

    /// Network alone is still confinement if the filesystem stays restricted.
    #[test]
    fn network_alone_remains_meaningful() {
        let policy = SandboxPolicy::strict()
            .allow_network_egress()
            .allow_read("/workspace");
        assert!(policy.is_meaningful());
    }

    /// A directory this checkout always has, so a test about an absent path is not
    /// quietly a test about a machine that happens to lack something.
    fn a_path_that_is_there() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")
    }

    fn a_path_that_is_not_there() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("no-such-entry-in-this-crate")
    }

    fn capabilities(grants_paths_that_do_not_exist: bool) -> Capabilities {
        Capabilities {
            level: ConfinementLevel::Kernel,
            mechanisms: vec!["a mechanism"],
            network_denial_enforced: true,
            grants_paths_that_do_not_exist,
        }
    }

    /// Resolution exists to pay the cost of an absent path only where a backend makes
    /// somebody pay it. A caller that stats regardless takes a grant away on the platform
    /// where the grant was installable, which is the cost the capability was added to
    /// avoid.
    #[test]
    fn a_backend_that_grants_an_absent_path_is_asked_for_the_policy_as_wanted() {
        let wanted = SandboxPolicy::strict()
            .allow_read(a_path_that_is_not_there())
            .allow_write(a_path_that_is_not_there());

        let resolved = wanted.nameable_under(&capabilities(true));

        assert_eq!(resolved.policy.readable, wanted.readable);
        assert_eq!(resolved.policy.writable, wanted.writable);
        assert!(
            resolved.omitted.is_empty(),
            "a backend that can name an absent path was told a path went missing"
        );
    }

    /// A profile is assembled from lists that name more paths than any one machine
    /// carries, so a backend refusing a policy over one of them refuses every program on a
    /// machine that lacks a toolchain some list knows. What the absent path must not do is
    /// take a path that is there with it.
    #[test]
    fn a_path_that_is_not_on_disk_is_left_out_and_named() {
        let wanted = SandboxPolicy::strict()
            .allow_read(a_path_that_is_there())
            .allow_read(a_path_that_is_not_there());

        let resolved = wanted.nameable_under(&capabilities(false));

        assert_eq!(resolved.policy.readable, vec![a_path_that_is_there()]);
        assert_eq!(resolved.omitted, vec![a_path_that_is_not_there()]);
    }

    /// Resolution decides what the policy names, so a path it keeps has to reach the
    /// backend saying what it said before. One that read a path into the writable list
    /// would grant a write nobody asked for, and the policy would still look like the one
    /// that was wanted.
    #[test]
    fn a_path_that_is_there_stays_in_the_list_it_was_named_in() {
        let wanted = SandboxPolicy::strict().allow_write(a_path_that_is_there());

        let resolved = wanted.nameable_under(&capabilities(false));

        assert!(
            resolved.policy.readable.is_empty(),
            "a path wanted for writing became readable"
        );
        assert_eq!(resolved.policy.writable, vec![a_path_that_is_there()]);
    }

    /// The two grants that are not paths are the ones resolution has no business touching,
    /// and either direction of getting them wrong is a policy that is not the one a caller
    /// decided on: granting egress it withheld, or withholding what a stage needs while the
    /// record says the policy was applied.
    #[test]
    fn resolution_carries_the_network_and_subprocess_grants_unchanged() {
        let granted = SandboxPolicy::strict()
            .allow_read(a_path_that_is_there())
            .allow_network_egress()
            .allow_subprocesses()
            .nameable_under(&capabilities(false))
            .policy;
        assert!(granted.allow_network);
        assert!(granted.allow_subprocesses);

        let withheld = SandboxPolicy::strict()
            .allow_read(a_path_that_is_there())
            .nameable_under(&capabilities(false))
            .policy;
        assert!(!withheld.allow_network);
        assert!(!withheld.allow_subprocesses);
    }

    /// What the caller has to tell somebody is which path their program will not reach, and
    /// a path named twice because two lists wanted it is one path they cannot act on twice.
    #[test]
    fn a_path_wanted_for_reading_and_for_writing_is_named_once_when_it_is_left_out() {
        let wanted = SandboxPolicy::strict()
            .allow_read(a_path_that_is_not_there())
            .allow_write(a_path_that_is_not_there());

        let resolved = wanted.nameable_under(&capabilities(false));

        assert_eq!(resolved.omitted, vec![a_path_that_is_not_there()]);
        assert!(resolved.policy.readable.is_empty());
        assert!(resolved.policy.writable.is_empty());
    }

    #[test]
    fn confinement_levels_render_for_the_audit_trail() {
        assert_eq!(ConfinementLevel::Kernel.to_string(), "kernel-enforced");
        assert_eq!(ConfinementLevel::None.to_string(), "none");
    }
}
