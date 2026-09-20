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
//! This agent's own credentials, which are the signing key, its key id and the variables the
//! gateways in force read their bearer tokens from, and whatever a person named in their settings
//! file. A gateway's variable is known rather than guessed: the provider block states it. Not a survey
//! of every credential the machine might hold: `AWS_PROFILE`, `GITHUB_TOKEN` and `NPM_TOKEN` are
//! left where they are, because `run aws s3 ls` and `run gh pr list` are ordinary requests and a
//! filter matching names cannot distinguish one from an exfiltration. Guessing would trade a claim
//! that holds exactly for one that mostly holds, and mostly holding is what was wrong before.
//!
//! Clearing the environment and allowing a chosen set back in was the other candidate. It fails on
//! a promise `run` already makes: `git push` needs `~/.ssh`, which needs `HOME` and `SSH_AUTH_SOCK`,
//! and the set of programs somebody might ask for cannot be enumerated in advance, so neither can
//! the variables they read.
//!
//! # A pattern in a person's list
//!
//! An entry in `run.scrubEnv` may hold `*` and `?`, matched case-insensitively against the names in
//! this process's environment, so `AWS_*` withholds a family without the person listing each
//! member. What a pattern is matched against is a variable name the person or the host set, and the
//! patterns themselves are the person's own file. Nothing a tool returned decides anything here,
//! and no pattern is built in: [`own_credentials`] is still exact names, so the no-guesswork
//! position holds.

use crate::Settings;
use std::sync::OnceLock;

/// What the settings file named in `run.scrubEnv`, read once per process.
///
/// A file on disk rather than something threaded through every caller: what it holds is a property
/// of the machine, not of a turn, and a path from here to the turn loop would put a configuration
/// argument on a dozen signatures that have no other use for one. Read once because a person who
/// edits it mid-session is describing the next session, and a run whose filtering changed halfway
/// through a turn would be the harder thing to explain.
///
/// The names out of the settings rather than the settings themselves, which is all this module wants
/// of them and is what lets them go. A `Settings` clears its `env` block when it is dropped
/// ([CRED-23](../../../docs/specs/credential-protection.md#CRED-23)), and one kept in a `static` is
/// never dropped at all, so caching the whole of it would hold a signing key written into that block
/// for the length of the run. A name is not a credential: it is what a value was called.
///
/// Only the file's half is cached. Whether the filtering is on at all is [`enabled`], which reads
/// the process environment every time it is asked, so a caller that changes that variable is
/// answered rather than told what the first call decided.
fn named_in_the_settings_file() -> &'static Vec<String> {
    static NAMED: OnceLock<Vec<String>> = OnceLock::new();
    NAMED.get_or_init(|| Settings::load().scrubbed().map(str::to_string).collect())
}

/// The credentials this agent holds for itself, which no program it starts is handed.
///
/// The signing key and its key id, and every variable a gateway in force reads its bearer token
/// from. The gateway names are read from the configuration on each call rather than once: a name
/// there only adds a variable to the set, and a gateway added or removed mid-session is then
/// withheld or released for the next program, without a restart.
///
/// Empty where the filtering is switched off. This much is what a program started for this agent's
/// own purposes withholds, the AWS CLI that resolves a Bedrock credential being the one of those:
/// nobody asked for that program, so there is no answer of theirs to read, and what the machine
/// holds for AWS is what it is being run to resolve.
pub fn own_credentials() -> Vec<String> {
    if !enabled() {
        return Vec::new();
    }
    credentials_of(crate::gateway_variables())
}

/// Every variable withheld from a program somebody asked for, as this machine is configured.
pub fn withheld() -> Vec<String> {
    with_listed(
        own_credentials(),
        named_in_the_settings_file().iter().map(String::as_str),
        &environment(),
    )
}

/// [`withheld`], against named settings rather than the files, so a test needs no ambient one.
///
/// Reads the gateways from `settings` alone. The managed layer and a Vertex AI service named by
/// the environment are [`withheld`]'s to add. A pattern in the list is matched against the names
/// this process holds; [`names_against`] takes those as an argument instead.
pub fn names(settings: &Settings) -> Vec<String> {
    names_against(settings, &environment())
}

