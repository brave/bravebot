//! What a stage reaches of the person's credentials, keyed on the operation its argv names.
//!
//! A push is how most sessions end, so a profile that refuses one is a profile somebody turns off.
//! A stage whose operation needs a credential therefore carries a scope: the remote scope for a
//! `git` operation that talks to a remote and for one of `gh`'s own commands, and one tool's own
//! directory for `aws`, `kubectl` and `docker`. `docs/specs/sandboxing.md` decides the rows, and
//! this is that table in code.
//!
//! A scope follows what the endorsed argv says will run and nothing a configuration file holds.
//! `git` runs whatever its argv or its environment names, `-c core.sshCommand=`, `--upload-pack=`
//! and `GIT_SSH_COMMAND=` among them, so a stage written with one of those, the variable as an
//! assignment in front of it, gets no scope: what would run with the credential is not what the
//! plan says would run.

use crate::base::{
    CREDENTIAL_DIRECTORIES, LINUX_CREDENTIAL_DIRECTORIES, MACOS_CREDENTIAL_DIRECTORIES,
    STATE_DIRECTORY, under,
};
use crate::policy::SandboxPolicy;
use crate::toolchain::Toolchain;
use std::path::{Component, Path, PathBuf};

/// A credential scope a stage of a plan carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// What `git` and `gh` read to authenticate to a remote, over ssh or over https.
    Remote,
    /// `~/.aws`.
    Aws,
    /// `~/.kube`.
    Kubernetes,
    /// `~/.docker`.
    Docker,
    /// The public key a signed commit is made with, and the agent that signs with the private
    /// half. A stage carries it by its argv or by a `run` call's request. `/reach` does not name
    /// it, so it is never remembered.
    Signing,
}

/// What the planner may ask a `run` to add to every stage of one line: a name from a fixed menu of
/// the credential scopes and the toolchain lists, and never a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requested {
    Scope(Scope),
    Toolchain(Toolchain),
}

impl Requested {
    /// Every name the planner may use, in the order it is told them.
    pub const MENU: [&'static str; 11] = [
        "remote",
        "signing",
        "aws",
        "kubernetes",
        "docker",
        "cargo",
        "node",
        "python",
        "go",
        "maven",
        "gradle",
    ];

    /// The request a word names, compared exactly: `Remote`, ` aws` and the empty string name none.
    pub fn named(word: &str) -> Option<Self> {
        match word {
            "signing" => Some(Self::Scope(Scope::Signing)),
            _ => Scope::named(word)
                .map(Self::Scope)
                .or_else(|| Toolchain::named(word).map(Self::Toolchain)),
        }
    }

    /// The word the menu, the prompt and the trail know it by.
    pub fn name(self) -> &'static str {
        match self {
            Self::Scope(scope) => scope.name(),
            Self::Toolchain(toolchain) => toolchain.name(),
        }
    }
}

impl Scope {
    /// The name a person and the planner know the scope by.
    pub fn name(self) -> &'static str {
        match self {
            Self::Remote => "remote",
            Self::Aws => "aws",
            Self::Kubernetes => "kubernetes",
            Self::Docker => "docker",
            Self::Signing => "signing",
        }
    }

    /// The scope `/reach` and a remembered grant name by the word the prompt knows it by. `Signing`
    /// is not named here: an argv that signs carries it and a `run` call may request it
    /// ([`Requested::named`]), and neither is remembered.
    pub fn named(word: &str) -> Option<Self> {
        [Self::Remote, Self::Aws, Self::Kubernetes, Self::Docker]
            .into_iter()
            .find(|scope| scope.name() == word)
    }

    /// The scope a stage carries, from the file its program resolved to, its argument vector, and
    /// the `NAME=value` assignments written in front of it.
    ///
    /// A stage with an assignment carries none, since every one of these programs reads a
    /// variable that names a program or a configuration file to run it with: `GIT_SSH_COMMAND`,
    /// `GH_PAGER`, `KUBECONFIG`, `DOCKER_CONFIG`, `AWS_CONFIG_FILE`. Neither does an argv naming
    /// another configuration file, `--kubeconfig` or docker's `--config`, for the same reason.
    pub fn of(resolved: &Path, args: &[String], environment: &[(String, String)]) -> Option<Self> {
        if !environment.is_empty() {
            return None;
        }
        let name = resolved.file_name()?.to_str()?;
        match name {
            "git" if a_remote_operation(args) => Some(Self::Remote),
            "git" if a_signing_operation(args) => Some(Self::Signing),
            "gh" if a_command_of_ghs_own(args) => Some(Self::Remote),
            "aws" => Some(Self::Aws),
            "kubectl" if !names_an_option(args, "--kubeconfig") => Some(Self::Kubernetes),
            "docker" if !names_an_option(args, "--config") => Some(Self::Docker),
            _ => None,
        }
    }

    /// `policy` with this scope's rows added to it, for the account whose home is `home`.
    ///
    /// No row names a private key or `~/.ssh` as a directory: a push signs through the agent, and
    /// what ssh reads beside it is the configuration, the hosts it has verified, and the public
    /// half of a key, which is how it names an identity to the agent. The public half is the one
    /// at each default name and the one at each `IdentityFile` the person's `~/.ssh/config`
    /// names, as [`named_public_keys`] judges them. The one row written is
    /// `known_hosts`, as a file, since a host ssh cannot record is a push that fails and an account
    /// without one gets one created rather than a directory nobody asked for. A tool's directory
    /// is read and never written, since what a write there leaves is a command the person's own
    /// shell runs later: a `credential_process`, an exec plugin, a `credsStore` helper.
    pub fn grant(self, policy: SandboxPolicy, home: &Path) -> SandboxPolicy {
        match self {
            Self::Remote => {
                let mut policy = policy;
                for row in REMOTE {
                    policy = policy.allow_read(under(home, row));
                }
                for public_key in named_public_keys(home) {
                    policy = policy.allow_read(public_key);
                }
                // A pull that merges or rebases signs the commit it makes.
                if let Some(key) = crate::signing::read(home).file() {
                    policy = policy.allow_read(key);
                }
                policy.allow_write_file(under(home, KNOWN_HOSTS))
            }
            Self::Signing => match crate::signing::read(home).file() {
                Some(key) => policy.allow_read(key),
                None => policy,
            },
            Self::Aws | Self::Kubernetes | Self::Docker => {
                let mut policy = policy;
                for row in rows(self) {
                    policy = policy.allow_read(under(home, row));
                }
                policy
            }
        }
    }
}

/// A location a stage reads because the environment it starts with moves a tool's configuration
/// there, beyond the fixed row the scope already holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reach {
    /// The variable the person set, for the prompt to name.
    pub variable: &'static str,
    /// Where it leads, as the sandbox will be told it: the directory or the file itself, after a
    /// link has been followed.
    pub path: PathBuf,
}

/// What a variable names.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Names {
    /// A directory, read whole.
    Directory,
    /// A file, read as that file and nothing around it.
    File,
    /// A list of files, separated as the platform separates `PATH`, each judged on its own.
    Files,
}

/// How one variable moves one row: `NAME`, what it names, and what is appended to the value to get
/// the file or directory the tool opens (`gh` reads `$XDG_CONFIG_HOME/gh`).
struct Moves {
    variable: &'static str,
    names: Names,
    beneath: &'static str,
}

macro_rules! moves {
    ($variable:literal, $names:expr, $beneath:literal) => {
        Moves {
            variable: $variable,
            names: $names,
            beneath: $beneath,
        }
    };
}

/// The variables each program of a scope reads, in the order the program prefers them: the first
/// one set is the one used, for a program that takes one location, and every one set is used where
/// each moves a different file.
///
/// The table of [SANDBOX-16](../../../docs/specs/sandboxing.md#SANDBOX-16). A program absent from it
/// has no variable that moves what its scope reads.
fn variables_of(scope: Scope, program: &str) -> &'static [Moves] {
    match (scope, program) {
        (Scope::Remote, "gh") => &[
            moves!("GH_CONFIG_DIR", Names::Directory, ""),
            moves!("XDG_CONFIG_HOME", Names::Directory, "gh"),
        ],
        (Scope::Remote, "git") => &[
            moves!("GIT_CONFIG_GLOBAL", Names::File, ""),
            moves!("XDG_CONFIG_HOME", Names::File, "git/credentials"),
        ],
        (Scope::Aws, _) => &[
            moves!("AWS_CONFIG_FILE", Names::File, ""),
            moves!("AWS_SHARED_CREDENTIALS_FILE", Names::File, ""),
        ],
        (Scope::Kubernetes, _) => &[moves!("KUBECONFIG", Names::Files, "")],
        (Scope::Docker, _) => &[moves!("DOCKER_CONFIG", Names::Directory, "")],
        _ => &[],
    }
}

