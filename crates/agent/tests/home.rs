//! Where `~/.bravebot` is.
//!
//! `HOME` and `BRAVEBOT_HOME` are process-wide, so these tests are the only ones in the crate that
//! touch them and they are kept in a file of their own. Everything else the agent reads from the
//! home directory takes the path as an argument, precisely so that no other test depends on the
//! environment.

use std::path::PathBuf;
use std::sync::Mutex;

/// One lock for the whole file, not one per test.
///
/// `HOME` is process-wide, so every test here contends for the same thing. A mutex declared
/// inside each function would be a different mutex, and two tests would then be free to run at
/// once and see each other's HOME.
static HOME_LOCK: Mutex<()> = Mutex::new(());

/// Point HOME at a scratch directory for the duration of the closure, with no `BRAVEBOT_HOME`.
///
/// The variable that names the state directory is cleared as well, since it answers before `HOME`
/// and one in the environment of whoever runs the tests would decide what every test here reads.
fn with_temp_home<T>(name: &str, body: impl FnOnce(&PathBuf) -> T) -> T {
    let _guard = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let dir =
        std::env::temp_dir().join(format!("bravebot-agent-home-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch home");

    let previous = std::env::var_os("HOME");
    let named = std::env::var_os(bravebot_agent::home::STATE_VARIABLE);
    // SAFETY: single-threaded within the lock, and restored before returning.
    unsafe {
        std::env::set_var("HOME", &dir);
        std::env::remove_var(bravebot_agent::home::STATE_VARIABLE);
    }

    let result = body(&dir);

    match previous {
        Some(value) => unsafe { std::env::set_var("HOME", value) },
        None => unsafe { std::env::remove_var("HOME") },
    }
    match named {
        Some(value) => unsafe { std::env::set_var(bravebot_agent::home::STATE_VARIABLE, value) },
        None => unsafe { std::env::remove_var(bravebot_agent::home::STATE_VARIABLE) },
    }
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// The directory is the user's, so it is found where the user's environment says it is, and
/// nowhere else. Guessing would mean reading files from a directory nobody chose.
#[test]
fn the_home_directory_is_the_one_the_environment_names() {
    with_temp_home("named", |home| {
        assert_eq!(
            bravebot_agent::home::directory(),
            Some(home.join(".bravebot"))
        );
    });
}

/// STATE-2: a directory a person names in `BRAVEBOT_HOME` is the state directory in every crate that
/// resolves one, and the profile directory a `~` stands for stays where `HOME` puts it.
///
/// The crates below this one cannot share its resolver, so what holds them together is each
/// reading the same variable and answering with the same directory. The sign-ins' files are the ones
/// of those this test can reach from here, and a `~` that followed the variable would send every
/// home-relative path somebody writes into the state directory.
#[test]
fn a_directory_named_in_the_environment_is_the_state_directory_in_every_crate() {
    with_temp_home("named-state", |home| {
        let named = home.join("elsewhere").join("state");
        // SAFETY: `with_temp_home` holds the lock, and restores this along with the rest.
        unsafe { std::env::set_var(bravebot_agent::home::STATE_VARIABLE, &named) };

        assert_eq!(bravebot_agent::home::directory(), Some(named.clone()));
        assert_eq!(
            bravebot_agent::home::resolved(),
            Some(("BRAVEBOT_HOME", named.clone()))
        );
        assert_eq!(
            bravebot_skus::chatgpt::path().expect("a named directory"),
            named.join("chatgpt-account.json"),
            "the ChatGPT sign-in is kept somewhere the state directory is not"
        );
        assert_eq!(
            bravebot_skus::xai::path().expect("a named directory"),
            named.join("xai-account.json"),
            "the xAI sign-in is kept somewhere the state directory is not"
        );
        assert_eq!(
            bravebot_agent::home::profile(),
            Some(home.clone()),
            "the profile directory a `~` stands for followed the state directory"
        );
    });
}

/// STATE-2: a value in `BRAVEBOT_HOME` that names nothing leaves a machine with no profile
/// directory with no state directory, whatever the value is. A relative one resolved against the
/// working directory, or an empty one joined onto the root, would be the guessed location the clause
/// rules out.
#[test]
fn a_value_that_names_no_directory_leaves_no_state_directory_to_guess() {
    for value in ["", "state", "./state"] {
        let found = with_no_home(|| {
            // SAFETY: `with_no_home` holds the lock, and restores this along with the rest.
            unsafe { std::env::set_var(bravebot_agent::home::STATE_VARIABLE, value) };
            (
                bravebot_agent::home::directory(),
                bravebot_skus::chatgpt::path().ok(),
                bravebot_skus::xai::path().ok(),
            )
        });

        assert_eq!(found, (None, None, None), "{value:?} produced a location");
    }
}

/// CMDLINE-4: a leading `~` stands for the profile directory itself, and the state directory is
/// one segment below it.
///
/// The two are one `join` apart, which is what makes answering with the wrong one silent: a
/// command line naming `~/notes.txt` would read `~/.bravebot/notes.txt`, a directory holding this
/// program's own settings and credentials rather than the person's files.
#[test]
fn the_profile_directory_is_the_home_itself_rather_than_the_state_directory() {
    with_temp_home("profile", |home| {
        assert_eq!(bravebot_agent::home::profile(), Some(home.clone()));
        assert_eq!(
            bravebot_agent::home::directory(),
            Some(home.join(".bravebot")),
            "the state directory moved out from under the profile directory"
        );
    });
}

/// PERM-3: a `~/` rule in a settings file is anchored at the home directory and a `/` rule at the
/// state directory inside it, when the accessor every caller reads is the one that answers.
///
/// The rule language is right about both and the anchors it is given decide where they land, so
/// the mistake this rejects is a caller handing over `~/.bravebot` where the home directory is
/// wanted: `Read(~/.ssh/**)` then covers `~/.bravebot/.ssh`, a directory nobody has, and the deny
/// rule protects nothing while reading as though it does.
#[test]
fn a_settings_rule_is_anchored_at_the_home_directory_the_accessor_names() {
    use bravebot_core::permissions::{Decision, Ruling, Subject};

    with_temp_home("permission-anchors", |home| {
        let settings = bravebot_config::Settings::parse(
            r#"{"permissions": {"deny": ["Read(~/.ssh/**)", "Read(/secrets/**)"]}}"#,
        );
        let (permissions, rejected) = bravebot_agent::permissions::from_settings(
            &settings,
            bravebot_agent::home::profile().as_deref(),
            home,
        );

        assert!(rejected.is_empty(), "{rejected:?}");
        assert_eq!(
            permissions.for_path(
                Subject::Read,
                &home.join(".ssh/id_ed25519").display().to_string()
            ),
            Decision::Ruled(Ruling::Deny),
            "a `~/` rule did not cover the home directory"
        );
        assert_eq!(
            permissions.for_path(
                Subject::Read,
                &home.join(".bravebot/secrets/key").display().to_string()
            ),
            Decision::Ruled(Ruling::Deny),
            "a `/` rule did not cover the directory the settings file is in"
        );
    });
}

/// Clear every variable the state directory is stated in, run the closure, and put them back.
///
/// Every one of them, not `HOME` alone: the platform decides which it states a profile directory in,
/// and a person may name the directory itself, so a test that left another set would be asking what
/// happens when one variable is missing rather than when nothing names a directory at all.
fn with_no_home<T>(body: impl FnOnce() -> T) -> T {
    let _guard = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let previous: Vec<_> = bravebot_agent::home::STATE_VARIABLES
        .iter()
        .map(|variable| (variable, std::env::var_os(variable)))
        .collect();
    // SAFETY: single-threaded within the lock, and restored before returning.
    for (variable, _) in &previous {
        unsafe { std::env::remove_var(variable) };
    }

    let result = body();

    for (variable, value) in previous {
        match value {
            Some(value) => unsafe { std::env::set_var(variable, value) },
            None => unsafe { std::env::remove_var(variable) },
        }
    }
    result
}

/// Daemons and containers run without a home. That is a case to do without, never a reason to
/// refuse to start, since everything kept there is optional.
#[test]
fn an_absent_home_is_not_an_error() {
    let found = with_no_home(bravebot_agent::home::directory);

    assert_eq!(found, None, "a missing home invented a directory");
}

/// A machine that names no home has no directory for a `~` to stand for either, and inventing one
/// would put a path in front of somebody that is not where the bytes would go.
#[test]
fn an_absent_home_names_no_profile_directory_for_a_tilde() {
    let found = with_no_home(bravebot_agent::home::profile);

    assert_eq!(found, None, "a missing home invented a profile directory");
}

/// An empty HOME is a misconfigured environment, not the filesystem root. Joining onto it would
/// put the user's own files in `/.bravebot`.
#[test]
fn an_empty_home_is_treated_as_no_home_at_all() {
    let found = with_no_home(|| {
        // SAFETY: `with_no_home` holds the lock, and restores this along with the rest.
        // state-home-kept: `with_no_home` has already cleared it
        unsafe { std::env::set_var("HOME", "") };
        bravebot_agent::home::directory()
    });

    assert_eq!(found, None, "an empty home was joined onto anyway");
}
