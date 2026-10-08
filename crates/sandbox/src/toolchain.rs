//! What a stage reaches because of the toolchain its program is, beyond the base.
//!
//! The paths a build resolves through belong to that build rather than to every program that
//! runs, so they are a list keyed on the file a stage's program resolved to: an `npm ci` is held to
//! the npm rows and a `cargo build` in the same session to the cargo rows, and a postinstall script
//! cannot leave something in `~/.cargo/registry` for a later `cargo build` to read.
//! `docs/specs/sandboxing.md` decides the rows, and this is that table in code.
//!
//! Nothing here reads the machine or a configuration file, so `CARGO_HOME`, `GOCACHE` and
//! `XDG_CACHE_HOME` move nothing. A toolchain told by one of them to keep its cache somewhere else
//! finds no row there.

use crate::base::{Prelude, under};
use crate::policy::SandboxPolicy;
use std::path::Path;

/// A toolchain that brings a list of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toolchain {
    Cargo,
    Node,
    Python,
    Go,
    Maven,
    Gradle,
}

impl Toolchain {
    /// The toolchain `resolved` is, from the name of the file a stage's program resolved to.
    ///
    /// The file rather than the name a line used, since the file is what the plan resolved and a
    /// person read, and never a name the model supplied or a value a configuration file holds.
    /// Resolution follows links, so each toolchain is known by every name its programs end at:
    /// `cargo` is a link to `rustup` where rustup installed it, `npm` and `npx` end at npm's own
    /// scripts, and `python3` at a versioned `python3.12`. A program no list knows, `make` or
    /// `just` among them, brings no list.
    pub fn of(resolved: &Path) -> Option<Self> {
        let name = resolved.file_name()?.to_str()?;
        match name {
            "cargo" | "rustup" => Some(Self::Cargo),
            "node" | "npm" | "npx" | "npm-cli.js" | "npx-cli.js" => Some(Self::Node),
            "go" => Some(Self::Go),
            "mvn" => Some(Self::Maven),
            "gradle" => Some(Self::Gradle),
            _ if versioned(name, "python") || versioned(name, "pip") => Some(Self::Python),
            _ => None,
        }
    }

    /// Whether the program `resolved` is one of this toolchain's that fetches what it builds with,
    /// so a stage of it keeps the network where the session has closed it.
    ///
    /// Keyed on the file, as [`Toolchain::of`] is, and not on the toolchain: `node` and `python3`
    /// run a script and fetch nothing, where `npm`, `npx` and `pip` do. A program that runs
    /// another, `python3 -m pip`, is the program it resolved to and brings none.
    pub fn fetches(self, resolved: &Path) -> bool {
        let Some(name) = resolved.file_name().and_then(|name| name.to_str()) else {
            return false;
        };
        match self {
            Self::Cargo | Self::Go | Self::Maven | Self::Gradle => true,
            Self::Node => name != "node",
            Self::Python => versioned(name, "pip"),
        }
    }

