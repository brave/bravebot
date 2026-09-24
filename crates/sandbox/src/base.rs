//! What every program a person asked for reaches, before its plan is read.
//!
//! A profile denies everything and then names what may be reached, so "everything except this
//! key" is not a profile anybody can write, and something has to carry what every program needs
//! before any plan is read: a dynamic executable cannot start without its loader, and a program
//! that cannot open a temporary file fails outright. That list is the base. It is the same for
//! every plan, it is here rather than assembled from anything a run produces, and it changes in
//! a diff somebody reviews.
//!
//! What it buys is what it leaves out. No directory holding a credential is in it and neither is
//! the home directory, so the key a push signs with and the token a publish uses are out of
//! reach of a program whose plan never named them. `docs/specs/sandboxing.md` is where the rows
//! are decided, row by row, and this is that table in code.
//!
//! Nothing here reads the machine. The two rows that are not fixed, the temporary directory the
//! session resolved as it opened and the account's home directory, are handed in by the caller
//! that resolved them, so what this produces is the same list from the same arguments on every
//! machine and every platform.

use crate::policy::SandboxPolicy;
use std::path::{Path, PathBuf};

/// Which platform's prelude a base holds.
///
/// A parameter rather than a compile-time branch, so what a program on each platform is given is
/// decided by code every job that runs the suite exercises, rather than by the one job compiled
/// for that platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prelude {
    Linux,
    MacOs,
}

impl Prelude {
    /// The prelude for the platform this is running on, or `None` where what a program needs to
    /// start there has not been written down.
    ///
    /// A platform with no prelude has no base, and a caller with no base has no profile to
    /// confine anything against. That is a refusal for whoever asks rather than a program run
    /// under a list that was never decided.
    pub fn current() -> Option<Self> {
        #[cfg(target_os = "linux")]
        {
            Some(Self::Linux)
        }

        #[cfg(target_os = "macos")]
        {
            Some(Self::MacOs)
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            None
        }
    }

    /// The paths this platform's programs read before any plan is read.
    fn rows(self) -> &'static [&'static str] {
        match self {
            Self::Linux => LINUX_PRELUDE,
            Self::MacOs => MACOS_PRELUDE,
        }
    }
}

/// What a Linux program reads before any plan is read.
///
/// The loader and the system libraries, the system binary directories, the locale data,
/// terminfo, the certificates a TLS client reads with the directory holding them, the files a
/// host lookup and a user lookup read, and the devices. Each entry is one of the rows the base
/// table names, spelled the way this platform spells it, and a spelling a given distribution
/// does not use is left out when the policy is resolved rather than refusing the program.
const LINUX_PRELUDE: &[&str] = &[
    "/lib",
    "/lib64",
    "/usr/lib",
    "/usr/lib64",
    "/bin",
    "/sbin",
    "/usr/bin",
    "/usr/sbin",
    "/usr/share/locale",
    "/usr/share/terminfo",
    "/etc/terminfo",
    "/usr/share/zoneinfo",
    "/etc/localtime",
    "/etc/ssl/certs",
    "/etc/ssl/cert.pem",
    "/etc/pki/tls/certs",
    "/etc/pki/tls/cert.pem",
    "/etc/hosts",
    "/etc/resolv.conf",
    "/etc/nsswitch.conf",
    "/etc/passwd",
    "/etc/group",
    "/dev/null",
    "/dev/zero",
    "/dev/random",
    "/dev/urandom",
];

/// What a macOS program reads before any plan is read.
///
/// The same rows as the Linux prelude, spelled the way this platform spells them: the dynamic
/// linker and the libraries beside it, the frameworks, and the two places the shared cache those
/// frameworks are read out of sits. The configuration rows are written as `/private/etc`, which
/// is where this platform keeps them, because the backend here matches the path a grant is
/// written on against the one a program opens after the link has been followed, so a row saying
/// `/etc` reaches nothing and is granted in silence.
const MACOS_PRELUDE: &[&str] = &[
    "/usr/lib",
    "/System/Library",
    "/System/Volumes/Preboot/Cryptexes/OS",
    "/bin",
    "/sbin",
    "/usr/bin",
    "/usr/sbin",
    "/usr/share/locale",
    "/usr/share/terminfo",
    "/usr/share/zoneinfo",
    "/private/etc/localtime",
    "/private/etc/ssl/certs",
    "/private/etc/ssl/cert.pem",
    "/private/etc/hosts",
    "/private/etc/resolv.conf",
    "/private/etc/passwd",
    "/private/etc/group",
    "/dev/null",
    "/dev/zero",
    "/dev/random",
    "/dev/urandom",
];