/// Where the environment moves what `program` reads of `scope`, for the account whose home is
/// `home`.
///
/// `GH_CONFIG_DIR` and `XDG_CONFIG_HOME` for `gh`, `GIT_CONFIG_GLOBAL` and `XDG_CONFIG_HOME` for
/// `git`, `AWS_CONFIG_FILE` and `AWS_SHARED_CREDENTIALS_FILE` for `aws`, `KUBECONFIG` for `kubectl`
/// and `DOCKER_CONFIG` for `docker`. Each is the location of a file the person's own tool is going
/// to open, which a toolchain cache is not, so it is read where the person set it. `gh` takes
/// `GH_CONFIG_DIR` over `XDG_CONFIG_HOME`, as `gh` does.
///
/// A value is refused, and the stage keeps the fixed rows only, where it is empty, relative or
/// holds `..`. A directory is refused where it is the home or above it, is `~/.ssh` or
/// `~/.bravebot` or inside either, or is `~/.config`, `~/.cache` or `~/Library`: places no row of
/// a scope reaches whole. A file is refused where it is inside `~/.ssh`, since that is where a
/// private key is, or inside `~/.bravebot`, where the gateway keys are, where it is the home or
/// above it, or where it is a directory, and is otherwise read as that file alone. A link is
/// judged by where it leads. A value that lands inside a row the scope already holds is no new
/// reach and is not returned, and one named twice is returned once. Where `KUBECONFIG` lists
/// several files, one refused entry takes none of the others with it.
///
/// Only what the caller passes counts: the environment the stage starts with, in which an
/// assignment written in front of the line has already removed the scope.
pub fn environment_reach(
    scope: Scope,
    program: &str,
    home: &Path,
    environment: &[(String, String)],
) -> Vec<Reach> {
    let set = |name: &str| {
        environment
            .iter()
            .find(|(held, _)| held == name)
            .map(|(_, value)| value.as_str())
            .filter(|value| !value.is_empty())
    };
    let mut reached = Vec::new();
    let mut directory_taken = false;
    for how in variables_of(scope, program) {
        let Some(value) = set(how.variable) else {
            continue;
        };
        // One location, so the first variable set is the one the program opens.
        if how.names == Names::Directory {
            if directory_taken {
                continue;
            }
            directory_taken = true;
        }
        let values: Vec<PathBuf> = match how.names {
            Names::Files => std::env::split_paths(value).collect(),
            _ => vec![PathBuf::from(value)],
        };
        for named in values {
            let named = match how.beneath {
                "" => named,
                beneath => named.join(beneath),
            };
            if let Some(path) = judged(&named, how.names == Names::Directory, home)
                && !already_read(scope, &path, home)
                && !reached.iter().any(|held: &Reach| held.path == path)
            {
                reached.push(Reach {
                    variable: how.variable,
                    path,
                });
            }
        }
    }
    reached
}

/// Where the environment moves what `scope` reads, for a stage that was asked to carry the scope
/// by name ([SANDBOX-26](../../../docs/specs/sandboxing.md#SANDBOX-26)) and so has no program of
/// that scope to key on: the places of every program the scope has, judged as
/// [`environment_reach`] judges them.
pub fn requested_reach(scope: Scope, home: &Path, environment: &[(String, String)]) -> Vec<Reach> {
    let programs: &[&str] = match scope {
        Scope::Remote => &["gh", "git"],
        Scope::Aws | Scope::Kubernetes | Scope::Docker | Scope::Signing => &[""],
    };
    programs
        .iter()
        .flat_map(|program| environment_reach(scope, program, home, environment))
        .collect()
}

/// A directory a person named, as the sandbox will be told it, or `None` where it is refused.
///
/// The same judgement a variable's value gets: absolute, no `..`, not the home or above it, not
/// `~/.ssh` or `~/.bravebot` or inside either, not `~/.config`, `~/.cache` or `~/Library` whole,
/// and a link judged by where it leads. It must also be a directory that exists, since a grant of a path nothing is at
/// reads as a grant of whatever is created there later.
pub fn judged_directory(named: &Path, home: &Path) -> Option<PathBuf> {
    judged(named, true, home).filter(|path| path.is_dir())
}

/// `named` as the sandbox will be told it, or `None` where it is refused.
fn judged(named: &Path, directory: bool, home: &Path) -> Option<PathBuf> {
    if !named.is_absolute() || named.components().any(|part| part == Component::ParentDir) {
        return None;
    }
    // Where it is a link, what it leads to is what a program opens, and both spellings are judged,
    // against both spellings of the home: a prefix of either may be a link (`/home` on macOS).
    let resolved = std::fs::canonicalize(named).unwrap_or_else(|_| named.to_path_buf());
    let real_home = std::fs::canonicalize(home).unwrap_or_else(|_| home.to_path_buf());
    // A grant of a path is a grant of everything under it, so a file is refused where it is the
    // home or above it, or a directory, as a directory is: what a variable names as a file is no
    // reason to read the tree it turns out to be.
    let refused = (!directory && resolved.is_dir())
        || [named, resolved.as_path()].iter().any(|named| {
            [home, real_home.as_path()].iter().any(|home| {
                named.starts_with(under(home, ".ssh"))
                    || named.starts_with(under(home, STATE_DIRECTORY))
                    || home.starts_with(named)
                    || (directory
                        && [".config", ".cache", "Library"]
                            .iter()
                            .any(|whole| *named == under(home, whole)))
            })
        });
    (!refused).then_some(resolved)
}

/// Whether `scope` already reads `path` through a fixed row, so that naming it adds nothing for a
/// person to be told.
fn already_read(scope: Scope, path: &Path, home: &Path) -> bool {
    let real_home = std::fs::canonicalize(home).unwrap_or_else(|_| home.to_path_buf());
    [home, real_home.as_path()].iter().any(|home| {
        rows(scope)
            .iter()
            .any(|row| path.starts_with(under(home, row)))
    })
}

/// The fixed rows of a scope's directory, read and not written beyond what `grant` adds.
fn rows(scope: Scope) -> &'static [&'static str] {
    match scope {
        Scope::Remote => REMOTE,
        Scope::Aws => &[".aws"],
        Scope::Kubernetes => &[".kube"],
        Scope::Docker => &[".docker"],
        Scope::Signing => &[],
    }
}

/// The most `~/.ssh/config` bytes read for its `IdentityFile` lines.
const SSH_CONFIG_LIMIT: u64 = 1 << 20;

/// The most public keys the configuration may add to the remote scope.
const NAMED_PUBLIC_KEYS_LIMIT: usize = 32;

/// The public key files the person's own `~/.ssh/config` names with an `IdentityFile` line, as
/// the sandbox will be told them.
///
/// ssh reads the `.pub` file beside an identity to choose which key the agent offers, so a key
/// named in the configuration needs the same read as one at a default name. A line naming `<name>`
/// adds `<name>.pub`, and one already ending in `.pub` adds that file. A name is absolute or
/// begins `~/` or `%d/`; one with any other `%` or a `$` adds nothing, since ssh would read it
/// from somewhere this cannot know, and neither does a line that is not valid UTF-8. A file is
/// added only where it is a regular file, the file a
/// link leads to is named `*.pub`, and that file is not in a credential location other than
/// `~/.ssh`. No private key name is added, whatever the line says. Only the user's own file is
/// read, and a repository's `core.sshCommand`, an `Include` and a `Match` are not followed. A file
/// that cannot be read adds nothing, and the configuration is read to a fixed length and for a
/// fixed count of keys.
fn named_public_keys(home: &Path) -> Vec<PathBuf> {
    use std::io::Read;
    let Ok(file) = std::fs::File::open(under(home, ".ssh/config")) else {
        return Vec::new();
    };
    let mut configuration = Vec::new();
    if file
        .take(SSH_CONFIG_LIMIT)
        .read_to_end(&mut configuration)
        .is_err()
    {
        return Vec::new();
    }
    let mut keys: Vec<PathBuf> = Vec::new();
    for line in configuration.split(|byte| *byte == b'\n') {
        // A lossy decode would name a different file than ssh opens.
        let Ok(line) = std::str::from_utf8(line) else {
            continue;
        };
        let line = line.strip_suffix('\r').unwrap_or(line);
        let Some(named) = identity_file(line) else {
            continue;
        };
        let Some(path) = judged_public_key(&named, home) else {
            continue;
        };
        if already_read(Scope::Remote, &path, home) || keys.contains(&path) {
            continue;
        }
        keys.push(path);
        if keys.len() == NAMED_PUBLIC_KEYS_LIMIT {
            break;
        }
    }
    keys
}

/// The path an `IdentityFile` line names, as a public key file's name, or `None` for any other
/// line or one that does not spell an absolute path.
fn identity_file(line: &str) -> Option<PathBuf> {
    let line = line.trim_start();
    let keyword = line.get(.."IdentityFile".len())?;
    if !keyword.eq_ignore_ascii_case("IdentityFile") {
        return None;
    }
    let rest = &line["IdentityFile".len()..];
    // ssh takes `Keyword value` and `Keyword=value`, and a separator is required.
    let rest = match rest.chars().next()? {
        '=' => &rest[1..],
        separator if separator.is_whitespace() => rest.trim_start().trim_start_matches('='),
        _ => return None,
    };
    let rest = rest.trim_start();
    let value = match rest.strip_prefix('"') {
        Some(quoted) => quoted.split('"').next()?,
        None => rest.split_whitespace().next()?,
    };
    if value.contains('$') || value.contains('\\') {
        return None;
    }
    let (home, beneath) = match value
        .strip_prefix("~/")
        .or_else(|| value.strip_prefix("%d/"))
    {
        Some(beneath) => (true, beneath),
        None => (false, value),
    };
    if beneath.contains('%') {
        return None;
    }
    public_name(if home {
        Path::new("~").join(beneath)
    } else {
        PathBuf::from(beneath)
    })
}

/// `path` with `.pub` on the end of its name, unless the name already ends in it.
fn public_name(path: PathBuf) -> Option<PathBuf> {
    if path.to_str()?.ends_with(".pub") {
        return Some(path);
    }
    let mut name = path.into_os_string();
    name.push(".pub");
    Some(PathBuf::from(name))
}

/// A public key file a configuration named, as the sandbox will be told it, or `None` where it is
/// refused.
pub(crate) fn judged_public_key(named: &Path, home: &Path) -> Option<PathBuf> {
    let spelled = match named.strip_prefix("~") {
        Ok(beneath) => home.join(beneath),
        Err(_) => named.to_path_buf(),
    };
    if !spelled.is_absolute()
        || spelled
            .components()
            .any(|part| part == Component::ParentDir)
    {
        return None;
    }
    // Where the name is a link, what it leads to is what ssh opens, and that is judged.
    let resolved = std::fs::canonicalize(&spelled).ok()?;
    let real_home = std::fs::canonicalize(home).unwrap_or_else(|_| home.to_path_buf());
    let a_public_key = resolved.extension().is_some_and(|ending| ending == "pub")
        && std::fs::metadata(&resolved).is_ok_and(|held| held.is_file());
    let in_another_credential_location = [home, real_home.as_path()].iter().any(|home| {
        CREDENTIAL_DIRECTORIES
            .iter()
            .chain(MACOS_CREDENTIAL_DIRECTORIES)
            .chain(LINUX_CREDENTIAL_DIRECTORIES)
            .filter(|row| **row != ".ssh")
            .any(|row| resolved.starts_with(under(home, row)))
    });
    (a_public_key && !in_another_credential_location).then_some(resolved)
}

