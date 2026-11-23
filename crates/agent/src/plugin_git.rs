//! The `git` program that fetches a plugin source (PLUGIN-3, SANDBOX-30).
//!
//! The program is found as SANDBOX-30 says: among the `PATH` entries outside every directory a
//! confined stage may write, so a `git` found only in one reads as not installed. It is started
//! with `bravebot_sandbox::programs::start_against`, with this agent's own credentials taken off
//! its environment, a reduced `PATH` and the variables that select code checked against the list
//! below.
//!
//! The fetch is `git init`, `git fetch --depth 1 <remote> <ref>` and `git checkout`, run against a
//! git directory and a work tree that are two directories side by side in a fresh directory under
//! the system temporary directory. The work tree holds no `.git`, so the copy the staging step
//! makes of it holds none, and nothing in it is run in place. This one sequence fetches a branch, a
//! tag, a full commit or the remote's default branch.
//!
//! The clone, its stderr and its files are untrusted. The only things the driver takes from git are
//! whether each step succeeded and the 40 lowercase hex digits of `git rev-parse HEAD`, which pass
//! through [`Commit::parse`] and decide nothing.

use crate::plugin_source::{Commit, GitSource, Source};
use crate::plugins::{Refusal, Staged, stage_directory};
use bravebot_sandbox::programs::{
    CodeSelectors, Inherits, Names, Resolved, SearchPath, outside_stage_writable, start_against,
};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The protocols a plugin source may be fetched over. `git://` and `http` carry no authentication
/// and no confidentiality, `file` and `ext` run local code or read local files, and every other
/// transport is refused by `protocol.allow=never`.
pub(crate) const PROTOCOLS: [&str; 2] = ["https", "ssh"];

/// How long one `git` step may run before it is stopped.
const STEP_LIMIT: Duration = Duration::from_secs(300);

/// The variables `git` and the `ssh` it starts read to choose a program, a library or a directory
/// of configuration, beyond the generic table every checked start applies
/// ([SANDBOX-30](../../../docs/specs/sandboxing.md#SANDBOX-30)).
///
/// - `GIT_SSH_COMMAND`, `GIT_SSH`, `GIT_PROXY_COMMAND`, `GIT_ASKPASS`, `GIT_EDITOR`, `GIT_PAGER` and
///   `GIT_EXTERNAL_DIFF` each name a program `git` runs. The clone starts the first four at most,
///   and none of them is kept: a person whose `ssh` needs `GIT_SSH_COMMAND` sets `core.sshCommand`
///   in their own git configuration, which is read.
/// - `SSH_ASKPASS` and `SSH_ASKPASS_REQUIRE` name the program `ssh` runs to ask for a passphrase.
///   `SSH_SK_PROVIDER` and `SSH_PKCS11_HELPER` name a library and `SSH_SK_HELPER` a program `ssh`
///   loads or runs for a hardware key. A clone asks for neither, so none is kept.
/// - `GIT_TEMPLATE_DIR` is a directory whose `hooks` are copied into a new repository, and
///   `XDG_CONFIG_HOME` holds the `git/config` `git` reads, which can name a program. A directory
///   outside every directory a stage may write is kept.
///
/// `GIT_SSL_*` are not here: they name a certificate or a key file and no program, and the person's
/// own settings for them apply. `GIT_CONFIG_*` is removed by the generic list.
const CODE_SELECTORS: [(&str, Names); 14] = [
    ("GIT_SSH_COMMAND", Names::Options),
    ("GIT_SSH", Names::Options),
    ("GIT_PROXY_COMMAND", Names::Options),
    ("GIT_ASKPASS", Names::Options),
    ("GIT_EDITOR", Names::Options),
    ("GIT_PAGER", Names::Options),
    ("GIT_EXTERNAL_DIFF", Names::Options),
    ("SSH_ASKPASS", Names::Options),
    ("SSH_ASKPASS_REQUIRE", Names::Options),
    ("SSH_SK_PROVIDER", Names::Options),
    ("SSH_SK_HELPER", Names::Options),
    ("SSH_PKCS11_HELPER", Names::Options),
    ("GIT_TEMPLATE_DIR", Names::Directory),
    ("XDG_CONFIG_HOME", Names::Directory),
];

/// Variables that move the repository `git` works in. Each step names its git directory and work
/// tree itself, and none of these is kept so no step reads or writes another place.
const REPOSITORY_VARIABLES: [&str; 6] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
];