/// The one path in the prelude a program may write.
///
/// Discarding output is what every pipeline and every build does, and a program that cannot
/// write here fails on a path it was granted. Nothing else the prelude names is writable: those
/// are the machine's own directories, and the certificates among them decide what every TLS
/// client on it trusts.
const THE_NULL_DEVICE: &str = "/dev/null";

/// The rows every program a person asked for gets, before its plan is read.
///
/// `temporary_directory` is the system temporary directory this process resolved as the session
/// opened, and not one a stage's own environment assignment names, so a line carrying a
/// `TMPDIR=` assignment changes where a program writes without changing what the profile allows.
/// It arrives with its links followed, since a backend matching a grant against the path a
/// program opens grants nothing for the name a link is reached by.
///
/// `home` is the account's home directory where the machine has one. It is in no row itself:
/// what it contributes is the two spellings of the git configuration, and a machine without one
/// gets the rest of the base rather than a program that cannot start.
///
/// Egress and children are left to the plan. What the base bounds is the filesystem: a profile
/// gates egress as a whole and so cannot tell an approved `git push` from an exfiltration, which
/// the endorsed argv can, and a policy denying children is one two of the three backends refuse
/// outright rather than apply, so a base asking for that denial is every program refused on
/// them.
pub fn base(prelude: Prelude, temporary_directory: &Path, home: Option<&Path>) -> SandboxPolicy {
    let mut policy = SandboxPolicy::strict()
        .allow_network_egress()
        .allow_subprocesses();

    for path in prelude.rows() {
        policy = policy.allow_read(*path);
    }

    // Neither write row says what is at the path it names, so nothing creates either of them.
    // The session resolved the temporary directory as it opened, and what is at the other one
    // is the kernel's rather than a file: a row saying which of the two it is would be a
    // regular file standing in for the null device on a machine that somehow lacked it.
    policy = policy
        .allow_read(temporary_directory)
        .allow_write(temporary_directory)
        .allow_write(THE_NULL_DEVICE);

    if let Some(home) = home {
        for path in git_configuration(home) {
            policy = policy.allow_read(path);
        }
    }

    policy
}