/// The hosts ssh has verified, the one row of the remote scope that is also written.
const KNOWN_HOSTS: &str = ".ssh/known_hosts";

/// What the remote scope reads.
///
/// The ssh half is its configuration, the hosts it has verified, and the public key at each name
/// ssh looks for by default. The https half is the stores a credential helper reads, with the XDG
/// spelling of git's own named beside the first, and `gh`'s, named one directory at a time so that
/// `~/.config` is in no row.
const REMOTE: &[&str] = &[
    ".ssh/config",
    KNOWN_HOSTS,
    ".ssh/id_rsa.pub",
    ".ssh/id_ecdsa.pub",
    ".ssh/id_ecdsa_sk.pub",
    ".ssh/id_ed25519.pub",
    ".ssh/id_ed25519_sk.pub",
    ".ssh/id_xmss.pub",
    ".ssh/id_dsa.pub",
    ".gitconfig",
    ".git-credentials",
    ".config/git/credentials",
    ".netrc",
    ".config/gh",
];

/// The `git` operations that talk to a remote.
const REMOTE_OPERATIONS: [&str; 5] = ["push", "fetch", "pull", "clone", "ls-remote"];

/// The long options of those operations whose value is a program, or a directory or setting that
/// names one: a hook template, `core.sshCommand` through `--config`, a merge strategy's
/// `git-merge-*`.
const OPTIONS_NAMING_A_PROGRAM: [&str; 6] = [
    "upload-pack",
    "receive-pack",
    "exec",
    "template",
    "config",
    "strategy",
];

/// Whether `args`, the argv after `git`, is an operation that talks to a remote and names no
/// program of its own.
///
/// `-C <directory>` is the one option allowed in front of the operation, since it moves where git
/// runs and not what it runs. Any other there, `-c`, `--exec-path`, `--config-env` or one git adds
/// later, is refused rather than read. The operation is matched exactly, so an alias, whatever
/// the configuration says it expands to, carries nothing. After it, a long option is refused where
/// it is any abbreviation git would accept of one naming a program, and a short one where it is the
/// spelling of one for that operation: `-u` and `-c` for `clone`, and `-s` for `pull`, since `-u`
/// is harmless for `push` and `fetch`. An argument holding `transport::` names the remote helper
/// git starts for it, `git-remote-<transport>`, and so does a `<scheme>://` address whose scheme is
/// none git reaches by itself or through a helper it ships, so both are refused too. What follows
/// `--` is read as well, since `--` can be the value of the option in front of it.
fn a_remote_operation(args: &[String]) -> bool {
    let mut rest = args.iter().map(String::as_str);
    let operation = loop {
        match rest.next() {
            Some("-C") => {
                if rest.next().is_none() {
                    return false;
                }
            }
            Some(operation) => break operation,
            None => return false,
        }
    };
    let short: &[char] = match operation {
        "clone" => &['u', 'c'],
        "pull" => &['s'],
        _ if REMOTE_OPERATIONS.contains(&operation) => &[],
        _ => return false,
    };
    rest.all(|argument| !names_a_program(argument, short))
}

/// The `git` operations that write a commit or a tag, and so sign it where the person's
/// configuration says to.
const SIGNING_OPERATIONS: [&str; 7] = [
    "commit",
    "merge",
    "rebase",
    "cherry-pick",
    "revert",
    "am",
    "tag",
];

/// Whether `args`, the argv after `git`, is an operation that signs what it writes and names no
/// program of its own.
///
/// Read as [`a_remote_operation`] reads an argv: `-C <directory>` is the one option in front of the
/// operation, which is matched exactly. After it, a long option is refused where it is any
/// abbreviation of `--exec` or `--strategy`, since the first runs a command between commits and the
/// second `git-merge-<name>` from the search path, and so is a short one that spells either for the
/// operation: `-x` and `-s` for `rebase`, `-s` for `merge`. Unlike a remote operation the argv holds
/// free text, a commit message with `::` in it, so an address is not read.
fn a_signing_operation(args: &[String]) -> bool {
    let mut rest = args.iter().map(String::as_str);
    let operation = loop {
        match rest.next() {
            Some("-C") => {
                if rest.next().is_none() {
                    return false;
                }
            }
            Some(operation) => break operation,
            None => return false,
        }
    };
    let short: &[char] = match operation {
        "rebase" => &['x', 's'],
        "merge" => &['s'],
        _ if SIGNING_OPERATIONS.contains(&operation) => &[],
        _ => return false,
    };
    rest.all(|argument| {
        if let Some(long) = argument.strip_prefix("--") {
            let name = long.split('=').next().unwrap_or(long);
            return name.is_empty()
                || !["exec", "strategy"]
                    .iter()
                    .any(|option| option.starts_with(name));
        }
        argument
            .strip_prefix('-')
            .is_none_or(|cluster| !cluster.chars().any(|flag| short.contains(&flag)))
    })
}

/// Whether one argument after the operation names a program for `git` to run.
fn names_a_program(argument: &str, short: &[char]) -> bool {
    if argument.contains("::") || names_a_scheme_git_hands_to_a_helper(argument) {
        return true;
    }
    if let Some(long) = argument.strip_prefix("--") {
        let name = long.split('=').next().unwrap_or(long);
        return !name.is_empty()
            && OPTIONS_NAMING_A_PROGRAM
                .iter()
                .any(|option| option.starts_with(name));
    }
    argument
        .strip_prefix('-')
        .is_some_and(|cluster| cluster.chars().any(|flag| short.contains(&flag)))
}

/// The schemes git reaches a remote through by itself or through a helper it ships.
const SCHEMES_GIT_HANDLES: [&str; 9] = [
    "ssh", "git", "file", "http", "https", "ftp", "ftps", "git+ssh", "ssh+git",
];