/// [`names`], against `environment` rather than this process's own.
///
/// `environment` is the variable names a program would be handed, which is what a pattern in
/// `run.scrubEnv` is matched against. Separate from [`names`] because the answer for a pattern is a
/// property of both the list and the environment, and a test that had to set a variable to ask
/// about one would be setting it for every other test in the process.
pub fn names_against(settings: &Settings, environment: &[String]) -> Vec<String> {
    if !enabled() {
        return Vec::new();
    }
    with_listed(
        credentials_of(crate::gateway_variable_names(settings.providers())),
        settings.scrubbed(),
        environment,
    )
}

/// Whether `name` is what `pattern` describes, with `*` standing for any run of characters and `?`
/// for exactly one, ignoring case.
///
/// Case-insensitive because a person writing `AWS_*` means the family, and a host that spells a
/// member `aws_region` has not thereby opted it back in. Both sides are lowercased whole, so a name
/// outside ASCII is folded by the same rule as the pattern that selects it.
///
/// `*` and `?` are the whole of the syntax. A character class or a brace group would be a second
/// pattern language in a file that already has globs for paths, and the list this serves only ever
/// takes a variable away, so there is nothing to be gained by selecting more precisely.
pub fn matches(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.to_lowercase().chars().collect();
    let name: Vec<char> = name.to_lowercase().chars().collect();
    let mut at_pattern = 0;
    let mut at_name = 0;
    // Where the last `*` stood and how much of the name it had consumed, so a mismatch after one
    // can give it another character rather than failing the whole pattern.
    let mut star: Option<(usize, usize)> = None;
    while at_name < name.len() {
        match pattern.get(at_pattern) {
            Some('*') => {
                star = Some((at_pattern, at_name));
                at_pattern += 1;
            }
            Some('?') => {
                at_pattern += 1;
                at_name += 1;
            }
            Some(character) if *character == name[at_name] => {
                at_pattern += 1;
                at_name += 1;
            }
            _ => match star {
                Some((at_star, consumed)) => {
                    at_pattern = at_star + 1;
                    at_name = consumed + 1;
                    star = Some((at_star, consumed + 1));
                }
                None => return false,
            },
        }
    }
    pattern[at_pattern..]
        .iter()
        .all(|character| *character == '*')
}

/// Which variable names a pattern in `run.scrubEnv` selected, each with the pattern that selected
/// it, in the order the entries are written.
///
/// For the report alone. An exact entry is not listed: it names what it withholds already, and a
/// person reading it back learns nothing. A pattern is the entry whose effect cannot be read off the
/// file, since what it stands for depends on the variables this machine holds.
///
/// Names only, never a value, on the same terms the rest of this module withholds them. A name
/// already among the built-in credentials is listed too where a pattern selected it: the question
/// this answers is what the person's pattern reached, not what would otherwise have leaked.
pub fn matched_by_patterns(settings: &Settings) -> Vec<(String, String)> {
    matched_by_patterns_against(settings, &environment())
}

/// [`matched_by_patterns`], against `environment` rather than this process's own, for the reason
/// [`names_against`] takes one.
pub fn matched_by_patterns_against(
    settings: &Settings,
    environment: &[String],
) -> Vec<(String, String)> {
    if !enabled() {
        return Vec::new();
    }
    settings
        .scrubbed()
        .filter(|entry| is_a_pattern(entry))
        .flat_map(|entry| {
            environment
                .iter()
                .filter(move |name| matches(entry, name))
                .map(move |name| (entry.to_string(), name.clone()))
        })
        .collect()
}

/// Whether `entry` is a pattern rather than a name.
///
/// A variable name cannot contain either character on any platform this runs on, so an entry
/// holding one was written as a pattern and there is no name it could have been instead.
fn is_a_pattern(entry: &str) -> bool {
    entry.contains('*') || entry.contains('?')
}

/// The variable names this process holds, which is what a pattern is matched against.
///
/// Names only. The values are what the list exists to withhold, and nothing here needs one.
fn environment() -> Vec<String> {
    std::env::vars_os()
        .filter_map(|(name, _)| name.into_string().ok())
        .collect()
}

