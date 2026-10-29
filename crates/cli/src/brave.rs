//! `bravebot auth login brave`, `logout brave` and the standing `status` reports: the key for
//! Brave's premium service.
//!
//! The key is typed here and kept by bravebot in a file of its own in the state directory, created
//! private to the account before anything is written to it. It is not read from a variable or baked
//! into a build, and where it goes is not stored with it: the premium host is compiled in
//! ([`bravebot_config::DEV_PREMIUM_ENDPOINT`]), so no file or variable decides where a key a person
//! typed is sent.

use crate::exit::{Ending, fail};
use crate::plain::Prompting;
use bravebot_agent::confirm::Decision;
use bravebot_agent::home;
use bravebot_config::{Secret, brave_key};
use bravebot_i18n::t;
use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;
use std::process::ExitCode;

/// What the places the sign-in reads and writes are, so a test runs the whole of it.
struct Task<'a> {
    incognito: bool,
    /// Whether both ends are a terminal, as `import-providers` and the gateway sign-in ask: a key
    /// asked for on a redirected stream is one nobody typed.
    terminal: bool,
    /// The directory the key is kept in, where there is one.
    directory: Option<&'a Path>,
}

/// `bravebot auth login brave`.
pub(crate) fn login() -> ExitCode {
    let directory = home::directory();
    let task = Task {
        incognito: bravebot_core::incognito::engaged(),
        terminal: std::io::stdin().is_terminal() && std::io::stderr().is_terminal(),
        directory: directory.as_deref(),
    };
    sign_in(
        &task,
        || Prompting::new(std::io::stdin().lock(), std::io::stderr()),
        bravebot_tui::hidden::read,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    )
    .code()
}

/// The sign-in over the places it reads and writes.
///
/// `open_questions` is called for the one question and what it returns is let go before the key is
/// read, so the lock on stdin is released before the terminal is read for the key.
fn sign_in<R: BufRead, W: Write>(
    task: &Task<'_>,
    open_questions: impl FnOnce() -> Prompting<R, W>,
    read_key: impl FnOnce(&str) -> std::io::Result<Option<Secret>>,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Ending {
    // INCOG-7: a session that leaves nothing behind stores no key either.
    if task.incognito {
        return refused(err, Ending::Failed, t!(brave_not_while_incognito));
    }
    if !task.terminal {
        return refused(err, Ending::Argument, t!(brave_needs_a_terminal));
    }
    let Some(directory) = task.directory else {
        return refused(err, Ending::Failed, t!(brave_no_home));
    };
    let path = brave_key::file(directory);
    match brave_key::read(directory) {
        Err(brave_key::Unreadable) => {
            return refused(
                err,
                Ending::Failed,
                t!(brave_key_unreadable, path = path.display()),
            );
        }
        // Replaced only on a yes, because the one replaced is gone once the file is written.
        Ok(Some(_)) => {
            let replace = {
                let mut asking = open_questions();
                asking.say(t!(brave_key_held));
                asking.ask(&[], t!(brave_replace)) == Decision::Approve
            };
            if !replace {
                return Ending::Done;
            }
        }
        Ok(None) => {}
    }

    let key = match read_key(t!(brave_key_question)) {
        Ok(Some(key)) => key,
        Ok(None) => {
            tell(err, t!(brave_nothing_stored));
            return Ending::Done;
        }
        Err(error) => {
            return refused(
                err,
                Ending::Failed,
                t!(brave_not_read, error = error.to_string()),
            );
        }
    };
    if let Err(error) = store(directory, &key) {
        return refused(
            err,
            Ending::Failed,
            t!(
                brave_not_stored,
                path = path.display(),
                error = error.to_string()
            ),
        );
    }
    tell(out, t!(brave_stored, path = path.display()));
    Ending::Done
}

/// `bravebot auth logout brave`: forget the stored key, which ends nothing at Brave.
///
/// Allowed in an incognito session, as forgetting any other sign-in is: it leaves less behind.
pub(crate) fn logout() -> ExitCode {
    let Some(directory) = home::directory() else {
        println!("{}", t!(brave_nothing_to_forget));
        return ExitCode::SUCCESS;
    };
    match forget(&directory) {
        Ok(true) => {
            println!("{}", t!(brave_forgotten));
            ExitCode::SUCCESS
        }
        Ok(false) => {
            println!("{}", t!(brave_nothing_to_forget));
            ExitCode::SUCCESS
        }
        Err(error) => fail(
            Ending::Failed,
            t!(
                brave_not_forgotten,
                path = brave_key::file(&directory).display(),
                error = error.to_string()
            ),
        ),
    }
}

/// How the stored key stands, as `bravebot auth status` reports it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Standing {
    /// A key is stored.
    Held,
    /// Nothing is stored.
    NotHeld,
    /// This machine has nowhere to keep one.
    Nowhere,
    /// What is stored cannot be read, with the sentence that names the remedy.
    Unusable(String),
}