/// Whether `argument` holds a `<scheme>://` address git hands to `git-remote-<scheme>`, a program
/// found on the search path, because the scheme is none of its own. git names the helper with the
/// scheme as written, so `HTTPS://` starts `git-remote-HTTPS`.
fn names_a_scheme_git_hands_to_a_helper(argument: &str) -> bool {
    argument.match_indices("://").any(|(at, _)| {
        let scheme = argument[..at]
            .rsplit(|character: char| {
                !(character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.'))
            })
            .next()
            .unwrap_or_default();
        !SCHEMES_GIT_HANDLES.contains(&scheme)
    })
}

/// The `gh` commands that talk to the host. gh runs each ahead of an alias or an extension given
/// the same name, so the first word of an argv names what runs.
const GH_COMMANDS: [&str; 21] = [
    "api",
    "attestation",
    "auth",
    "cache",
    "gist",
    "gpg-key",
    "issue",
    "label",
    "org",
    "pr",
    "project",
    "release",
    "repo",
    "ruleset",
    "run",
    "search",
    "secret",
    "ssh-key",
    "status",
    "variable",
    "workflow",
];

/// Whether `args`, the argv after `gh`, starts with one of gh's own commands and hands nothing on
/// to `git`.
///
/// A first word gh has no command for is an alias or an extension, a program a configuration file
/// or an install chose. `--` is what `gh repo clone` and `gh repo fork` write in front of the
/// options they hand to `git` unread, so an argv holding one carries nothing, whatever follows it.
fn a_command_of_ghs_own(args: &[String]) -> bool {
    args.first()
        .is_some_and(|command| GH_COMMANDS.contains(&command.as_str()))
        && !args.iter().any(|argument| argument == "--")
}

/// Whether `args` holds `option`, alone or with its value after `=`.
fn names_an_option(args: &[String], option: &str) -> bool {
    args.iter().any(|argument| {
        argument
            .strip_prefix(option)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('='))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::{Prelude, base};
    use crate::policy::PathKind;
    #[cfg(unix)]
    use crate::testutil::scratch_dir;
    use std::path::PathBuf;

    const A_HOME: &str = "/home/a-person";
    const THE_SESSIONS_TEMPORARY_DIRECTORY: &str = "/scratch/tmp-of-this-session";

    const EVERY_SCOPE: [Scope; 4] = [Scope::Remote, Scope::Aws, Scope::Kubernetes, Scope::Docker];

    /// SANDBOX-26: a request is a word of the fixed menu compared exactly. `root` is a user, not a
    /// scope; `Remote` and the empty string are near misses that a case-folding or empty-matching
    /// lookup would accept and so lend a credential the planner did not name.
    #[test]
    fn a_request_is_a_word_of_the_menu_and_nothing_near_it() {
        for word in ["root", "Remote", "", " aws", "aws ", "all", "*", "ssh"] {
            assert_eq!(Requested::named(word), None, "{word:?} named a request");
        }
        for word in Requested::MENU {
            let request = Requested::named(word).unwrap_or_else(|| panic!("{word} is not named"));
            assert_eq!(request.name(), word);
        }
    }

    /// SANDBOX-26: the menu is every scope and every toolchain, so a name the prompt offers cannot
    /// fail to parse and a scope cannot exist that the planner has no word for.
    #[test]
    fn the_menu_is_every_scope_and_every_toolchain() {
        let scopes = EVERY_SCOPE.map(Scope::name);
        assert!(scopes.iter().all(|name| Requested::MENU.contains(name)));
        assert!(Requested::MENU.contains(&Scope::Signing.name()));
        for toolchain in [
            Toolchain::Cargo,
            Toolchain::Node,
            Toolchain::Python,
            Toolchain::Go,
            Toolchain::Maven,
            Toolchain::Gradle,
        ] {
            assert!(Requested::MENU.contains(&toolchain.name()));
        }
        assert_eq!(Requested::MENU.len(), scopes.len() + 1 + 6);
    }

    const GIT: &str = "/usr/bin/git";

    fn the_environment(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }

    fn gh_reads(pairs: &[(&str, &str)]) -> Option<PathBuf> {
        reached(Scope::Remote, "gh", pairs).into_iter().next()
    }

    /// The paths the environment `pairs` moves `program`'s `scope` to, under `A_HOME`.
    fn reached(scope: Scope, program: &str, pairs: &[(&str, &str)]) -> Vec<PathBuf> {
        environment_reach(scope, program, Path::new(A_HOME), &the_environment(pairs))
            .into_iter()
            .map(|reach| reach.path)
            .collect()
    }

    fn argv(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_string).collect()
    }

    fn of(resolved: &str, line: &str) -> Option<Scope> {
        Scope::of(Path::new(resolved), &argv(line), &[])
    }

    fn a_scope(scope: Scope) -> SandboxPolicy {
        scope.grant(SandboxPolicy::strict(), Path::new(A_HOME))
    }

    fn granted_paths(policy: &SandboxPolicy) -> Vec<PathBuf> {
        policy
            .readable
            .iter()
            .cloned()
            .chain(policy.writable.iter().map(|row| row.path.clone()))
            .collect()
    }

    fn reaches(policy: &SandboxPolicy, path: &str) -> bool {
        granted_paths(policy)
            .iter()
            .any(|row| Path::new(path).starts_with(row))
    }

    /// A push is how most sessions end, so a profile refusing one is a profile somebody turns
    /// off. Each operation that talks to a remote carries the scope wherever the repository it
    /// runs in is, and so does each of `gh`'s own commands, since every one talks to the same host
    /// with the same token.
    #[test]
    fn an_operation_that_talks_to_a_remote_carries_the_remote_scope() {
        for line in [
            "push",
            "push origin main",
            "fetch",
            "pull",
            "clone https://github.com/brave/bravebot.git",
            "ls-remote origin",
            "-C ../another-checkout push",
            "-C a -C b fetch",
        ] {
            assert_eq!(of(GIT, line), Some(Scope::Remote), "git {line}");
        }
        for line in [
            "pr create --fill",
            "api user",
            "run list",
            "repo clone brave/bravebot",
            "auth status",
            "workflow run a-workflow",
        ] {
            assert_eq!(
                of("/usr/local/bin/gh", line),
                Some(Scope::Remote),
                "gh {line}"
            );
        }
        assert_eq!(
            of("/opt/homebrew/Cellar/git/2.51.0/bin/git", "push"),
            Some(Scope::Remote)
        );
    }

    /// The scope follows the operation rather than the first word of the argv. One keyed on
    /// `git` alone hands `~/.git-credentials` to a `git status`, a `git submodule update` and an
    /// alias whose expansion a configuration file decides. A `git commit` is not among them for the
    /// remote scope: it signs, and what it is lent for that is the signing scope.
    #[test]
    fn a_git_operation_that_talks_to_no_remote_carries_none() {
        for line in [
            "",
            "status",
            "log",
            "submodule update --init",
            "remote update",
            "p",
            "PUSH",
            "-C",
            "-C a-directory",
            "help push",
        ] {
            assert_eq!(of(GIT, line), None, "git {line}");
        }
        assert_ne!(of(GIT, "commit -m a-message"), Some(Scope::Remote));
    }

    /// A first word `gh` has no command for runs an alias or an extension, a program a
    /// configuration file or an install chose, and what follows `--` is handed to `git` unread,
    /// so `gh repo clone a/b -- --upload-pack=` is the option `git` itself is refused for.
    #[test]
    fn a_gh_argv_that_runs_a_program_gh_did_not_write_carries_none() {
        for line in [
            "",
            "an-alias",
            "an-extension",
            "copilot suggest",
            "extension exec an-extension",
            "alias set a-name !a-program",
            "codespace ssh",
            "browse",
            "config get editor",
            "--version",
            "-R brave/bravebot pr list",
            "PR list",
            "repo clone brave/bravebot -- --upload-pack=a-program",
            "repo clone brave/bravebot -- --template=/tmp/hooks",
            "repo fork brave/bravebot --clone -- -c core.sshCommand=a-program",
            "pr list --search --",
        ] {
            assert_eq!(of("/usr/local/bin/gh", line), None, "gh {line}");
        }
    }

    /// An option in front of the operation is how `git` is made to run something else:
    /// `-c core.sshCommand=`, `-c alias.push=!` and `--exec-path` each put a program of the
    /// line's choosing where ssh or the operation would be. None of them is read for what it
    /// says, so one git adds later carries nothing either.
    #[test]
    fn an_option_in_front_of_the_operation_carries_none() {
        for line in [
            "-c core.sshCommand=a-program push",
            "-c alias.push=!a-program push",
            "-c credential.helper=!a-program fetch",
            "-ccore.sshCommand=a-program push",
            "--exec-path=/tmp/a-directory push",
            "--config-env=core.sshCommand=A_VARIABLE push",
            "--git-dir=/tmp/a-repository push",
            "--no-pager push",
            "-P push",
            "-C a-directory -c core.sshCommand=a-program push",
        ] {
            assert_eq!(of(GIT, line), None, "git {line}");
        }
    }

    /// The operations have options of their own whose value is run, and git accepts any
    /// unambiguous abbreviation of a long one, so a check on the full name alone is passed by
    /// `--upload=`. A `transport::` remote is the same thing written as an address: git starts
    /// `git-remote-<transport>` for it, and `git-remote-<scheme>` for a `<scheme>://` one whose
    /// scheme it has no transport of its own for, wherever in the argument the address is.
    #[test]
    fn an_operation_option_naming_a_program_carries_none() {
        for line in [
            "fetch --upload-pack=a-program origin",
            "fetch --upload-pack a-program origin",
            "fetch --upload=a-program origin",
            "fetch --u=a-program origin",
            "ls-remote --upload-pack=a-program origin",
            "push --receive-pack=a-program origin",
            "push --receive=a-program origin",
            "push --exec=a-program origin",
            "push --ex=a-program origin",
            "clone --template=/tmp/hooks https://example.com/a.git",
            "clone --temp=/tmp/hooks https://example.com/a.git",
            "clone --config=core.sshCommand=a-program https://example.com/a.git",
            "clone --conf core.sshCommand=a-program https://example.com/a.git",
            "clone -c core.sshCommand=a-program https://example.com/a.git",
            "clone -u a-program https://example.com/a.git",
            "clone -qu a-program https://example.com/a.git",
            "pull --strategy=a-strategy",
            "pull --str=a-strategy",
            "pull -s a-strategy",
            "clone -b -- --upload-pack=a-program https://example.com/a.git",
            "fetch ext::a-program",
            "clone a-helper::https://example.com/a.git",
            "push origin -- --exec=a-program",
            "fetch a-helper://example.com/a.git",
            "clone a-b.c://example.com/a.git",
            "ls-remote x+https://example.com/a.git",
            "fetch a-ssh://example.com/a.git",
            "fetch a.git://example.com/a.git",
            "clone HTTPS://example.com/a.git",
            "push --repo=a-helper://example.com/a.git",
            "clone https://example.com/a.git?a-helper://",
            "fetch ://example.com/a.git",
        ] {
            assert_eq!(of(GIT, line), None, "git {line}");
        }
    }

    /// Every program here reads a variable that names a program or a configuration file to run
    /// it with. An assignment in front of the stage is shown in the plan, but what it would run
    /// is not, so the stage carries nothing rather than a check on which names are harmless.
    #[test]
    fn a_stage_with_an_assignment_in_front_of_it_carries_none() {
        for (resolved, line, name) in [
            (GIT, "push", "GIT_SSH_COMMAND"),
            (GIT, "fetch", "GIT_CONFIG_GLOBAL"),
            (GIT, "push", "GIT_TRACE"),
            ("/usr/local/bin/gh", "pr view", "GH_PAGER"),
            ("/usr/local/aws-cli/aws", "s3 ls", "AWS_CONFIG_FILE"),
            ("/usr/local/bin/kubectl", "get pods", "KUBECONFIG"),
            ("/usr/local/bin/docker", "pull an-image", "DOCKER_CONFIG"),
        ] {
            let assignment = [(name.to_string(), "a-value".to_string())];
            assert_eq!(
                Scope::of(Path::new(resolved), &argv(line), &assignment),
                None,
                "{name}= {resolved} {line}"
            );
            assert!(
                Scope::of(Path::new(resolved), &argv(line), &[]).is_some(),
                "{resolved} {line} carries nothing even without {name}="
            );
        }
    }

    /// The options and addresses an ordinary push, fetch or clone is written with run nothing
    /// git did not ship, so refusing every one refuses the command people actually type.
    #[test]
    fn an_option_that_runs_nothing_keeps_the_scope() {
        for line in [
            "push -u origin main",
            "push --set-upstream origin main",
            "push --force-with-lease origin main",
            "push --tags",
            "push --delete origin a-branch",
            "push -o ci.skip origin main",
            "fetch --all --prune",
            "fetch -u origin main",
            "fetch --depth=1 origin",
            "fetch --recurse-submodules=no",
            "pull --rebase",
            "pull --stat",
            "pull -X ours",
            "pull --strategy-option=ours",
            "clone --depth 1 https://example.com/a.git",
            "clone -b main https://example.com/a.git a-directory",
            "clone --recurse-submodules https://example.com/a.git",
            "clone -- https://example.com/a.git",
            "clone ssh://git@example.com/a.git",
            "clone git://example.com/a.git",
            "clone file:///tmp/a-repository",
            "clone git+ssh://git@example.com/a.git",
            "clone ssh+git://git@example.com/a.git",
            "fetch http://example.com/a.git",
            "fetch ftp://example.com/a.git",
            "fetch ftps://example.com/a.git",
            "push --repo=https://example.com/a.git",
            "ls-remote --heads origin",
            "ls-remote --tags origin",
        ] {
            assert_eq!(of(GIT, line), Some(Scope::Remote), "git {line}");
        }
    }

    /// A tool's configuration file can name the program that fetches its credential, an exec
    /// plugin in a kubeconfig or a `credsStore` helper, so pointing the tool at another one is
    /// choosing what runs with the directory it reads.
    #[test]
    fn a_tool_pointed_at_another_configuration_file_carries_none() {
        for (resolved, line) in [
            (
                "/usr/local/bin/kubectl",
                "--kubeconfig ./a-kubeconfig get pods",
            ),
            (
                "/usr/local/bin/kubectl",
                "get pods --kubeconfig=./a-kubeconfig",
            ),
            (
                "/usr/local/bin/docker",
                "--config ./a-directory pull an-image",
            ),
            (
                "/usr/local/bin/docker",
                "--config=./a-directory pull an-image",
            ),
        ] {
            assert_eq!(of(resolved, line), None, "{resolved} {line}");
        }
        assert_eq!(
            of("/usr/local/bin/kubectl", "get pods --kubeconfigs"),
            Some(Scope::Kubernetes)
        );
        assert_eq!(
            of("/usr/local/bin/docker", "run --config-file a-file an-image"),
            Some(Scope::Docker)
        );
    }

    /// A scope is reach a plan's stage is lent because its operation needs it, so a program
    /// that merely sits beside one, or talks to a remote through `git` without being it, is lent
    /// nothing.
    #[test]
    fn a_program_no_scope_knows_carries_none() {
        for resolved in [
            "/usr/bin/ssh",
            "/usr/bin/scp",
            "/usr/bin/curl",
            "/usr/bin/git-lfs",
            "/usr/lib/git-core/git-remote-https",
            "/home/a-person/.cargo/bin/cargo",
            "/usr/bin/make",
            "/usr/local/bin/ghq",
            "/usr/local/bin/aws-vault",
            "/usr/local/bin/docker-compose",
            "/",
        ] {
            assert_eq!(of(resolved, "push"), None, "{resolved}");
        }
    }

    /// Each stage reaches its own row and no other. One shared scope hands `~/.aws` to a
    /// `docker pull` and `~/.git-credentials` to a `kubectl get`, which is every stage lent
    /// whatever any stage of the session needed.
    #[test]
    fn a_stage_reaches_its_own_scope_and_no_other() {
        let owned = [
            (Scope::Remote, "/home/a-person/.git-credentials"),
            (Scope::Remote, "/home/a-person/.config/gh/hosts.yml"),
            (Scope::Aws, "/home/a-person/.aws/credentials"),
            (Scope::Kubernetes, "/home/a-person/.kube/config"),
            (Scope::Docker, "/home/a-person/.docker/config.json"),
        ];

        for scope in EVERY_SCOPE {
            let policy = a_scope(scope);
            for (owner, path) in owned {
                assert_eq!(
                    reaches(&policy, path),
                    owner == scope,
                    "the {scope:?} scope and {path}, which is {owner:?}'s"
                );
            }
        }
    }

    /// A push signs through the agent, so no scope needs a private key, and a row naming
    /// `~/.ssh` whole is every key in it. The public half and the configuration are what ssh
    /// reads to name an identity to the agent.
    #[test]
    fn no_scope_reaches_a_private_key_or_the_directory_holding_one() {
        for scope in EVERY_SCOPE {
            let policy = a_scope(scope);
            for private in [
                "/home/a-person/.ssh/id_rsa",
                "/home/a-person/.ssh/id_ecdsa",
                "/home/a-person/.ssh/id_ecdsa_sk",
                "/home/a-person/.ssh/id_ed25519",
                "/home/a-person/.ssh/id_ed25519_sk",
                "/home/a-person/.ssh/id_xmss",
                "/home/a-person/.ssh/id_dsa",
                "/home/a-person/.ssh/a-key-of-its-own",
                "/home/a-person/.ssh/authorized_keys",
                "/home/a-person/.gnupg/private-keys-v1.d/a-key",
                "/home/a-person/Library/Keychains/login.keychain-db",
                "/home/a-person/.cargo/credentials.toml",
                "/home/a-person/.npmrc",
            ] {
                assert!(
                    !reaches(&policy, private),
                    "the {scope:?} scope reaches {private}"
                );
            }
            for row in granted_paths(&policy) {
                for too_wide in [
                    A_HOME,
                    "/home/a-person/.ssh",
                    "/home/a-person/.config",
                    "/home/a-person/.config/git",
                ] {
                    assert_ne!(row, PathBuf::from(too_wide), "the {scope:?} scope");
                }
            }
        }
        assert!(reaches(
            &a_scope(Scope::Remote),
            "/home/a-person/.ssh/id_ed25519.pub"
        ));
    }

    /// A host ssh cannot record is a push that fails, and an account that has never made one has
    /// no file to record it in, so `known_hosts` is written, as a file, and nothing else is. A
    /// tool's directory written to is a command the person's own shell runs later.
    #[test]
    fn the_one_row_a_scope_writes_is_the_hosts_ssh_has_verified() {
        let remote = a_scope(Scope::Remote);
        assert_eq!(remote.writable.len(), 1);
        assert_eq!(
            remote.writable[0].path,
            PathBuf::from("/home/a-person/.ssh/known_hosts")
        );
        assert_eq!(remote.writable[0].kind, PathKind::File);

        for scope in [Scope::Aws, Scope::Kubernetes, Scope::Docker] {
            assert!(
                a_scope(scope).writable.is_empty(),
                "the {scope:?} scope writes"
            );
        }
    }

    /// Which transport a remote uses is written in a configuration file, and no file's contents
    /// decide a scope, so the remote scope is both halves at once. The https half names `gh`'s
    /// store one directory at a time: `~/.config` whole is every other program's configuration.
    #[test]
    fn the_remote_scope_reaches_both_transports() {
        let remote = a_scope(Scope::Remote);
        for path in [
            "/home/a-person/.ssh/config",
            "/home/a-person/.ssh/known_hosts",
            "/home/a-person/.ssh/id_ed25519.pub",
            "/home/a-person/.ssh/id_rsa.pub",
            "/home/a-person/.gitconfig",
            "/home/a-person/.git-credentials",
            "/home/a-person/.config/git/credentials",
            "/home/a-person/.netrc",
            "/home/a-person/.config/gh/hosts.yml",
        ] {
            assert!(reaches(&remote, path), "the remote scope misses {path}");
        }
        for path in [
            "/home/a-person/.config/another-program/token",
            "/home/a-person/.config/git/another-file",
        ] {
            assert!(!reaches(&remote, path), "the remote scope reaches {path}");
        }
    }

    /// A home on disk with `~/.ssh/config` holding `configuration`, and the files under it that
    /// `files` names (each with a body that is not read).
    #[cfg(unix)]
    fn a_home_with(name: &str, configuration: &str, files: &[&str]) -> PathBuf {
        let home = scratch_dir(name);
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".ssh")).unwrap();
        for file in files {
            let path = home.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "a key").unwrap();
        }
        std::fs::write(home.join(".ssh/config"), configuration).unwrap();
        std::fs::canonicalize(home).unwrap()
    }

    /// What the remote scope reads for `home` beyond its fixed rows.
    #[cfg(unix)]
    fn read_for_the_configuration(home: &Path) -> Vec<PathBuf> {
        let fixed: Vec<PathBuf> = REMOTE.iter().map(|row| under(home, row)).collect();
        Scope::Remote
            .grant(SandboxPolicy::strict(), home)
            .readable
            .into_iter()
            .filter(|row| !fixed.contains(row))
            .collect()
    }

    /// `IdentityFile ~/.ssh/work` makes ssh read `work.pub` to choose the key the agent offers. A
    /// push by a key at a name ssh was told, with `IdentitiesOnly yes`, fails without that read.
    /// The keyword is case-insensitive, the value may follow `=`, and `%d` is the home.
    #[test]
    #[cfg(unix)]
    fn an_identity_file_in_the_ssh_configuration_adds_its_public_key() {
        let home = a_home_with(
            "identity-files-named",
            "Host github.com\n  User git\n  IdentityFile ~/.ssh/work\n  identityfile=~/.ssh/other.pub\n\
             IdentityFile \"%d/.ssh/quoted\"\nIdentitiesOnly yes\n",
            &[
                ".ssh/work",
                ".ssh/work.pub",
                ".ssh/other.pub",
                ".ssh/quoted",
                ".ssh/quoted.pub",
            ],
        );
        let mut read = read_for_the_configuration(&home);
        read.sort();
        assert_eq!(
            read,
            [
                home.join(".ssh/other.pub"),
                home.join(".ssh/quoted.pub"),
                home.join(".ssh/work.pub"),
            ]
        );
        let policy = Scope::Remote.grant(SandboxPolicy::strict(), &home);
        for private in [".ssh/work", ".ssh/quoted", ".ssh"] {
            assert!(
                !granted_paths(&policy).contains(&home.join(private)),
                "the remote scope reads {private}"
            );
        }
        assert!(
            policy
                .writable
                .iter()
                .all(|row| row.path == home.join(".ssh/known_hosts"))
        );
        std::fs::remove_dir_all(&home).unwrap();
    }

    /// A line adds a public key and nothing else: not a name with no `.pub` file, not a private
    /// key behind a `.pub` link, not a file in another credential location, not a directory, and
    /// not a path whose spelling ssh resolves somewhere this cannot know.
    #[test]
    #[cfg(unix)]
    fn an_identity_file_that_is_not_a_public_key_adds_nothing() {
        use std::os::unix::fs::symlink;
        let home = a_home_with(
            "identity-files-refused",
            "IdentityFile ~/.ssh/missing\n\
             IdentityFile ~/.ssh/leak.pub\n\
             IdentityFile ~/.ssh/aws.pub\n\
             IdentityFile ~/.ssh/state.pub\n\
             IdentityFile ~/.ssh/a-directory.pub\n\
             IdentityFile ~/.ssh/../.aws/creds.pub\n\
             IdentityFile ~/.aws/creds\n\
             IdentityFile ~/.bravebot/gateway\n\
             IdentityFile relative.pub\n\
             IdentityFile ~/.ssh/%h.pub\n\
             IdentityFile $HOME/.ssh/home.pub\n\
             IdentityFileOther ~/.ssh/other.pub\n\
             IdentityFile\n\
             # IdentityFile ~/.ssh/commented.pub\n",
            &[
                ".ssh/id_leaked",
                ".aws/creds.pub",
                ".bravebot/gateway.pub",
                ".ssh/home.pub",
                ".ssh/other.pub",
                ".ssh/commented.pub",
                "relative.pub",
            ],
        );
        symlink(home.join(".ssh/id_leaked"), home.join(".ssh/leak.pub")).unwrap();
        symlink(home.join(".aws/creds.pub"), home.join(".ssh/aws.pub")).unwrap();
        symlink(
            home.join(".bravebot/gateway.pub"),
            home.join(".ssh/state.pub"),
        )
        .unwrap();
        std::fs::create_dir_all(home.join(".ssh/a-directory.pub")).unwrap();
        assert_eq!(read_for_the_configuration(&home), Vec::<PathBuf>::new());
        std::fs::remove_dir_all(&home).unwrap();
    }

    /// A link is judged by where it leads: the row is the file ssh opens, and a link out of
    /// `~/.ssh` to a public key somewhere else is that file and nothing around it.
    #[test]
    #[cfg(unix)]
    fn a_named_public_key_behind_a_link_is_read_where_it_leads() {
        use std::os::unix::fs::symlink;
        let home = a_home_with(
            "identity-files-linked",
            "IdentityFile ~/.ssh/linked\n",
            &["keys/real.pub", "keys/beside-it"],
        );
        symlink(home.join("keys/real.pub"), home.join(".ssh/linked.pub")).unwrap();
        assert_eq!(
            read_for_the_configuration(&home),
            [home.join("keys/real.pub")]
        );
        std::fs::remove_dir_all(&home).unwrap();
    }

    /// A line holding bytes that are not UTF-8 is skipped whole. A lossy decode would spell its
    /// name with U+FFFD and grant the file of that spelling, which ssh never opens. The lines
    /// either side of it are still read.
    #[test]
    #[cfg(unix)]
    fn an_identity_file_line_that_is_not_text_adds_nothing_and_its_lossy_lookalike_is_not_read() {
        let home = a_home_with(
            "identity-files-not-text",
            "",
            &[
                ".ssh/before.pub",
                ".ssh/tool-\u{FFFD}.pub",
                ".ssh/after.pub",
            ],
        );
        std::fs::write(
            home.join(".ssh/config"),
            b"IdentityFile ~/.ssh/before\nIdentityFile ~/.ssh/tool-\xff\nIdentityFile ~/.ssh/after\n",
        )
        .unwrap();
        let mut read = read_for_the_configuration(&home);
        read.sort();
        assert_eq!(
            read,
            [home.join(".ssh/after.pub"), home.join(".ssh/before.pub")]
        );
        std::fs::remove_dir_all(&home).unwrap();
    }

    /// On a file system that holds a name that is not UTF-8, the file the line spells exists
    /// beside its lossy lookalike, and neither is granted.
    #[test]
    #[cfg(target_os = "linux")]
    fn an_identity_file_whose_name_is_not_text_adds_neither_it_nor_its_lookalike() {
        use std::os::unix::ffi::OsStrExt;
        let home = a_home_with(
            "identity-files-not-text-on-disk",
            "",
            &[".ssh/tool-\u{FFFD}.pub"],
        );
        let spelled = home
            .join(".ssh")
            .join(std::ffi::OsStr::from_bytes(b"tool-\xff.pub"));
        std::fs::write(&spelled, "a key").unwrap();
        std::fs::write(home.join(".ssh/config"), b"IdentityFile ~/.ssh/tool-\xff\n").unwrap();
        assert_eq!(read_for_the_configuration(&home), Vec::<PathBuf>::new());
        std::fs::remove_dir_all(&home).unwrap();
    }

    /// The configuration is the person's, read to a fixed length and for a fixed count of keys,
    /// and a home without one reads only the fixed rows. Neither the aws, kubernetes nor docker
    /// scope reads it.
    #[test]
    #[cfg(unix)]
    fn the_ssh_configuration_adds_a_bounded_number_of_keys_to_the_remote_scope_alone() {
        let names: Vec<String> = (0..40)
            .map(|number| format!(".ssh/key-{number}.pub"))
            .collect();
        let files: Vec<&str> = names.iter().map(String::as_str).collect();
        let configuration: String = (0..40)
            .map(|number| format!("IdentityFile ~/.ssh/key-{number}.pub\n"))
            .collect();
        let home = a_home_with("identity-files-counted", &configuration, &files);
        assert_eq!(
            read_for_the_configuration(&home).len(),
            NAMED_PUBLIC_KEYS_LIMIT
        );
        for scope in [Scope::Aws, Scope::Kubernetes, Scope::Docker] {
            let policy = scope.grant(SandboxPolicy::strict(), &home);
            assert!(!reaches(
                &policy,
                home.join(".ssh/key-0.pub").to_str().unwrap()
            ));
        }
        std::fs::remove_file(home.join(".ssh/config")).unwrap();
        assert_eq!(read_for_the_configuration(&home), Vec::<PathBuf>::new());
        std::fs::remove_dir_all(&home).unwrap();
    }

    /// A scope is added to the base and to whatever list the stage brings, so what those granted
    /// has to reach the backend as it was.
    #[test]
    fn a_scope_leaves_the_policy_it_is_added_to_as_it_was() {
        let base = base(
            Prelude::Linux,
            Path::new(THE_SESSIONS_TEMPORARY_DIRECTORY),
            None,
            Some(Path::new(A_HOME)),
        );

        for scope in EVERY_SCOPE {
            let policy = scope.grant(base.clone(), Path::new(A_HOME));
            assert!(policy.readable.starts_with(&base.readable), "{scope:?}");
            assert!(policy.writable.starts_with(&base.writable), "{scope:?}");
            assert_eq!(policy.allow_network, base.allow_network, "{scope:?}");
            assert_eq!(
                policy.allow_subprocesses, base.allow_subprocesses,
                "{scope:?}"
            );
        }
    }

    /// A person who keeps a second account's configuration elsewhere has `gh` refused at the
    /// path before it runs a subcommand. The directory `gh` would use is read, and nothing wider.
    #[test]
    fn gh_reads_the_configuration_directory_its_environment_names() {
        assert_eq!(
            gh_reads(&[("GH_CONFIG_DIR", "/home/a-person/.config/gh-second")]),
            Some(PathBuf::from("/home/a-person/.config/gh-second"))
        );
        assert_eq!(
            gh_reads(&[("XDG_CONFIG_HOME", "/home/a-person/xdg")]),
            Some(PathBuf::from("/home/a-person/xdg/gh"))
        );
        assert_eq!(
            gh_reads(&[
                ("GH_CONFIG_DIR", "/home/a-person/first"),
                ("XDG_CONFIG_HOME", "/home/a-person/xdg"),
            ]),
            Some(PathBuf::from("/home/a-person/first"))
        );
        assert_eq!(gh_reads(&[]), None);
        assert_eq!(gh_reads(&[("GH_CONFIG_DIR", "")]), None);
        let policy = SandboxPolicy::strict()
            .allow_read(gh_reads(&[("GH_CONFIG_DIR", "/home/a-person/second")]).unwrap());
        assert!(reaches(&policy, "/home/a-person/second/hosts.yml"));
        assert!(!reaches(&policy, "/home/a-person/.ssh/id_ed25519"));
    }

    /// A variable the session inherited is the person's own, but a directory that is a whole
    /// place no scope reaches, or a spelling that moves under the check, is not one `gh` is lent.
    #[test]
    fn a_gh_directory_that_is_too_wide_or_not_a_path_is_refused() {
        for directory in [
            "relative/gh",
            "gh",
            "/home/a-person/../another-person/gh",
            "/home/a-person",
            "/home",
            "/",
            "/home/a-person/.ssh",
            "/home/a-person/.ssh/gh",
            "/home/a-person/.config",
            "/home/a-person/.cache",
            "/home/a-person/Library",
        ] {
            assert_eq!(
                gh_reads(&[("GH_CONFIG_DIR", directory)]),
                None,
                "{directory}"
            );
        }
        assert_eq!(
            gh_reads(&[("XDG_CONFIG_HOME", "/home/a-person")]),
            Some(PathBuf::from("/home/a-person/gh"))
        );
        assert_eq!(
            gh_reads(&[("XDG_CONFIG_HOME", "/home/a-person/.ssh")]),
            None
        );
        assert_eq!(gh_reads(&[("XDG_CONFIG_HOME", "relative")]), None);
    }

    /// A link is judged by where it leads, so one into `~/.ssh` or to the home is refused though
    /// its own spelling is neither.
    #[cfg(unix)]
    #[test]
    fn a_gh_directory_that_is_a_link_is_judged_by_where_it_leads() {
        use std::os::unix::fs::symlink;
        let home = scratch_dir("gh-configuration-links");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".ssh")).unwrap();
        std::fs::create_dir_all(home.join("second")).unwrap();
        symlink(home.join(".ssh"), home.join("to-ssh")).unwrap();
        symlink(&home, home.join("to-home")).unwrap();
        symlink(home.join("second"), home.join("to-second")).unwrap();
        let reads = |directory: &Path| {
            environment_reach(
                Scope::Remote,
                "gh",
                &home,
                &the_environment(&[("GH_CONFIG_DIR", directory.to_str().unwrap())]),
            )
            .into_iter()
            .next()
            .map(|reach| reach.path)
        };
        assert_eq!(reads(&home.join("to-ssh")), None);
        assert_eq!(reads(&home.join("to-home")), None);
        assert_eq!(
            reads(&home.join("to-second")),
            Some(std::fs::canonicalize(home.join("second")).unwrap())
        );
        std::fs::remove_dir_all(&home).unwrap();
    }

    /// A tool whose configuration a variable moves reads it where the variable says, and the
    /// scope of the tool beside it does not follow. The grant is the file a variable names, never
    /// the directory around it.
    #[test]
    fn a_variable_that_moves_a_tools_configuration_moves_its_row() {
        assert_eq!(
            reached(
                Scope::Docker,
                "docker",
                &[("DOCKER_CONFIG", "/home/a-person/docker-work")]
            ),
            [PathBuf::from("/home/a-person/docker-work")]
        );
        assert_eq!(
            reached(
                Scope::Aws,
                "aws",
                &[
                    ("AWS_CONFIG_FILE", "/home/a-person/work/aws-config"),
                    (
                        "AWS_SHARED_CREDENTIALS_FILE",
                        "/home/a-person/work/aws-keys"
                    ),
                ]
            ),
            [
                PathBuf::from("/home/a-person/work/aws-config"),
                PathBuf::from("/home/a-person/work/aws-keys")
            ]
        );
        assert_eq!(
            reached(
                Scope::Remote,
                "git",
                &[
                    ("GIT_CONFIG_GLOBAL", "/home/a-person/work/gitconfig"),
                    ("XDG_CONFIG_HOME", "/home/a-person/xdg"),
                ]
            ),
            [
                PathBuf::from("/home/a-person/work/gitconfig"),
                PathBuf::from("/home/a-person/xdg/git/credentials")
            ]
        );
        for (scope, program, variable) in [
            (Scope::Docker, "docker", "DOCKER_CONFIG"),
            (Scope::Aws, "aws", "AWS_CONFIG_FILE"),
            (Scope::Kubernetes, "kubectl", "KUBECONFIG"),
            (Scope::Remote, "git", "GIT_CONFIG_GLOBAL"),
        ] {
            assert!(reached(scope, program, &[]).is_empty(), "{program} unset");
            assert!(
                reached(scope, program, &[(variable, "")]).is_empty(),
                "{program} empty"
            );
        }
        let policy =
            SandboxPolicy::strict().allow_read(PathBuf::from("/home/a-person/work/aws-config"));
        assert!(reaches(&policy, "/home/a-person/work/aws-config"));
        assert!(!reaches(&policy, "/home/a-person/work/aws-keys"));
        assert!(!reaches(&policy, "/home/a-person/work/another-file"));
    }

    /// A variable belongs to the tool that reads it: `git` is not lent the directory `docker`
    /// reads, nor `docker` the file `aws` does.
    #[test]
    fn a_variable_moves_the_row_of_the_tool_that_reads_it_and_no_other() {
        let everything = [
            ("GH_CONFIG_DIR", "/home/a-person/a"),
            ("GIT_CONFIG_GLOBAL", "/home/a-person/b"),
            ("AWS_CONFIG_FILE", "/home/a-person/c"),
            ("KUBECONFIG", "/home/a-person/d"),
            ("DOCKER_CONFIG", "/home/a-person/e"),
        ];
        for (scope, program, expected) in [
            (Scope::Remote, "git", "/home/a-person/b"),
            (Scope::Remote, "gh", "/home/a-person/a"),
            (Scope::Aws, "aws", "/home/a-person/c"),
            (Scope::Kubernetes, "kubectl", "/home/a-person/d"),
            (Scope::Docker, "docker", "/home/a-person/e"),
        ] {
            assert_eq!(
                reached(scope, program, &everything),
                [PathBuf::from(expected)],
                "{program}"
            );
        }
    }

    /// SANDBOX-16, SANDBOX-26: a scope asked for by name has no program to key on, so it follows
    /// the variables of every program the scope has, and of no other scope. The regression it
    /// rejects is a request that follows nothing, or only `gh`'s variables for `remote`.
    #[test]
    fn a_requested_scope_follows_the_variables_of_every_program_it_has() {
        let everything = the_environment(&[
            ("GH_CONFIG_DIR", "/home/a-person/a"),
            ("GIT_CONFIG_GLOBAL", "/home/a-person/b"),
            ("AWS_CONFIG_FILE", "/home/a-person/c"),
            ("AWS_SHARED_CREDENTIALS_FILE", "/home/a-person/c2"),
            ("KUBECONFIG", "/home/a-person/d"),
            ("DOCKER_CONFIG", "/home/a-person/e"),
        ]);
        for (scope, expected) in [
            (Scope::Remote, vec!["/home/a-person/a", "/home/a-person/b"]),
            (Scope::Aws, vec!["/home/a-person/c", "/home/a-person/c2"]),
            (Scope::Kubernetes, vec!["/home/a-person/d"]),
            (Scope::Docker, vec!["/home/a-person/e"]),
        ] {
            let found: Vec<PathBuf> = requested_reach(scope, Path::new(A_HOME), &everything)
                .into_iter()
                .map(|reach| reach.path)
                .collect();
            let expected: Vec<PathBuf> = expected.into_iter().map(PathBuf::from).collect();
            assert_eq!(found, expected, "{scope:?}");
        }
        let xdg = the_environment(&[("XDG_CONFIG_HOME", "/home/a-person/xdg")]);
        let found: Vec<PathBuf> = requested_reach(Scope::Remote, Path::new(A_HOME), &xdg)
            .into_iter()
            .map(|reach| reach.path)
            .collect();
        assert_eq!(
            found,
            [
                PathBuf::from("/home/a-person/xdg/gh"),
                PathBuf::from("/home/a-person/xdg/git/credentials")
            ]
        );
    }

    /// SANDBOX-26: a requested scope refuses a value as the tool's own does, so naming the scope
    /// does not widen what a variable may point at.
    #[cfg(unix)]
    #[test]
    fn a_requested_scope_refuses_the_values_a_carried_one_refuses() {
        let refused = the_environment(&[
            ("GH_CONFIG_DIR", "relative/gh"),
            ("GIT_CONFIG_GLOBAL", "/home/a-person/.ssh/id_ed25519"),
            ("AWS_CONFIG_FILE", "/home/a-person"),
            ("DOCKER_CONFIG", "/home/a-person/../b"),
            (
                "KUBECONFIG",
                "/home/a-person/.ssh/config:/home/a-person/kept",
            ),
        ]);
        for scope in EVERY_SCOPE {
            let found: Vec<PathBuf> = requested_reach(scope, Path::new(A_HOME), &refused)
                .into_iter()
                .map(|reach| reach.path)
                .collect();
            let expected: Vec<PathBuf> = match scope {
                Scope::Kubernetes => vec![PathBuf::from("/home/a-person/kept")],
                _ => Vec::new(),
            };
            assert_eq!(found, expected, "{scope:?}");
        }
    }

    /// `KUBECONFIG` is a list. Each entry is judged on its own, so one that is refused leaves the
    /// others, and none of the list is read as one path.
    #[cfg(unix)]
    #[test]
    fn a_kubeconfig_list_is_judged_one_entry_at_a_time() {
        assert_eq!(
            reached(
                Scope::Kubernetes,
                "kubectl",
                &[(
                    "KUBECONFIG",
                    "/home/a-person/one:/home/a-person/.ssh/id_ed25519:relative:/home/a-person/two"
                )]
            ),
            [
                PathBuf::from("/home/a-person/one"),
                PathBuf::from("/home/a-person/two")
            ]
        );
    }

    /// An entry named twice is one row in the prompt and one in the profile.
    #[test]
    fn an_entry_a_list_names_twice_is_reached_once() {
        assert_eq!(
            reached(
                Scope::Kubernetes,
                "kubectl",
                &[("KUBECONFIG", "/home/a-person/one:/home/a-person/one")]
            ),
            [PathBuf::from("/home/a-person/one")]
        );
    }

    /// A value that is not a place a person's tool opens, or that holds a private key, is not
    /// granted, for a file as for a directory.
    #[test]
    fn a_value_that_is_relative_or_reaches_a_key_is_refused_for_every_tool() {
        for (scope, program, variable) in [
            (Scope::Docker, "docker", "DOCKER_CONFIG"),
            (Scope::Aws, "aws", "AWS_CONFIG_FILE"),
            (Scope::Aws, "aws", "AWS_SHARED_CREDENTIALS_FILE"),
            (Scope::Kubernetes, "kubectl", "KUBECONFIG"),
            (Scope::Remote, "git", "GIT_CONFIG_GLOBAL"),
        ] {
            for value in [
                "relative/config",
                "config",
                "/home/a-person/../another-person/config",
                "/home/a-person/.ssh/id_ed25519",
                "/home/a-person/.ssh",
            ] {
                assert!(
                    reached(scope, program, &[(variable, value)]).is_empty(),
                    "{variable}={value}"
                );
            }
        }
        // A path is granted with everything under it, so a variable that names a file may not name
        // the home, what holds it, or a directory.
        for (scope, program, variable) in [
            (Scope::Aws, "aws", "AWS_CONFIG_FILE"),
            (Scope::Kubernetes, "kubectl", "KUBECONFIG"),
            (Scope::Remote, "git", "GIT_CONFIG_GLOBAL"),
        ] {
            for value in ["/home/a-person", "/home", "/"] {
                assert!(
                    reached(scope, program, &[(variable, value)]).is_empty(),
                    "{variable}={value}"
                );
            }
        }
        // A directory is read whole, so the places that hold other programs' files are refused;
        // a file is read as itself.
        for value in [
            "/home/a-person",
            "/",
            "/home/a-person/.config",
            "/home/a-person/.cache",
        ] {
            assert!(
                reached(Scope::Docker, "docker", &[("DOCKER_CONFIG", value)]).is_empty(),
                "DOCKER_CONFIG={value}"
            );
        }
        assert_eq!(
            reached(
                Scope::Aws,
                "aws",
                &[("AWS_CONFIG_FILE", "/home/a-person/.config/aws-config")]
            ),
            [PathBuf::from("/home/a-person/.config/aws-config")]
        );
    }

    /// The state directory holds the gateway keys, and a scope's variable or a `/reach` directory
    /// that lifts the refusal of it hands them to a stage. A name inside it, or a link that leads
    /// there, is refused as `~/.ssh` is; a sibling that shares the prefix is not the directory.
    #[cfg(unix)]
    #[test]
    fn a_name_inside_the_state_directory_is_refused_whatever_spells_it() {
        use std::os::unix::fs::symlink;
        let home = scratch_dir("state-directory-reach");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".bravebot")).unwrap();
        std::fs::create_dir_all(home.join(".bravebotx")).unwrap();
        std::fs::write(home.join(".bravebot/gateway-keys.json"), "").unwrap();
        std::fs::write(home.join(".bravebotx/config"), "").unwrap();
        symlink(home.join(".bravebot"), home.join("to-state")).unwrap();
        symlink(
            home.join(".bravebot/gateway-keys.json"),
            home.join("to-keys"),
        )
        .unwrap();
        let file = |variable: &str, value: &Path| {
            environment_reach(
                Scope::Aws,
                "aws",
                &home,
                &the_environment(&[(variable, value.to_str().unwrap())]),
            )
        };
        for spelled in [
            home.join(".bravebot/gateway-keys.json"),
            home.join("to-keys"),
        ] {
            assert!(file("AWS_CONFIG_FILE", &spelled).is_empty(), "{spelled:?}");
        }
        assert!(file("AWS_CONFIG_FILE", &home.join(".bravebotx/config")).len() == 1);

        for spelled in [
            home.join(".bravebot"),
            home.join(".bravebot/sessions"),
            home.join("to-state"),
        ] {
            assert_eq!(judged_directory(&spelled, &home), None, "{spelled:?}");
        }
        assert!(judged_directory(&home.join(".bravebotx"), &home).is_some());
        std::fs::remove_dir_all(&home).unwrap();
    }

    /// A file that is a link into `~/.ssh` is judged by where it leads.
    #[cfg(unix)]
    #[test]
    fn a_file_that_is_a_link_to_a_private_key_is_refused() {
        use std::os::unix::fs::symlink;
        let home = scratch_dir("file-configuration-links");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".ssh")).unwrap();
        std::fs::write(home.join(".ssh/id_ed25519"), "").unwrap();
        std::fs::write(home.join("config"), "").unwrap();
        symlink(home.join(".ssh/id_ed25519"), home.join("to-key")).unwrap();
        symlink(home.join("config"), home.join("to-config")).unwrap();
        let reads = |file: &Path| {
            environment_reach(
                Scope::Aws,
                "aws",
                &home,
                &the_environment(&[("AWS_CONFIG_FILE", file.to_str().unwrap())]),
            )
        };
        assert!(reads(&home.join("to-key")).is_empty());
        std::fs::create_dir_all(home.join("a-directory")).unwrap();
        assert!(reads(&home.join("a-directory")).is_empty());
        assert!(reads(&home).is_empty());
        assert_eq!(
            reads(&home.join("to-config"))
                .into_iter()
                .map(|reach| reach.path)
                .collect::<Vec<_>>(),
            [std::fs::canonicalize(home.join("config")).unwrap()]
        );
        std::fs::remove_dir_all(&home).unwrap();
    }

    /// A variable that points where the scope already reads says nothing a person is not told, so
    /// the prompt carries no row for it. `XDG_CONFIG_HOME=~/.config` is set on most machines.
    #[test]
    fn a_variable_pointing_at_a_row_the_scope_already_holds_adds_none() {
        assert!(
            reached(
                Scope::Remote,
                "gh",
                &[("XDG_CONFIG_HOME", "/home/a-person/.config")]
            )
            .is_empty()
        );
        assert!(
            reached(
                Scope::Remote,
                "git",
                &[("XDG_CONFIG_HOME", "/home/a-person/.config")]
            )
            .is_empty()
        );
        assert!(
            reached(
                Scope::Kubernetes,
                "kubectl",
                &[("KUBECONFIG", "/home/a-person/.kube/config")]
            )
            .is_empty()
        );
        assert!(
            reached(
                Scope::Remote,
                "git",
                &[("GIT_CONFIG_GLOBAL", "/home/a-person/.gitconfig")]
            )
            .is_empty()
        );
    }

    /// The variable is named with the path it led to, since that is what the prompt says.
    #[test]
    fn a_reach_names_the_variable_it_came_from() {
        let found = environment_reach(
            Scope::Remote,
            "gh",
            Path::new(A_HOME),
            &the_environment(&[("XDG_CONFIG_HOME", "/home/a-person/xdg")]),
        );
        assert_eq!(
            found,
            [Reach {
                variable: "XDG_CONFIG_HOME",
                path: PathBuf::from("/home/a-person/xdg/gh")
            }]
        );
    }

    /// SANDBOX-16: an operation that writes a signed commit or tag carries the signing scope, wherever
    /// the repository it runs in is, and a commit message holding `::` or an address is only text.
    #[test]
    fn an_operation_that_signs_carries_the_signing_scope() {
        for line in [
            "commit -m message",
            "commit --amend --no-edit",
            "commit -m fix::the-thing",
            "commit -m see:https://example.com/a",
            "-C ../other commit -m message",
            "rebase main",
            "rebase --continue",
            "rebase -i HEAD~3",
            "rebase --onto main a b",
            "merge feature",
            "merge --no-ff feature",
            "cherry-pick abc123",
            "cherry-pick -s abc123",
            "revert abc123",
            "am patch.mbox",
            "tag -s v1",
        ] {
            assert_eq!(of(GIT, line), Some(Scope::Signing), "git {line}");
        }
        assert_eq!(Scope::Signing.name(), "signing");
    }

    /// An option that makes git run a program from the argv, or a configuration written in front of
    /// the operation, leaves the stage with no scope: the signature is then made on the strength of
    /// a command the plan chose.
    #[test]
    fn a_signing_operation_that_names_a_program_carries_nothing() {
        for line in [
            "-c core.sshCommand=x commit -m message",
            "-c gpg.ssh.program=x commit -m message",
            "--exec-path=/x commit -m message",
            "--config-env=a=B commit -m message",
            "rebase --exec make main",
            "rebase --exe=make main",
            "rebase --e=make main",
            "rebase -x make main",
            "rebase -ix make main",
            "rebase --strategy=x main",
            "rebase --str=x main",
            "rebase -s x main",
            "merge --strategy=x feature",
            "merge --s=x feature",
            "merge -s x feature",
            "commit --exec=x",
            "cherry-pick --strategy=x abc",
            "alias-for-commit",
            "stash",
            "status",
            "log",
            "diff",
        ] {
            assert_eq!(of(GIT, line), None, "git {line}");
        }
        let assignment = the_environment(&[("GIT_SSH_COMMAND", "x")]);
        assert_eq!(
            Scope::of(Path::new(GIT), &argv("commit -m message"), &assignment),
            None
        );
    }

    /// An operation that talks to a remote stays remote, and only a program named `git` signs.
    #[test]
    fn signing_does_not_change_the_other_scopes() {
        assert_eq!(of(GIT, "push origin main"), Some(Scope::Remote));
        assert_eq!(of(GIT, "pull --rebase"), Some(Scope::Remote));
        assert_eq!(of(GIT, "pull -s ours"), None);
        assert_eq!(of("/usr/bin/gh", "commit"), None);
        assert_eq!(of("/tmp/not-git", "commit -m message"), None);
    }

    /// SANDBOX-16, SANDBOX-26: a `run` call may ask for the signing scope by name, so a line that
    /// signs without the argv shape the scope is keyed on (a script, or `-c` before the
    /// operation) can have it. `/reach` and a remembered grant do not name it.
    #[test]
    fn signing_can_be_requested_and_not_remembered() {
        assert_eq!(
            Requested::named("signing"),
            Some(Requested::Scope(Scope::Signing))
        );
        assert!(Requested::MENU.contains(&"signing"));
        assert_eq!(Scope::named("signing"), None);
    }

    /// A home with `.gitconfig` signing with `~/keys/work.pub`, and an unrelated private key.
    #[cfg(unix)]
    fn a_home_that_signs(name: &str) -> PathBuf {
        let home = a_home_with(name, "", &[".ssh/id_ed25519", "keys/work.pub", "keys/work"]);
        std::fs::write(
            home.join(".gitconfig"),
            "[gpg]\nformat = ssh\n[user]\nsigningkey = ~/keys/work.pub\n",
        )
        .unwrap();
        home
    }

    /// SANDBOX-16: a stage that signs reads the one public key `user.signingkey` names and nothing
    /// else of the remote scope: no `~/.ssh`, no known hosts, no private key, no write.
    #[test]
    #[cfg(unix)]
    fn the_signing_scope_reads_the_public_key_and_nothing_else() {
        let home = a_home_that_signs("signing-scope-grant");
        let policy = Scope::Signing.grant(SandboxPolicy::strict(), &home);
        assert_eq!(granted_paths(&policy), [home.join("keys/work.pub")]);
        assert!(policy.writable.is_empty());
        std::fs::remove_dir_all(&home).unwrap();
    }

    /// A person who does not sign with ssh, or whose key is no public key in the home, is lent
    /// nothing: the grant follows the configuration and not the argv.
    #[test]
    #[cfg(unix)]
    fn the_signing_scope_reads_nothing_where_the_key_is_refused_or_signing_is_off() {
        let home = a_home_that_signs("signing-scope-refused");
        for gitconfig in [
            "[gpg]\nformat = ssh\n[user]\nsigningkey = ~/keys/work\n",
            "[gpg]\nformat = ssh\n[user]\nsigningkey = ~/.ssh/id_ed25519\n",
            "[gpg]\nformat = ssh\n",
            "[user]\nsigningkey = ~/keys/work.pub\n",
        ] {
            std::fs::write(home.join(".gitconfig"), gitconfig).unwrap();
            let policy = Scope::Signing.grant(SandboxPolicy::strict(), &home);
            assert!(granted_paths(&policy).is_empty(), "{gitconfig}");
        }
        std::fs::remove_dir_all(&home).unwrap();
    }

    /// `git pull` merges or rebases and signs the commit it makes, so the remote scope reads the
    /// key too, and still no private key.
    #[test]
    #[cfg(unix)]
    fn the_remote_scope_reads_the_signing_key_as_well() {
        let home = a_home_that_signs("signing-remote-grant");
        let policy = Scope::Remote.grant(SandboxPolicy::strict(), &home);
        assert!(granted_paths(&policy).contains(&home.join("keys/work.pub")));
        for private in ["keys/work", ".ssh/id_ed25519", ".ssh"] {
            assert!(
                !granted_paths(&policy).contains(&home.join(private)),
                "{private}"
            );
        }
        std::fs::remove_dir_all(&home).unwrap();
    }
}