/// The built-in credentials, then the gateway variables that are not already among them.
fn credentials_of(gateway_variables: Vec<String>) -> Vec<String> {
    let mut names: Vec<String> = crate::env_var::SCRUBBED
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    for name in gateway_variables {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// `own`, then whatever a settings file listed that is not already among them.
///
/// An entry holding `*` or `?` stands for the names in `environment` it matches, and contributes
/// nothing where it matches none: a pattern is a way of naming variables, and one naming none
/// withholds nothing. An exact name is added whether or not the environment holds it, since
/// removing a variable that was never set is what a machine without it already looks like.
fn with_listed<'a>(
    mut own: Vec<String>,
    listed: impl Iterator<Item = &'a str>,
    environment: &[String],
) -> Vec<String> {
    // Nothing of its own is the filtering switched off, which withholds a person's list too.
    if own.is_empty() {
        return own;
    }
    for entry in listed {
        if is_a_pattern(entry) {
            for name in environment.iter().filter(|name| matches(entry, name)) {
                if !own.iter().any(|already| already == name) {
                    own.push(name.clone());
                }
            }
        } else if !own.iter().any(|already| already == entry) {
            own.push(entry.to_string());
        }
    }
    own
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
        for withheld in [
            "SERVICES_KEY_AICHAT",
            "BRAVE_SERVICES_KEY_ID",
            "BRAVE_AI_CHAT_API_KEY",
        ] {
            assert!(
                names.iter().any(|name| name == withheld),
                "{withheld} reached a program the agent started"
            );
        }
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
            !own_credentials().iter().any(|name| name == "AWS_PROFILE"),
            "a settings file decided what this agent withholds from its own credential resolution"
        );
    }

    /// CRED-23: what this module caches for the length of the run is names, so that the settings
    /// they came out of can be dropped and clear their `env` block.
    ///
    /// A `static` is never dropped, so a `Settings` in one would hold a signing key written into
    /// that block until the process exited. The list the two entry points build has to be the same
    /// either way, which is what makes caching the names rather than the settings a saving and not
    /// a second answer: [`withheld`] reads the cache and [`names`] reads a `Settings` handed to it,
    /// and a reader of this module needs them to agree.
    #[test]
    fn the_names_a_settings_file_added_are_what_is_kept_rather_than_the_settings() {
        let listed = "MY_TOKEN";
        let settings = Settings::parse(&format!(r#"{{"run": {{"scrubEnv": ["{listed}"]}}}}"#));

        let asked = names(&settings);
        let cached = with_listed(
            credentials_of(Vec::new()),
            std::iter::once(listed),
            &environment(),
        );

        assert_eq!(
            cached, asked,
            "the cached route answers differently from the one a Settings is handed to"
        );
        assert!(
            asked.iter().any(|name| name == listed),
            "the fixture's own name is missing, so the comparison says nothing"
        );
    }

    /// RUN-12: the variables a gateway's token is read from are withheld with no configuration
    /// beyond the provider block that names them, and every one it names, not only the first.
    #[test]
    fn the_variables_a_provider_block_names_are_withheld() {
        let settings = Settings::parse(
            r#"{"provider": {"gateway": {
                "options": {"baseURL": "https://gateway.example/v1"},
                "env": ["GATEWAY_FIRST_TOKEN", "GATEWAY_SECOND_TOKEN"]
            }}}"#,
        );
        let names = names(&settings);
        for expected in [
            "GATEWAY_FIRST_TOKEN",
            "GATEWAY_SECOND_TOKEN",
            "SERVICES_KEY_AICHAT",
            "BRAVE_SERVICES_KEY_ID",
        ] {
            assert!(
                names.iter().any(|name| name == expected),
                "{expected} was not withheld"
            );
        }
    }

    /// The names come out of the block rather than out of a list of well-known gateways, and a
    /// block that states no variable adds none: the set is exactly what the configuration states.
    #[test]
    fn a_variable_no_provider_block_names_is_not_withheld() {
        let settings = Settings::parse(
            r#"{"provider": {
                "openrouter": {"env": ["OPENROUTER_CUSTOM_NAME"]},
                "keyless": {"options": {"baseURL": "https://keyless.example/v1", "apiKey": "k"}}
            }}"#,
        );
        let names = names(&settings);
        assert!(names.iter().any(|name| name == "OPENROUTER_CUSTOM_NAME"));
        assert!(
            !names.iter().any(|name| name == "OPENROUTER_API_KEY"),
            "a well-known name was withheld that no block reads"
        );
        assert_eq!(
            names.len(),
            3,
            "a block naming no variable added to the set: {names:?}"
        );
    }

    /// A variable two blocks read, or one the built-in set already holds, is listed once.
    #[test]
    fn a_variable_several_blocks_name_is_withheld_once() {
        let settings = Settings::parse(
            r#"{"provider": {
                "a": {"options": {"baseURL": "https://a.example/v1"}, "env": ["SHARED_TOKEN"]},
                "b": {"options": {"baseURL": "https://b.example/v1"},
                      "env": ["SHARED_TOKEN", "SERVICES_KEY_AICHAT"]}
            }}"#,
        );
        let names = names(&settings);
        for repeated in ["SHARED_TOKEN", "SERVICES_KEY_AICHAT"] {
            assert_eq!(names.iter().filter(|name| *name == repeated).count(), 1);
        }
    }

    /// A gateway's variable is this agent's own credential, so the programs it starts for itself
    /// are held to it as well, which a name in `run.scrubEnv` is not.
    #[test]
    fn a_gateway_variable_is_one_of_this_agents_own() {
        assert_eq!(
            credentials_of(vec!["GATEWAY_TOKEN".to_string()]),
            [
                "SERVICES_KEY_AICHAT",
                "BRAVE_SERVICES_KEY_ID",
                "GATEWAY_TOKEN"
            ]
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

    /// The environment a pattern is matched against in these tests, named rather than ambient so
    /// no test has to set a variable on the process every other test shares.
    fn an_environment(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    /// RUN-28: a pattern withholds the family it describes, matched without regard to case, and
    /// withholds nothing else.
    ///
    /// `aws_region` is the case half: a pattern compared byte for byte passes on `AWS_PROFILE` and
    /// fails here. `PATH` is the other half: an implementation treating a pattern as "withhold
    /// everything" or matching only the literal prefix passes the first two and fails this one.
    #[test]
    fn a_pattern_withholds_the_family_it_names_whatever_the_case() {
        let settings = Settings::parse(r#"{"run": {"scrubEnv": ["AWS_*"]}}"#);
        let names = names_against(
            &settings,
            &an_environment(&["AWS_PROFILE", "aws_region", "PATH"]),
        );
        for withheld in ["AWS_PROFILE", "aws_region"] {
            assert!(
                names.iter().any(|name| name == withheld),
                "{withheld} was not withheld by AWS_*"
            );
        }
        assert!(
            !names.iter().any(|name| name == "PATH"),
            "PATH was withheld by AWS_*, so the pattern matches more than it names"
        );
    }

    /// A pattern covers the whole name rather than a prefix of it, so `AWS_*` does not reach a
    /// variable that merely mentions `AWS_`, and `?` stands for exactly one character.
    #[test]
    fn a_pattern_covers_the_whole_name_and_a_question_mark_one_character() {
        let environment = an_environment(&["MY_AWS_KEY", "LOG_1", "LOG_12", "LOG_"]);
        let kept = names_against(
            &Settings::parse(r#"{"run": {"scrubEnv": ["AWS_*"]}}"#),
            &environment,
        );
        assert!(
            !kept.iter().any(|name| name == "MY_AWS_KEY"),
            "AWS_* matched a name it is only a part of"
        );

        let single = names_against(
            &Settings::parse(r#"{"run": {"scrubEnv": ["LOG_?"]}}"#),
            &environment,
        );
        assert!(single.iter().any(|name| name == "LOG_1"));
        for kept in ["LOG_12", "LOG_"] {
            assert!(
                !single.iter().any(|name| name == kept),
                "LOG_? matched {kept}, so it stands for other than one character"
            );
        }
    }

    /// RUN-28: a pattern that covers a built-in credential leaves it withheld once. The built-in
    /// set is added first, so an implementation appending every match would list it twice and
    /// `env_remove` would be called on the same name twice over.
    #[test]
    fn a_pattern_matching_a_built_in_credential_does_not_list_it_twice() {
        let settings = Settings::parse(r#"{"run": {"scrubEnv": ["SERVICES_*"]}}"#);
        let names = names_against(&settings, &an_environment(&["SERVICES_KEY_AICHAT"]));
        assert_eq!(
            names
                .iter()
                .filter(|name| *name == "SERVICES_KEY_AICHAT")
                .count(),
            1,
            "a pattern added a built-in credential a second time: {names:?}"
        );
    }

    /// A pattern matching no variable withholds nothing, while an exact name is withheld whether
    /// the environment holds it or not: removing a variable that was never set is the state a
    /// machine without it is already in.
    #[test]
    fn a_pattern_naming_no_variable_adds_nothing_and_an_exact_name_still_does() {
        let settings = Settings::parse(r#"{"run": {"scrubEnv": ["NOTHING_*", "ABSENT_TOKEN"]}}"#);
        let names = names_against(&settings, &an_environment(&["PATH"]));
        assert!(names.iter().any(|name| name == "ABSENT_TOKEN"));
        assert_eq!(
            names.len(),
            credentials_of(Vec::new()).len() + 1,
            "a pattern that matched nothing added a name: {names:?}"
        );
    }

    /// RUN-28: a pattern is the person's own list and reaches no further than it did before. A
    /// program this agent starts for itself is held to [`own_credentials`], which is exact names,
    /// so `AWS_*` cannot stop the AWS CLI resolving the credential it is run to resolve.
    #[test]
    fn a_pattern_is_not_one_of_this_agents_own() {
        let settings = Settings::parse(r#"{"run": {"scrubEnv": ["AWS_*"]}}"#);
        assert!(
            names_against(&settings, &an_environment(&["AWS_PROFILE"]))
                .iter()
                .any(|name| name == "AWS_PROFILE"),
            "a pattern somebody listed stopped reaching a program they asked for"
        );
        assert!(
            !own_credentials().iter().any(|name| name == "AWS_PROFILE"),
            "a pattern in a settings file decided what this agent withholds from its own \
             credential resolution"
        );
    }

    /// The matching itself, at the edges a family pattern does not reach: a `*` on its own, one in
    /// the middle, several in a row, and an entry with no wildcard at all, which must stay an
    /// exact comparison.
    #[test]
    fn a_star_spans_any_run_of_characters_and_a_plain_name_matches_only_itself() {
        for (pattern, name) in [
            ("*", "ANYTHING"),
            ("*", ""),
            ("*_TOKEN", "CI_TOKEN"),
            ("*token*", "MY_TOKEN_B"),
            ("A**B", "AXYB"),
            ("A*", "A"),
            ("MY_TOKEN", "my_token"),
        ] {
            assert!(matches(pattern, name), "{pattern} did not match {name}");
        }
        for (pattern, name) in [
            ("*_TOKEN", "TOKEN_CI"),
            ("A*B", "AXY"),
            ("MY_TOKEN", "MY_TOKEN_B"),
            ("MY_TOKEN", "MY_TOKE"),
            ("?", ""),
        ] {
            assert!(!matches(pattern, name), "{pattern} matched {name}");
        }
    }

    /// RUN-28: what a pattern is matched against is this process's own environment.
    ///
    /// [`names`] and [`withheld`] both read [`environment`], and a pattern matched against an
    /// empty list withholds nothing however correct the matching is. `PATH` is set in every
    /// environment this runs in, so a pattern naming it reaching the answer is the wiring.
    #[test]
    fn a_pattern_is_matched_against_this_processes_own_environment() {
        assert!(
            environment().iter().any(|name| name == "PATH"),
            "the environment this is matched against does not hold PATH, so the test says nothing"
        );
        let settings = Settings::parse(r#"{"run": {"scrubEnv": ["PAT?"]}}"#);
        assert!(
            names(&settings).iter().any(|name| name == "PATH"),
            "a pattern was matched against something other than this process's environment"
        );
    }

    /// RUN-28: the report says what a pattern reached on this machine, which is the one thing the
    /// file cannot say. An exact entry withholds what it spells, so listing it back teaches nobody
    /// anything; a pattern's reach depends on the variables this process holds.
    ///
    /// Both halves are asserted together because a report listing every withheld name would pass a
    /// test that only checked the pattern's matches.
    #[test]
    fn the_report_names_what_a_pattern_reached_and_not_what_an_exact_entry_spelled() {
        let settings = Settings::parse(r#"{"run": {"scrubEnv": ["AWS_*", "MY_EXACT_NAME"]}}"#);
        let matched = matched_by_patterns_against(
            &settings,
            &an_environment(&["AWS_PROFILE", "aws_region", "MY_EXACT_NAME", "PATH"]),
        );

        assert_eq!(
            matched,
            vec![
                ("AWS_*".to_string(), "AWS_PROFILE".to_string()),
                ("AWS_*".to_string(), "aws_region".to_string()),
            ],
            "the report does not name each variable the pattern reached, and only those"
        );
    }

    /// A pattern that reached nothing has nothing to report, so a person is not shown an empty
    /// claim that something was withheld.
    #[test]
    fn a_pattern_that_reached_nothing_is_not_reported() {
        let settings = Settings::parse(r#"{"run": {"scrubEnv": ["NOTHING_*"]}}"#);

        assert!(
            matched_by_patterns_against(&settings, &an_environment(&["PATH", "HOME"])).is_empty(),
            "a pattern matching no variable was reported as having withheld one"
        );
    }
}
