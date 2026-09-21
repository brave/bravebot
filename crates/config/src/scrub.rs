//! Which of this process's own variables are withheld from a program it starts.
//!
//! A person approving a run is shown the binary, the argument vector and the directory. The
//! environment is not among those, so anything travelling in it is granted without having been
//! seen, and this is what closes the gap between what was read and what was handed over.
//!
//! # The names here, the removal at the caller
//!
//! This module decides which variables a subprocess must not be handed, and does nothing with a
//! process: starting one is not this crate's business, the same split [`crate::hooks`] makes for a
//! hook. The callers are not one place. The `run` tool and a hook start a program somebody named, a
//! language server host starts a server, and the Bedrock backend starts the AWS CLI to resolve a
//! credential. Every one of those is a program this agent started, so every one of them asks here
//! rather than writing down its own idea of what this agent's credentials are: a second list is a
//! list that is one name out of date the first time this one changes.
//!
//! # Two sets, because a person's list answers a narrower question
//!
//! [`own_credentials`] is this agent's own, and [`withheld`] is that plus whatever a settings file
//! named. A program somebody asked for gets the second; a program this agent starts for itself
//! gets the first. A name in `run.scrubEnv` is an answer about what *their* commands may see, and
//! reading it anywhere else turns a list that can only take something away into one that can stop
//! this agent resolving its own credential, from a project layer that arrived with a clone.
//!
//! # Narrow on purpose
//!
//! This agent's own credentials, and whatever a person named in their settings file. Not a survey
//! of every credential the machine might hold: `AWS_PROFILE`, `GITHUB_TOKEN` and `NPM_TOKEN` are
//! left where they are, because `run aws s3 ls` and `run gh pr list` are ordinary requests and a
//! filter matching names cannot distinguish one from an exfiltration. Guessing would trade a claim
//! that holds exactly for one that mostly holds, and mostly holding is what was wrong before.
//!
//! Clearing the environment and allowing a chosen set back in was the other candidate. It fails on
//! a promise `run` already makes: `git push` needs `~/.ssh`, which needs `HOME` and `SSH_AUTH_SOCK`,
//! and the set of programs somebody might ask for cannot be enumerated in advance, so neither can
//! the variables they read.

use crate::Settings;
use std::sync::OnceLock;

/// The settings file, read once per process.
///
/// A file on disk rather than something threaded through every caller: what it holds is a property
/// of the machine, not of a turn, and a path from here to the turn loop would put a configuration
/// argument on a dozen signatures that have no other use for one. Read once because a person who
/// edits it mid-session is describing the next session, and a run whose filtering changed halfway
/// through a turn would be the harder thing to explain.
fn settings() -> &'static Settings {
    static SETTINGS: OnceLock<Settings> = OnceLock::new();
    SETTINGS.get_or_init(Settings::load)
}

/// The credentials this agent holds for itself, which no program it starts is handed.
///
/// Empty where the filtering is switched off. This much is what a program started for this agent's
/// own purposes withholds, the AWS CLI that resolves a Bedrock credential being the one of those:
/// nobody asked for that program, so there is no answer of theirs to read, and what the machine
/// holds for AWS is what it is being run to resolve.
pub fn own_credentials() -> Vec<&'static str> {
    if !enabled() {
        return Vec::new();
    }
    crate::env_var::SCRUBBED.to_vec()
}

/// Every variable withheld from a program somebody asked for, as this machine is configured.
pub fn withheld() -> Vec<String> {
    names(settings())
}

/// [`withheld`], against named settings rather than the file, so a test needs no ambient one.
///
/// The built-in credentials, then whatever the settings file added.
pub fn names(settings: &Settings) -> Vec<String> {
    if !enabled() {
        return Vec::new();
    }
    let mut names: Vec<String> = own_credentials().into_iter().map(str::to_string).collect();
    for named in settings.scrubbed() {
        if !names.iter().any(|already| already == named) {
            names.push(named.to_string());
        }
    }
    names
}

/// Whether the filtering is in force.
///
/// On unless [`crate::env_var::SUBPROCESS_ENV_SCRUB`] is exactly `0`. The escape hatch is
/// deliberately hard to hit by accident: somebody who sets a variable to `false`, `no` or `off`
/// meant to turn something off, but a credential reaching every subprocess is not a thing to
/// switch off by near-miss, so only the one documented spelling does it.
fn enabled() -> bool {
    match std::env::var(crate::env_var::SUBPROCESS_ENV_SCRUB) {
        Ok(value) => value.trim() != "0",
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The credentials this agent holds are withheld with no configuration at all. The default is
    /// the whole of the fix: an opt-in would have left every existing user where they were.
    #[test]
    fn this_agents_credentials_are_withheld_without_being_configured() {
        let names = names(&Settings::default());
        assert!(names.iter().any(|name| name == "SERVICES_KEY_AICHAT"));
        assert!(names.iter().any(|name| name == "BRAVE_SERVICES_KEY_ID"));
    }

    /// The user's own environment is not guessed at. A name-matching filter cannot tell `run aws
    /// s3 ls` from an exfiltration, so it is not attempted.
    #[test]
    fn nothing_of_the_users_own_is_withheld_by_guesswork() {
        let names = names(&Settings::default());
        for kept in [
            "AWS_PROFILE",
            "AWS_ACCESS_KEY_ID",
            "GITHUB_TOKEN",
            "NPM_TOKEN",
            "PATH",
            "HOME",
            "SSH_AUTH_SOCK",
        ] {
            assert!(
                !names.iter().any(|name| name == kept),
                "{kept} was withheld, which is broader than what holds"
            );
        }
    }

    /// The escape hatch for somebody's own token: naming it withholds it, on top of the built-in
    /// set rather than in place of it.
    #[test]
    fn a_name_from_the_settings_file_is_withheld_as_well() {
        let settings = Settings::parse(r#"{"run": {"scrubEnv": ["MY_TOKEN"]}}"#);
        let names = names(&settings);
        assert!(names.iter().any(|name| name == "MY_TOKEN"));
        assert!(
            names.iter().any(|name| name == "SERVICES_KEY_AICHAT"),
            "naming one replaced the built-in set instead of adding to it"
        );
    }

    /// A program this agent starts for its own purposes is not held to somebody's own list. A name
    /// there answers what the commands *they* ask for may see, and `AWS_PROFILE` in it would
    /// otherwise stop the AWS CLI resolving the credential it is run to resolve.
    #[test]
    fn a_name_from_the_settings_file_is_not_one_of_this_agents_own() {
        let settings = Settings::parse(r#"{"run": {"scrubEnv": ["AWS_PROFILE"]}}"#);
        assert!(
            names(&settings).iter().any(|name| name == "AWS_PROFILE"),
            "a name somebody listed stopped reaching a program they asked for"
        );
        assert!(
            !own_credentials().contains(&"AWS_PROFILE"),
            "a settings file decided what this agent withholds from its own credential resolution"
        );
    }

    /// A file naming what is already built in changes nothing, and must not make the same name
    /// appear twice.
    #[test]
    fn a_name_already_built_in_is_not_repeated() {
        let settings =
            Settings::parse(r#"{"env": {}, "run": {"scrubEnv": ["SERVICES_KEY_AICHAT"]}}"#);
        let names = names(&settings);
        assert_eq!(
            names.iter().filter(|n| *n == "SERVICES_KEY_AICHAT").count(),
            1
        );
    }
}
