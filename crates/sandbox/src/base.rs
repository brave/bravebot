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
//! Nothing here reads the machine. The rows that are not fixed, the temporary directory the
//! session resolved as it opened, the developer directory on macOS and the account's home
//! directory, are handed in by the caller that resolved them, so what this produces is the same
//! list from the same arguments on every machine and every platform.

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
    Windows,
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

        #[cfg(windows)]
        {
            Some(Self::Windows)
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
        {
            None
        }
    }

    /// The paths this platform's programs read before any plan is read.
    fn rows(self) -> &'static [&'static str] {
        match self {
            Self::Linux => LINUX_PRELUDE,
            Self::MacOs => MACOS_PRELUDE,
            Self::Windows => WINDOWS_PRELUDE,
        }
    }

    /// The device a program discards output to, where this platform has one a profile can name.
    ///
    /// `NUL` is not a path under any directory, so there is no row to write for it. A container
    /// opens it only where the device's access list names ALL APPLICATION PACKAGES, which no
    /// grant of this backend can add.
    fn null_device(self) -> Option<&'static str> {
        match self {
            Self::Linux | Self::MacOs => Some(THE_NULL_DEVICE),
            Self::Windows => None,
        }
    }
}

/// What a Linux program reads before any plan is read.
///
/// The loader and the system libraries, the system binary directories, the locale data,
/// terminfo, the certificates a TLS client reads with the directory holding them, the files a
/// host lookup and a user lookup read, the machine's git configuration, and the devices. `git`
/// stops on a configuration file that is there and that it is refused, where one that is absent
/// is nothing, so a row for it is what lets a machine that has one run git at all. Each entry is
/// one of the rows the base
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
    "/etc/gitconfig",
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
///
/// One row has no Linux counterpart: the TLS library this platform ships aborts in every program
/// linked against it, `curl`, `openssl` and rustup's `cargo` among them, when it cannot read its
/// configuration file. The developer directory the `/usr/bin` shims run out of is the other thing
/// a program here needs to start, and it is not in this list because where it is differs by
/// machine.
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
    "/private/etc/ssl/openssl.cnf",
    "/private/etc/hosts",
    "/private/etc/resolv.conf",
    "/private/etc/passwd",
    "/private/etc/group",
    "/private/etc/gitconfig",
    "/dev/null",
    "/dev/zero",
    "/dev/random",
    "/dev/urandom",
];

/// What a Windows program reads before any plan is read: nothing.
///
/// A container is granted read and execute on `C:\Windows` and the Program Files directories
/// by the group every container belongs to, and that is what a program needs to start, so the
/// system directories are reached without a row. They are left out rather than listed because a
/// row is an entry written onto the directory, and only an account holding the right to change
/// a directory's permissions may write one: naming `C:\Windows` would be every program refused
/// for an entry that adds nothing. Nothing outside those directories is reachable by default,
/// which is the property the other two preludes get by listing.
const WINDOWS_PRELUDE: &[&str] = &[];

/// The one path in the prelude a program may write.
///
/// Discarding output is what every pipeline and every build does, and a program that cannot
/// write here fails on a path it was granted. Nothing else the prelude names is writable: those
/// are the machine's own directories, and the certificates among them decide what every TLS
/// client on it trusts.
const THE_NULL_DEVICE: &str = "/dev/null";

/// Where the Command Line Tools install their developer directory, which is granted as it is.
const THE_COMMAND_LINE_TOOLS: &str = "/Library/Developer/CommandLineTools";

/// The directory an application bundle holding a developer directory is granted from.
const APPLICATIONS: &str = "/Applications";

/// The rows every program a person asked for gets, before its plan is read.
///
/// `temporary_directory` is the system temporary directory this process resolved as the session
/// opened, and not one a stage's own environment assignment names, so a line carrying a
/// `TMPDIR=` assignment changes where a program writes without changing what the profile allows.
/// It arrives with its links followed, since a backend matching a grant against the path a
/// program opens grants nothing for the name a link is reached by.
///
/// `developer_directory` is, on macOS, the one `xcode-select -p` prints, resolved the same way
/// and with its links followed: `git`, `cc`, `make` and `python3` in `/usr/bin` are shims that
/// run the real program out of it. What is granted is [`developer_row`]'s, and elsewhere nothing.
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
pub fn base(
    prelude: Prelude,
    temporary_directory: &Path,
    developer_directory: Option<&Path>,
    home: Option<&Path>,
) -> SandboxPolicy {
    let mut policy = SandboxPolicy::strict()
        .allow_network_egress()
        .allow_subprocesses();

    for path in prelude.rows() {
        policy = policy.allow_read(*path);
    }

    if let Some(row) = developer_directory.and_then(developer_row) {
        policy = policy.allow_read(row);
    }

    // Neither write row says what is at the path it names, so nothing creates either of them.
    // The session resolved the temporary directory as it opened, and what is at the other one
    // is the kernel's rather than a file: a row saying which of the two it is would be a
    // regular file standing in for the null device on a machine that somehow lacked it.
    policy = policy
        .allow_read(temporary_directory)
        .allow_write(temporary_directory);
    if let Some(null_device) = prelude.null_device() {
        policy = policy.allow_write(null_device);
    }

    if let Some(home) = home {
        for path in git_configuration(home) {
            policy = policy.allow_read(path);
        }
    }

    policy
}