/// The git configuration any stage may read for an identity, in both spellings.
///
/// In the base rather than in one program's list because a stage that never mentions `git` still
/// shells out to it for an identity, a `cargo` fetching a git dependency among them, and the
/// file can name a credential store without holding one. Named one file at a time: `~/.config`
/// holds a credential store of its own, so the directory the second spelling sits in is not a
/// row.
fn git_configuration(home: &Path) -> [PathBuf; 2] {
    [
        home.join(".gitconfig"),
        home.join(".config").join("git").join("config"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{Capabilities, ConfinementLevel, PathKind};
    use crate::testutil::scratch_dir;

    /// A home directory and a session's temporary directory as paths rather than as directories
    /// on disk, since a base is assembled out of the two and reads neither.
    const A_HOME: &str = "/home/a-person";
    const THE_SESSIONS_TEMPORARY_DIRECTORY: &str = "/scratch/tmp-of-this-session";

    const BOTH_PLATFORMS: [Prelude; 2] = [Prelude::Linux, Prelude::MacOs];

    fn a_base(prelude: Prelude) -> SandboxPolicy {
        base(
            prelude,
            Path::new(THE_SESSIONS_TEMPORARY_DIRECTORY),
            Some(Path::new(A_HOME)),
        )
    }

    /// Every path the policy names, whichever list it is in, since a program reaches what either
    /// one grants.
    fn granted_paths(policy: &SandboxPolicy) -> Vec<PathBuf> {
        policy
            .readable
            .iter()
            .cloned()
            .chain(policy.writable.iter().map(|row| row.path.clone()))
            .collect()
    }

    fn written_paths(policy: &SandboxPolicy) -> Vec<PathBuf> {
        policy.writable.iter().map(|row| row.path.clone()).collect()
    }

    /// Whether a row of this policy would let a program open `path`, which a grant on any
    /// directory above it does.
    fn reaches(policy: &SandboxPolicy, path: &str) -> bool {
        granted_paths(policy)
            .iter()
            .any(|row| Path::new(path).starts_with(row))
    }

    fn a_backend_that_cannot_name_an_absent_path() -> Capabilities {
        Capabilities {
            level: ConfinementLevel::Kernel,
            mechanisms: vec!["a mechanism"],
            network_denial_enforced: true,
            grants_paths_that_do_not_exist: false,
        }
    }

    /// The whole of what the base buys is what it leaves out. A row added to it for convenience,
    /// the home directory itself or the `~/.config` the XDG git configuration sits in, hands the
    /// key a push signs with and the token a publish uses to a program whose plan named neither.
    #[test]
    fn the_base_reaches_no_credential() {
        for prelude in BOTH_PLATFORMS {
            let policy = a_base(prelude);
            for credential in [
                "/home/a-person/.ssh/id_rsa",
                "/home/a-person/.aws/credentials",
                "/home/a-person/.npmrc",
                "/home/a-person/.pypirc",
                "/home/a-person/.netrc",
                "/home/a-person/.git-credentials",
                "/home/a-person/.cargo/credentials.toml",
                "/home/a-person/.m2/settings.xml",
                "/home/a-person/.gradle/gradle.properties",
                "/home/a-person/.config/gh/hosts.yml",
            ] {
                assert!(
                    !reaches(&policy, credential),
                    "the {prelude:?} base reaches {credential}"
                );
            }
        }
    }

    /// A base naming the home directory is every file in it granted at once, and the rows a plan
    /// carries are what decide which of them a given program reaches.
    #[test]
    fn the_only_rows_under_a_home_directory_are_the_git_configuration() {
        for prelude in BOTH_PLATFORMS {
            let policy = a_base(prelude);
            let under_a_home: Vec<PathBuf> = granted_paths(&policy)
                .into_iter()
                .filter(|path| path.starts_with(A_HOME))
                .collect();
            assert_eq!(
                under_a_home,
                vec![
                    PathBuf::from("/home/a-person/.gitconfig"),
                    PathBuf::from("/home/a-person/.config/git/config"),
                ],
                "the {prelude:?} base names something else of a person's"
            );
        }
    }

    /// The configuration is in the base so that a stage nobody asked about can read an identity
    /// out of it. Writing it is a different thing: a program that can write this file names the
    /// command `git` runs for a push in it.
    #[test]
    fn the_git_configuration_is_read_and_never_written() {
        let policy = a_base(Prelude::Linux);

        assert!(
            policy
                .readable
                .contains(&PathBuf::from("/home/a-person/.gitconfig"))
        );
        assert!(
            policy
                .readable
                .contains(&PathBuf::from("/home/a-person/.config/git/config"))
        );
        assert!(
            written_paths(&policy)
                .iter()
                .all(|path| !path.starts_with(A_HOME)),
            "a base grants writing somewhere under a home directory"
        );
    }

    /// The git rows are the only part of the base a home directory decides, so a machine with
    /// none of one still gets a profile a program can start under.
    #[test]
    fn a_machine_with_no_home_directory_gets_the_rest_of_the_base() {
        let policy = base(
            Prelude::Linux,
            Path::new(THE_SESSIONS_TEMPORARY_DIRECTORY),
            None,
        );

        assert!(reaches(&policy, "/usr/lib/libc.so.6"));
        assert!(reaches(&policy, THE_SESSIONS_TEMPORARY_DIRECTORY));
        assert!(
            granted_paths(&policy)
                .iter()
                .all(|path| !path.starts_with("/home")),
            "a base with no home directory named one anyway"
        );
    }

    /// A base resolving the directory for itself would hold a program to this process's
    /// temporary directory rather than to the session's, and to whatever a stage's own `TMPDIR=`
    /// assignment had made it by the time the profile was built.
    #[test]
    fn the_temporary_directory_is_the_one_the_caller_resolved() {
        let policy = a_base(Prelude::Linux);

        assert!(
            policy
                .readable
                .contains(&PathBuf::from(THE_SESSIONS_TEMPORARY_DIRECTORY))
        );
        assert!(written_paths(&policy).contains(&PathBuf::from(THE_SESSIONS_TEMPORARY_DIRECTORY)));
        assert!(
            !granted_paths(&policy).contains(&std::env::temp_dir()),
            "a base named the temporary directory this process resolved"
        );
    }

    /// Everything else the base names is the machine's: the system directories a program starts
    /// out of, and the certificates deciding what every TLS client on it trusts.
    #[test]
    fn only_the_temporary_directory_and_the_null_device_are_written() {
        for prelude in BOTH_PLATFORMS {
            assert_eq!(
                written_paths(&a_base(prelude)),
                vec![
                    PathBuf::from(THE_SESSIONS_TEMPORARY_DIRECTORY),
                    PathBuf::from("/dev/null"),
                ],
                "the {prelude:?} base grants writing somewhere else"
            );
        }
    }

    /// A write row saying what is at the path it names is a row a caller creates before the
    /// policy is resolved. Neither of the base's says: the session resolved its temporary
    /// directory as it opened, and creating the other would put a regular file where the kernel
    /// keeps a device.
    #[test]
    fn the_base_asks_for_nothing_to_be_created() {
        let temporary_directory = scratch_dir("a-base-creates-nothing");
        let _ = std::fs::remove_dir_all(&temporary_directory);
        let policy = base(
            Prelude::Linux,
            &temporary_directory,
            Some(Path::new(A_HOME)),
        );

        assert!(
            policy
                .writable
                .iter()
                .all(|row| row.kind == PathKind::Unsaid),
            "a base write row says what is at the path it names"
        );

        let created =
            policy.create_missing_write_rows(&a_backend_that_cannot_name_an_absent_path());

        assert!(
            created.is_empty(),
            "the base had {created:?} created for it"
        );
        assert!(
            !temporary_directory.exists(),
            "a temporary directory was created for a base that only named one"
        );
    }

    /// Neither is a stricter base. Two of the three backends refuse a policy asking for children
    /// to be denied rather than applying one without that denial, so a base asking for it is
    /// every program refused there, and what tells an approved `git push` from an exfiltration
    /// is the argv somebody endorsed rather than a profile that can only gate egress whole.
    #[test]
    fn the_base_leaves_egress_and_children_to_the_plan() {
        for prelude in BOTH_PLATFORMS {
            let policy = a_base(prelude);
            assert!(policy.allow_network, "the {prelude:?} base closed egress");
            assert!(
                policy.allow_subprocesses,
                "the {prelude:?} base denied children"
            );
        }
    }

    /// Egress and children are open, so what makes the base confinement at all is the filesystem
    /// it bounds, and the root is the one row that stops bounding it. A row naming it grants
    /// every path on the machine while every other assertion here still holds, and what a person
    /// is told is that the program was confined.
    #[test]
    fn the_base_names_no_filesystem_root() {
        for prelude in BOTH_PLATFORMS {
            let policy = a_base(prelude);
            assert!(
                granted_paths(&policy)
                    .iter()
                    .all(|path| path != Path::new("/")),
                "the {prelude:?} base names the whole filesystem"
            );
            assert!(
                policy.is_meaningful(),
                "the {prelude:?} base confines nothing"
            );
        }
    }

    /// The prelude is what a dynamic executable needs to start at all, and what that is differs
    /// by platform. A base carrying the other platform's rows is a program that cannot start,
    /// reported as a program that was confined.
    #[test]
    fn each_platform_starts_a_program_out_of_its_own_directories() {
        let linux = a_base(Prelude::Linux);
        assert!(reaches(&linux, "/lib64/ld-linux-x86-64.so.2"));
        assert!(reaches(&linux, "/etc/nsswitch.conf"));

        let macos = a_base(Prelude::MacOs);
        assert!(reaches(&macos, "/usr/lib/dyld"));
        assert!(reaches(
            &macos,
            "/System/Library/Frameworks/Foundation.framework"
        ));
        // The path a program opens once the link is followed, since that is the one the backend
        // there matches a grant against. A row spelled the other way is granted in silence and
        // reaches nothing.
        assert!(reaches(&macos, "/private/etc/hosts"));
        assert!(!reaches(&macos, "/etc/hosts"));

        for prelude in BOTH_PLATFORMS {
            let policy = a_base(prelude);
            assert!(reaches(&policy, "/usr/bin/git"), "{prelude:?}");
            assert!(reaches(&policy, "/dev/urandom"), "{prelude:?}");
        }
    }

    /// Every row here is shown by no prompt, so a path of a person's own in one is reach nobody
    /// sees and nobody audits. The machine's own directories are the whole of what may be in it.
    #[test]
    fn a_prelude_names_the_machines_directories_and_none_of_a_persons() {
        const THE_MACHINES_OWN: [&str; 9] = [
            "/bin", "/sbin", "/usr", "/lib", "/lib64", "/etc", "/private", "/dev", "/System",
        ];

        for prelude in BOTH_PLATFORMS {
            for row in prelude.rows() {
                assert!(
                    THE_MACHINES_OWN
                        .iter()
                        .any(|root| Path::new(row).starts_with(root)),
                    "the {prelude:?} prelude names {row}"
                );
            }
        }
    }

    /// A platform whose prelude is written down gets it, and one whose is not gets no base at
    /// all rather than another platform's rows.
    #[test]
    fn a_platform_with_no_prelude_written_down_has_no_base() {
        let written_down_for_this_platform = if cfg!(target_os = "linux") {
            Some(Prelude::Linux)
        } else if cfg!(target_os = "macos") {
            Some(Prelude::MacOs)
        } else {
            None
        };

        assert_eq!(Prelude::current(), written_down_for_this_platform);
    }
}
