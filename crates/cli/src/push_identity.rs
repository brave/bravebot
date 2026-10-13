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
    /// Whether `git.identities` names this key for the owner asked about.
    pub chosen: bool,
}

/// Ask about each key. `chosen` is the key the person's settings name for the owner, judged as a
/// stage judges it.
pub fn probe(ssh: &dyn Ssh, keys: &[PathBuf], host: &str, chosen: Option<&Path>) -> Vec<Row> {
    keys.iter()
        .map(|key| Row {
            key: key.clone(),
            fingerprint: ssh.fingerprint(key),
            greeting: ssh.greeting(key, host),
            chosen: chosen == Some(key.as_path()),
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
            t!(doctor_push_no_remote, host = host).to_string(),
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
        if row.chosen {
            lines.push((
                t!(doctor_push_chosen).to_string(),
                t!(doctor_push_chosen_by).to_string(),
            ));
        }
    }
    lines
}

/// The real machine: `ssh-keygen` and `ssh`, each bounded.
struct Machine;

impl Ssh for Machine {
    fn fingerprint(&self, key: &Path) -> Option<String> {
        let answer = run(
            "ssh-keygen",
            &[std::ffi::OsStr::new("-lf"), key.as_os_str()],
        )?;
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
        use std::ffi::OsStr;
        let target = format!("git@{host}");
        let args = [
            OsStr::new("-T"),
            OsStr::new("-i"),
            key.as_os_str(),
            OsStr::new("-o"),
            OsStr::new("IdentitiesOnly=yes"),
            OsStr::new("-o"),
            OsStr::new("ConnectTimeout=15"),
            OsStr::new(&target),
        ];
        run("ssh", &args).map_or(Greeting::Unanswered, |answer| classify(&answer))
    }
}

/// Standard output and standard error of `program`, run to completion within [`WAIT`].
fn run(program: &str, args: &[&std::ffi::OsStr]) -> Option<String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut child = Command::new(program)
        .args(args)
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
    let answer = run(
        "git",
        &[
            std::ffi::OsStr::new("config"),
            std::ffi::OsStr::new("--get"),
            std::ffi::OsStr::new("remote.origin.url"),
        ],
    )?;
    bravebot_sandbox::scope::ssh_address(&["push".to_string(), answer.lines().next()?.to_string()])
}

/// The address the command names: `<host>/<owner>` as an argument, or `origin`'s.
fn named(argument: Option<&str>) -> Result<Option<Address>, ()> {
    match argument {
        None => Ok(None),
        Some(text) => text
            .split_once('/')
            .and_then(|(host, owner)| Address::spelled(host, owner))
            .map(Some)
            .ok_or(()),
    }
}

/// Probe `keys` and print the report.
pub fn report(
    ssh: &dyn Ssh,
    keys: &[PathBuf],
    remote: Option<&Address>,
    chosen: Option<&Path>,
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
        .and_then(|address| settings.git_identities().for_address(address))
        .and_then(|entry| bravebot_sandbox::scope::identity_key(&entry.key, &home));
    report(&Machine, &keys, remote.as_ref(), chosen.as_deref()).code()
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
        let rows = probe(&ssh, &keys(), "github.com", Some(&chosen));

        assert_eq!(
            rows.iter().map(|row| row.chosen).collect::<Vec<_>>(),
            [true, false]
        );
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