/// The name of the directory under the home directory where the program keeps its own state.
///
/// It holds the gateway keys, the premium token and the server list, so a program a stage starts
/// is refused it like any other credential location. The state helper in `bravebot-agent` takes its
/// name from here, which is what keeps this table and the place the files are written from naming
/// two different directories.
pub const STATE_DIRECTORY: &str = ".bravebot";

/// The name of the directory under the home directory that holds the browser relay's socket and
/// the secret a peer presents to it, so a program that reads the secret can drive the browser.
///
/// `bravebot-browser` links no workspace crate and so carries its own spelling of this name
/// (`DIRECTORY_NAME` in its `paths` module), which a person can move with `BRAVEBOT_BROWSER_DIR`.
/// The table refuses the default location.
const BROWSER_DIRECTORY: &str = ".bravebot-browser";

/// Locations under the home directory that hold a credential on every unix platform, each refused
/// with everything beneath it.
///
/// `~/.ssh` is here as a whole and [`SSH_READABLE`] lifts the three kinds of file in it that hold
/// no secret. Token files that a program reads by name (`~/.config/gh`, `~/.git-credentials`,
/// `~/.netrc`, `~/.npmrc`, `~/.cargo/credentials.toml`, `~/.pypirc` and the rest SANDBOX-12 names
/// as read) are not here: they are what `gh`, `git`, `npm`, `cargo` and `pip` read to do what a
/// person asked, and the network is where they could leave, which is decided by the plan.
///
/// Most histories are files rather than directories. No program a stage starts reads one, since a
/// stage's standard input is never a terminal and so no shell or client it starts is interactive.
pub(crate) const CREDENTIAL_DIRECTORIES: &[&str] = &[
    STATE_DIRECTORY,
    BROWSER_DIRECTORY,
    ".ssh",
    ".aws",
    ".kube",
    ".docker",
    ".azure",
    ".config/gcloud",
    ".gnupg",
    ".config/sops/age",
    ".password-store",
    ".bash_history",
    ".zsh_history",
    ".zsh_sessions",
    ".bash_sessions",
    ".local/share/fish/fish_history",
    ".psql_history",
    ".mysql_history",
    ".node_repl_history",
    ".python_history",
    ".rediscli_history",
    ".sqlite_history",
    ".dbshell",
    ".mongodb/mongosh/mongosh_repl_history",
    ".claude",
    ".claude.json",
    ".codex",
    ".gemini",
    ".local/share/opencode",
    ".config/github-copilot",
    ".config/openconnect",
    ".gitlab-runner",
    ".pgadmin",
    "Postman",
];

/// The same, where only Windows keeps them, under the home directory.
///
/// `gcloud` and `gpg` keep their directories under `%APPDATA%` there, which is
/// `AppData\Roaming` unless the account's profile moves it. The browsers the other two tables name
/// keep their profiles under `%LOCALAPPDATA%` (`AppData\Local`), apart from Firefox, which keeps
/// them under `%APPDATA%`. The last five are the platform's own stores, which stand where the
/// keychains and keyrings stand elsewhere: Credential Manager's files, the keys the data protection
/// API encrypts with, and the vault.
pub(crate) const WINDOWS_CREDENTIAL_DIRECTORIES: &[&str] = &[
    "AppData/Roaming/gcloud",
    "AppData/Roaming/gnupg",
    "AppData/Local/BraveSoftware",
    "AppData/Local/Google/Chrome",
    "AppData/Local/Chromium",
    "AppData/Local/Microsoft/Edge",
    "AppData/Roaming/Mozilla",
    "AppData/Local/Microsoft/Credentials",
    "AppData/Roaming/Microsoft/Credentials",
    "AppData/Roaming/Microsoft/Protect",
    "AppData/Local/Microsoft/Vault",
    "AppData/Roaming/Microsoft/Vault",
];

/// The same, where only macOS keeps them, under the home directory.
///
/// A browser's channels keep separate directories, so `Google/Chrome` holds nothing of
/// `Google/Chrome Beta`. JetBrains has no row here: its Toolbox puts the scripts that start each
/// IDE in `Library/Application Support/JetBrains/Toolbox/scripts`, on the search path.
pub(crate) const MACOS_CREDENTIAL_DIRECTORIES: &[&str] = &[
    "Library/Keychains",
    "Library/Application Support/BraveSoftware",
    "Library/Application Support/Google/Chrome",
    "Library/Application Support/Firefox",
    "Library/Cookies",
    "Library/Safari",
    "Library/Application Support/Google/Chrome Beta",
    "Library/Application Support/Google/Chrome Canary",
    "Library/Application Support/Google/Chrome Dev",
    "Library/Application Support/Chromium",
    "Library/Application Support/Microsoft Edge",
    "Library/Application Support/Microsoft Edge Beta",
    "Library/Application Support/Microsoft Edge Dev",
    "Library/Application Support/Microsoft Edge Canary",
    "Library/Application Support/Arc",
    "Library/Application Support/Vivaldi",
    "Library/Messages",
    "Library/Mail",
    "Library/Thunderbird",
    "Library/Application Support/Slack",
    "Library/Application Support/discord",
    "Library/Application Support/Signal",
    "Library/Group Containers/2BUA8C4S2C.com.1password",
    "Library/Application Support/Code",
    "Library/Application Support/Code - Insiders",
    "Library/Application Support/VSCodium",
    "Library/Application Support/Cursor",
    "Library/Application Support/Claude",
    "Library/Application Support/Insomnia",
    "Library/Application Support/Bruno",
    "Library/Application Support/Postman",
    "Library/DBeaverData",
    "Library/Application Support/sops/age",
];