/// Read the stored key and print nothing and probe nothing.
pub(crate) fn standing() -> Standing {
    match home::directory() {
        None => Standing::Nowhere,
        Some(directory) => standing_in(&directory),
    }
}

fn standing_in(directory: &Path) -> Standing {
    match brave_key::read(directory) {
        Ok(Some(_)) => Standing::Held,
        Ok(None) => Standing::NotHeld,
        Err(brave_key::Unreadable) => Standing::Unusable(
            t!(
                brave_key_unreadable,
                path = brave_key::file(directory).display()
            )
            .to_string(),
        ),
    }
}

/// Write the key over the file in `directory`.
///
/// Written beside the file and renamed over it, as the sign-ins are, so a write that stops part way
/// leaves the key that was there. Created by the call that gives the sign-ins their protection, 0600
/// on Unix and granted to this account alone on Windows (STATE-1), because the key is a bearer
/// token.
fn store(directory: &Path, key: &Secret) -> std::io::Result<()> {
    let path = brave_key::file(directory);
    home::create_directory(directory)?;
    let temporary = path.with_file_name(format!("{}.{}.tmp", brave_key::FILE, std::process::id()));
    // The file is closed at the end of the closure, before the rename, which Windows refuses while
    // a handle is still open on either name.
    let written = bravebot_skus::store::create_private(&temporary)
        .and_then(|mut file| {
            file.write_all(brave_key::to_text(key).expose().as_bytes())?;
            // On disk before the rename, or a crash just after it can leave the name on an empty
            // file where a filesystem orders the rename first.
            file.sync_all()
        })
        .and_then(|()| std::fs::rename(&temporary, &path));
    if written.is_err() {
        std::fs::remove_file(&temporary).unwrap_or_default();
    }
    written
}

