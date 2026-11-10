//! `bravebot doctor --push-identity`: which GitHub account each of the person's ssh keys pushes as
//! ([SANDBOX-16]).
//!
//! One connection per public key, `ssh -T -i <key> -o IdentitiesOnly=yes git@<host>`, so GitHub
//! answers for that key alone. The greeting is bytes from a remote host: it is read for the login
//! it names, shown on the person's own screen, and goes nowhere else. It never selects a key and
//! it is not an input to a stage, a prompt or a trace.
//!
//! [SANDBOX-16]: ../../../docs/specs/sandboxing.md

use crate::exit::{Ending, fail};
use bravebot_i18n::t;
use bravebot_sandbox::programs::{Names, selectors};
use bravebot_sandbox::scope::Address;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// The host asked about when neither the argument nor `origin` names another.
const DEFAULT_HOST: &str = "github.com";

/// The most bytes of an answer read.
const ANSWER_LIMIT: usize = 4096;

/// How long one connection may take. A hardware key waits here for a touch.
const WAIT: std::time::Duration = std::time::Duration::from_secs(60);

/// What the host said for one key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Greeting {
    /// The key is registered to this account.
    Login(String),
    /// No account has the key: `Permission denied (publickey)`.
    NotRecognised,
    /// Anything else, including no answer.
    Unanswered,
}

/// Read an answer for the login it greets, as `Hi <login>! You've successfully authenticated`.
///
/// The login is kept only where it is written as GitHub writes one, letters, digits and `-`, so
/// that nothing else the host sent reaches the screen.
pub fn classify(answer: &str) -> Greeting {
    for line in answer.lines() {
        if let Some(rest) = line.strip_prefix("Hi ")
            && let Some((login, after)) = rest.split_once('!')
            && after
                .trim_start()
                .starts_with("You've successfully authenticated")
            && !login.is_empty()
            && login.len() <= 39
            && login
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Greeting::Login(login.to_string());
        }
        if line.contains("Permission denied (publickey") {
            return Greeting::NotRecognised;
        }
    }
    Greeting::Unanswered
}

/// What the machine is asked. Injected, so a test connects to nothing.
pub trait Ssh {
    /// The `SHA256:...` fingerprint of the public key file, or `None`.
    fn fingerprint(&self, key: &Path) -> Option<String>;
    /// What `host` says to a connection that offers `key` and no other.
    fn greeting(&self, key: &Path, host: &str) -> Greeting;
}

/// One key and what is known about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub key: PathBuf,
    pub fingerprint: Option<String>,
    pub greeting: Greeting,
    /// Who chose this key for the owner asked about, where anybody did.
    pub chosen: Option<ChosenBy>,
}

/// Who chose the key a push to the owner offers first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChosenBy {
    /// The person's `git.identities`.
    Settings,
    /// The person, at a run prompt that numbered the keys.
    Prompt,
}

/// Ask about each key. `chosen` is the key a push to the owner offers first and who chose it,
/// judged as a stage judges it.
pub fn probe(
    ssh: &dyn Ssh,
    keys: &[PathBuf],
    host: &str,
    chosen: Option<(&Path, ChosenBy)>,
) -> Vec<Row> {
    keys.iter()
        .map(|key| Row {
            key: key.clone(),
            fingerprint: ssh.fingerprint(key),
            greeting: ssh.greeting(key, host),
            chosen: chosen
                .filter(|(path, _)| *path == key.as_path())
                .map(|(_, by)| by),
        })
        .collect()
}

/// The lines the report prints, as `(name, value)` pairs.
///
/// `owner` is the account the repository belongs to, where one was named. A login that equals it
/// owns the repository, and one that does not may still write to it as a member of an
/// organisation, which the line says rather than guessing.
pub fn facts(host: &str, owner: Option<&str>, rows: &[Row]) -> Vec<(String, String)> {
    let mut lines = Vec::new();
    match owner {
        Some(owner) => lines.push((
            t!(doctor_push_remote).to_string(),
            format!("{host}/{owner}"),
        )),
        None => lines.push((
            t!(doctor_push_remote).to_string(),
            t!(doctor_push_no_remote, host = host),
        )),
    }
    for row in rows {
        let name = row.key.display().to_string();
        let fingerprint = row
            .fingerprint
            .clone()
            .unwrap_or_else(|| t!(doctor_push_no_fingerprint).to_string());
        let login = match &row.greeting {
            Greeting::Login(login) => match owner {
                Some(owner) if login.eq_ignore_ascii_case(owner) => {
                    t!(doctor_push_login_owns, login = login, owner = owner).to_string()
                }
                Some(owner) => {
                    t!(doctor_push_login_differs, login = login, owner = owner).to_string()
                }
                None => t!(doctor_push_login, login = login).to_string(),
            },
            Greeting::NotRecognised => t!(doctor_push_not_recognised, host = host).to_string(),
            Greeting::Unanswered => t!(doctor_push_unanswered, host = host).to_string(),
        };
        lines.push((name, format!("{fingerprint}, {login}")));
        if let Some(by) = row.chosen {
            lines.push((
                t!(doctor_push_chosen).to_string(),
                match by {
                    ChosenBy::Settings => t!(doctor_push_chosen_by),
                    ChosenBy::Prompt => t!(doctor_push_chosen_at_prompt),
                }
                .to_string(),
            ));
        }
    }
    lines
}