/// The same, where only Linux keeps them, under the home directory.
///
/// `~/.var/app` is every Flatpak application's data, its browsers' profiles among them.
pub(crate) const LINUX_CREDENTIAL_DIRECTORIES: &[&str] = &[
    ".local/share/keyrings",
    ".config/BraveSoftware",
    ".config/google-chrome",
    ".config/chromium",
    ".mozilla",
    ".config/google-chrome-beta",
    ".config/google-chrome-unstable",
    ".config/microsoft-edge",
    ".config/microsoft-edge-beta",
    ".config/microsoft-edge-dev",
    ".config/vivaldi",
    "snap/firefox/common/.mozilla",
    "snap/chromium",
    "snap/brave",
    ".var/app",
    ".thunderbird",
    ".local/share/kwalletd",
    ".Xauthority",
    ".config/Slack",
    ".config/discord",
    ".config/Signal",
    ".config/1Password",
    ".config/Code",
    ".config/Code - Insiders",
    ".config/VSCodium",
    ".config/Cursor",
    ".config/JetBrains",
    ".config/Insomnia",
    ".config/Bruno",
    ".config/Postman",
    ".local/share/DBeaverData",
];

/// The machine-wide keychain directory on macOS.
pub const MACOS_SYSTEM_KEYCHAINS: &str = "/Library/Keychains";

/// The one file in `~/Library/Keychains` that is read, under the home directory.
///
/// `gh` and git's `osxkeychain` helper keep their tokens in it, and the process asking opens the
/// database file itself, so a refusal of the file is a refusal of the lookup. Another file in the
/// directory, such as `aws-vault.keychain-db`, stays refused.
const MACOS_LOGIN_KEYCHAIN: &str = "Library/Keychains/login.keychain-db";

/// The directory under the per-user cache directory that the Security framework keeps its
/// framework database in.
///
/// A process that opens the login keychain writes a lock and two database files here first, and
/// `gh` and `osxkeychain` fail without it even when the keychain file is readable.
const MACOS_SECURITY_CACHE: &str = "mds";

/// The files of `~/.ssh` that hold no secret: the client configuration, the hosts already
/// verified, and the default public keys. Everything else in the directory is a private key or
/// something named after one.
const SSH_READABLE: &[&str] = &[
    ".ssh/config",
    ".ssh/known_hosts",
    ".ssh/id_rsa.pub",
    ".ssh/id_dsa.pub",
    ".ssh/id_ecdsa.pub",
    ".ssh/id_ecdsa_sk.pub",
    ".ssh/id_ed25519.pub",
    ".ssh/id_ed25519_sk.pub",
];

/// Every row of the four tables, whatever platform keeps it.
pub(crate) fn every_credential_directory() -> impl Iterator<Item = &'static &'static str> {
    CREDENTIAL_DIRECTORIES
        .iter()
        .chain(MACOS_CREDENTIAL_DIRECTORIES)
        .chain(LINUX_CREDENTIAL_DIRECTORIES)
        .chain(WINDOWS_CREDENTIAL_DIRECTORIES)
}

/// The locations under `home` that hold credentials or the state directory, which a stage of a
/// `run` command is refused on `prelude` (SANDBOX-12).
///
/// A Windows container cannot be refused a location inside a directory it is granted, so the
/// caller checks the directories it would grant against this list instead.
pub fn credential_locations(prelude: Prelude, home: &Path) -> Vec<PathBuf> {
    let platform = match prelude {
        Prelude::MacOs => MACOS_CREDENTIAL_DIRECTORIES,
        Prelude::Linux => LINUX_CREDENTIAL_DIRECTORIES,
        Prelude::Windows => WINDOWS_CREDENTIAL_DIRECTORIES,
    };
    CREDENTIAL_DIRECTORIES
        .iter()
        .chain(platform)
        .map(|row| under(home, row))
        .collect()
}

/// The rows a stage of a `run` command gets before its plan is read.
///
/// On Linux and macOS a stage reads the whole machine except the locations in the credential
/// tables above, and writes only the temporary directory and the null device here, with the
/// session's own directories added by the caller. That is what lets a script that starts `gh`,
/// `git` or `cargo` run: what such a script reaches is decided by this table and not by a
/// per-tool list that a program the list never heard of is refused for.
///
/// `home` is the account's home directory. Without one only the rows that are absolute apply.
///
/// On Windows a container is granted the paths it is given and reaches nothing else, so the base
/// is [`base`] and nothing here widens it.
pub fn run_base(
    prelude: Prelude,
    temporary_directory: &Path,
    home: Option<&Path>,
) -> SandboxPolicy {
    if prelude == Prelude::Windows {
        return base(prelude, temporary_directory, None, home);
    }

    let mut policy = SandboxPolicy::strict()
        .allow_network_egress()
        .allow_subprocesses()
        .allow_read("/")
        .allow_write(temporary_directory);
    if let Some(null_device) = prelude.null_device() {
        policy = policy.allow_write(null_device);
    }

    if prelude == Prelude::MacOs {
        policy = policy.deny_read(MACOS_SYSTEM_KEYCHAINS);
    }
    if let Some(home) = home {
        for location in credential_locations(prelude, home) {
            policy = policy.deny_read(location);
        }
        for row in SSH_READABLE {
            policy = policy.allow_read(under(home, row));
        }
        if prelude == Prelude::MacOs {
            policy = policy.allow_read(under(home, MACOS_LOGIN_KEYCHAIN));
        }
    }

    policy
}

