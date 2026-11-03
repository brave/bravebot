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
use bravebot_agent::accounts;
use bravebot_agent::confirm::Decision;
use bravebot_agent::home;
use bravebot_agent::key_entry::{self, Refusal};
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
    let (directory, stored) = match key_entry::brave_place(task.directory) {
        Ok(place) => place,
        Err(Refusal::Nowhere) => return refused(err, Ending::Failed, t!(brave_no_home)),
        Err(Refusal::BraveUnreadable) => {
            let path = task.directory.map(brave_key::file).unwrap_or_default();
            return refused(
                err,
                Ending::Failed,
                t!(brave_key_unreadable, path = path.display()),
            );
        }
        Err(other) => unreachable!("not a refusal of the place a Brave key is stored: {other:?}"),
    };
    let path = brave_key::file(directory);
    // Replaced only on a yes, because the one replaced is gone once the file is written.
    if stored {
        let replace = {
            let mut asking = open_questions();
            asking.say(t!(brave_key_held));
            asking.ask(&[], t!(brave_replace)) == Decision::Approve
        };
        if !replace {
            return Ending::Done;
        }
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
    if let Err(error) = key_entry::store_brave_key(directory, &key) {
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
    match accounts::forget(&accounts::Way::Brave, home::directory().as_deref()) {
        Ok(forgot) if forgot.was_there => {
            println!("{}", t!(brave_forgotten));
            ExitCode::SUCCESS
        }
        Ok(_) => {
            println!("{}", t!(brave_nothing_to_forget));
            ExitCode::SUCCESS
        }
        Err(accounts::ForgetError::NotRemoved { path, error }) => fail(
            Ending::Failed,
            t!(
                brave_not_forgotten,
                path = path.display(),
                error = error.to_string()
            ),
        ),
        Err(other) => unreachable!("the Brave key is only ever not removed: {other:?}"),
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
        key_entry::store_brave_key(&scratch.0, &Secret::new(THE_KEY)).expect("a first key");

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
        key_entry::store_brave_key(&scratch.0, &Secret::new(THE_KEY)).expect("a first key");

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
        assert!(matches!(
            accounts::brave(Some(&scratch.0)).standing,
            accounts::Standing::Unusable(_)
        ));
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
}
