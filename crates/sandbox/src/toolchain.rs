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
use crate::programs::program_name;
use crate::rules::{clear_of_every_refused_location, resolved};
use std::path::{Path, PathBuf};

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
        let name = program_name(resolved)?;
        let name = name.as_str();
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
        let Some(name) = program_name(resolved) else {
            return false;
        };
        let name = name.as_str();
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
    pub fn grant(
        self,
        policy: SandboxPolicy,
        prelude: Prelude,
        home: &Path,
        moved: Option<&Path>,
    ) -> SandboxPolicy {
        let policy = self.grant_reads(policy, home, moved);
        self.grant_caches(policy, prelude, home, moved)
    }

    /// `policy` with this toolchain's installs and configuration readable and nothing written, for
    /// a stage that may not write (the mode `read-only`).
    ///
    /// Each row is judged where its name leads and opened without following a link, as a cache row
    /// is ([`Toolchain::grant`]): an install or a configuration file is a name under the home
    /// directory too, and a stage able to write the directory above it could link it to `~/.ssh`,
    /// which a read row opened by name would then grant. An install is a directory and a
    /// configuration file is a file.
    pub fn grant_reads(
        self,
        policy: SandboxPolicy,
        home: &Path,
        moved: Option<&Path>,
    ) -> SandboxPolicy {
        let mut policy = policy;
        let rows = self
            .installs()
            .iter()
            .map(|row| (under(home, row), true))
            .chain(
                self.configurations()
                    .iter()
                    .map(|row| (under(home, row), false)),
            );
        for (name, directory) in rows {
            let Some(path) = led_to(&name, home, moved, directory) else {
                continue;
            };
            policy = policy.allow_read(&path).not_through_a_link(path);
        }
        policy
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
        moved: Option<&Path>,
    ) -> SandboxPolicy {
        EVERY_TOOLCHAIN
            .into_iter()
            .fold(policy, |policy, toolchain| {
                toolchain.grant_caches(policy, prelude, home, moved)
            })
    }

    /// `policy` with this toolchain's caches added, each judged where its name leads now.
    ///
    /// A cache row is a name under the home directory that a stage able to write its parent can
    /// move aside and put a link at (SANDBOX-15, SANDBOX-18). A name with a link in it is given
    /// no row where it leads to a credential or persistence location, to a directory that holds
    /// one, or to the other kind of thing than the row names, and the stage still starts. A
    /// name that leads somewhere ordinary gets the row at the place it leads to. A judged row is
    /// held to its name not being a link when a backend opens it
    /// ([`SandboxPolicy::not_through_a_link`]), so a name relinked after this judgement is
    /// refused and not attached to the new target.
    fn grant_caches(
        self,
        mut policy: SandboxPolicy,
        prelude: Prelude,
        home: &Path,
        moved: Option<&Path>,
    ) -> SandboxPolicy {
        let rows: Vec<(PathBuf, bool)> = self
            .cache_directories(prelude)
            .iter()
            .map(|row| (under(home, row), true))
            .chain(
                self.cache_files()
                    .iter()
                    .map(|row| (under(home, row), false)),
            )
            .collect();
        for (name, directory) in rows {
            let Some(path) = led_to(&name, home, moved, directory) else {
                continue;
            };
            if policy.writable.iter().any(|granted| granted.path == path) {
                continue;
            }
            policy = policy.allow_read(&path).not_through_a_link(&path);
            policy = match directory {
                true => policy.allow_write_directory(path),
                false => policy.allow_write_file(path),
            };
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

/// The path a cache row at `name` is given, or `None` where the row is not given: the place the
/// name leads to is a credential or persistence location or holds one, or is not the kind of
/// thing the row names (a file for a directory row, a directory for a file row).
///
/// A name with a link in it below `home` is given at the place it leads to, judged by
/// [`clear_of_every_refused_location`] on the very path that is given, so the grant is not a
/// second reading of a name a stage may have changed in between. Any other name is given as it
/// is, as the table's rows are caches and none is a refused location, so a row keeps the spelling
/// `home` was given in and a `home` that is itself a link is the person's own. Nothing at the name
/// leaves the row as it is, since a row opens nothing and the toolchain makes the cache on its
/// first run.
///
/// A link that leads nowhere, at the name or above it, has no place to be judged at: resolving it
/// gives back its own name, which looks clear, and a backend that opens a judged row without
/// following a link refuses the whole policy over it. It is given no row. So is a name that
/// cannot be examined at all, since it may be a link. Reads file system metadata and the path
/// alone.
fn led_to(name: &Path, home: &Path, moved: Option<&Path>, directory: bool) -> Option<PathBuf> {
    use std::io::ErrorKind;

    let below_home = name.strip_prefix(home).unwrap_or(name);
    let mut at = home.to_path_buf();
    let mut through_a_link = false;
    for component in below_home.components() {
        at.push(component);
        match std::fs::symlink_metadata(&at) {
            Ok(found) if found.file_type().is_symlink() => {
                if std::fs::metadata(&at).is_err() {
                    return None;
                }
                through_a_link = true;
            }
            Ok(_) => {}
            Err(error)
                if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {}
            Err(_) => return None,
        }
    }
    let path = if through_a_link {
        let path = resolved(name);
        if !clear_of_every_refused_location(&path, home, moved) {
            return None;
        }
        path
    } else {
        name.to_path_buf()
    };
    match std::fs::metadata(&path) {
        Ok(found) if found.is_dir() != directory => None,
        _ => Some(path),
    }
}

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
        toolchain.grant(SandboxPolicy::strict(), prelude, Path::new(A_HOME), None)
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

    /// On Windows the file a program resolved to keeps its extension, so each list is keyed on the
    /// name without it. The regression it rejects: comparing the whole file name, which gives a
    /// Windows `cargo build` no list and, under a closed network, no egress.
    #[test]
    fn a_list_is_keyed_on_the_file_without_a_windows_executable_extension() {
        for (resolved, toolchain, fetches) in [
            ("C:/Users/p/.cargo/bin/cargo.exe", Toolchain::Cargo, true),
            ("C:/Program Files/nodejs/node.exe", Toolchain::Node, false),
            ("C:/Program Files/nodejs/NPX.EXE", Toolchain::Node, true),
            ("C:/Python312/python.exe", Toolchain::Python, false),
            ("C:/Python312/Scripts/pip3.12.exe", Toolchain::Python, true),
            ("C:/Go/bin/go.exe", Toolchain::Go, true),
            ("C:/gradle/bin/gradle.com", Toolchain::Gradle, true),
        ] {
            let path = Path::new(resolved);
            assert_eq!(Toolchain::of(path), Some(toolchain), "{resolved}");
            assert_eq!(toolchain.fetches(path), fetches, "{resolved}");
        }
        for resolved in [
            "C:/bin/cargo-deny.exe",
            "C:/bin/pnpm.exe",
            "C:/bin/make.exe",
        ] {
            assert_eq!(Toolchain::of(Path::new(resolved)), None, "{resolved}");
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
        let policy = Toolchain::Cargo.grant(SandboxPolicy::strict(), Prelude::Linux, &home, None);

        policy.create_missing_write_rows(&Capabilities {
            level: ConfinementLevel::Kernel,
            mechanisms: vec!["a mechanism"],
            network_denial_enforced: true,
            egress_limited_to_a_port: false,
            grants_paths_that_do_not_exist: false,
            subtracts_from_a_grant: true,
            governs_sockets_with_a_path: true,
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

    /// A cache row is a name under the home directory, and a stage able to write the parent moves
    /// the cache aside and puts a link at the name. A list built after that is judged where the
    /// name leads: a row given at `~/.ssh`, at a shell startup file or at a directory that holds
    /// either would hand a later stage the credentials the base refuses (SANDBOX-15, SANDBOX-18).
    /// The rejected implementation grants `under(home, row)` as a name, which is why each case
    /// below checks that no row leads to the place and that the list does not stop at one row.
    #[test]
    #[cfg(unix)]
    fn a_cache_that_has_become_a_link_to_somewhere_refused_is_given_no_row() {
        use std::os::unix::fs::symlink;

        let home = scratch_dir("a-toolchain-cache-relinked");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".ssh")).expect("the credential directory is creatable");
        std::fs::create_dir_all(home.join(".cargo")).expect("the cargo directory is creatable");
        std::fs::create_dir_all(home.join(".config/autostart")).expect("autostart is creatable");
        std::fs::write(home.join(".bashrc"), "").expect("the startup file is writable");
        std::fs::write(home.join(".ssh/id_test"), "key").expect("the key is writable");
        let home = crate::rules::resolved(&home);

        // (what the cache row is replaced by a link to, the row's name under the home)
        for (leads_to, replaced) in [
            // A credential location, a directory above one, and a persistence location.
            (home.join(".ssh"), ".cargo/registry"),
            (home.clone(), ".cargo/registry"),
            (home.join(".ssh/id_test"), ".cargo/registry"),
            (home.join(".config"), ".cargo/registry"),
            (home.join(".config/autostart"), ".cargo/registry"),
            // A shell startup file in the place of the file row, and the home directory.
            (home.join(".bashrc"), ".cargo/.package-cache"),
            (home.clone(), ".cargo/.package-cache"),
        ] {
            let name = home.join(replaced);
            let _ = std::fs::remove_dir_all(&name);
            let _ = std::fs::remove_file(&name);
            symlink(&leads_to, &name).expect("the cache is replaceable by a link");

            let policy =
                Toolchain::Cargo.grant(SandboxPolicy::strict(), Prelude::Linux, &home, None);

            for row in written_paths(&policy) {
                let at = crate::rules::resolved(&row);
                assert!(
                    at != leads_to && (leads_to == home || !at.starts_with(&leads_to)),
                    "{replaced} leads to {}, and the list writes {}",
                    leads_to.display(),
                    row.display()
                );
                assert_ne!(
                    row, name,
                    "{replaced} is granted as a name that is now a link"
                );
            }
            assert!(
                policy
                    .writable
                    .iter()
                    .any(|row| row.path.starts_with(home.join(".cargo/git"))),
                "the rows beside the one that leads elsewhere are still granted"
            );

            let _ = std::fs::remove_file(&name);
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A row names a kind of thing, and one that is the other kind is a name that was replaced.
    /// The cargo lock is a file and `registry` a directory, so a file at the directory row or a
    /// directory at the file row is no row, and the ordinary case of each stays granted.
    #[test]
    fn a_cache_that_is_the_other_kind_of_thing_is_given_no_row() {
        let home = scratch_dir("a-toolchain-cache-other-kind");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".cargo/.package-cache"))
            .expect("a directory for a file");
        std::fs::write(home.join(".cargo/registry"), "").expect("a file for a directory");
        std::fs::create_dir_all(home.join(".cargo/git")).expect("the git cache is creatable");
        let home = crate::rules::resolved(&home);

        let policy = Toolchain::Cargo.grant(SandboxPolicy::strict(), Prelude::Linux, &home, None);

        let written = written_paths(&policy);
        assert!(
            !written.contains(&home.join(".cargo/registry")),
            "{written:?}"
        );
        assert!(
            !written.contains(&home.join(".cargo/.package-cache")),
            "{written:?}"
        );
        assert!(written.contains(&home.join(".cargo/git")), "{written:?}");

        let _ = std::fs::remove_dir_all(&home);
    }

    /// A link whose target is not there has no place to be judged at: resolving it gives back its
    /// own name, which is clear of every refused location. A judged row is opened without following
    /// a link, so granting it would refuse the whole policy on Linux and attach the row to the link
    /// on the other platforms, where the cache row is meant to be left out and the stage started.
    /// The rejected implementation judges the resolved name only, which for a dangling link is the
    /// name itself, and grants it.
    #[test]
    #[cfg(unix)]
    fn a_cache_that_is_a_link_to_nowhere_is_given_no_row() {
        use std::os::unix::fs::symlink;

        let home = scratch_dir("a-toolchain-cache-dangling");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".cargo/git")).expect("the git cache is creatable");
        let home = crate::rules::resolved(&home);
        // The shell startup file is not there, as on a machine that never made one.
        symlink(home.join(".bashrc"), home.join(".cargo/.package-cache"))
            .expect("the lock is linkable");
        symlink(home.join("nowhere"), home.join(".cargo/registry"))
            .expect("the registry is linkable");

        let policy = Toolchain::Cargo.grant(SandboxPolicy::strict(), Prelude::Linux, &home, None);

        let granted = granted_paths(&policy);
        for name in [
            ".cargo/.package-cache",
            ".cargo/registry",
            ".bashrc",
            "nowhere",
        ] {
            assert!(
                !granted.contains(&home.join(name)),
                "{name} is a link to nowhere and is granted: {granted:?}"
            );
        }
        assert!(
            written_paths(&policy).contains(&home.join(".cargo/git")),
            "the row beside the dangling ones is still granted"
        );

        let _ = std::fs::remove_dir_all(&home);
    }

    /// A cache whose name is a link to the other kind of thing is no row either: a link to a file
    /// where a directory is wanted, and a link to a directory where the lock file is. The target
    /// is an ordinary place, so only the kind refuses it. The rejected implementation checks the
    /// kind of a name that is not a link and takes the kind of a link's name as it is.
    #[test]
    #[cfg(unix)]
    fn a_cache_that_is_a_link_to_the_other_kind_of_thing_is_given_no_row() {
        use std::os::unix::fs::symlink;

        let home = scratch_dir("a-toolchain-cache-link-other-kind");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".cargo/git")).expect("the git cache is creatable");
        std::fs::create_dir_all(home.join("disk/a-directory")).expect("a directory is creatable");
        std::fs::write(home.join("disk/a-file"), "").expect("a file is writable");
        let home = crate::rules::resolved(&home);
        symlink(home.join("disk/a-file"), home.join(".cargo/registry"))
            .expect("the registry is linkable");
        symlink(
            home.join("disk/a-directory"),
            home.join(".cargo/.package-cache"),
        )
        .expect("the lock is linkable");

        let policy = Toolchain::Cargo.grant(SandboxPolicy::strict(), Prelude::Linux, &home, None);

        let granted = granted_paths(&policy);
        for name in [
            "disk/a-file",
            "disk/a-directory",
            ".cargo/registry",
            ".cargo/.package-cache",
        ] {
            assert!(
                !granted.contains(&home.join(name)),
                "{name} is the wrong kind of thing and is granted: {granted:?}"
            );
        }
        assert!(
            written_paths(&policy).contains(&home.join(".cargo/git")),
            "the row beside the refused ones is still granted"
        );

        let _ = std::fs::remove_dir_all(&home);
    }

    /// An install and a configuration file are read rows under the home directory, so a stage able
    /// to write the directory above them can link one to the keys, and a row opened by name is
    /// attached to wherever the name leads. They are judged and held to their name as a cache is,
    /// and an ordinary one is still read. The rejected implementation reads `under(home, row)` as
    /// a name and marks nothing.
    #[test]
    #[cfg(unix)]
    fn an_install_or_configuration_that_has_become_a_link_to_a_refused_place_is_given_no_row() {
        use std::os::unix::fs::symlink;

        let home = scratch_dir("a-toolchain-install-relinked");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".ssh")).expect("the credential directory is creatable");
        std::fs::create_dir_all(home.join(".cargo/bin")).expect("the install is creatable");
        std::fs::write(home.join(".bashrc"), "").expect("the startup file is writable");
        let home = crate::rules::resolved(&home);
        symlink(home.join(".ssh"), home.join(".rustup")).expect("the install is linkable");
        symlink(home.join(".bashrc"), home.join(".cargo/config.toml"))
            .expect("the configuration is linkable");

        let policy = Toolchain::Cargo.grant_reads(SandboxPolicy::strict(), &home, None);

        let granted = granted_paths(&policy);
        assert!(granted.contains(&home.join(".cargo/bin")), "{granted:?}");
        for name in [".rustup", ".cargo/config.toml", ".ssh", ".bashrc"] {
            assert!(
                !granted.contains(&home.join(name)),
                "{name} leads to a refused place and is read: {granted:?}"
            );
        }
        assert!(!policy.readable.is_empty());
        assert_eq!(policy.not_through_a_link, policy.readable);

        let _ = std::fs::remove_dir_all(&home);
    }

    /// A cache that is where it was, and one a person linked to a disk of their own, is still
    /// granted, and granted to be opened as the thing it is. The rejected implementation drops
    /// every row that is a link, which breaks a machine that keeps `~/.cargo` elsewhere, or
    /// lists no row for a linked cache to be held to its name.
    #[test]
    #[cfg(unix)]
    fn a_cache_linked_to_somewhere_ordinary_is_granted_there_and_held_to_that_name() {
        let top = scratch_dir("a-toolchain-cache-on-another-disk");
        let _ = std::fs::remove_dir_all(&top);
        std::fs::create_dir_all(top.join("home/.cargo")).expect("the cargo directory is creatable");
        std::fs::create_dir_all(top.join("disk/registry")).expect("the disk is creatable");
        let top = crate::rules::resolved(&top);
        std::os::unix::fs::symlink(top.join("disk/registry"), top.join("home/.cargo/registry"))
            .expect("the cache is linkable");

        let policy = Toolchain::Cargo.grant(
            SandboxPolicy::strict(),
            Prelude::Linux,
            &top.join("home"),
            None,
        );

        let written = written_paths(&policy);
        assert!(written.contains(&top.join("disk/registry")), "{written:?}");
        assert!(
            !written.contains(&top.join("home/.cargo/registry")),
            "{written:?}"
        );
        for row in &written {
            assert!(
                policy.not_through_a_link.contains(row),
                "{} is judged and is not held to its name",
                row.display()
            );
        }

        let _ = std::fs::remove_dir_all(&top);
    }

    /// A cache linked into the state directory `BRAVEBOT_HOME` moved is given no row, as one linked
    /// into `~/.bravebot` is (SANDBOX-15, #2917). The regression it rejects: judging the place the
    /// link leads without the moved directory, which hands the stage the gateway keys.
    #[test]
    #[cfg(unix)]
    fn a_cache_linked_into_a_state_directory_that_is_moved_is_given_no_row() {
        let top = scratch_dir("a-toolchain-cache-in-a-moved-state-directory");
        let _ = std::fs::remove_dir_all(&top);
        std::fs::create_dir_all(top.join("home/.cargo")).expect("the cargo directory is creatable");
        std::fs::create_dir_all(top.join("state/registry")).expect("the state is creatable");
        let top = crate::rules::resolved(&top);
        std::os::unix::fs::symlink(top.join("state/registry"), top.join("home/.cargo/registry"))
            .expect("the cache is linkable");
        let moved = top.join("state");

        let grant = |moved: Option<&Path>| {
            Toolchain::Cargo.grant(
                SandboxPolicy::strict(),
                Prelude::Linux,
                &top.join("home"),
                moved,
            )
        };
        let unmoved = granted_paths(&grant(None));
        assert!(unmoved.contains(&moved.join("registry")), "{unmoved:?}");
        let held = granted_paths(&grant(Some(&moved)));
        assert!(held.iter().all(|row| !row.starts_with(&moved)), "{held:?}");

        let _ = std::fs::remove_dir_all(&top);
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
            let policy = toolchain.grant(base.clone(), Prelude::Linux, Path::new(A_HOME), None);
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