/// Remove the file, and say whether there was one.
fn forget(directory: &Path) -> std::io::Result<bool> {
    match std::fs::remove_file(brave_key::file(directory)) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// One line to a stream. A failed write is ignored, since stdout and stderr are the only places one
/// could be reported.
fn tell(to: &mut impl Write, line: impl std::fmt::Display) {
    writeln!(to, "{line}").unwrap_or_default();
}

fn refused(err: &mut impl Write, ending: Ending, message: impl std::fmt::Display) -> Ending {
    ending.say(err, message);
    ending
}

#[cfg(test)]
mod tests {
    use super::*;

    const THE_KEY: &str = "placeholder-brave-key";

    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("bravebot-cli-brave-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a scratch directory");
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn task(directory: Option<&Path>) -> Task<'_> {
        Task {
            incognito: false,
            terminal: true,
            directory,
        }
    }

    /// The sign-in with `answers` typed at the question and `typed` as the key, and what it said.
    fn signed_in(task: &Task<'_>, answers: &str, typed: Option<&str>) -> (Ending, String, String) {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let ending = sign_in(
            task,
            || Prompting::new(answers.as_bytes(), Vec::new()),
            |_| Ok(typed.map(Secret::new)),
            &mut out,
            &mut err,
        );
        (
            ending,
            String::from_utf8(out).expect("text"),
            String::from_utf8(err).expect("text"),
        )
    }

    fn the_stored_key(directory: &Path) -> Option<String> {
        brave_key::read(directory)
            .expect("a readable file")
            .map(|key| key.expose().to_string())
    }

    /// CLI-18: a typed key is stored where the premium key is read from, private to the account, and
    /// the line said names the file and never the key.
    #[test]
    fn a_typed_key_is_stored_private_and_never_said() {
        let scratch = Scratch::new("stored");
        let (ending, out, err) = signed_in(&task(Some(&scratch.0)), "", Some(THE_KEY));

        assert_eq!(ending, Ending::Done, "{err}");
        assert_eq!(the_stored_key(&scratch.0).as_deref(), Some(THE_KEY));
        assert!(
            out.contains(&brave_key::file(&scratch.0).display().to_string()),
            "{out}"
        );
        assert!(
            !out.contains(THE_KEY) && !err.contains(THE_KEY),
            "{out}{err}"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(brave_key::file(&scratch.0))
                .expect("the file")
                .permissions()
                .mode();
            assert_eq!(mode & 0o077, 0, "group or other can reach the key");
        }
        let left: Vec<_> = std::fs::read_dir(&scratch.0)
            .expect("a listing")
            .map(|entry| entry.expect("an entry").file_name())
            .collect();
        assert_eq!(left, [std::ffi::OsString::from(brave_key::FILE)]);
    }

    /// CLI-18: nothing typed stores nothing and is not a failure.
    #[test]
    fn no_key_typed_stores_nothing() {
        let scratch = Scratch::new("nothing");
        let (ending, _, _) = signed_in(&task(Some(&scratch.0)), "", None);
        assert_eq!(ending, Ending::Done);
        assert_eq!(the_stored_key(&scratch.0), None);
    }

    /// CLI-18: a yes to replacing a key, followed by nothing typed, leaves the key that was stored.
    #[test]
    fn a_yes_and_no_key_typed_leaves_the_stored_key() {
        let scratch = Scratch::new("yes-and-nothing");
        store(&scratch.0, &Secret::new(THE_KEY)).expect("a first key");

        let (ending, _, err) = signed_in(&task(Some(&scratch.0)), "y\n", None);

        assert_eq!(ending, Ending::Done, "{err}");
        assert_eq!(
            the_stored_key(&scratch.0).as_deref(),
            Some(THE_KEY),
            "nothing typed replaced the key"
        );
    }

    /// CLI-18: a stored key is replaced only on a yes, since the one replaced is gone once the file is
    /// written. The key is asked for only after the yes.
    #[test]
    fn a_stored_key_is_replaced_only_on_a_yes() {
        let scratch = Scratch::new("replace");
        store(&scratch.0, &Secret::new(THE_KEY)).expect("a first key");

        for answer in ["n\n", "\n", ""] {
            let (ending, _, err) = signed_in(&task(Some(&scratch.0)), answer, Some("another-key"));
            assert_eq!(ending, Ending::Done, "{answer:?}: {err}");
            assert_eq!(
                the_stored_key(&scratch.0).as_deref(),
                Some(THE_KEY),
                "{answer:?} replaced the key"
            );
        }

        let (ending, _, err) = signed_in(&task(Some(&scratch.0)), "y\n", Some("another-key"));
        assert_eq!(ending, Ending::Done, "{err}");
        assert_eq!(the_stored_key(&scratch.0).as_deref(), Some("another-key"));
    }

    /// CLI-18: a file that cannot be read as a key is neither read as none nor written over, and the
    /// refusal names it.
    #[test]
    fn an_unreadable_file_is_refused_and_left() {
        let scratch = Scratch::new("unreadable");
        std::fs::write(brave_key::file(&scratch.0), "not a key").expect("a file");

        let (ending, out, err) = signed_in(&task(Some(&scratch.0)), "y\n", Some(THE_KEY));

        assert_eq!(ending, Ending::Failed);
        assert!(out.is_empty(), "{out}");
        assert!(err.contains(brave_key::FILE), "{err}");
        assert_eq!(
            std::fs::read_to_string(brave_key::file(&scratch.0)).expect("the file"),
            "not a key"
        );
        assert!(matches!(standing_in(&scratch.0), Standing::Unusable(_)));
    }

    /// INCOG-7 and CLI-18: an incognito session stores no key, a pipe has nobody to type one, and a
    /// machine with nowhere to keep one says so. None of them reads a key first.
    #[test]
    fn the_sign_in_is_refused_where_a_key_cannot_be_asked_for_or_kept() {
        let scratch = Scratch::new("refused");
        let asked_for_a_key =
            |_: &str| -> std::io::Result<Option<Secret>> { panic!("the key was asked for") };
        let cases = [
            (
                Task {
                    incognito: true,
                    ..task(Some(&scratch.0))
                },
                Ending::Failed,
            ),
            (
                Task {
                    terminal: false,
                    ..task(Some(&scratch.0))
                },
                Ending::Argument,
            ),
            (task(None), Ending::Failed),
        ];
        for (case, expected) in cases {
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let ending = sign_in(
                &case,
                || Prompting::new(&b""[..], Vec::new()),
                asked_for_a_key,
                &mut out,
                &mut err,
            );
            assert_eq!(ending, expected);
            assert!(out.is_empty());
            assert!(!err.is_empty());
        }
        assert_eq!(the_stored_key(&scratch.0), None);
    }

    /// CLI-18: forgetting removes the one file, says whether there was one, and can be repeated.
    #[test]
    fn forgetting_removes_the_file_and_is_repeatable() {
        let scratch = Scratch::new("forget");
        store(&scratch.0, &Secret::new(THE_KEY)).expect("a key");

        assert!(forget(&scratch.0).expect("removed"));
        assert!(!brave_key::file(&scratch.0).exists());
        assert!(!forget(&scratch.0).expect("nothing to remove"));
    }

    /// CLI-23: the standing is read from the file alone, said without the key.
    #[test]
    fn the_standing_follows_the_file() {
        let scratch = Scratch::new("standing");
        assert_eq!(standing_in(&scratch.0), Standing::NotHeld);
        store(&scratch.0, &Secret::new(THE_KEY)).expect("a key");
        assert_eq!(standing_in(&scratch.0), Standing::Held);
    }
}
