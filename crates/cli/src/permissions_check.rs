//! `bravebot permissions check <Family> <specifier>`: which rule decides a call (CLI-30).
//!
//! The rules are loaded the way a session in this directory loads them, and the call is only
//! matched against them by [`bravebot_agent::rule_check`]: nothing is opened, started or fetched.
//! The specifier is what a person typed, so the match is on routing alone (PERM-1).

use crate::exit::{Ending, fail};
use crate::progress::{printable, printable_path};
use bravebot_agent::granted::Proposed;
use bravebot_agent::rule_check::{Call, Decision, Explained, Origin};
use bravebot_config::Settings;
use bravebot_core::permissions::Ruling;
use bravebot_i18n::t;
use std::process::ExitCode;

/// Print what `explained` says decides the call.
fn print(explained: &Explained) {
    match &explained.decision {
        Decision::ShortName => println!(
            "{}: {}",
            t!(permissions_check_decision),
            t!(permissions_check_short_name)
        ),
        Decision::NoRule => println!(
            "{}: {}",
            t!(permissions_check_decision),
            t!(permissions_check_none)
        ),
        Decision::Rule {
            ruling,
            written,
            origin,
            reason,
        } => {
            println!(
                "{}: {}",
                t!(permissions_check_decision),
                match ruling {
                    Ruling::Deny => t!(permissions_check_deny),
                    Ruling::Ask => t!(permissions_check_ask),
                    Ruling::Allow => t!(permissions_check_allow),
                }
            );
            println!("{}: {}", t!(permissions_check_rule), printable(written));
            match origin {
                Some(Origin::File(file)) => {
                    println!("{}: {}", t!(permissions_check_file), printable_path(file));
                }
                Some(Origin::Granted(file)) => println!(
                    "{}: {}",
                    t!(permissions_check_file),
                    t!(permissions_check_granted, path = printable_path(file))
                ),
                None => {}
            }
            // Display text only, and present only where the person's own file wrote one (PERM-1).
            if let Some(reason) = reason {
                println!("{}: {reason}", t!(permissions_check_reason));
            }
        }
    }
    if let Some(waiting) = &explained.not_in_force {
        println!(
            "{}",
            t!(
                permissions_check_not_in_force,
                rule = printable(&waiting.rule),
                path = printable_path(&waiting.file)
            )
        );
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
    let waiting: Vec<Proposed> = proposed
        .into_iter()
        .filter(|rule| !granted.contains(rule))
        .collect();
    print(&bravebot_agent::rule_check::explain(
        &call,
        &permissions,
        &settings.rule_sources(),
        &granted,
        &waiting,
        profile.as_deref(),
        &workspace,
    ));
    ExitCode::SUCCESS
}