/// The real machine: `ssh-keygen` and `ssh`, each bounded.
struct Machine;

impl Ssh for Machine {
    fn fingerprint(&self, key: &Path) -> Option<String> {
        let answer = run("ssh-keygen", |program| fingerprint_command(program, key))?;
        // `256 SHA256:<hash> <comment> (ED25519)`: the comment is file content and is not kept.
        let hash = answer.split_whitespace().nth(1)?;
        let (kind, body) = hash.split_once(':')?;
        (matches!(kind, "SHA256" | "MD5")
            && body.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=' | b':')
            }))
        .then(|| hash.to_string())
    }

    fn greeting(&self, key: &Path, host: &str) -> Greeting {
        run("ssh", |program| greeting_command(program, key, host))
            .map_or(Greeting::Unanswered, |answer| classify(&answer))
    }
}

/// `ssh-keygen -lf <key>`: the fingerprint of a public key file.
fn fingerprint_command(program: &Path, key: &Path) -> std::process::Command {
    let mut command = started(program);
    command.arg("-lf").arg(key);
    command
}

/// `ssh -T -i <key> -o IdentitiesOnly=yes -o ConnectTimeout=15 git@<host>`: a connection that
/// offers `key` and no other.
fn greeting_command(program: &Path, key: &Path, host: &str) -> std::process::Command {
    let mut command = started(program);
    command
        .args(["-T", "-i"])
        .arg(key)
        .args(["-o", "IdentitiesOnly=yes", "-o", "ConnectTimeout=15"])
        .arg(format!("git@{host}"));
    command
}

/// `git config --get remote.origin.url`: the address `origin` has.
fn origin_command(program: &Path) -> std::process::Command {
    let mut command = started(program);
    command.args(["config", "--get", "remote.origin.url"]);
    command
}

/// The directories a confined stage opened here could have written: the one this command ran in
/// and the checkout holding it, which is the nearest ancestor with a `.git`.
///
/// This is a new process, so [`bravebot_sandbox::programs`] knows of no session. A checkout's
/// `bin` first on `PATH` would otherwise be searched for the programs started below.
fn checkout(cwd: &Path) -> Vec<PathBuf> {
    let mut directories = vec![cwd.to_path_buf()];
    if let Some(root) = cwd
        .ancestors()
        .find(|ancestor| ancestor.join(".git").exists())
        && root != cwd
    {
        directories.push(root.to_path_buf());
    }
    directories
}

/// Leave out of the lookup for programs the directory this command ran in and the checkout holding
/// it. A command that starts programs with no session of its own calls this before its first
/// lookup.
pub(crate) fn keep_out_where_this_ran() {
    if let Ok(cwd) = std::env::current_dir() {
        for directory in checkout(&cwd) {
            bravebot_sandbox::programs::keep_out(&directory);
        }
    }
}

/// The variables `ssh-keygen`, `ssh` and `git` read to choose a program to run
/// ([SANDBOX-30], [PATH-008](../../../docs/best-practices/paths.md#PATH-008)).
///
/// - `SSH_ASKPASS` is the program `ssh` runs to ask for a passphrase, and `SSH_ASKPASS_REQUIRE`
///   makes it run with a terminal present. `SSH_SK_PROVIDER` is a library `ssh` and `ssh-keygen`
///   load for a hardware key, and `SSH_SK_HELPER` is the helper program `ssh` runs. This command
///   asks for no passphrase and no hardware key from the environment, so none is kept
///   ([`Names::Options`] never passes).
/// - `DEVELOPER_DIR` is where the macOS `/usr/bin/git` shim finds the toolchain it runs, so one
///   outside every directory a stage may write is kept.
/// - `BROWSER` and the four `XDG_` directories are what `xdg-open` and Python's `webbrowser` read to
///   choose the program that opens a page. None of the three commands opens one itself, but `git`
///   reads the configuration under `XDG_CONFIG_HOME`, and a `credential.helper` or `core.sshCommand`
///   it names can be a login flow that does. The names and kinds are those of the browser opener
///   (`crates/tui/src/opener.rs`). `XDG_DATA_DIRS` is also in the generic list and is repeated here
///   so this list reads as complete.
///
/// [SANDBOX-30]: ../../../docs/specs/sandboxing.md
const CODE_SELECTORS: [(&str, Names); 10] = [
    ("SSH_ASKPASS", Names::Options),
    ("SSH_ASKPASS_REQUIRE", Names::Options),
    ("SSH_SK_PROVIDER", Names::Options),
    ("SSH_SK_HELPER", Names::Options),
    ("DEVELOPER_DIR", Names::Directory),
    selectors::BROWSER,
    ("XDG_DATA_HOME", Names::Directory),
    ("XDG_DATA_DIRS", Names::Directories),
    ("XDG_CONFIG_HOME", Names::Directory),
    ("XDG_CONFIG_DIRS", Names::Directories),
];

/// The shell `ssh` runs a `ProxyCommand` or a `LocalCommand` with when `SHELL` names one.
const SHELL: &str = "/bin/sh";