/// Configuration given to every step on its command line, where it outranks every file. It is not
/// given through `GIT_CONFIG_*`, which the environment check removes.
const PINNED: [&str; 12] = [
    // A redirect from the host the person was asked about is not followed (FETCH-4).
    "-c",
    "http.followRedirects=false",
    // No program named by the configuration of the fetched repository or by a hook path runs.
    "-c",
    "core.hooksPath=/dev/null",
    "-c",
    "core.fsmonitor=false",
    // No submodule is fetched.
    "-c",
    "submodule.recurse=false",
    "-c",
    "fetch.recurseSubmodules=false",
    // Nothing is asked of a terminal, which the interface owns: `GIT_TERMINAL_PROMPT=0` is set on
    // the start as well.
    "-c",
    "advice.detachedHead=false",
];

/// The `git` this agent may start, or `None` where the only one on `path` is in a directory a
/// confined stage may write.
pub(crate) fn locate_on(path: &OsStr) -> Option<Resolved> {
    Resolved::find_on(OsStr::new("git"), path)
}

/// [`locate_on`] for this process's own `PATH`.
pub(crate) fn locate() -> Option<Resolved> {
    locate_on(&std::env::var_os("PATH").unwrap_or_default())
}

/// A command for `git` with the environment SANDBOX-30 asks of it, against the given environment
/// and the given judgment of which files are outside the directories a stage may write.
fn command_against(
    git: &Resolved,
    held: impl IntoIterator<Item = (OsString, OsString)>,
    outside: &impl Fn(&Path) -> bool,
) -> Command {
    let mut command = start_against(
        git,
        Inherits::Without(&bravebot_config::scrub::own_credentials()),
        SearchPath::Reduced,
        CodeSelectors::Checked(&CODE_SELECTORS),
        held,
        outside,
    );
    for name in REPOSITORY_VARIABLES {
        command.env_remove(name);
    }
    command.env("GIT_TERMINAL_PROMPT", "0");
    command
}

fn command(git: &Resolved) -> Command {
    command_against(git, std::env::vars_os(), &outside_stage_writable)
}

/// A step's command: `base`, the pinned configuration, then the protocols `allow` names, then
/// `rest`.
fn step(base: Command, allow: &[&str], rest: &[&OsStr]) -> Command {
    let mut command = base;
    command.args(PINNED);
    command.args(["-c", "protocol.allow=never"]);
    for protocol in allow {
        command
            .arg("-c")
            .arg(format!("protocol.{protocol}.allow=always"));
    }
    command.args(rest);
    command
}

/// Runs `command` in `dir` with no input and no stderr, stopped after [`STEP_LIMIT`]. The output,
/// when `capture` asks for it, is at most the 41 bytes of a commit id and a newline. `None` where
/// the program did not start, did not succeed or did not finish in time. The working directory is
/// the staging directory, so no repository around this process's own directory is discovered.
fn run(mut command: Command, dir: &Path, capture: bool) -> Option<Vec<u8>> {
    use std::io::Read as _;
    command.current_dir(dir);
    command.stdin(Stdio::null()).stderr(Stdio::null());
    command.stdout(if capture {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    let mut child = bravebot_sandbox::process::starting_a_program(|| command.spawn()).ok()?;
    let deadline = Instant::now() + STEP_LIMIT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => {
                child.kill().ok();
                child.wait().ok();
                return None;
            }
        }
    };
    if !status.success() {
        return None;
    }
    let mut out = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        stdout.take(64).read_to_end(&mut out).ok()?;
    }
    Some(out)
}

/// A step that has failed. The reason is git's and is not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CloneFailed;

/// Fetches `remote` at `requested_ref`, or at its default branch where none is named, into
/// `<dir>/tree`, one commit deep, and returns the commit it resolved to. `dir` is an existing empty
/// directory and the work tree holds no `.git`. `allow` names the protocols that may be used.
pub(crate) fn shallow_clone(
    git: &Resolved,
    remote: &str,
    requested_ref: Option<&str>,
    allow: &[&str],
    dir: &Path,
) -> Result<Commit, CloneFailed> {
    shallow_clone_from(&|| command(git), remote, requested_ref, allow, dir)
}

