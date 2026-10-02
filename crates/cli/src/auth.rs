//! `bravebot auth`: every way this program signs in to a model service, from one command.
//!
//! Each way is a flow that already exists, run by the same function its own command runs, so the
//! two cannot drift apart. What this adds is the list: a person who has not read the documentation
//! learns from it which ways there are and which are already signed in.

use crate::exit::{Ending, fail};
use crate::plain::Prompting;
use crate::progress::printable;
use bravebot_agent::backend::Backend;
use bravebot_agent::confirm::Decision;
use bravebot_config::bedrock::Bedrock;
use bravebot_config::{Config, env_var};
use bravebot_i18n::t;
use std::io::{BufRead, IsTerminal, Write};
use std::process::ExitCode;

/// A way to sign in, in the order the list shows them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Way {
    Leo,
    Bedrock,
    Import,
}

impl Way {
    const ALL: [Way; 3] = [Way::Leo, Way::Bedrock, Way::Import];

    /// The word that names this way on the command line. Not translated, because a script types it.
    fn name(self) -> &'static str {
        match self {
            Way::Leo => "leo",
            Way::Bedrock => "bedrock",
            Way::Import => "import",
        }
    }

    fn named(word: &str) -> Option<Way> {
        Way::ALL
            .into_iter()
            .find(|way| way.name().eq_ignore_ascii_case(word))
    }

    fn description(self) -> &'static str {
        match self {
            Way::Leo => t!(auth_way_leo),
            Way::Bedrock => t!(auth_way_bedrock),
            Way::Import => t!(auth_way_import),
        }
    }
}

/// Run `bravebot auth <command>`.
pub(crate) fn command(args: &[String]) -> ExitCode {
    match args.split_first() {
        Some((command, rest)) if command == "login" => login(rest),
        Some((command, rest)) if command == "logout" => logout(rest),
        Some((other, _)) => {
            refused_with_the_forms(t!(auth_unknown_command, command = printable(other)))
        }
        None => refused_with_the_forms(t!(auth_needs_a_command)),
    }
}

/// A refusal of the command itself, with every form it could have taken under it.
fn refused_with_the_forms(message: impl std::fmt::Display) -> ExitCode {
    let mut said = message.to_string();
    said.push('\n');
    said.push_str(t!(auth_forms_heading));
    for form in [
        "bravebot auth login",
        "bravebot auth login leo [stable|beta|nightly|development]",
        "bravebot auth login bedrock",
        "bravebot auth login import",
        "bravebot auth logout leo",
    ] {
        said.push_str("\n  ");
        said.push_str(form);
    }
    fail(Ending::Argument, said)
}

/// `bravebot auth login [way]`: the way named, or the list and a question where none is.
fn login(args: &[String]) -> ExitCode {
    // No incognito check of its own: Leo and the import are refused by the checks their commands
    // make, and an AWS session is the AWS CLI's, which a session in that mode signs in to as well.
    if let Some((word, rest)) = args.split_first() {
        return match Way::named(word) {
            Some(way) => sign_in(way, rest),
            None => refused_with_the_forms(t!(auth_unknown_way, way = printable(word))),
        };
    }
    // Both ends, as `import-providers` asks for them: a question on a redirected stream is one
    // nobody read, and a pipe on stdin would be answering for them.
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return refused_with_the_forms(t!(auth_needs_a_terminal));
    }
    // Scoped, so the lock on stdin is released before the way runs: `import-providers` takes its
    // own, and a second lock on stdin from the same thread waits forever.
    let picked = {
        let mut asking = Prompting::new(std::io::stdin().lock(), std::io::stderr());
        picked(&mut asking, &held)
    };
    match picked {
        Ok(Some((way, rest))) => sign_in(way, &rest),
        Ok(None) => ExitCode::SUCCESS,
        Err(refused) => fail(Ending::Argument, refused),
    }
}

/// The way picked with the arguments it runs with, `None` where nothing was, or the refusal of an
/// answer that names no way.
type Picked = Result<Option<(Way, Vec<String>)>, String>;