/// A command for `program`, with this agent's own credentials taken off it.
///
/// Every program this file starts is built here: they run unconfined with the person's access to
/// `~/.ssh`, and [CRED-14](../../../docs/specs/credential-protection.md#CRED-14) keeps what this
/// agent authenticates with out of reach of such a program. Its `PATH` is reduced to the entries
/// the lookup searches, so a program `ssh` starts by name from the person's configuration, a
/// `ProxyCommand` among them, is not one a stage wrote ([SANDBOX-30]). Each of [`CODE_SELECTORS`]
/// is taken off unless it passes the check the other starts share, and `SHELL` is [`SHELL`]
/// whatever the person's is.
///
/// [SANDBOX-30]: ../../../docs/specs/sandboxing.md
fn started(program: &Path) -> std::process::Command {
    started_with(
        program,
        std::env::vars_os(),
        &bravebot_sandbox::programs::outside_stage_writable,
    )
}

/// [`started`] against a given environment and a given judgment of which files are outside the
/// directories a stage may write.
fn started_with(
    program: &Path,
    held: impl IntoIterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
    outside_stage_writable: &impl Fn(&Path) -> bool,
) -> std::process::Command {
    let mut command = bravebot_sandbox::programs::start_against(
        &bravebot_sandbox::programs::Resolved::found_earlier(program),
        bravebot_sandbox::programs::Inherits::Without(&bravebot_config::scrub::own_credentials()),
        bravebot_sandbox::programs::SearchPath::Reduced,
        bravebot_sandbox::programs::CodeSelectors::Checked(&CODE_SELECTORS),
        held,
        outside_stage_writable,
    );
    command.env("SHELL", SHELL);
    command
}

/// Standard output and standard error of the command `build` makes from `program`, run to
/// completion within [`WAIT`].
///
/// `program` is found as [SANDBOX-30] says, and one found only where a stage could have written is
/// not started: it reads as absent. `build` is one of the `_command` functions above, each of which
/// begins from [`started`].
///
/// [SANDBOX-30]: ../../../docs/specs/sandboxing.md
fn run(program: &str, build: impl FnOnce(&Path) -> std::process::Command) -> Option<String> {
    use std::io::Read;
    use std::process::Stdio;
    let found = bravebot_sandbox::programs::find(std::ffi::OsStr::new(program))?;
    let mut child = build(&found)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let mut streams = Vec::new();
    if let Some(out) = child.stdout.take() {
        streams.push(Box::new(out) as Box<dyn Read + Send>);
    }
    if let Some(err) = child.stderr.take() {
        streams.push(Box::new(err) as Box<dyn Read + Send>);
    }
    let readers: Vec<_> = streams
        .into_iter()
        .map(|stream| {
            std::thread::spawn(move || {
                let mut bytes = Vec::new();
                let _ = stream.take(ANSWER_LIMIT as u64).read_to_end(&mut bytes);
                bytes
            })
        })
        .collect();
    let deadline = std::time::Instant::now() + WAIT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
        }
    }
    let mut answer = Vec::new();
    for reader in readers {
        answer.extend(reader.join().unwrap_or_default());
        answer.push(b'\n');
    }
    Some(String::from_utf8_lossy(&answer).into_owned())
}

/// What the person's repository names as its remote: the address `origin` has, read from the
/// directory the command ran in. Display only, so a repository's own configuration choosing
/// nothing here is not a trust question.
fn origin() -> Option<Address> {
    let answer = run("git", origin_command)?;
    bravebot_sandbox::scope::ssh_address(&["push".to_string(), answer.lines().next()?.to_string()])
}

/// The address the command names: `<host>/<owner>` as an argument, or `origin`'s.
fn named(argument: Option<&str>) -> Result<Option<Address>, ()> {
    match argument {
        None => Ok(None),
        Some(text) => Address::from_entry_name(text).map(Some).ok_or(()),
    }
}

/// Probe `keys` and print the report.
pub fn report(
    ssh: &dyn Ssh,
    keys: &[PathBuf],
    remote: Option<&Address>,
    chosen: Option<(&Path, ChosenBy)>,
) -> Ending {
    let host = remote.map_or(DEFAULT_HOST, |address| address.host.as_str());
    let rows = probe(ssh, keys, host, chosen);
    for (name, value) in facts(host, remote.map(|address| address.owner.as_str()), &rows) {
        crate::doctor::fact(name, value);
    }
    Ending::Done
}

pub fn run_command(argument: Option<&str>) -> ExitCode {
    let Ok(argument) = named(argument) else {
        return fail(Ending::Argument, t!(cli_doctor_push_identity_argument));
    };
    keep_out_where_this_ran();
    let Some(home) = bravebot_agent::home::profile() else {
        return fail(Ending::Failed, t!(doctor_push_no_home));
    };
    let keys = bravebot_sandbox::scope::candidate_public_keys(&home);
    if keys.is_empty() {
        return fail(Ending::Failed, t!(doctor_push_no_keys));
    }
    let remote = argument.or_else(origin);
    let settings = bravebot_config::Settings::load();
    let chosen = remote
        .as_ref()
        .and_then(|address| chosen_for(address, &settings, &home));
    report(
        &Machine,
        &keys,
        remote.as_ref(),
        chosen.as_ref().map(|(key, by)| (key.as_path(), *by)),
    )
    .code()
}