/// [`shallow_clone`] with the command each step starts from given by `base`, so a test can start
/// the steps against an environment of its own choosing.
fn shallow_clone_from(
    base: &dyn Fn() -> Command,
    remote: &str,
    requested_ref: Option<&str>,
    allow: &[&str],
    dir: &Path,
) -> Result<Commit, CloneFailed> {
    let git_dir = dir.join("git");
    let tree = dir.join("tree");
    std::fs::create_dir(&tree).map_err(|_| CloneFailed)?;
    let separate = |what: &str, path: &Path| {
        let mut text = OsString::from(what);
        text.push(path);
        text
    };
    let git_dir_flag = separate("--git-dir=", &git_dir);
    let tree_flag = separate("--work-tree=", &tree);

    run(
        step(
            base(),
            allow,
            &[
                OsStr::new("init"),
                OsStr::new("-q"),
                OsStr::new("--bare"),
                OsStr::new("--template="),
                git_dir.as_os_str(),
            ],
        ),
        dir,
        false,
    )
    .ok_or(CloneFailed)?;
    run(
        step(
            base(),
            allow,
            &[
                &git_dir_flag,
                OsStr::new("fetch"),
                OsStr::new("-q"),
                OsStr::new("--depth=1"),
                OsStr::new("--no-recurse-submodules"),
                OsStr::new("--"),
                OsStr::new(remote),
                OsStr::new(requested_ref.unwrap_or("HEAD")),
            ],
        ),
        dir,
        false,
    )
    .ok_or(CloneFailed)?;
    run(
        step(
            base(),
            allow,
            &[
                OsStr::new("-c"),
                OsStr::new("core.bare=false"),
                &git_dir_flag,
                &tree_flag,
                OsStr::new("checkout"),
                OsStr::new("-q"),
                OsStr::new("--detach"),
                OsStr::new("FETCH_HEAD"),
                OsStr::new("--"),
            ],
        ),
        dir,
        false,
    )
    .ok_or(CloneFailed)?;
    let head = run(
        step(
            base(),
            allow,
            &[&git_dir_flag, OsStr::new("rev-parse"), OsStr::new("HEAD")],
        ),
        dir,
        true,
    )
    .ok_or(CloneFailed)?;
    let text = String::from_utf8(head).map_err(|_| CloneFailed)?;
    Commit::parse(text.trim_end_matches('\n')).ok_or(CloneFailed)
}