    /// The name a person and the planner know this toolchain by.
    pub fn name(self) -> &'static str {
        match self {
            Self::Cargo => "cargo",
            Self::Node => "node",
            Self::Python => "python",
            Self::Go => "go",
            Self::Maven => "maven",
            Self::Gradle => "gradle",
        }
    }

    /// The toolchain a person or the planner names by the word [`Toolchain::name`] gives.
    pub fn named(word: &str) -> Option<Self> {
        [
            Self::Cargo,
            Self::Node,
            Self::Python,
            Self::Go,
            Self::Maven,
            Self::Gradle,
        ]
        .into_iter()
        .find(|toolchain| toolchain.name() == word)
    }

    /// `policy` with this toolchain's list added to it, for the account whose home is `home`.
    ///
    /// An install is read and never written: a confined stage able to replace the `cargo` or the
    /// `node` a later stage resolves to has reach past its own run. A cache is read and written,
    /// since a build that cannot write one fetches everything again or fails, and each write row
    /// says what it names so that, on a backend that cannot name an absent path, a machine which
    /// has not run the toolchain yet gets an empty cache rather than a build that fails on a row
    /// the resolution left out.
    pub fn grant(self, policy: SandboxPolicy, prelude: Prelude, home: &Path) -> SandboxPolicy {
        let mut policy = policy;
        for row in self.installs().iter().chain(self.configurations()) {
            policy = policy.allow_read(under(home, row));
        }
        self.grant_caches(policy, prelude, home)
    }

    /// `policy` with every toolchain's caches added to it, whatever program the stage runs.
    ///
    /// A stage is a script as often as one program, and a script that starts `cargo` or `npm` is
    /// no program the plan resolved to a toolchain. The caches are the one thing a toolchain
    /// brings that is written, and each is a directory named below the one holding its token, so
    /// writing all of them lets any build cache what it fetched without handing any stage the
    /// token or an install. Installs and configuration need no row where the machine is read.
    pub fn grant_every_cache(
        policy: SandboxPolicy,
        prelude: Prelude,
        home: &Path,
    ) -> SandboxPolicy {
        EVERY_TOOLCHAIN
            .into_iter()
            .fold(policy, |policy, toolchain| {
                toolchain.grant_caches(policy, prelude, home)
            })
    }

    fn grant_caches(
        self,
        mut policy: SandboxPolicy,
        prelude: Prelude,
        home: &Path,
    ) -> SandboxPolicy {
        for row in self.cache_directories(prelude) {
            let path = under(home, row);
            if !policy.writable.iter().any(|granted| granted.path == path) {
                policy = policy.allow_read(&path).allow_write_directory(path);
            }
        }
        for row in self.cache_files() {
            let path = under(home, row);
            if !policy.writable.iter().any(|granted| granted.path == path) {
                policy = policy.allow_read(&path).allow_write_file(path);
            }
        }
        policy
    }

    /// Where the toolchain's programs are installed, read so they can start.
    fn installs(self) -> &'static [&'static str] {
        match self {
            Self::Cargo => &[".rustup", ".cargo/bin", ".asdf"],
            Self::Node => &[".nvm", ".asdf"],
            Self::Python => &[".pyenv", ".asdf"],
            Self::Go | Self::Maven | Self::Gradle => &[".asdf"],
        }
    }

    /// Configuration the toolchain refuses to run without reading, where one exists.
    ///
    /// Cargo reads its configuration file on every invocation and fails outright on one it cannot
    /// open, so a machine with a `~/.cargo/config.toml` builds nothing unless the file is read.
    /// Read and never written, since a stage able to set `build.rustc-wrapper` there runs its own
    /// program in every later build. npm passes over a configuration file it cannot open, so its
    /// `~/.npmrc`, which is where it keeps its token, is not named.
    fn configurations(self) -> &'static [&'static str] {
        match self {
            Self::Cargo => &[".cargo/config.toml", ".cargo/config"],
            Self::Node | Self::Python | Self::Go | Self::Maven | Self::Gradle => &[],
        }
    }

    /// The caches the toolchain writes, each named rather than the directory holding it.
    ///
    /// A package manager keeps its token beside its cache, `~/.cargo/credentials.toml`,
    /// `~/.m2/settings.xml` and `~/.gradle/gradle.properties` among them, so a row naming the
    /// parent grants the token with it. Go and pip keep their caches where the platform keeps
    /// every program's, which on macOS is `~/Library/Caches`, and a row there names the one
    /// subdirectory for the same reason.
    fn cache_directories(self, prelude: Prelude) -> &'static [&'static str] {
        match (self, prelude) {
            (Self::Cargo, _) => &[".cargo/registry", ".cargo/git"],
            (Self::Node, Prelude::Linux | Prelude::MacOs) => &[".npm/_cacache"],
            (Self::Node, Prelude::Windows) => &["AppData/Local/npm-cache/_cacache"],
            (Self::Python, Prelude::Linux) => &[".cache/pip"],
            (Self::Python, Prelude::MacOs) => &["Library/Caches/pip"],
            (Self::Python, Prelude::Windows) => &["AppData/Local/pip/Cache"],
            (Self::Go, Prelude::Linux) => &[".cache/go-build", "go/pkg/mod", "go/pkg/sumdb"],
            (Self::Go, Prelude::MacOs) => {
                &["Library/Caches/go-build", "go/pkg/mod", "go/pkg/sumdb"]
            }
            (Self::Go, Prelude::Windows) => {
                &["AppData/Local/go-build", "go/pkg/mod", "go/pkg/sumdb"]
            }
            (Self::Maven, _) => &[".m2/repository"],
            (Self::Gradle, _) => &[".gradle/caches", ".gradle/wrapper", ".gradle/native"],
        }
    }

    /// State a toolchain keeps in a file at the top of its own directory, named file by file so
    /// the token file beside it is not.
    fn cache_files(self) -> &'static [&'static str] {
        match self {
            Self::Cargo => &[".cargo/.package-cache"],
            Self::Node | Self::Python | Self::Go | Self::Maven | Self::Gradle => &[],
        }
    }
}