/// List the ways, ask which, and return what was picked.
///
/// A Leo sign-in already held is asked about again before it is repeated, because a second one
/// registers this machine with Brave as one more device. The other two repeat harmlessly: an AWS
/// session that is still good is left alone, and an import says when it has nothing new.
fn picked<R: BufRead, W: Write>(
    asking: &mut Prompting<R, W>,
    held: &dyn Fn(Way) -> Option<String>,
) -> Picked {
    // Asked once per way, so the question about a held sign-in is about the one the list showed.
    let holding = Way::ALL.map(|way| (way, held(way)));
    asking.say(t!(auth_ways_heading));
    for (number, (way, status)) in (1..).zip(&holding) {
        let description = match status {
            Some(status) => t!(
                auth_way_held,
                description = way.description(),
                status = status
            ),
            None => way.description().to_string(),
        };
        asking.say(&format!("  {number}. {:<9}{description}", way.name()));
    }

    let answer = match asking.answer(t!(auth_which_way)) {
        Some(answer) if !answer.is_empty() => answer,
        _ => return Ok(None),
    };
    let way = answer
        .parse::<usize>()
        .ok()
        .and_then(|number| number.checked_sub(1))
        .and_then(|index| Way::ALL.get(index).copied())
        .or_else(|| Way::named(&answer));
    let Some(way) = way else {
        return Err(t!(auth_not_a_listed_way, answer = printable(&answer)));
    };
    if way != Way::Leo {
        return Ok(Some((way, Vec::new())));
    }

    if let Some((_, Some(status))) = holding.iter().find(|(listed, _)| *listed == way) {
        asking.say(&t!(auth_leo_held, status = status));
        if asking.ask(&[], t!(auth_sign_in_again)) != Decision::Approve {
            return Ok(None);
        }
    }
    // Passed on rather than checked here, so a word that names no channel is refused by the check
    // `import-leo-creds` makes, with its message. Made printable first, because that message
    // repeats the word.
    match asking.answer(t!(auth_which_channel)) {
        None => Ok(None),
        Some(channel) if channel.is_empty() => Ok(Some((way, Vec::new()))),
        Some(channel) => Ok(Some((way, vec![printable(&channel)]))),
    }
}

/// What a way already holds, as the list shows it, or `None` where it holds nothing.
///
/// Never the credential: the Leo line is the one `doctor` prints, a count of what is left, and an
/// AWS account is reported only as signed in.
fn held(way: Way) -> Option<String> {
    match way {
        Way::Leo => bravebot_skus::store::load().ok().map(|stored| {
            t!(
                doctor_subscription,
                environment = stored.environment.as_str(),
                unspent = stored.remaining(),
                total = stored.credentials.len()
            )
        }),
        Way::Bedrock => {
            let config = Config::from_env().ok()?;
            let accounts = accounts(&config, inherited_profile().as_deref());
            (!accounts.is_empty()
                && accounts
                    .into_iter()
                    .all(|(_, account)| Backend::signed_in_to(account)))
            .then(|| t!(auth_signed_in).to_string())
        }
        // An import writes settings and holds nothing of its own, so it is never signed in.
        Way::Import => None,
    }
}

/// Run one way with the arguments that followed its name.
fn sign_in(way: Way, rest: &[String]) -> ExitCode {
    match way {
        // `--forget` included: signing out is `logout`, and a sign-in that forgot would be the
        // opposite of what was typed.
        Way::Leo => match rest {
            [flag, ..] if flag.starts_with('-') => fail(
                Ending::Argument,
                t!(cli_unknown_option, flag = printable(flag)),
            ),
            // One channel, where `import-leo-creds` would take the last of several.
            [_, extra, ..] => unexpected(way, extra),
            _ => crate::import_leo_creds(rest),
        },
        Way::Bedrock | Way::Import if !rest.is_empty() => unexpected(way, &rest[0]),
        Way::Bedrock => bedrock(),
        Way::Import => crate::import::providers(&[]),
    }
}

/// The refusal of a word after a way that takes no more.
fn unexpected(way: Way, argument: &str) -> ExitCode {
    fail(
        Ending::Argument,
        t!(
            auth_unexpected_argument,
            command = format!("bravebot auth login {}", way.name()),
            argument = printable(argument)
        ),
    )
}

/// Sign in to every AWS account the configuration names, where it has no usable session.
///
/// The sign-in a session would make before its first turn (BACKEND-9), made now, so the URL and
/// the code are on a terminal somebody is looking at for that purpose.
fn bedrock() -> ExitCode {
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(problem) => return fail(Ending::Configuration, problem),
    };
    let accounts = accounts(&config, inherited_profile().as_deref());
    if accounts.is_empty() {
        return fail(
            Ending::Configuration,
            t!(
                auth_no_aws_account,
                switch = env_var::USE_BEDROCK,
                region = env_var::AWS_REGION
            ),
        );
    }
    // Every account, past one that fails: each is a session of its own, and a profile that cannot
    // sign in says nothing about the next.
    let mut ending = ExitCode::SUCCESS;
    for (profile, account) in accounts {
        let signed_in = match Backend::sign_in_to(account, |line| println!("{line}")) {
            Err(failure) => Err(failure.to_string()),
            // Asked again, because `aws sso login` can finish for a profile whose credentials still
            // cannot be exported, and the export is what a turn signs with.
            Ok(()) if !Backend::signed_in_to(account) => Err(t!(auth_aws_still_signed_out).into()),
            Ok(()) => Ok(()),
        };
        let profile = profile.as_deref().map(printable);
        match (signed_in, profile) {
            (Ok(()), Some(profile)) => {
                println!("{}", t!(auth_aws_profile_signed_in, profile = profile));
            }
            (Ok(()), None) => println!("{}", t!(auth_aws_default_signed_in)),
            (Err(failure), Some(profile)) => {
                ending = fail(
                    Ending::Failed,
                    t!(
                        auth_aws_profile_failed,
                        profile = profile,
                        failure = failure
                    ),
                );
            }
            (Err(failure), None) => {
                ending = fail(
                    Ending::Failed,
                    t!(auth_aws_default_failed, failure = failure),
                );
            }
        }
    }
    ending
}