/// The key a push to `address` offers first and who chose it: the key the person's `git.identities`
/// entry names where there is one, and otherwise the one they chose at a run prompt, each judged
/// as a stage judges it.
fn chosen_for(
    address: &Address,
    settings: &bravebot_config::Settings,
    home: &Path,
) -> Option<(PathBuf, ChosenBy)> {
    if let Some(entry) = settings.git_identities().for_address(address) {
        return bravebot_sandbox::scope::identity_key(&entry.key, home)
            .map(|key| (key, ChosenBy::Settings));
    }
    let state = bravebot_agent::home::directory()?;
    let chosen = bravebot_agent::reach::Store::new(&state)
        .chosen_keys()
        .into_iter()
        .find(|held| held.address.same_as(address))?;
    bravebot_sandbox::scope::identity_key(chosen.key?.to_str()?, home)
        .map(|key| (key, ChosenBy::Prompt))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GREETING: &str = "Hi netzenbot! You've successfully authenticated, but GitHub does not provide shell access.";

    /// A canned machine: an answer per key file name, and nothing is connected to.
    struct Canned(Vec<(&'static str, &'static str)>);

    impl Ssh for Canned {
        fn fingerprint(&self, key: &Path) -> Option<String> {
            let name = key.file_name()?.to_str()?;
            Some(format!("SHA256:fp-of-{name}"))
        }
        fn greeting(&self, key: &Path, _host: &str) -> Greeting {
            let name = key.file_name().and_then(|name| name.to_str()).unwrap_or("");
            self.0
                .iter()
                .find(|(held, _)| *held == name)
                .map_or(Greeting::Unanswered, |(_, answer)| classify(answer))
        }
    }

    fn keys() -> Vec<PathBuf> {
        vec![
            PathBuf::from("/h/.ssh/work.pub"),
            PathBuf::from("/h/.ssh/bot.pub"),
        ]
    }

    fn text(lines: &[(String, String)]) -> String {
        lines
            .iter()
            .map(|(name, value)| format!("{name} {value}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// A key GitHub accepts and a key no account has are told apart, so a person sees which key is
    /// dead rather than a pair of identical lines. Rejects a classifier that treats every answer
    /// that is not a greeting as unknown, or every refusal as a login.
    #[test]
    fn a_greeting_and_a_refusal_are_told_apart() {
        assert_eq!(classify(GREETING), Greeting::Login("netzenbot".to_string()));
        assert_eq!(
            classify("git@github.com: Permission denied (publickey)."),
            Greeting::NotRecognised
        );
        assert_eq!(classify("Connection timed out"), Greeting::Unanswered);
        assert_eq!(classify(""), Greeting::Unanswered);
    }

    /// The login is bytes from a remote host and goes to a terminal, so one that is not spelled as a
    /// GitHub login is not shown. Rejects a classifier that takes whatever sits between `Hi ` and
    /// `!`, which would print an escape sequence.
    #[test]
    fn a_login_that_is_not_spelled_as_one_is_not_shown() {
        for hostile in [
            "Hi \u{1b}[31mred! You've successfully authenticated",
            "Hi a b! You've successfully authenticated",
            "Hi ! You've successfully authenticated",
            &format!("Hi {}! You've successfully authenticated", "a".repeat(40)),
        ] {
            assert_eq!(classify(hostile), Greeting::Unanswered, "{hostile:?}");
        }
    }

    /// Each key gets its own row, with the login its own connection reported. Rejects a report
    /// that probes once and prints the same account under every key, which is the confusion the
    /// command exists to clear.
    #[test]
    fn each_key_reports_the_login_its_own_connection_got() {
        let ssh = Canned(vec![
            (
                "work.pub",
                "Hi bbondy! You've successfully authenticated, but nothing else.",
            ),
            ("bot.pub", GREETING),
        ]);
        let rows = probe(&ssh, &keys(), "github.com", None);
        let shown = text(&facts("github.com", Some("netzenbot"), &rows));

        let work = shown
            .lines()
            .find(|line| line.contains("work.pub"))
            .unwrap();
        let bot = shown.lines().find(|line| line.contains("bot.pub")).unwrap();
        assert!(
            work.contains("fp-of-work.pub") && work.contains("bbondy"),
            "{shown}"
        );
        assert!(
            bot.contains("fp-of-bot.pub") && bot.contains("netzenbot"),
            "{shown}"
        );
        assert!(
            !work.contains(&*t!(
                doctor_push_login_owns,
                login = "bbondy",
                owner = "netzenbot"
            )),
            "{shown}"
        );
        assert!(
            bot.contains(&*t!(
                doctor_push_login_owns,
                login = "netzenbot",
                owner = "netzenbot"
            )),
            "{shown}"
        );
    }

    /// A key GitHub does not recognise is shown as such, not as a login.
    #[test]
    fn a_key_no_account_has_is_shown_as_not_recognised() {
        let ssh = Canned(vec![(
            "work.pub",
            "git@github.com: Permission denied (publickey).",
        )]);
        let rows = probe(&ssh, &keys()[..1], "github.com", None);
        let shown = text(&facts("github.com", None, &rows));

        assert!(
            shown.contains(&*t!(doctor_push_not_recognised, host = "github.com")),
            "{shown}"
        );
        assert!(!shown.contains("Hi "), "{shown}");
    }

    /// Only the key `git.identities` names is marked, and the greeting never decides which that is.
    /// Rejects a report that marks the key whose login equals the owner, which would be a choice
    /// computed from remote bytes.
    #[test]
    fn the_chosen_key_is_the_one_the_settings_named_not_the_one_that_greeted() {
        let ssh = Canned(vec![("bot.pub", GREETING)]);
        let chosen = PathBuf::from("/h/.ssh/work.pub");
        let rows = probe(
            &ssh,
            &keys(),
            "github.com",
            Some((&chosen, ChosenBy::Settings)),
        );

        assert_eq!(
            rows.iter().map(|row| row.chosen).collect::<Vec<_>>(),
            [Some(ChosenBy::Settings), None]
        );
    }

    /// A key chosen at a run prompt is marked as chosen there, not as named by the settings. Rejects
    /// a report that credits `git.identities` with a choice the person made at a prompt, which sends
    /// them to edit a setting they never wrote.
    #[test]
    fn a_key_chosen_at_a_run_prompt_is_marked_as_chosen_there() {
        let ssh = Canned(vec![("bot.pub", GREETING)]);
        let chosen = PathBuf::from("/h/.ssh/work.pub");
        let rows = probe(
            &ssh,
            &keys(),
            "github.com",
            Some((&chosen, ChosenBy::Prompt)),
        );
        let shown = text(&facts("github.com", Some("netzenbot"), &rows));
        assert!(
            shown.contains(&t!(doctor_push_chosen_at_prompt).to_string()),
            "{shown}"
        );
        assert!(
            !shown.contains(&t!(doctor_push_chosen_by).to_string()),
            "{shown}"
        );
    }

    /// What a command was told to unset, which is how `env_remove` reads back: a name a child will
    /// not be handed is present here with no value, and one left alone is absent altogether.
    fn withheld_from(command: &std::process::Command) -> Vec<String> {
        let mut names: Vec<String> = command
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(name, _)| name.to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// Whether the start takes `name` off, when the person's environment holds `name` set to `value`
    /// and only a path under `/safe` is outside the directories a stage may write.
    fn withheld_value(name: &str, value: &str) -> bool {
        let command = started_with(
            Path::new("/usr/bin/git"),
            [(
                std::ffi::OsString::from(name),
                std::ffi::OsString::from(value),
            )],
            &|path: &Path| path.starts_with("/safe"),
        );
        command
            .get_envs()
            .any(|(set, now)| set == std::ffi::OsStr::new(name) && now.is_none())
    }

    /// SANDBOX-30. `git` reads `XDG_CONFIG_HOME` and the helper its configuration names, which may
    /// open a browser with `xdg-open` or Python's `webbrowser`. The regression it rejects: the start
    /// left as `Checked(&[])`, so a value naming a file a stage wrote reaches `git`, `ssh` and
    /// `ssh-keygen` as the person set it.
    #[test]
    fn a_browser_or_xdg_directory_a_stage_may_write_is_not_handed_to_the_programs_started_here() {
        assert!(withheld_value("BROWSER", "/stage/browser.sh %s"));
        assert!(withheld_value(
            "BROWSER",
            "/safe/bin/firefox:/stage/browser.sh"
        ));
        assert!(withheld_value("BROWSER", "firefox --new-window %s"));
        assert!(!withheld_value("BROWSER", "firefox"));
        assert!(!withheld_value("BROWSER", "/safe/bin/firefox %s"));

        assert!(withheld_value("XDG_DATA_HOME", "/stage/share"));
        assert!(!withheld_value("XDG_DATA_HOME", "/safe/share"));
        assert!(withheld_value("XDG_DATA_DIRS", "/safe/share:/stage/share"));
        assert!(!withheld_value("XDG_DATA_DIRS", "/safe/share"));
        assert!(withheld_value("XDG_CONFIG_HOME", "/stage/config"));
        assert!(!withheld_value("XDG_CONFIG_HOME", "/safe/config"));
        assert!(withheld_value("XDG_CONFIG_DIRS", "/safe/etc:/stage/etc"));
        assert!(!withheld_value("XDG_CONFIG_DIRS", "/safe/etc/xdg"));
    }

    /// Variables the three programs are run to read stay as the person set them. Rejects a start
    /// that removes the whole `XDG_` family or `HOME`, which would move `git` off the person's
    /// configuration.
    #[test]
    fn the_variables_the_programs_started_here_are_run_to_read_are_kept() {
        for name in ["HOME", "SSH_AUTH_SOCK", "XDG_RUNTIME_DIR", "DISPLAY"] {
            assert!(
                !withheld_value(name, "/stage/anything"),
                "{name} was removed"
            );
        }
    }

    /// A `BROWSER` entry that is not text is judged as the bytes it is: a lossy rendering would
    /// read `fire\u{FFFD}ox` as a bare program name and keep it.
    #[cfg(unix)]
    #[test]
    fn a_browser_entry_that_is_not_text_is_removed_from_the_programs_started_here() {
        use std::os::unix::ffi::OsStrExt;
        let removed = |value: &std::ffi::OsStr| {
            started_with(
                Path::new("/usr/bin/git"),
                [(std::ffi::OsString::from("BROWSER"), value.to_os_string())],
                &|path: &Path| path.starts_with("/safe"),
            )
            .get_envs()
            .any(|(set, now)| set == std::ffi::OsStr::new("BROWSER") && now.is_none())
        };

        assert!(removed(std::ffi::OsStr::from_bytes(b"fire\xffox")));
        assert!(!removed(std::ffi::OsStr::new("fire\u{FFFD}ox")));
    }

    /// Every program this command starts is built by [`started`], so the one place that removes this
    /// agent's own credentials covers all three. Rejects a command built without `env_remove`,
    /// which hands the signing key to whatever `ssh-keygen` resolves to.
    #[test]
    fn a_program_started_here_is_not_handed_this_agents_own_credentials() {
        let mut expected = bravebot_config::scrub::own_credentials();
        expected.sort();

        // An environment that holds none of the selector names, so what is removed is the
        // credentials alone whatever the machine running the test has set.
        let command = started_with(
            Path::new("/usr/bin/ssh"),
            [],
            &bravebot_sandbox::programs::outside_stage_writable,
        );
        assert_eq!(withheld_from(&command), expected);
    }

    /// A command built for each start, with the program path a lookup would have given.
    #[cfg(unix)]
    fn the_three_starts() -> [std::process::Command; 3] {
        [
            fingerprint_command(
                Path::new("/usr/bin/ssh-keygen"),
                Path::new("/h/.ssh/bot.pub"),
            ),
            greeting_command(
                Path::new("/usr/bin/ssh"),
                Path::new("/h/.ssh/bot.pub"),
                "github.com",
            ),
            origin_command(Path::new("/usr/bin/git")),
        ]
    }

    /// The `ssh-keygen`, `ssh` and `git` starts are each built from [`started`], so the removal of
    /// the selector variables and the `SHELL` it sets cover all three. A start built from
    /// `programs::start` directly carries neither the credentials removal nor `SHELL`. Rejects a
    /// start that leaves `started` out, and arguments that changed with the move.
    #[cfg(unix)]
    #[test]
    fn the_ssh_keygen_ssh_and_git_starts_all_go_through_started() {
        let mut credentials = bravebot_config::scrub::own_credentials();
        credentials.sort();
        let [keygen, ssh, git] = the_three_starts();

        for (command, program, args) in [
            (
                &keygen,
                "/usr/bin/ssh-keygen",
                vec!["-lf", "/h/.ssh/bot.pub"],
            ),
            (
                &ssh,
                "/usr/bin/ssh",
                vec![
                    "-T",
                    "-i",
                    "/h/.ssh/bot.pub",
                    "-o",
                    "IdentitiesOnly=yes",
                    "-o",
                    "ConnectTimeout=15",
                    "git@github.com",
                ],
            ),
            (
                &git,
                "/usr/bin/git",
                vec!["config", "--get", "remote.origin.url"],
            ),
        ] {
            assert_eq!(command.get_program(), program);
            assert_eq!(command.get_args().collect::<Vec<_>>(), args, "{program}");
            let shell = command
                .get_envs()
                .find(|(name, _)| *name == std::ffi::OsStr::new("SHELL"));
            assert_eq!(
                shell.and_then(|(_, value)| value),
                Some(std::ffi::OsStr::new("/bin/sh")),
                "{program}"
            );
            let withheld = withheld_from(command);
            assert!(
                credentials.iter().all(|name| withheld.contains(name)),
                "{program} was handed one of this agent's own credentials"
            );
        }
    }

    /// Whether `name` is taken off the environment the start is given, when the person's
    /// environment holds `value` for it and `writable` is the directory a stage may write.
    #[cfg(unix)]
    fn withheld(name: &str, value: &Path, writable: &Path) -> bool {
        let row = writable.to_path_buf();
        let rows = std::slice::from_ref(&row);
        let command = started_with(
            Path::new("/usr/bin/ssh"),
            [(
                std::ffi::OsString::from(name),
                value.as_os_str().to_os_string(),
            )],
            &|file: &Path| bravebot_sandbox::programs::outside_stage_writable_with(file, rows, &[]),
        );
        command
            .get_envs()
            .any(|(set, now)| set == std::ffi::OsStr::new(name) && now.is_none())
    }

    /// A script a stage can plant under a row it holds, and a genuine one beside it outside every
    /// row. Each records that it ran in a file next to it, which no test here lets happen.
    #[cfg(unix)]
    fn planted_and_genuine(
        name: &str,
    ) -> (
        crate::test_support::Scratch,
        crate::test_support::Scratch,
        PathBuf,
        PathBuf,
    ) {
        use std::os::unix::fs::PermissionsExt;
        let session = crate::test_support::Scratch::new(&format!("push-identity-{name}-session"));
        let safe = crate::test_support::Scratch::new(&format!("push-identity-{name}-safe"));
        let mut made = Vec::new();
        for root in [&session.path, &safe.path] {
            let file = root.join(".venv/bin/helper");
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(
                &file,
                format!("#!/bin/sh\n: > '{}'\n", root.join("ran").display()),
            )
            .unwrap();
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
            made.push(file);
        }
        let genuine = made.pop().unwrap();
        let planted = made.pop().unwrap();
        (session, safe, planted, genuine)
    }

    /// SANDBOX-30. A list in `XDG_DATA_DIRS` or `XDG_CONFIG_DIRS` is judged entry by entry on the
    /// disk. The regression it rejects: a list judged as one path, which names no directory, so a
    /// person's list of installed directories is removed from the programs started here.
    #[cfg(unix)]
    #[test]
    fn a_list_of_xdg_directories_outside_every_directory_a_stage_may_write_is_kept_for_the_programs_started_here()
     {
        let (session, safe, planted, _genuine) = planted_and_genuine("xdg-list");
        let inner = safe.path.join(".venv");
        let genuine_list = std::env::join_paths([&inner, &safe.path]).unwrap();
        let mixed_list = std::env::join_paths([&inner, planted.parent().unwrap()]).unwrap();

        for name in ["XDG_DATA_DIRS", "XDG_CONFIG_DIRS"] {
            assert!(
                !withheld(name, Path::new(&genuine_list), &session.path),
                "{name}: a genuine list"
            );
            assert!(
                withheld(name, Path::new(&mixed_list), &session.path),
                "{name}: a list with a directory a stage may write"
            );
        }
    }

    /// The regression it rejects: starting `ssh` with `SSH_ASKPASS` as the person set it, so a
    /// program a stage wrote is run to ask for a passphrase, outside confinement. The variable
    /// names a program this command has no use for, so it is taken off whatever it holds.
    #[cfg(unix)]
    #[test]
    fn an_askpass_program_is_never_handed_to_the_start() {
        let (session, _safe, planted, genuine) = planted_and_genuine("askpass");

        assert!(withheld("SSH_ASKPASS", &planted, &session.path));
        assert!(withheld("SSH_ASKPASS", &genuine, &session.path));
        assert!(withheld("SSH_ASKPASS", Path::new("askpass"), &session.path));
    }

    /// `SSH_ASKPASS_REQUIRE=force` makes `ssh` run the askpass program even with a terminal, so it
    /// is taken off with it. Rejects a start that removes the program and leaves the switch.
    #[cfg(unix)]
    #[test]
    fn the_switch_that_forces_the_askpass_program_is_never_handed_to_the_start() {
        let (session, _safe, planted, _genuine) = planted_and_genuine("askpass-require");

        assert!(withheld(
            "SSH_ASKPASS_REQUIRE",
            Path::new("force"),
            &session.path
        ));
        assert!(withheld("SSH_ASKPASS_REQUIRE", &planted, &session.path));
    }

    /// The regression it rejects: starting `ssh` or `ssh-keygen` with `SSH_SK_PROVIDER` naming a
    /// library a stage wrote, which is loaded for a hardware key.
    #[cfg(unix)]
    #[test]
    fn a_security_key_provider_is_never_handed_to_the_start() {
        let (session, _safe, planted, genuine) = planted_and_genuine("sk-provider");

        assert!(withheld("SSH_SK_PROVIDER", &planted, &session.path));
        assert!(withheld("SSH_SK_PROVIDER", &genuine, &session.path));
    }

    /// The regression it rejects: starting `ssh` with `SSH_SK_HELPER` naming a program a stage
    /// wrote, which `ssh` runs for a hardware key.
    #[cfg(unix)]
    #[test]
    fn a_security_key_helper_is_never_handed_to_the_start() {
        let (session, _safe, planted, genuine) = planted_and_genuine("sk-helper");

        assert!(withheld("SSH_SK_HELPER", &planted, &session.path));
        assert!(withheld("SSH_SK_HELPER", &genuine, &session.path));
    }

    /// The regression it rejects: leaving `SHELL` as the person's environment has it. `ssh` runs a
    /// `ProxyCommand` and a `LocalCommand` with `$SHELL -c`, so a program a stage wrote there runs
    /// outside confinement. Each start is given `/bin/sh` whatever `SHELL` holds, and also when it
    /// holds nothing.
    #[cfg(unix)]
    #[test]
    fn the_start_is_given_bin_sh_for_shell_whatever_the_person_has() {
        let (session, _safe, planted, genuine) = planted_and_genuine("shell");
        let rows = [session.path.clone()];
        for held in [Some(planted), Some(genuine), None] {
            let command = started_with(
                Path::new("/usr/bin/ssh"),
                held.clone()
                    .map(|shell| (std::ffi::OsString::from("SHELL"), shell.into_os_string())),
                &|file: &Path| {
                    bravebot_sandbox::programs::outside_stage_writable_with(file, &rows, &[])
                },
            );
            let shell: Vec<_> = command
                .get_envs()
                .filter(|(name, _)| *name == std::ffi::OsStr::new("SHELL"))
                .collect();
            assert_eq!(
                shell,
                [(
                    std::ffi::OsStr::new("SHELL"),
                    Some(std::ffi::OsStr::new("/bin/sh"))
                )],
                "{held:?}"
            );
        }
    }

    /// The regression it rejects: starting the macOS `/usr/bin/git` shim with `DEVELOPER_DIR`
    /// naming a directory a stage wrote, whose `usr/bin/xcrun` the shim runs. A relative or missing
    /// directory is read from, or made in, a directory a stage writes.
    #[cfg(unix)]
    #[test]
    fn a_developer_directory_a_stage_may_write_is_not_handed_to_the_start() {
        let (session, safe, planted, _genuine) = planted_and_genuine("developer-dir");
        let xcrun = session.path.join("Xcode/usr/bin/xcrun");
        std::fs::create_dir_all(xcrun.parent().unwrap()).unwrap();
        std::fs::copy(&planted, &xcrun).unwrap();
        let linked = safe.path.join("linked-developer-dir");
        std::os::unix::fs::symlink(session.path.join("Xcode"), &linked).unwrap();

        assert!(withheld(
            "DEVELOPER_DIR",
            &session.path.join("Xcode"),
            &session.path
        ));
        assert!(withheld("DEVELOPER_DIR", &linked, &session.path));
        assert!(withheld("DEVELOPER_DIR", Path::new("Xcode"), &session.path));
        assert!(withheld(
            "DEVELOPER_DIR",
            &safe.path.join("not-made-yet"),
            &session.path
        ));
    }

    /// A `DEVELOPER_DIR` outside every directory a stage may write still reaches the start, so a
    /// person who selects a toolchain with it keeps `git`. Rejects a start that removes it whatever
    /// it holds.
    #[cfg(unix)]
    #[test]
    fn a_developer_directory_outside_every_directory_a_stage_may_write_still_reaches_the_start() {
        let (session, safe, _planted, _genuine) = planted_and_genuine("developer-dir-kept");
        let developer = safe.path.join("Xcode");
        std::fs::create_dir_all(developer.join("usr/bin")).unwrap();

        assert!(!withheld("DEVELOPER_DIR", &developer, &session.path));
    }

    /// The directory the command ran in and the checkout holding it are both left out of the lookup.
    /// Rejects a lookup that leaves out only the directory itself, which finds a repository's
    /// `bin` when the command runs in a subdirectory.
    #[test]
    fn the_checkout_holding_the_directory_is_left_out_with_it() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch/push-identity-checkout");
        let _ = std::fs::remove_dir_all(&root);
        let here = root.join("crates").join("cli");
        std::fs::create_dir_all(&here).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();

        assert_eq!(checkout(&here), [here.clone(), root]);
    }

    /// A directory under no checkout is left out by itself. Rejects a lookup that leaves out the
    /// checkout alone, which finds the `bin` of a project that has no `.git`.
    #[test]
    fn a_directory_under_no_checkout_is_left_out_by_itself() {
        let here = Path::new("/bravebot-no-such-directory/project");

        assert_eq!(checkout(here), [here.to_path_buf()]);
    }

    /// SANDBOX-30: `ssh` reads the person's configuration, and a `ProxyCommand` there names a
    /// program that is looked up on the `PATH` it was started with. A file a confined stage wrote
    /// under that name in a directory the command leaves out, first on `PATH`, must not be the one
    /// that runs. The test binary runs again with `PATH` holding the checkout's `.venv/bin`, an
    /// installed directory and the system directories, in that order. `ssh` in the installed
    /// directory is `#!/usr/bin/env interp`, and each `interp` records that it ran. A program
    /// started with bravebot's whole `PATH` runs the planted one.
    #[cfg(unix)]
    #[test]
    fn a_program_ssh_starts_by_name_is_never_one_a_stage_may_have_written() {
        use crate::test_support::Scratch;
        use std::os::unix::fs::PermissionsExt;
        const TEST: &str = "push_identity::tests::a_program_ssh_starts_by_name_is_never_one_a_stage_may_have_written";
        const SESSION: &str = "BRAVEBOT_TEST_PUSH_IDENTITY_SESSION";
        const INSTALLED: &str = "BRAVEBOT_TEST_PUSH_IDENTITY_INSTALLED";

        if let Some(session) = std::env::var_os(SESSION) {
            let session = PathBuf::from(session);
            std::fs::write(session.join("ran"), "").unwrap();
            bravebot_sandbox::programs::keep_out(&session);

            let answer = run("ssh", started);
            assert!(answer.is_some(), "the installed ssh was not started");
            assert!(
                !session.join("planted-ran").exists(),
                "a program ssh starts by name was looked up in a directory a stage may write"
            );
            assert!(
                session.join("installed-ran").exists(),
                "ssh did not run the installed interpreter"
            );
            return;
        }

        let session = Scratch::new("push-identity-session");
        let installed = Scratch::new("push-identity-installed");
        let planted = session.path.join(".venv/bin");
        std::fs::create_dir_all(&planted).unwrap();
        let write = |directory: &Path, name: &str, body: String| {
            let file = directory.join(name);
            std::fs::write(&file, body).unwrap();
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        let records =
            |marker: &str| format!("#!/bin/sh\n: > '{}'\n", session.path.join(marker).display());
        write(&planted, "interp", records("planted-ran"));
        write(&installed.path, "interp", records("installed-ran"));
        write(
            &installed.path,
            "ssh",
            "#!/usr/bin/env interp\n".to_string(),
        );
        let path = std::env::join_paths([
            planted,
            installed.path.clone(),
            PathBuf::from("/usr/bin"),
            PathBuf::from("/bin"),
        ])
        .unwrap();

        let own = bravebot_sandbox::programs::Resolved::own_executable().unwrap();
        let child = bravebot_sandbox::programs::start(
            &own,
            bravebot_sandbox::programs::Inherits::All,
            bravebot_sandbox::programs::SearchPath::Inherited,
            bravebot_sandbox::programs::CodeSelectors::Kept,
        )
        .args(["--exact", TEST, "--test-threads=1", "--nocapture"])
        .env(SESSION, &session.path)
        .env(INSTALLED, &installed.path)
        .env("PATH", &path)
        .output()
        .unwrap();

        let ran = session.path.join("ran").exists();
        let report = String::from_utf8_lossy(&child.stderr).into_owned();
        assert!(ran, "the child did not run the test: {report}");
        assert!(child.status.success(), "{report}");
    }

    /// The argument is `<host>/<owner>` and nothing else.
    #[test]
    fn the_argument_is_a_host_and_an_owner() {
        assert_eq!(named(None), Ok(None));
        assert_eq!(
            named(Some("github.com/netzenbot")).unwrap().unwrap().owner,
            "netzenbot"
        );
        for bad in ["netzenbot", "github.com/", "/owner", "github.com/a b"] {
            assert_eq!(named(Some(bad)), Err(()), "{bad}");
        }
    }
}
