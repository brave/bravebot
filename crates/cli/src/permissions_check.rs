//! `bravebot permissions check <Family> <specifier>`: which rule decides a call (CLI-30).
//!
//! The rules are loaded the way a session in this directory loads them, and the call is only
//! matched against them: nothing is opened, started or fetched. The specifier is what a person
//! typed, so the match is on routing alone (PERM-1).

use crate::exit::{Ending, fail};
use bravebot_agent::granted::Proposed;
use bravebot_config::Settings;
use bravebot_core::permissions::{Permissions, Rule, Ruling, Subject};
use bravebot_i18n::t;
use std::process::ExitCode;

/// The call a person asked about, in the one spelling each family is matched in.
enum Call {
    Path(Subject, String),
    Command(Vec<String>),
    Host(String),
    Tool(String, String),
}

impl Call {
    /// What the arguments after `check` name, or `None` where they name no call.
    fn read(words: &[String]) -> Option<Self> {
        let (family, rest) = words.split_first()?;
        match (Subject::parse(family)?, rest) {
            (subject @ (Subject::Read | Subject::Edit), [path]) => {
                Some(Self::Path(subject, path.clone()))
            }
            (Subject::Bash, argv) if !argv.is_empty() => Some(Self::Command(argv.to_vec())),
            // A URL is reduced to its host, as a fetch is (PERM-1); anything that is not one is
            // taken as the host it spells.
            (Subject::WebFetch, [target]) => Some(Self::Host(
                bravebot_core::url::host_of(target).unwrap_or_else(|| target.clone()),
            )),
            (Subject::Mcp, [target]) => {
                let (alias, tool) = target.split_once(':')?;
                Some(Self::Tool(alias.to_string(), tool.to_string()))
            }
            _ => None,
        }
    }

    /// The rule of `permissions` that decides this call, and the list it is in.
    fn decided_by<'a>(&self, permissions: &'a Permissions) -> Option<(Ruling, &'a Rule)> {
        match self {
            Self::Path(subject, path) => permissions.rule_for_path(*subject, path),
            Self::Command(argv) => permissions.rule_for_command(argv),
            Self::Host(host) => permissions.rule_for_host(host),
            Self::Tool(alias, tool) => permissions.rule_for_mcp(alias, tool),
        }
    }
}

/// Run `bravebot permissions <command>`.
pub(crate) fn command(args: &[String]) -> ExitCode {
    let Some(call) = args
        .split_first()
        .filter(|(check, _)| *check == "check")
        .and_then(|(_, words)| Call::read(words))
    else {
        return fail(Ending::Argument, t!(permissions_check_usage));
    };
    let settings = Settings::load();
    let profile = bravebot_agent::home::profile();
    // Canonicalized as `doctor` does, so the record of what a person granted is read for the
    // directory a session here would have keyed it on.
    let workspace = std::env::current_dir()
        .and_then(|cwd| cwd.canonicalize())
        .unwrap_or_default();
    let proposed = bravebot_agent::permissions::proposed(&settings, profile.as_deref());
    let granted: Vec<Proposed> = match bravebot_agent::home::directory() {
        Some(home) if !workspace.as_os_str().is_empty() => {
            bravebot_agent::granted::Store::new(&home, &workspace)
                .granted(&proposed)
                .into_iter()
                .cloned()
                .collect()
        }
        _ => Vec::new(),
    };
    let (permissions, _) = bravebot_agent::permissions::with_granted(
        &settings,
        &granted
            .iter()
            .map(|rule| rule.rule.clone())
            .collect::<Vec<_>>(),
        profile.as_deref(),
        &workspace,
    );
    let decided = call.decided_by(&permissions);
    match decided {
        None => println!(
            "{}: {}",
            t!(permissions_check_decision),
            t!(permissions_check_none)
        ),
        Some((ruling, rule)) => {
            println!(
                "{}: {}",
                t!(permissions_check_decision),
                match ruling {
                    Ruling::Deny => t!(permissions_check_deny),
                    Ruling::Ask => t!(permissions_check_ask),
                    Ruling::Allow => t!(permissions_check_allow),
                }
            );
            // The file's own spelling where a layer wrote the rule, and the grant's where the
            // person granted a checkout's rule at the question (PERM-15).
            match settings.rule_source(ruling.as_str(), rule.written()) {
                Some((file, written)) => {
                    println!("{}: {written}", t!(permissions_check_rule));
                    println!("{}: {}", t!(permissions_check_file), file.display());
                }
                None => {
                    println!("{}: {}", t!(permissions_check_rule), rule.written());
                    if let Some(from) = granted.iter().find(|g| g.rule.trim() == rule.written()) {
                        println!(
                            "{}: {}",
                            t!(permissions_check_file),
                            t!(
                                permissions_check_granted,
                                path = from.path.display().to_string()
                            )
                        );
                    }
                }
            }
        }
    }
    // A rule that restricts decides before any grant, so one that could not grant yet is only
    // worth naming where nothing restricted the call.
    if !matches!(decided, Some((Ruling::Deny | Ruling::Ask, _))) {
        let waiting: Vec<Proposed> = proposed
            .into_iter()
            .filter(|rule| !granted.contains(rule))
            .collect();
        let would_allow =
            bravebot_agent::permissions::as_allow_list(&waiting, profile.as_deref(), &workspace);
        if let Some((_, rule)) = call.decided_by(&would_allow) {
            let from = waiting.iter().find(|w| w.rule.trim() == rule.written());
            println!(
                "{}",
                t!(
                    permissions_check_not_in_force,
                    rule = rule.written(),
                    path = from
                        .map(|from| from.path.display().to_string())
                        .unwrap_or_default()
                )
            );
        }
    }
    ExitCode::SUCCESS
}