/// The profile an `aws` started with no `--profile` uses: the one `AWS_PROFILE` names, which it
/// inherits.
fn inherited_profile() -> Option<String> {
    std::env::var(env_var::AWS_PROFILE)
        .ok()
        .map(|profile| profile.trim().to_string())
        .filter(|profile| !profile.is_empty())
}

/// Every AWS account the configuration names, the tier variables' first, once per profile, each
/// with the profile the AWS CLI will sign it in to.
///
/// Once per profile because the session is the profile's, so a second account on one would check
/// the same session again and report it twice. An account naming no profile is on the one
/// `inherited` names, as the `aws` it starts is.
fn accounts<'c>(config: &'c Config, inherited: Option<&str>) -> Vec<(Option<String>, &'c Bedrock)> {
    let mut accounts: Vec<(Option<String>, &Bedrock)> = Vec::new();
    let named = config
        .bedrock
        .iter()
        .chain(config.bedrock_providers().map(|(_, bedrock)| bedrock));
    for account in named {
        let profile = account.profile.as_deref().or(inherited).map(str::to_string);
        if !accounts.iter().any(|(seen, _)| *seen == profile) {
            accounts.push((profile, account));
        }
    }
    accounts
}

/// `bravebot auth logout <way>`. Only Leo keeps a credential of its own, so it is the only way
/// signed out of here; the other two say where what they left is kept.
fn logout(args: &[String]) -> ExitCode {
    let Some((word, rest)) = args.split_first() else {
        return refused_with_the_forms(t!(auth_logout_needs_a_way));
    };
    let Some(way) = Way::named(word) else {
        return refused_with_the_forms(t!(auth_unknown_way, way = printable(word)));
    };
    if let Some(extra) = rest.first() {
        return fail(
            Ending::Argument,
            t!(
                auth_unexpected_argument,
                command = format!("bravebot auth logout {}", way.name()),
                argument = printable(extra)
            ),
        );
    }
    match way {
        // Allowed in an incognito session, as INCOG-7 allows forgetting an import: it leaves less
        // behind, not more.
        Way::Leo => crate::import_leo_creds(&["--forget".to_string()]),
        Way::Bedrock => fail(Ending::Argument, t!(auth_logout_bedrock)),
        Way::Import => fail(Ending::Argument, t!(auth_logout_import)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the list said and what was picked, for `answers` typed in turn.
    fn asked(answers: &str, held: &dyn Fn(Way) -> Option<String>) -> (String, Picked) {
        let mut said = Vec::new();
        let picked = {
            let mut asking = Prompting::new(answers.as_bytes(), &mut said);
            picked(&mut asking, held)
        };
        (String::from_utf8(said).expect("utf-8"), picked)
    }

    fn nothing_held(_: Way) -> Option<String> {
        None
    }

    fn leo_held(way: Way) -> Option<String> {
        (way == Way::Leo).then(|| "production subscription imported, 3 of 20".to_string())
    }

    /// CLI-18: every way is on the list by the word that names it, and only one that holds
    /// something says what it holds.
    #[test]
    fn every_way_is_listed_and_only_a_held_one_says_what_it_holds() {
        let (said, _) = asked("\n", &leo_held);

        let lines: Vec<&str> = said.lines().collect();
        for (number, way) in (1..).zip(Way::ALL) {
            let line = lines
                .iter()
                .find(|line| line.starts_with(&format!("  {number}. {}", way.name())))
                .unwrap_or_else(|| panic!("{} is not listed: {said}", way.name()));
            assert_eq!(
                line.contains("3 of 20"),
                way == Way::Leo,
                "{} says the wrong thing about what it holds: {line}",
                way.name()
            );
        }
    }

    /// CLI-18: a way is picked by its number or by its name, nothing typed picks nothing, and an
    /// answer naming no way listed is refused rather than guessed at.
    #[test]
    fn a_way_is_picked_by_its_number_or_its_name() {
        for (answers, expected) in [
            ("2\n", Some(Way::Bedrock)),
            ("3\n", Some(Way::Import)),
            ("bedrock\n", Some(Way::Bedrock)),
            ("  import  \n", Some(Way::Import)),
            ("Bedrock\n", Some(Way::Bedrock)),
            ("\n", None),
            ("", None),
        ] {
            let (said, picked) = asked(answers, &nothing_held);
            assert_eq!(
                picked.map(|picked| picked.map(|(way, _)| way)),
                Ok(expected),
                "{answers:?}: {said}"
            );
        }
        for refused in ["0\n", "4\n", "aws\n"] {
            let (said, picked) = asked(refused, &nothing_held);
            let complaint = picked.expect_err(refused);
            assert!(
                complaint.contains(refused.trim()),
                "{refused:?} was refused without being named: {complaint} {said}"
            );
        }
    }

    /// CLI-18: Leo is asked which channel, and nothing typed is stable, which is what
    /// `import-leo-creds` takes no channel to mean.
    #[test]
    fn leo_is_asked_which_channel_and_passes_the_word_on() {
        let (_, picked) = asked("1\n\n", &nothing_held);
        assert_eq!(picked, Ok(Some((Way::Leo, Vec::new()))));

        let (_, picked) = asked("leo\nnightly\n", &nothing_held);
        assert_eq!(picked, Ok(Some((Way::Leo, vec!["nightly".to_string()]))));

        let (_, picked) = asked("leo\n", &nothing_held);
        assert_eq!(picked, Ok(None), "the end of the input picked a channel");
    }

    /// CLI-18: a Leo sign-in already held is repeated only on a yes, since a second one registers
    /// another device, and the question names the command that signs out instead. What each way
    /// holds is asked once, so the question is about the sign-in the list showed.
    #[test]
    fn a_held_leo_sign_in_is_repeated_only_when_asked_to() {
        let looked = std::cell::Cell::new(0);
        let counted = |way| {
            looked.set(looked.get() + 1);
            leo_held(way)
        };
        let (said, picked) = asked("1\n\n", &counted);
        assert_eq!(picked, Ok(None), "{said}");
        assert!(said.contains("bravebot auth logout leo"), "{said}");
        assert_eq!(looked.get(), Way::ALL.len(), "a way was looked at twice");

        let (said, picked) = asked("1\ny\nbeta\n", &leo_held);
        assert_eq!(
            picked,
            Ok(Some((Way::Leo, vec!["beta".to_string()]))),
            "{said}"
        );
    }

    /// The tier variables' account on `profile`, beside an `amazon-bedrock` block on
    /// `block_profile`, or on none.
    fn two_accounts(profile: &str, block_profile: Option<&str>) -> Config {
        let mut config = Config::from_lookup(|name| match name {
            env_var::USE_BEDROCK => Some("1".into()),
            env_var::AWS_REGION => Some("us-east-1".into()),
            env_var::AWS_PROFILE => Some(profile.into()),
            _ => None,
        })
        .expect("an AWS account");
        let profile =
            block_profile.map_or(String::new(), |named| format!(r#", "profile": "{named}""#));
        config.providers = bravebot_config::Settings::parse(&format!(
            r#"{{"provider": {{"amazon-bedrock": {{
                "options": {{"region": "us-west-2"{profile}}}}}}}}}"#
        ))
        .providers()
        .to_vec();
        config
    }

    /// CLI-18: the session is the profile's, so a block on the profile the tier variables named is
    /// signed in to once, and so is a block naming none where `AWS_PROFILE` names that one, since
    /// the `aws` it starts inherits it. A block on another profile is signed in to as well.
    #[test]
    fn each_aws_profile_is_signed_in_to_once() {
        let signed_in = |config: &Config, inherited| -> Vec<(Option<String>, String)> {
            accounts(config, inherited)
                .into_iter()
                .map(|(profile, account)| (profile, account.region.clone()))
                .collect()
        };
        let work = || Some("work".to_string());
        assert_eq!(
            signed_in(&two_accounts("work", Some("work")), None),
            [(work(), "us-east-1".to_string())]
        );
        assert_eq!(
            signed_in(&two_accounts("work", None), Some("work")),
            [(work(), "us-east-1".to_string())]
        );
        assert_eq!(
            signed_in(&two_accounts("work", Some("home")), None),
            [
                (work(), "us-east-1".to_string()),
                (Some("home".to_string()), "us-west-2".to_string())
            ]
        );
        assert_eq!(
            signed_in(&two_accounts("work", None), None),
            [
                (work(), "us-east-1".to_string()),
                (None, "us-west-2".to_string())
            ]
        );
    }
}