/// `policy` with the one directory the Security framework writes before it opens a keychain, which
/// is under `user_cache`, the per-user cache directory the caller resolved (`confstr` with
/// `_CS_DARWIN_USER_CACHE_DIR`, links followed).
///
/// Separate from [`run_base`] because that directory is the machine's to say and not a row under
/// the home directory. The caller adds it for [`Prelude::MacOs`] only.
#[must_use]
pub fn with_security_cache(policy: SandboxPolicy, user_cache: &Path) -> SandboxPolicy {
    policy.allow_write(user_cache.join(MACOS_SECURITY_CACHE))
}

/// The row a developer directory is granted as, where it is one of the two the platform installs.
///
/// The Command Line Tools' is granted as it is. An application bundle's, one directly in
/// `/Applications` whatever the bundle is called, is granted as the bundle whole: its developer
/// directory loads frameworks from beside it, and with the lookup cache the shims keep in the
/// temporary directory empty, a shim asks `xcodebuild`, which loads from further across the
/// bundle. Anywhere else is a directory of a person's choosing rather than the machine's, and is
/// in no row.
fn developer_row(developer_directory: &Path) -> Option<PathBuf> {
    if developer_directory == Path::new(THE_COMMAND_LINE_TOOLS) {
        return Some(developer_directory.to_path_buf());
    }
    let bundle = developer_directory.parent()?.parent()?;
    let is_a_bundles = developer_directory.ends_with("Contents/Developer")
        && bundle.parent() == Some(Path::new(APPLICATIONS))
        && bundle
            .extension()
            .is_some_and(|extension| extension == "app");
    is_a_bundles.then(|| bundle.to_path_buf())
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

/// `row`, written with `/` between its components, under `home`.
pub(crate) fn under(home: &Path, row: &str) -> PathBuf {
    row.split('/')
        .fold(home.to_path_buf(), |path, component| path.join(component))
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

    const EVERY_PLATFORM: [Prelude; 3] = [Prelude::Linux, Prelude::MacOs, Prelude::Windows];

    /// What `xcode-select -p` prints for an Xcode installed where the platform puts one, which
    /// is the widest developer row a base names.
    const AN_XCODE: &str = "/Applications/Xcode.app/Contents/Developer";

    fn a_base(prelude: Prelude) -> SandboxPolicy {
        let developer_directory = (prelude == Prelude::MacOs).then(|| Path::new(AN_XCODE));
        base(
            prelude,
            Path::new(THE_SESSIONS_TEMPORARY_DIRECTORY),
            developer_directory,
            Some(Path::new(A_HOME)),
        )
    }

    fn a_macos_base_with(developer_directory: &str) -> SandboxPolicy {
        base(
            Prelude::MacOs,
            Path::new(THE_SESSIONS_TEMPORARY_DIRECTORY),
            Some(Path::new(developer_directory)),
            Some(Path::new(A_HOME)),
        )
    }

    fn a_run_base(prelude: Prelude) -> SandboxPolicy {
        run_base(
            prelude,
            Path::new(THE_SESSIONS_TEMPORARY_DIRECTORY),
            Some(Path::new(A_HOME)),
        )
    }

    /// The regression it rejects: a run base that lists what a stage reaches, which refuses a
    /// script for starting a program the list never heard of, or one that reads the machine with
    /// no refusals, which hands the first program that asks a credential; or one whose table leaves out
    /// the program's own state directory, where the gateway keys are, or the browser relay's, where
    /// the secret that drives the browser is, or a history, another agent's directory or another
    /// application's data, which no stage needs. Spelled out here and
    /// not read from the tables, so a row dropped from them fails this.
    #[test]
    fn the_run_base_reads_the_machine_and_refuses_each_credential_location() {
        let expected_everywhere = [
            ".bravebot",
            ".bravebot-browser",
            ".ssh",
            ".aws",
            ".kube",
            ".docker",
            ".azure",
            ".config/gcloud",
            ".gnupg",
            ".config/sops/age",
            ".password-store",
            ".bash_history",
            ".zsh_history",
            ".zsh_sessions",
            ".bash_sessions",
            ".local/share/fish/fish_history",
            ".psql_history",
            ".mysql_history",
            ".node_repl_history",
            ".python_history",
            ".rediscli_history",
            ".sqlite_history",
            ".dbshell",
            ".mongodb/mongosh/mongosh_repl_history",
            ".claude",
            ".claude.json",
            ".codex",
            ".gemini",
            ".local/share/opencode",
            ".config/github-copilot",
            ".config/openconnect",
            ".gitlab-runner",
            ".pgadmin",
            "Postman",
        ];
        let expected_on_macos = [
            "Library/Keychains",
            "Library/Application Support/BraveSoftware",
            "Library/Application Support/Google/Chrome",
            "Library/Application Support/Firefox",
            "Library/Cookies",
            "Library/Safari",
            "Library/Application Support/Google/Chrome Beta",
            "Library/Application Support/Google/Chrome Canary",
            "Library/Application Support/Google/Chrome Dev",
            "Library/Application Support/Chromium",
            "Library/Application Support/Microsoft Edge",
            "Library/Application Support/Microsoft Edge Beta",
            "Library/Application Support/Microsoft Edge Dev",
            "Library/Application Support/Microsoft Edge Canary",
            "Library/Application Support/Arc",
            "Library/Application Support/Vivaldi",
            "Library/Messages",
            "Library/Mail",
            "Library/Thunderbird",
            "Library/Application Support/Slack",
            "Library/Application Support/discord",
            "Library/Application Support/Signal",
            "Library/Group Containers/2BUA8C4S2C.com.1password",
            "Library/Application Support/Code",
            "Library/Application Support/Code - Insiders",
            "Library/Application Support/VSCodium",
            "Library/Application Support/Cursor",
            "Library/Application Support/Claude",
            "Library/Application Support/Insomnia",
            "Library/Application Support/Bruno",
            "Library/Application Support/Postman",
            "Library/DBeaverData",
            "Library/Application Support/sops/age",
        ];
        let expected_on_linux = [
            ".local/share/keyrings",
            ".config/BraveSoftware",
            ".config/google-chrome",
            ".config/chromium",
            ".mozilla",
            ".config/google-chrome-beta",
            ".config/google-chrome-unstable",
            ".config/microsoft-edge",
            ".config/microsoft-edge-beta",
            ".config/microsoft-edge-dev",
            ".config/vivaldi",
            "snap/firefox/common/.mozilla",
            "snap/chromium",
            "snap/brave",
            ".var/app",
            ".thunderbird",
            ".local/share/kwalletd",
            ".Xauthority",
            ".config/Slack",
            ".config/discord",
            ".config/Signal",
            ".config/1Password",
            ".config/Code",
            ".config/Code - Insiders",
            ".config/VSCodium",
            ".config/Cursor",
            ".config/JetBrains",
            ".config/Insomnia",
            ".config/Bruno",
            ".config/Postman",
            ".local/share/DBeaverData",
        ];

        for (prelude, own) in [
            (Prelude::MacOs, &expected_on_macos[..]),
            (Prelude::Linux, &expected_on_linux[..]),
        ] {
            let policy = a_run_base(prelude);
            let mut expected: Vec<PathBuf> = expected_everywhere
                .iter()
                .chain(own)
                .map(|row| under(Path::new(A_HOME), row))
                .collect();
            if prelude == Prelude::MacOs {
                expected.push(PathBuf::from("/Library/Keychains"));
            }
            expected.sort();
            let mut refused = policy.unreadable.clone();
            refused.sort();

            assert_eq!(refused, expected, "{prelude:?}");
            assert!(policy.readable.contains(&PathBuf::from("/")), "{prelude:?}");
            assert!(policy.is_meaningful(), "{prelude:?}");
            assert!(
                policy.allow_network && policy.allow_subprocesses,
                "{prelude:?}"
            );
        }
    }

    /// The regression it rejects: a refusal of `~/.ssh` as a whole, which takes the client
    /// configuration, the verified hosts and the public keys from `ssh`, `git` and `gh`; or a
    /// lift written as a glob, which reads a private key named after one.
    #[test]
    fn the_run_base_lifts_the_ssh_files_that_hold_no_secret_and_only_those() {
        let lifted: Vec<PathBuf> = a_run_base(Prelude::Linux)
            .readable
            .into_iter()
            .filter(|row| row.starts_with(under(Path::new(A_HOME), ".ssh")))
            .collect();
        let expected: Vec<PathBuf> = [
            "config",
            "known_hosts",
            "id_rsa.pub",
            "id_dsa.pub",
            "id_ecdsa.pub",
            "id_ecdsa_sk.pub",
            "id_ed25519.pub",
            "id_ed25519_sk.pub",
        ]
        .iter()
        .map(|name| under(Path::new(A_HOME), &format!(".ssh/{name}")))
        .collect();

        assert_eq!(lifted, expected);
    }

    /// The regression it rejects: the login keychain refused, which is `gh` and git's
    /// `osxkeychain` helper finding no token, or the lift written as the directory, which reads
    /// every other keychain file beside it. Linux has no such file, so no row names one there.
    #[test]
    fn the_run_base_on_macos_reads_the_login_keychain_file_and_no_other_keychain_file() {
        let keychains = under(Path::new(A_HOME), "Library/Keychains");
        let macos = a_run_base(Prelude::MacOs);

        let lifted: Vec<PathBuf> = macos
            .readable
            .iter()
            .filter(|row| row.starts_with(&keychains))
            .cloned()
            .collect();
        assert_eq!(lifted, vec![keychains.join("login.keychain-db")]);
        assert!(macos.unreadable.contains(&keychains));
        assert!(
            macos
                .unreadable
                .contains(&PathBuf::from("/Library/Keychains"))
        );

        let linux = a_run_base(Prelude::Linux);
        assert!(
            !linux.readable.iter().any(|row| row.starts_with(&keychains)),
            "a Linux base names a macOS keychain"
        );
    }

    /// The regression it rejects: the keychain lookup failing for want of the framework's cache
    /// directory, which is `gh` and `osxkeychain` finding no token with the keychain file
    /// readable, or the row written as the whole cache directory, which writes every cache of the
    /// account's.
    #[test]
    fn the_security_cache_row_is_the_one_directory_under_the_user_cache() {
        let cache = Path::new("/private/var/folders/ab/cdef/C");
        let policy = with_security_cache(a_run_base(Prelude::MacOs), cache);

        let written: Vec<PathBuf> = policy
            .writable
            .iter()
            .filter(|row| row.path.starts_with(cache))
            .map(|row| row.path.clone())
            .collect();
        assert_eq!(written, vec![cache.join("mds")]);
    }

    /// The regression it rejects: a token file refused, which is `gh`, `git`, `npm`, `cargo` and
    /// `pip` refused the login they were started to use.
    #[test]
    fn the_run_base_leaves_the_token_files_readable() {
        for prelude in [Prelude::Linux, Prelude::MacOs] {
            let policy = a_run_base(prelude);
            for token in [
                ".config/gh",
                ".git-credentials",
                ".netrc",
                ".npmrc",
                ".cargo/credentials.toml",
                ".pypirc",
                ".gitconfig",
            ] {
                let path = under(Path::new(A_HOME), token);
                assert!(
                    !policy
                        .unreadable
                        .iter()
                        .any(|refused| path.starts_with(refused)),
                    "{token} is refused on {prelude:?}"
                );
            }
        }
    }

    /// The `~/` paths in the code spans of the rows of the first table at or after `marker` in the
    /// spec `name`, its header and separator left out, without the `~/`. A path holding a wildcard
    /// is cut back to the directory above the first part that holds one, and a trailing `/` is
    /// dropped.
    fn home_paths_in_a_table(name: &str, marker: &str) -> Vec<String> {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let spec = std::fs::read_to_string(workspace.join(name))
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let at = spec
            .find(marker)
            .unwrap_or_else(|| panic!("{name} no longer holds `{marker}`"));
        let rows = spec[at..]
            .lines()
            .skip_while(|line| !line.starts_with('|'))
            .take_while(|line| line.starts_with('|'))
            .skip(2);

        let mut paths = Vec::new();
        for row in rows {
            for code in row.split('`').skip(1).step_by(2) {
                let Some(path) = code.strip_prefix("~/") else {
                    continue;
                };
                let kept: Vec<&str> = path
                    .split('/')
                    .take_while(|part| !part.contains(['*', '?']))
                    .filter(|part| !part.is_empty())
                    .collect();
                paths.push(kept.join("/"));
            }
        }
        paths
    }

    /// Whether `policy` holds `path` back: a refusal covers it and no row granted beneath that
    /// refusal lifts it again.
    fn holds_back(policy: &SandboxPolicy, path: &Path) -> bool {
        policy.unreadable.iter().any(|refused| {
            path.starts_with(refused)
                && !policy
                    .readable
                    .iter()
                    .any(|lift| lift.starts_with(refused) && path.starts_with(lift))
        })
    }

    /// The catalog's stores at a path only Linux keeps them at: the desktop's secret stores and
    /// display cookie, and applications that keep their data under `~/Library` on macOS instead.
    /// Every other store the catalog names outside `~/Library` is at the same path on both.
    const ONLY_LINUX_KEEPS_THERE: &[&str] = &[
        ".local/share/keyrings",
        ".local/share/kwalletd",
        ".Xauthority",
        ".config/Code",
        ".config/JetBrains",
        ".config/Insomnia",
        ".config/Bruno",
        ".config/Postman",
        ".local/share/DBeaverData",
    ];

    /// A store the catalog lists that no row refuses and SANDBOX-12 does not name as read is a
    /// store every stage under the mode `standard` reads and, with the network open, can send,
    /// while the specs read as though it were held back. The regression it rejects is a row
    /// missing from the table on a platform that keeps the store at that path, a path named as
    /// read that the table refuses, or one named as read that the catalog lists nothing under.
    /// Both lists are read from the specs, so a `~/` path added to either fails this until the
    /// table and the other list agree with it.
    #[test]
    fn every_store_the_catalog_names_is_refused_or_read_on_purpose() {
        let home = Path::new(A_HOME);
        let catalog = home_paths_in_a_table(
            "docs/specs/credential-protection.md",
            "### Already on the machine, outside the working tree",
        );
        let read = home_paths_in_a_table("docs/specs/sandboxing.md", "| What reads it | Read |");
        let refuses =
            |prelude: Prelude, path: &str| holds_back(&a_run_base(prelude), &under(home, path));
        let beneath =
            |path: &str, list: &[&str]| list.iter().any(|row| Path::new(path).starts_with(row));
        let read_rows: Vec<&str> = read.iter().map(String::as_str).collect();

        for path in &catalog {
            if beneath(path, &read_rows) {
                for prelude in [Prelude::Linux, Prelude::MacOs] {
                    assert!(
                        !refuses(prelude, path),
                        "~/{path} is named as read on purpose and the {prelude:?} run base \
                         refuses it"
                    );
                }
                continue;
            }
            let kept_on: &[Prelude] = if path.starts_with("Library/") {
                &[Prelude::MacOs]
            } else if beneath(path, ONLY_LINUX_KEEPS_THERE) {
                &[Prelude::Linux]
            } else {
                &[Prelude::Linux, Prelude::MacOs]
            };
            for prelude in kept_on {
                assert!(
                    refuses(*prelude, path),
                    "~/{path} is in the catalog, in no {prelude:?} row of the table and not named \
                     as read on purpose"
                );
            }
        }
        for named in &read {
            assert!(
                catalog
                    .iter()
                    .any(|path| Path::new(path).starts_with(named)),
                "~/{named} is named as read on purpose and the catalog lists nothing there"
            );
        }
    }

    /// The regression it rejects: the run base widening a stage's writes past the temporary
    /// directory and the null device.
    #[test]
    fn the_run_base_writes_only_the_temporary_directory_and_the_null_device() {
        for prelude in [Prelude::Linux, Prelude::MacOs] {
            let written: Vec<PathBuf> = a_run_base(prelude)
                .writable
                .into_iter()
                .map(|row| row.path)
                .collect();
            let mut expected = vec![PathBuf::from(THE_SESSIONS_TEMPORARY_DIRECTORY)];
            expected.extend(prelude.null_device().map(PathBuf::from));

            assert_eq!(written, expected, "{prelude:?}");
        }
    }

    /// The regression it rejects: a Windows container handed a read of `/`, which it has no
    /// meaning for, or a refusal, which an AppContainer cannot hold.
    #[test]
    fn the_run_base_on_windows_is_the_keyed_base() {
        let run = a_run_base(Prelude::Windows);
        let keyed = base(
            Prelude::Windows,
            Path::new(THE_SESSIONS_TEMPORARY_DIRECTORY),
            None,
            Some(Path::new(A_HOME)),
        );

        assert_eq!(run.readable, keyed.readable);
        assert!(run.unreadable.is_empty());
    }

    /// The credential table the Windows caller checks its grants against names the state
    /// directory and the same shared rows the other platforms refuse, and the run base refuses
    /// exactly the table on the platforms that refuse anything.
    #[test]
    fn the_credential_locations_are_the_rows_the_run_base_refuses() {
        let home = Path::new(A_HOME);
        let windows = credential_locations(Prelude::Windows, home);

        for row in [
            ".bravebot",
            ".bravebot-browser",
            ".ssh",
            ".aws",
            ".config/gcloud",
            "AppData/Roaming/gcloud",
            "AppData/Roaming/gnupg",
        ] {
            assert!(windows.contains(&under(home, row)), "{row}");
        }
        for prelude in [Prelude::MacOs, Prelude::Linux] {
            let refused = a_run_base(prelude).unreadable;
            for location in credential_locations(prelude, home) {
                assert!(refused.contains(&location), "{prelude:?} {location:?}");
            }
        }
    }

    /// The regression it rejects: a Windows table holding only `gcloud` and `gpg`, which lets a
    /// person's yes grant `AppData\Local` or a browser profile, or the files of Credential Manager,
    /// the data protection keys and the vault, to a container that cannot be refused a path inside
    /// a grant. Spelled out here and not read from the table, so a row dropped from it fails this.
    #[test]
    fn the_windows_credential_locations_name_the_browser_profiles_and_the_platforms_stores() {
        let home = Path::new(A_HOME);
        let mut expected: Vec<PathBuf> = [
            ".bravebot",
            ".bravebot-browser",
            ".ssh",
            ".aws",
            ".kube",
            ".docker",
            ".azure",
            ".config/gcloud",
            ".gnupg",
            "AppData/Roaming/gcloud",
            "AppData/Roaming/gnupg",
            "AppData/Local/BraveSoftware",
            "AppData/Local/Google/Chrome",
            "AppData/Local/Chromium",
            "AppData/Local/Microsoft/Edge",
            "AppData/Roaming/Mozilla",
            "AppData/Local/Microsoft/Credentials",
            "AppData/Roaming/Microsoft/Credentials",
            "AppData/Roaming/Microsoft/Protect",
            "AppData/Local/Microsoft/Vault",
            "AppData/Roaming/Microsoft/Vault",
        ]
        .iter()
        .map(|row| under(home, row))
        .collect();
        expected.sort();
        let mut windows = credential_locations(Prelude::Windows, home);
        windows.sort();

        assert_eq!(windows, expected);
    }

    /// Without a home directory only the absolute refusal can apply, and the policy still means
    /// something.
    #[test]
    fn the_run_base_without_a_home_refuses_only_the_machine_wide_keychains() {
        let policy = run_base(
            Prelude::MacOs,
            Path::new(THE_SESSIONS_TEMPORARY_DIRECTORY),
            None,
        );

        assert_eq!(policy.unreadable, vec![PathBuf::from("/Library/Keychains")]);
        assert!(policy.is_meaningful());
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
            egress_limited_to_a_port: false,
            grants_paths_that_do_not_exist: false,
            subtracts_from_a_grant: true,
            governs_sockets_with_a_path: true,
        }
    }

    /// The whole of what the base buys is what it leaves out. A row added to it for convenience,
    /// the home directory itself or the `~/.config` the XDG git configuration sits in, hands the
    /// key a push signs with and the token a publish uses to a program whose plan named neither.
    #[test]
    fn the_base_reaches_no_credential() {
        for prelude in EVERY_PLATFORM {
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
        for prelude in EVERY_PLATFORM {
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
    /// out of it, and the machine's copy is in it because git stops on a configuration file that
    /// is there and that it is refused. Writing any of them is a different thing: a program that
    /// can write one names the command `git` runs for a push in it.
    #[test]
    fn the_git_configuration_is_read_and_never_written() {
        for (prelude, the_machines) in [
            (Prelude::Linux, "/etc/gitconfig"),
            (Prelude::MacOs, "/private/etc/gitconfig"),
        ] {
            let policy = a_base(prelude);
            for configuration in [
                the_machines,
                "/home/a-person/.gitconfig",
                "/home/a-person/.config/git/config",
            ] {
                assert!(
                    policy.readable.contains(&PathBuf::from(configuration)),
                    "the {prelude:?} base misses {configuration}"
                );
                assert!(
                    !written_paths(&policy)
                        .iter()
                        .any(|path| Path::new(configuration).starts_with(path)),
                    "the {prelude:?} base grants writing {configuration}"
                );
            }
        }
    }

    /// The git rows are the only part of the base a home directory decides, so a machine with
    /// none of one still gets a profile a program can start under.
    #[test]
    fn a_machine_with_no_home_directory_gets_the_rest_of_the_base() {
        let policy = base(
            Prelude::Linux,
            Path::new(THE_SESSIONS_TEMPORARY_DIRECTORY),
            None,
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
        for prelude in EVERY_PLATFORM {
            // Windows has no null device to name: `NUL` is opened by name from any directory.
            let null_device = (prelude != Prelude::Windows).then(|| PathBuf::from("/dev/null"));
            let mut expected = vec![PathBuf::from(THE_SESSIONS_TEMPORARY_DIRECTORY)];
            expected.extend(null_device);
            assert_eq!(
                written_paths(&a_base(prelude)),
                expected,
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
            None,
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
        for prelude in EVERY_PLATFORM {
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
        for prelude in EVERY_PLATFORM {
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

        for prelude in [Prelude::Linux, Prelude::MacOs] {
            let policy = a_base(prelude);
            assert!(reaches(&policy, "/usr/bin/git"), "{prelude:?}");
            assert!(reaches(&policy, "/dev/urandom"), "{prelude:?}");
        }
    }

    /// A container reads the system directories without a grant, and a grant there is an entry
    /// only an account allowed to change that directory's permissions can write. A row for one
    /// is every program refused on a machine whose account is not that, so the Windows base
    /// holds the session's temporary directory and the git configuration a person keeps, and
    /// nothing of the machine's.
    #[test]
    fn a_windows_base_names_no_system_directory() {
        let windows = a_base(Prelude::Windows);

        assert!(Prelude::Windows.rows().is_empty());
        assert_eq!(
            granted_paths(&windows),
            vec![
                PathBuf::from(THE_SESSIONS_TEMPORARY_DIRECTORY),
                PathBuf::from("/home/a-person/.gitconfig"),
                PathBuf::from("/home/a-person/.config/git/config"),
                PathBuf::from(THE_SESSIONS_TEMPORARY_DIRECTORY),
            ]
        );
        assert!(windows.allow_network && windows.allow_subprocesses);
    }

    /// Without its configuration file the TLS library macOS ships aborts every program linked
    /// against it, and without the developer directory the `/usr/bin` developer tools are shims
    /// with nothing to run: `cargo`, `curl` and `git` all refused before any plan is read. The
    /// developer directory is the one the caller resolved, whatever the bundle holding it is
    /// called, and no row widens to what holds it, since `/Applications` is every program a
    /// person installed.
    #[test]
    fn a_macos_base_names_what_its_tls_library_and_developer_tools_start_from() {
        for (developer_directory, needed, another) in [
            (
                THE_COMMAND_LINE_TOOLS,
                "/Library/Developer/CommandLineTools/usr/bin/git",
                "/Applications/Xcode.app/Contents/Developer/usr/bin/git",
            ),
            (
                AN_XCODE,
                "/Applications/Xcode.app/Contents/SharedFrameworks/DVTSystemPrerequisites.framework",
                "/Library/Developer/CommandLineTools/usr/bin/git",
            ),
            (
                "/Applications/Xcode_16.4.app/Contents/Developer",
                "/Applications/Xcode_16.4.app/Contents/Developer/usr/bin/git",
                "/Applications/Xcode.app/Contents/Developer/usr/bin/git",
            ),
        ] {
            let macos = a_macos_base_with(developer_directory);
            assert!(reaches(&macos, "/private/etc/ssl/openssl.cnf"));
            assert!(reaches(&macos, needed), "{developer_directory}: {needed}");
            for elsewhere in [
                another,
                "/Applications/Docker.app/Contents/Resources/bin/docker",
                "/Library/Developer/CoreSimulator/Devices",
                "/Library/Keychains/System.keychain",
            ] {
                assert!(
                    !reaches(&macos, elsewhere),
                    "{developer_directory}: {elsewhere}"
                );
            }
        }
    }

    /// A developer directory is granted in the two shapes the platform installs one in. Any other
    /// is a path somebody pointed `xcode-select` at, and a row for it is the base naming a
    /// directory of a person's, or `/Applications` whole, on the word of that setting.
    #[test]
    fn a_developer_directory_anywhere_else_is_in_no_row() {
        let without_one = base(
            Prelude::MacOs,
            Path::new(THE_SESSIONS_TEMPORARY_DIRECTORY),
            None,
            Some(Path::new(A_HOME)),
        );

        for elsewhere in [
            "/Users/a-person/Applications/Xcode.app/Contents/Developer",
            "/Applications/../Users/a-person/Xcode.app/Contents/Developer",
            "/Applications/Utilities/Xcode.app/Contents/Developer",
            "/Applications/Xcode/Contents/Developer",
            "/Applications/.app/Contents/Developer",
            "/Applications/Xcode.app/Contents",
            "/Applications/Xcode.app/Contents/Resources",
            "/Applications/Xcode.app/Contents/Developer/usr",
            "/Applications/Xcode.app",
            "/Applications",
            "/Library/Developer",
            "/Library/Developer/CommandLineTools/usr",
            A_HOME,
            "/",
        ] {
            assert_eq!(
                granted_paths(&a_macos_base_with(elsewhere)),
                granted_paths(&without_one),
                "a developer directory at {elsewhere} is a row"
            );
        }
    }

    /// Every row here is shown by no prompt, so a path of a person's own in one is reach nobody
    /// sees and nobody audits. The machine's own directories are the whole of what may be in it.
    #[test]
    fn a_prelude_names_the_machines_directories_and_none_of_a_persons() {
        const THE_MACHINES_OWN: [&str; 9] = [
            "/bin", "/sbin", "/usr", "/lib", "/lib64", "/etc", "/private", "/dev", "/System",
        ];

        for prelude in EVERY_PLATFORM {
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
        } else if cfg!(windows) {
            Some(Prelude::Windows)
        } else {
            None
        };

        assert_eq!(Prelude::current(), written_down_for_this_platform);
    }
}