/// A fresh directory that only this account can enter, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    /// Makes a directory directly under `parent`, which exists. A name that is taken is not
    /// reused: the directory is made with `create_dir`, which fails on an existing name, a link
    /// included.
    fn under(parent: &Path) -> std::io::Result<Self> {
        use std::hash::{BuildHasher as _, Hasher as _};
        let seed = std::collections::hash_map::RandomState::new();
        for attempt in 0_u32..64 {
            let mut hasher = seed.build_hasher();
            hasher.write_u32(attempt);
            hasher.write_u32(std::process::id());
            let path = parent.join(format!(".bravebot-plugin-{:016x}", hasher.finish()));
            match make_private(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
        Err(std::io::ErrorKind::AlreadyExists.into())
    }
}

#[cfg(unix)]
fn make_private(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt as _;
    std::fs::DirBuilder::new().mode(0o700).create(path)
}

#[cfg(not(unix))]
fn make_private(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir(path)
}

impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

/// Fetches `source` with `git` into a fresh directory under `parent` and stages the tree it holds,
/// recorded with the requested ref and the commit the fetch resolved to. The directory under
/// `parent` is removed before this returns, whatever the answer.
///
/// `parent` is the system temporary directory for a person's install. It is outside the state
/// directory and outside every workspace and session directory.
pub(crate) fn stage_from(
    git: &Resolved,
    source: &GitSource,
    allow: &[&str],
    parent: &Path,
    home: &Path,
) -> Result<Staged, Refusal> {
    let scratch = Scratch::under(parent).map_err(|error| Refusal::Io(error.kind()))?;
    let commit = shallow_clone(
        git,
        &source.url,
        source.requested_ref.as_deref(),
        allow,
        &scratch.0,
    )
    .map_err(|CloneFailed| Refusal::CloneFailed)?;
    stage_directory(
        &scratch.0.join("tree"),
        Source::Git {
            url: source.url.clone(),
            requested_ref: source.requested_ref.clone(),
            commit,
        },
        home,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether the start takes `name` off the environment when the person's environment holds it
    /// at `value` and only a path under `/safe` is outside the directories a stage may write.
    fn withheld_value(name: &str, value: &str) -> bool {
        let command = command_against(
            &Resolved::written_in_source("/usr/bin/git"),
            [(OsString::from(name), OsString::from(value))],
            &|path: &Path| path.starts_with("/safe"),
        );
        command
            .get_envs()
            .any(|(held, set)| held == OsStr::new(name) && set.is_none())
    }

    /// SANDBOX-30, PLUGIN-3: each variable that names a program for `git` or `ssh` to run is taken
    /// off whatever it holds, and a directory it names is kept only outside the directories a
    /// stage may write.
    ///
    /// Rejects: a start that keeps `GIT_SSH_COMMAND` or `GIT_ASKPASS` (code the person's
    /// environment names runs during a clone), a list that leaves out an ssh helper, and a
    /// directory check that keeps a stage-writable template directory or one that removes every
    /// value.
    #[test]
    fn the_variables_that_name_a_program_for_git_or_ssh_are_removed() {
        assert!(withheld_value("GIT_SSH_COMMAND", "/safe/tool"));
        assert!(withheld_value("GIT_SSH", "/safe/tool"));
        assert!(withheld_value("GIT_PROXY_COMMAND", "/safe/tool"));
        assert!(withheld_value("GIT_ASKPASS", "/safe/tool"));
        assert!(withheld_value("GIT_EDITOR", "/safe/tool"));
        assert!(withheld_value("GIT_PAGER", "/safe/tool"));
        assert!(withheld_value("GIT_EXTERNAL_DIFF", "/safe/tool"));
        assert!(withheld_value("SSH_ASKPASS", "/safe/tool"));
        assert!(withheld_value("SSH_ASKPASS_REQUIRE", "force"));
        assert!(withheld_value("SSH_SK_PROVIDER", "/safe/lib.so"));
        assert!(withheld_value("SSH_SK_HELPER", "/safe/tool"));
        assert!(withheld_value("SSH_PKCS11_HELPER", "/safe/tool"));
        assert!(withheld_value("GIT_TEMPLATE_DIR", "/stage/templates"));
        assert!(!withheld_value("GIT_TEMPLATE_DIR", "/safe/templates"));
        assert!(withheld_value("XDG_CONFIG_HOME", "/stage/config"));
        assert!(!withheld_value("XDG_CONFIG_HOME", "/safe/config"));
        let listed: Vec<&str> = CODE_SELECTORS.iter().map(|(name, _)| *name).collect();
        assert_eq!(listed.len(), 14, "{listed:?}");
    }

    /// SANDBOX-30: the generic selectors apply to this start as well, `GIT_CONFIG_*` and
    /// `GIT_EXEC_PATH` among them, and a variable that only names a certificate is left for the
    /// person's own settings.
    ///
    /// Rejects: an unchecked start (`CodeSelectors::Kept`), which passes `GIT_CONFIG_COUNT` and a
    /// stage-writable `GIT_EXEC_PATH` to git, and a start that removes `GIT_SSL_CAINFO`.
    #[test]
    fn the_generic_selectors_apply_and_a_certificate_variable_is_left_alone() {
        assert!(withheld_value("GIT_CONFIG_COUNT", "1"));
        assert!(withheld_value("GIT_CONFIG_KEY_0", "core.sshCommand"));
        assert!(withheld_value("GIT_CONFIG_GLOBAL", "/safe/gitconfig"));
        assert!(withheld_value("GIT_EXEC_PATH", "/stage/exec"));
        assert!(!withheld_value("GIT_EXEC_PATH", "/safe/exec"));
        assert!(withheld_value("LD_PRELOAD", "/stage/lib.so"));
        assert!(!withheld_value("GIT_SSL_CAINFO", "/safe/ca.pem"));
    }

    /// CRED-14: this agent's own credentials, and the variables that move the repository a step
    /// works in, never reach git.
    ///
    /// Rejects: `Inherits::All`, and a step that lets `GIT_DIR` or `GIT_WORK_TREE` override where
    /// the fetch is made.
    #[test]
    fn this_agents_credentials_and_the_repository_variables_are_not_passed() {
        let own = bravebot_config::scrub::own_credentials();
        let name = own
            .first()
            .expect("the agent holds at least one credential");
        assert!(withheld_value(name, "secret"));
        for name in REPOSITORY_VARIABLES {
            assert!(withheld_value(name, "/safe/elsewhere"), "{name}");
        }
    }

    /// SANDBOX-30, FETCH-4, PLUGIN-3: each step carries the pinned configuration and the protocols
    /// on its argv, and the person's environment is never the place a setting comes from.
    ///
    /// Rejects: a dropped `http.followRedirects=false`, a missing `--no-recurse-submodules`, a
    /// hook path or fsmonitor left to the configuration, and a protocol that is not on the list.
    #[test]
    fn every_step_pins_the_redirect_hook_fsmonitor_and_protocol_settings_on_its_argv() {
        let git = Resolved::written_in_source("/usr/bin/git");
        let built = step(command(&git), &PROTOCOLS, &[OsStr::new("fetch")]);
        let args: Vec<String> = built
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        for wanted in [
            "http.followRedirects=false",
            "core.hooksPath=/dev/null",
            "core.fsmonitor=false",
            "submodule.recurse=false",
            "fetch.recurseSubmodules=false",
            "protocol.allow=never",
            "protocol.https.allow=always",
            "protocol.ssh.allow=always",
        ] {
            assert!(args.iter().any(|arg| arg == wanted), "{wanted}: {args:?}");
        }
        assert!(!args.iter().any(|arg| arg == "protocol.file.allow=always"));
        assert!(!args.iter().any(|arg| arg == "protocol.git.allow=always"));
        assert!(!args.iter().any(|arg| arg == "protocol.http.allow=always"));
        assert_eq!(args.last().map(String::as_str), Some("fetch"));
    }

    /// SANDBOX-30: a `git` found only in a directory a stage may write is not installed.
    ///
    /// Rejects: a lookup that searches every `PATH` entry, which would start a program a stage
    /// wrote.
    #[test]
    fn a_git_found_only_where_a_stage_may_write_reads_as_not_installed() {
        assert!(locate_on(OsStr::new("")).is_none());
        let scratch = tempfile::tempdir().unwrap();
        let planted = scratch.path().join("git");
        std::fs::write(&planted, b"#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&planted, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        bravebot_sandbox::programs::keep_out(scratch.path());
        assert!(locate_on(scratch.path().as_os_str()).is_none());
    }

    fn source_of(remote: &fixtures::Remote, requested: Option<&str>) -> GitSource {
        GitSource {
            url: remote.path.to_string_lossy().into_owned(),
            host: "local.test".to_string(),
            requested_ref: requested.map(str::to_string),
        }
    }

    /// Stages `requested` from `remote` into a fresh state directory and installs it, returning the
    /// state directory and the installed plugin's record.
    fn installed_from(
        remote: &fixtures::Remote,
        requested: Option<&str>,
    ) -> (tempfile::TempDir, crate::plugins::Installed) {
        let git = locate().expect("git must be on PATH for this test");
        let parent = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let staged = stage_from(
            &git,
            &source_of(remote, requested),
            &["file"],
            parent.path(),
            home.path(),
        )
        .unwrap();
        crate::plugins::commit_install(staged, home.path()).unwrap();
        let record = crate::plugins::read_record(home.path(), "demo").unwrap();
        assert_eq!(
            std::fs::read_dir(parent.path()).unwrap().count(),
            0,
            "the fetch's own directory is removed"
        );
        (home, record)
    }

    /// PLUGIN-14: a branch records the commit it pointed to, a tag the commit it names, a full
    /// commit itself and no ref the remote's default branch, and the installed tree is the tree of
    /// that commit.
    ///
    /// Rejects: a fetch that ignores the ref and takes the default branch (the tag and the side
    /// branch would record the default branch's commit), a record that holds the ref text where the
    /// commit belongs, a full commit that is not fetched by its id, and a tree that is not the
    /// resolved commit's.
    #[test]
    fn a_branch_a_tag_a_full_commit_and_no_ref_each_record_the_commit_they_resolve_to() {
        let remote = fixtures::remote();
        let commit = |id: &str| Commit::parse(id).unwrap();
        let of = |requested: Option<&str>| {
            let (home, record) = installed_from(&remote, requested);
            let tree = crate::plugins::installed_dir(home.path(), "demo");
            let files = (
                tree.join("second.txt").exists(),
                tree.join("third.txt").exists(),
            );
            (record.source, files)
        };
        let git_source = |requested: Option<&str>, id: &str| Source::Git {
            url: remote.path.to_string_lossy().into_owned(),
            requested_ref: requested.map(str::to_string),
            commit: commit(id),
        };
        assert_eq!(
            of(Some("main")),
            (git_source(Some("main"), &remote.second), (true, false))
        );
        assert_eq!(
            of(Some("v1")),
            (git_source(Some("v1"), &remote.first), (false, false))
        );
        assert_eq!(
            of(Some("side")),
            (git_source(Some("side"), &remote.side), (false, true))
        );
        assert_eq!(
            of(Some(&remote.first)),
            (
                git_source(Some(&remote.first), &remote.first),
                (false, false)
            )
        );
        assert_eq!(of(None), (git_source(None, &remote.second), (true, false)));
    }

    /// S9, S15: the installed tree holds no `.git`, and what the fetch made under the staging parent
    /// is gone after an install and after a fetch that failed.
    ///
    /// Rejects: a work tree that carries the repository's own directory into the copy (its
    /// configuration and hooks would then sit in the installed plugin), and a staging directory
    /// left behind on either road.
    #[test]
    fn the_installed_tree_holds_no_git_directory_and_a_failed_fetch_leaves_nothing() {
        let remote = fixtures::remote();
        let (home, _) = installed_from(&remote, Some("main"));
        let tree = crate::plugins::installed_dir(home.path(), "demo");
        assert!(tree.join("plugin.toml").exists());
        assert!(!tree.join(".git").exists());

        let git = locate().expect("git must be on PATH for this test");
        let parent = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let home = state.path().join("home");
        let refused = stage_from(
            &git,
            &source_of(&remote, Some("no-such-branch")),
            &["file"],
            parent.path(),
            &home,
        );
        assert_eq!(refused.err(), Some(Refusal::CloneFailed));
        assert_eq!(std::fs::read_dir(parent.path()).unwrap().count(), 0);
        assert!(!home.exists());
    }

    /// S9: the fetch's directory is made private to the account and under the parent it is given,
    /// and a name that is already taken is never reused.
    ///
    /// Rejects: a directory made with the process umask (readable by other accounts) and one made
    /// with `create_dir_all`, which accepts a directory another account planted.
    #[cfg(unix)]
    #[test]
    fn the_fetch_directory_is_private_and_never_one_that_was_already_there() {
        use std::os::unix::fs::PermissionsExt as _;
        let parent = tempfile::tempdir().unwrap();
        let scratch = Scratch::under(parent.path()).unwrap();
        let mode = std::fs::metadata(&scratch.0).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
        assert_eq!(scratch.0.parent(), Some(parent.path()));
        assert_eq!(
            make_private(&scratch.0).unwrap_err().kind(),
            std::io::ErrorKind::AlreadyExists
        );
    }
}

#[cfg(test)]
#[cfg(unix)]
mod safety_tests;

#[cfg(test)]
pub(crate) mod fixtures {
    use std::path::{Path, PathBuf};

    /// Runs `git` in `dir` with no configuration of the person's own and returns what it printed.
    pub(crate) fn git(dir: &Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.test",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "init.defaultBranch=main",
            ])
            .args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .output()
            .expect("git must be on PATH for this test");
        assert!(output.status.success(), "git {args:?} failed");
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    }

    /// A repository holding a plugin on `main` (two commits, the first tagged `v1`) and on `side`
    /// (the first commit and one more), so the tree of each ref is told from the others by a file.
    pub(crate) struct Remote {
        _keep: tempfile::TempDir,
        pub path: PathBuf,
        pub first: String,
        pub second: String,
        pub side: String,
    }

    pub(crate) fn remote() -> Remote {
        let keep = tempfile::tempdir().unwrap();
        let path = keep.path().join("demo-remote");
        std::fs::create_dir_all(path.join("skills").join("hello")).unwrap();
        git(&path, &["init", "-q"]);
        std::fs::write(
            path.join("plugin.toml"),
            "name = \"demo\"\nversion = \"1.0.0\"\ndescription = \"A demo\"\nskills = \"skills\"\n",
        )
        .unwrap();
        std::fs::write(
            path.join("skills").join("hello").join("SKILL.md"),
            "---\nname: hello\ndescription: Says hello.\n---\n\nBody.\n",
        )
        .unwrap();
        git(&path, &["add", "-A"]);
        git(&path, &["commit", "-q", "-m", "one"]);
        git(&path, &["tag", "v1"]);
        let first = git(&path, &["rev-parse", "HEAD"]);
        std::fs::write(path.join("second.txt"), "two\n").unwrap();
        git(&path, &["add", "-A"]);
        git(&path, &["commit", "-q", "-m", "two"]);
        let second = git(&path, &["rev-parse", "HEAD"]);
        git(&path, &["checkout", "-q", "-b", "side", "v1"]);
        std::fs::write(path.join("third.txt"), "three\n").unwrap();
        git(&path, &["add", "-A"]);
        git(&path, &["commit", "-q", "-m", "three"]);
        let side = git(&path, &["rev-parse", "HEAD"]);
        git(&path, &["checkout", "-q", "main"]);
        Remote {
            _keep: keep,
            path,
            first,
            second,
            side,
        }
    }
}