const EVERY_TOOLCHAIN: [Toolchain; 6] = [
    Toolchain::Cargo,
    Toolchain::Node,
    Toolchain::Python,
    Toolchain::Go,
    Toolchain::Maven,
    Toolchain::Gradle,
];

/// Whether `name` is `program`, or `program` followed by a version: `python3` and `python3.12`
/// are `python`, and `python3-config` is not.
fn versioned(name: &str, program: &str) -> bool {
    let Some(version) = name.strip_prefix(program) else {
        return false;
    };
    version.is_empty()
        || (version.starts_with(|c: char| c.is_ascii_digit())
            && version.chars().all(|c| c.is_ascii_digit() || c == '.'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::base;
    use crate::policy::{Capabilities, ConfinementLevel, PathKind};
    use crate::testutil::scratch_dir;
    use std::path::PathBuf;

    const A_HOME: &str = "/home/a-person";
    const THE_SESSIONS_TEMPORARY_DIRECTORY: &str = "/scratch/tmp-of-this-session";

    const EVERY_PLATFORM: [Prelude; 3] = [Prelude::Linux, Prelude::MacOs, Prelude::Windows];

    fn a_list(toolchain: Toolchain, prelude: Prelude) -> SandboxPolicy {
        toolchain.grant(SandboxPolicy::strict(), prelude, Path::new(A_HOME))
    }

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

    fn reaches(policy: &SandboxPolicy, path: &str) -> bool {
        granted_paths(policy)
            .iter()
            .any(|row| Path::new(path).starts_with(row))
    }

    fn writes(policy: &SandboxPolicy, path: &str) -> bool {
        written_paths(policy)
            .iter()
            .any(|row| Path::new(path).starts_with(row))
    }

    /// Resolution follows links, so the name a list is keyed on is the one a link ends at. Keyed
    /// on the names a line uses instead, the list is missing on every machine where rustup, nvm or
    /// a system python installed the toolchain, which is most of them, and the build fails on a
    /// cache nothing named.
    #[test]
    fn a_list_is_keyed_on_the_file_a_program_resolved_to() {
        for (resolved, toolchain) in [
            ("/home/a-person/.cargo/bin/rustup", Toolchain::Cargo),
            ("/usr/bin/cargo", Toolchain::Cargo),
            ("/usr/bin/node", Toolchain::Node),
            (
                "/home/a-person/.nvm/versions/node/v24.1.0/lib/node_modules/npm/bin/npm-cli.js",
                Toolchain::Node,
            ),
            ("/usr/share/nodejs/npm/bin/npx-cli.js", Toolchain::Node),
            ("/usr/bin/python3.12", Toolchain::Python),
            ("/usr/bin/python3", Toolchain::Python),
            ("/home/a-person/.pyenv/shims/python", Toolchain::Python),
            ("/usr/bin/pip3", Toolchain::Python),
            ("/usr/local/go/bin/go", Toolchain::Go),
            ("/usr/share/maven/bin/mvn", Toolchain::Maven),
            ("/opt/gradle/bin/gradle", Toolchain::Gradle),
        ] {
            assert_eq!(
                Toolchain::of(Path::new(resolved)),
                Some(toolchain),
                "{resolved}"
            );
        }
    }

    /// Fetching is a property of the file a stage resolved to. Keyed on the toolchain, `python3`
    /// and `node` running a script would keep the network a closed session took from `cat`.
    #[test]
    fn only_the_programs_that_fetch_are_known_to_fetch() {
        for (resolved, fetches) in [
            ("/usr/bin/cargo", true),
            ("/home/a-person/.cargo/bin/rustup", true),
            ("/usr/bin/npm-cli.js", true),
            ("/usr/share/nodejs/npm/bin/npx-cli.js", true),
            ("/usr/bin/node", false),
            ("/usr/bin/pip3", true),
            ("/usr/bin/pip", true),
            ("/usr/bin/python3", false),
            ("/usr/bin/python3.12", false),
            ("/usr/local/go/bin/go", true),
            ("/usr/share/maven/bin/mvn", true),
            ("/opt/gradle/bin/gradle", true),
        ] {
            let path = Path::new(resolved);
            let toolchain = Toolchain::of(path).expect(resolved);
            assert_eq!(toolchain.fetches(path), fetches, "{resolved}");
        }
    }

    /// A list is reach a person never named, so a program gets one only where its file is one of
    /// the toolchain's own. A key matched on part of a name hands the npm list to `pnpm` and the
    /// python list to `python3-config`, and a wrapper such as `make` is asked about rather than
    /// handed a list it may not need.
    #[test]
    fn a_program_no_list_knows_brings_none() {
        for resolved in [
            "/usr/bin/make",
            "/usr/bin/just",
            "/bin/sh",
            "/usr/bin/env",
            "/usr/bin/git",
            "/usr/local/bin/pnpm",
            "/usr/bin/python3-config",
            "/usr/bin/pythonw",
            "/usr/local/bin/pipx",
            "/usr/bin/gofmt",
            "/home/a-person/.cargo/bin/cargo-deny",
            "/workspace/gradlew",
            "/",
        ] {
            assert_eq!(Toolchain::of(Path::new(resolved)), None, "{resolved}");
        }
    }

    /// The token a publish uses sits beside the cache a build writes, so a list naming the
    /// directory holding the cache hands the token to every build in that ecosystem.
    #[test]
    fn a_list_names_a_cache_and_never_the_directory_holding_it() {
        for prelude in EVERY_PLATFORM {
            for toolchain in EVERY_TOOLCHAIN {
                let policy = a_list(toolchain, prelude);
                for credential in [
                    "/home/a-person/.cargo/credentials.toml",
                    "/home/a-person/.cargo/credentials",
                    "/home/a-person/.m2/settings.xml",
                    "/home/a-person/.m2/settings-security.xml",
                    "/home/a-person/.gradle/gradle.properties",
                    "/home/a-person/.npmrc",
                    "/home/a-person/.npm/_authToken",
                    "/home/a-person/.pypirc",
                    "/home/a-person/.config/pip/pip.conf",
                    "/home/a-person/.ssh/id_rsa",
                    "/home/a-person/.config/gh/hosts.yml",
                    "/home/a-person/.cache/gh/token",
                    "/home/a-person/Library/Caches/com.apple.keychain/token",
                ] {
                    assert!(
                        !reaches(&policy, credential),
                        "the {toolchain:?} list on {prelude:?} reaches {credential}"
                    );
                }
            }
        }
    }

    /// An install is read so that its programs start. Writing one lets a confined stage replace
    /// the `cargo` or the `node` a later stage resolves to, which is reach past its own run.
    #[test]
    fn an_install_is_read_and_never_written() {
        for prelude in EVERY_PLATFORM {
            let cargo = a_list(Toolchain::Cargo, prelude);
            assert!(reaches(&cargo, "/home/a-person/.cargo/bin/cargo"));
            assert!(reaches(
                &cargo,
                "/home/a-person/.rustup/toolchains/stable/bin/rustc"
            ));
            let node = a_list(Toolchain::Node, prelude);
            assert!(reaches(
                &node,
                "/home/a-person/.nvm/versions/node/v24.1.0/bin/node"
            ));

            for toolchain in EVERY_TOOLCHAIN {
                let policy = a_list(toolchain, prelude);
                for install in [
                    "/home/a-person/.rustup/toolchains/stable/bin/rustc",
                    "/home/a-person/.cargo/bin/cargo",
                    "/home/a-person/.nvm/versions/node/v24.1.0/bin/node",
                    "/home/a-person/.pyenv/versions/3.12.0/bin/python",
                    "/home/a-person/.asdf/shims/go",
                ] {
                    assert!(
                        !writes(&policy, install),
                        "the {toolchain:?} list on {prelude:?} writes {install}"
                    );
                }
            }
        }
    }

    /// Cargo fails every invocation on a configuration file it cannot open, so a list without it
    /// is every build refused on a machine that has one. It is read and never written: a stage
    /// able to set `build.rustc-wrapper` there runs a program of its own in every later build.
    #[test]
    fn a_cargo_list_reads_the_configuration_cargo_cannot_start_without() {
        for prelude in EVERY_PLATFORM {
            let cargo = a_list(Toolchain::Cargo, prelude);
            for configuration in [
                "/home/a-person/.cargo/config.toml",
                "/home/a-person/.cargo/config",
            ] {
                assert!(
                    reaches(&cargo, configuration),
                    "the cargo list on {prelude:?} leaves out {configuration}"
                );
                assert!(
                    !writes(&cargo, configuration),
                    "the cargo list on {prelude:?} writes {configuration}"
                );
            }
        }
    }

    /// What keying on the binary buys is that a build is trusted with its own ecosystem's cache
    /// and no other's. One list shared by every toolchain lets a postinstall script leave
    /// something in the cargo registry for a later `cargo build` to read.
    #[test]
    fn a_list_writes_its_own_ecosystems_cache_and_no_other() {
        for prelude in [Prelude::Linux, Prelude::MacOs] {
            let platform_caches = match prelude {
                Prelude::Linux => "/home/a-person/.cache",
                _ => "/home/a-person/Library/Caches",
            };
            let caches = [
                (
                    Toolchain::Cargo,
                    "/home/a-person/.cargo/registry/index".to_owned(),
                ),
                (
                    Toolchain::Cargo,
                    "/home/a-person/.cargo/git/checkouts".to_owned(),
                ),
                (
                    Toolchain::Node,
                    "/home/a-person/.npm/_cacache/index-v5".to_owned(),
                ),
                (Toolchain::Python, format!("{platform_caches}/pip/http")),
                (Toolchain::Go, format!("{platform_caches}/go-build/00")),
                (Toolchain::Go, "/home/a-person/go/pkg/mod/cache".to_owned()),
                (
                    Toolchain::Go,
                    "/home/a-person/go/pkg/sumdb/sum.golang.org/latest".to_owned(),
                ),
                (
                    Toolchain::Maven,
                    "/home/a-person/.m2/repository/org".to_owned(),
                ),
                (
                    Toolchain::Gradle,
                    "/home/a-person/.gradle/caches/modules-2".to_owned(),
                ),
                (
                    Toolchain::Gradle,
                    "/home/a-person/.gradle/wrapper/dists".to_owned(),
                ),
                (
                    Toolchain::Gradle,
                    "/home/a-person/.gradle/native/lib".to_owned(),
                ),
            ];

            for toolchain in EVERY_TOOLCHAIN {
                let policy = a_list(toolchain, prelude);
                for (owner, cache) in &caches {
                    assert_eq!(
                        writes(&policy, cache),
                        *owner == toolchain,
                        "the {toolchain:?} list on {prelude:?} and {cache}, which is {owner:?}'s"
                    );
                }
            }
        }
    }

    /// Go and pip keep their caches where the platform keeps every program's. A macOS list
    /// written with the Linux spelling names a directory neither uses there, and a `go build`
    /// that cannot open its cache fails.
    #[test]
    fn each_platform_writes_the_cache_its_toolchain_uses_there() {
        let go_on_macos = a_list(Toolchain::Go, Prelude::MacOs);
        assert!(writes(
            &go_on_macos,
            "/home/a-person/Library/Caches/go-build/00"
        ));
        assert!(!reaches(&go_on_macos, "/home/a-person/.cache/go-build/00"));
        let pip_on_macos = a_list(Toolchain::Python, Prelude::MacOs);
        assert!(writes(
            &pip_on_macos,
            "/home/a-person/Library/Caches/pip/http"
        ));

        let go_on_linux = a_list(Toolchain::Go, Prelude::Linux);
        assert!(writes(&go_on_linux, "/home/a-person/.cache/go-build/00"));
        assert!(!reaches(
            &go_on_linux,
            "/home/a-person/Library/Caches/go-build/00"
        ));

        for prelude in EVERY_PLATFORM {
            for toolchain in EVERY_TOOLCHAIN {
                let policy = a_list(toolchain, prelude);
                for someone_elses in [
                    "/home/a-person/.cache/another-program/state",
                    "/home/a-person/Library/Caches/another-program/state",
                    "/home/a-person/go/bin/a-tool",
                ] {
                    assert!(
                        !reaches(&policy, someone_elses),
                        "the {toolchain:?} list on {prelude:?} reaches {someone_elses}"
                    );
                }
            }
        }
    }

    /// Windows keeps a program's caches under `AppData\Local`, and a row naming that directory is
    /// every other program's state with them, so each cache is named on its own.
    #[test]
    fn windows_writes_the_cache_its_toolchain_uses_there() {
        for (toolchain, cache) in [
            (
                Toolchain::Python,
                "/home/a-person/AppData/Local/pip/Cache/http",
            ),
            (Toolchain::Go, "/home/a-person/AppData/Local/go-build/00"),
            (
                Toolchain::Node,
                "/home/a-person/AppData/Local/npm-cache/_cacache/index-v5",
            ),
        ] {
            let on_windows = a_list(toolchain, Prelude::Windows);
            assert!(
                writes(&on_windows, cache),
                "{toolchain:?} on Windows: {cache}"
            );
            for another in EVERY_TOOLCHAIN
                .into_iter()
                .filter(|other| *other != toolchain)
            {
                assert!(
                    !writes(&a_list(another, Prelude::Windows), cache),
                    "{another:?} writes {toolchain:?}'s cache {cache}"
                );
            }
            assert!(!reaches(
                &on_windows,
                "/home/a-person/AppData/Local/another-program/state"
            ));
        }
    }

    /// Every file under a home directory is either named by a row or out of reach, so a row
    /// naming the directory itself, or `~/.config`, `~/.cache` or `~/Library` whole, is every
    /// other file there granted at once.
    #[test]
    fn no_list_names_a_directory_that_holds_other_programs_files() {
        for prelude in EVERY_PLATFORM {
            for toolchain in EVERY_TOOLCHAIN {
                for row in granted_paths(&a_list(toolchain, prelude)) {
                    for too_wide in [
                        A_HOME,
                        "/home/a-person/.config",
                        "/home/a-person/.cache",
                        "/home/a-person/Library",
                        "/home/a-person/Library/Caches",
                        "/home/a-person/.cargo",
                        "/home/a-person/.npm",
                        "/home/a-person/.m2",
                        "/home/a-person/.gradle",
                        "/home/a-person/go",
                        "/home/a-person/go/pkg",
                    ] {
                        assert_ne!(
                            row,
                            PathBuf::from(too_wide),
                            "the {toolchain:?} list on {prelude:?}"
                        );
                    }
                }
            }
        }
    }

    /// A machine that has not run a toolchain yet has no cache, and a backend that cannot name an
    /// absent path leaves a row that says nothing out, which is a build refused its cache. The
    /// cargo lock is a file: created as a directory, cargo cannot take the lock and fails.
    #[test]
    fn a_missing_cache_is_created_as_what_the_toolchain_expects_there() {
        let home = scratch_dir("a-toolchain-list-creates-its-cache");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("the scratch home is creatable");
        let policy = Toolchain::Cargo.grant(SandboxPolicy::strict(), Prelude::Linux, &home);

        policy.create_missing_write_rows(&Capabilities {
            level: ConfinementLevel::Kernel,
            mechanisms: vec!["a mechanism"],
            network_denial_enforced: true,
            grants_paths_that_do_not_exist: false,
        });

        assert!(home.join(".cargo").join("registry").is_dir());
        assert!(home.join(".cargo").join("git").is_dir());
        assert!(home.join(".cargo").join(".package-cache").is_file());
        assert!(
            !home.join(".rustup").exists(),
            "an install was created, which a list only reads"
        );
        assert!(
            policy
                .writable
                .iter()
                .all(|row| row.kind != PathKind::Unsaid),
            "a cache row says nothing about what is at it"
        );

        let _ = std::fs::remove_dir_all(&home);
    }

    /// A list is added to the base a profile starts from, so what the base granted has to reach
    /// the backend as it was. A list that started from a strict policy of its own hands the
    /// program a profile without its loader, and one that closed egress refuses every fetch.
    #[test]
    fn a_list_leaves_the_policy_it_is_added_to_as_it_was() {
        let base = base(
            Prelude::Linux,
            Path::new(THE_SESSIONS_TEMPORARY_DIRECTORY),
            None,
            Some(Path::new(A_HOME)),
        );

        for toolchain in EVERY_TOOLCHAIN {
            let policy = toolchain.grant(base.clone(), Prelude::Linux, Path::new(A_HOME));
            assert!(policy.readable.starts_with(&base.readable), "{toolchain:?}");
            assert!(policy.writable.starts_with(&base.writable), "{toolchain:?}");
            assert_eq!(policy.allow_network, base.allow_network, "{toolchain:?}");
            assert_eq!(
                policy.allow_subprocesses, base.allow_subprocesses,
                "{toolchain:?}"
            );
        }
    }
}
