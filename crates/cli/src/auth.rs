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
use bravebot_agent::home;
use bravebot_config::bedrock::{Bedrock, Tier};
use bravebot_config::import::{Destination, Unwritable};
use bravebot_config::keys::{self, Keys};
use bravebot_config::provider::Provider;
use bravebot_config::{Config, ConfigError, Managed, Settings, env_var};
use bravebot_i18n::t;
use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;
use std::process::ExitCode;

/// A way to sign in, in the order the list shows them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Way {
    Leo,
    Bedrock,
    Import,
    Gateway,
}

impl Way {
    const ALL: [Way; 4] = [Way::Leo, Way::Bedrock, Way::Import, Way::Gateway];

    /// The word that names this way on the command line. Not translated, because a script types it.
    fn name(self) -> &'static str {
        match self {
            Way::Leo => "leo",
            Way::Bedrock => "bedrock",
            Way::Import => "import",
            Way::Gateway => "gateway",
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
            Way::Gateway => t!(auth_way_gateway),
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
        "bravebot auth login gateway [id]",
        "bravebot auth logout leo",
        "bravebot auth logout gateway [id]",
    ] {
        said.push_str("\n  ");
        said.push_str(form);
    }
    fail(Ending::Argument, said)
}

/// `bravebot auth login [way]`: the way named, or the list and a question where none is.
fn login(args: &[String]) -> ExitCode {
    // No incognito check of its own: Leo, the import and a gateway key are refused by the checks
    // their own code makes, and an AWS session is the AWS CLI's, which a session in that mode signs
    // in to as well.
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
            let config = bedrock_config(&Settings::load(), &Managed::load()).ok()?;
            let accounts = accounts(&config, inherited_profile().as_deref());
            (!accounts.is_empty()
                && accounts
                    .into_iter()
                    .all(|(_, account)| Backend::signed_in_to(account)))
            .then(|| t!(auth_signed_in).to_string())
        }
        // An import writes settings and holds nothing of its own, so it is never signed in.
        Way::Import => None,
        // The ids alone. Which host each key goes to is the provider block's to say.
        Way::Gateway => match Keys::read(&home::directory()?) {
            Ok(stored) if stored.is_empty() => None,
            Ok(stored) => Some(t!(auth_gateway_held, ids = listed(stored.ids()))),
            Err(keys::Unreadable) => Some(t!(auth_gateway_held_unreadable).to_string()),
        },
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
        Way::Gateway => match rest {
            [flag, ..] if flag.starts_with('-') => fail(
                Ending::Argument,
                t!(cli_unknown_option, flag = printable(option_name(flag))),
            ),
            // Not repeated back: a word after the id is most likely the key, and a refusal that
            // printed it would put it on the screen as well as in the shell's history.
            [id, _, ..] => fail(
                Ending::Argument,
                t!(auth_gateway_key_argument, id = printable(id)),
            ),
            _ => gateway(rest.first().map(String::as_str)),
        },
    }
}

/// An option without the value after its `=`, which for `--key=` is the key.
fn option_name(flag: &str) -> &str {
    flag.split_once('=').map_or(flag, |(name, _)| name)
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
///
/// Picking this way is the opt-in [`env_var::USE_BEDROCK`] states, so the accounts are the ones
/// [`bedrock_config`] reads, and it is recorded in the person's own settings file once the account
/// it turns on is signed in.
fn bedrock() -> ExitCode {
    let settings = Settings::load();
    let managed = Managed::load();
    let switch = switch(
        managed.get(env_var::USE_BEDROCK),
        std::env::var(env_var::USE_BEDROCK).ok().as_deref(),
        settings.get(env_var::USE_BEDROCK),
    );
    let config = match bedrock_config(&settings, &managed) {
        Ok(config) => config,
        Err(problem) => return fail(Ending::Configuration, problem),
    };
    let accounts = accounts(&config, inherited_profile().as_deref());
    if accounts.is_empty() {
        return fail(
            Ending::Configuration,
            match switch {
                Switch::PinnedOff => t!(
                    auth_bedrock_pinned_off,
                    path = printable(&bravebot_config::managed_file().display().to_string()),
                    switch = env_var::USE_BEDROCK
                ),
                Switch::Off => t!(auth_bedrock_off, switch = env_var::USE_BEDROCK),
                _ => t!(
                    auth_no_aws_account,
                    region = env_var::AWS_REGION,
                    tiers = listed(Tier::ALL.into_iter().map(Tier::env_var))
                ),
            },
        );
    }
    // Every account, past one that fails: each is a session of its own, and a profile that cannot
    // sign in says nothing about the next.
    let mut ending = ExitCode::SUCCESS;
    let mut switched_on = false;
    for (profile, account) in accounts {
        let signed_in = match Backend::sign_in_to(account, |line| println!("{line}")) {
            Err(failure) => Err(failure.to_string()),
            // Asked again, because `aws sso login` can finish for a profile whose credentials still
            // cannot be exported, and the export is what a turn signs with.
            Ok(()) if !Backend::signed_in_to(account) => Err(t!(auth_aws_still_signed_out).into()),
            Ok(()) => Ok(()),
        };
        if signed_in.is_ok()
            && config
                .bedrock
                .as_ref()
                .is_some_and(|tier| std::ptr::eq(tier, account))
        {
            switched_on = true;
        }
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
    // An account a provider block names is used without the switch, so only the tier variables'
    // account signing in is a reason to record it.
    if !switched_on {
        return ending;
    }
    match switch {
        Switch::Pinned | Switch::PinnedOff | Switch::Off => {}
        Switch::Overruled => eprintln!(
            "{}",
            t!(auth_bedrock_overruled, switch = env_var::USE_BEDROCK)
        ),
        // INCOG-7: a session that leaves nothing behind records nothing either.
        Switch::Recordable if bravebot_core::incognito::engaged() => eprintln!(
            "{}",
            t!(
                auth_bedrock_not_recorded_incognito,
                switch = env_var::USE_BEDROCK
            )
        ),
        Switch::Recordable => {
            if let Err(problem) = record() {
                ending = fail(
                    Ending::Failed,
                    t!(
                        auth_bedrock_not_recorded,
                        switch = env_var::USE_BEDROCK,
                        problem = problem
                    ),
                );
            }
        }
    }
    ending
}

/// The configuration `bravebot auth login bedrock` signs in to: the one a session reads, with
/// [`env_var::USE_BEDROCK`] on where nothing sets it, so long as that names a model to use.
///
/// Without the model the tier variables' account is not one a session could use, and switching it
/// on would sign in to the default AWS profile for someone whose Bedrock is a provider block.
fn bedrock_config(settings: &Settings, managed: &Managed) -> Result<Config, ConfigError> {
    let defaulted = settings.clone().with_env_default(env_var::USE_BEDROCK, "1");
    match Config::from_env_and_settings(&defaulted, managed) {
        Ok(config)
            if config
                .bedrock
                .as_ref()
                .is_some_and(|tier| tier.default_model().is_some()) =>
        {
            Ok(config)
        }
        _ => Config::from_env_and_settings(settings, managed),
    }
}

/// What the configuration already says about [`env_var::USE_BEDROCK`], as far as recording it goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Switch {
    /// Nothing turns Bedrock off, so the person's own file may record it on.
    Recordable,
    /// The machine-level layer turns Bedrock on for every user of this machine.
    Pinned,
    /// The machine-level layer turns Bedrock off for every user of this machine.
    PinnedOff,
    /// An export turns Bedrock on over a settings file that turns it off, which is left as it says.
    Overruled,
    /// An export or a settings file turns Bedrock off, whatever is signed in.
    Off,
}

/// [`Switch`] from the three places that may set the name, in the order `Config` ranks them: the
/// machine-level pin, then the export, then the settings files.
///
/// An export of any other value turns Bedrock off, a blank one included: the name is not baked in,
/// so a blank is not passed over for a value under it. A settings file that turns it on may be a
/// checkout's, which a session started elsewhere does not read, so it leaves the switch recordable.
fn switch(pinned: Option<&str>, exported: Option<&str>, recorded: Option<&str>) -> Switch {
    let on = |value: &str| value.trim() == "1";
    match (pinned, exported, recorded) {
        (Some(pin), _, _) if on(pin) => Switch::Pinned,
        (Some(_), _, _) => Switch::PinnedOff,
        (None, Some(export), _) if !on(export) => Switch::Off,
        (None, _, Some(file)) if on(file) => Switch::Recordable,
        (None, Some(_), Some(_)) => Switch::Overruled,
        (None, None, Some(_)) => Switch::Off,
        (None, _, None) => Switch::Recordable,
    }
}

/// Set [`env_var::USE_BEDROCK`] to 1 in the person's own settings file, which is what every session
/// reads and the lowest layer, so a project file that turns Bedrock off still does.
///
/// A value the person wrote there is theirs, so the file is written only where it names nothing.
fn record() -> Result<(), String> {
    let Some(directory) = home::writable() else {
        return Err(t!(import_no_home).to_string());
    };
    let file = bravebot_config::user_settings_file(&directory);
    let shown = printable(&file.display().to_string());
    let mut destination = Destination::open(&file).map_err(|why| unrecordable(why, &file))?;
    let held = destination.env(env_var::USE_BEDROCK);
    if held.is_some_and(|held| held.trim() == "1") {
        return Ok(());
    }
    if held.is_none() && destination.holds_env(env_var::USE_BEDROCK) {
        return Err(t!(auth_bedrock_env_not_a_block, file = shown));
    }
    if destination.names_env(env_var::USE_BEDROCK) {
        eprintln!(
            "{}",
            t!(
                auth_bedrock_left,
                file = shown,
                switch = env_var::USE_BEDROCK
            )
        );
        return Ok(());
    }
    destination.add_env(env_var::USE_BEDROCK, "1");
    crate::import::write(&destination, unrecordable)?;
    println!(
        "{}",
        t!(
            auth_bedrock_recorded,
            file = shown,
            switch = env_var::USE_BEDROCK
        )
    );
    Ok(())
}

fn unrecordable(why: Unwritable, file: &Path) -> String {
    let file = printable(&file.display().to_string());
    match why {
        Unwritable::NotADocument => t!(mcp_settings_not_a_document, path = file),
        Unwritable::TooLarge => t!(mcp_settings_too_large, path = file),
        Unwritable::Changed => t!(auth_bedrock_settings_changed, file = file),
    }
}

/// The profile an `aws` started with no `--profile` uses: the one `AWS_PROFILE` names, which it
/// inherits.
pub(crate) fn inherited_profile() -> Option<String> {
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

/// Store a key for a gateway a provider block names, typed at the terminal with nothing drawn.
///
/// Never taken from the command line (CRED-24), where every process on the machine can read it and
/// the shell keeps it in its history. Every check that needs no answer is made before the key is
/// asked for, so nobody types a key only to be told it cannot be kept.
fn gateway(named: Option<&str>) -> ExitCode {
    // INCOG-7: a session that leaves nothing behind stores no key either.
    if bravebot_core::incognito::engaged() {
        return fail(Ending::Failed, t!(auth_gateway_not_while_incognito));
    }
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(problem) => return fail(Ending::Configuration, problem),
    };
    let gateways: Vec<&Provider> = config
        .providers
        .iter()
        .filter(|provider| provider.bedrock.is_none())
        .collect();
    if gateways.is_empty() {
        return fail(Ending::Configuration, t!(auth_gateway_none_configured));
    }
    let named = match named {
        None => None,
        Some(id) => match gateways.iter().find(|provider| provider.id == id) {
            Some(provider) => Some(*provider),
            // Not repeated back either: a word naming no gateway may be the key, typed where the id
            // goes by somebody who took it for the argument.
            None => {
                return fail(
                    Ending::Argument,
                    t!(
                        auth_gateway_not_configured,
                        ids = listed(gateways.iter().map(|provider| provider.id.as_str()))
                    ),
                );
            }
        },
    };
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return fail(Ending::Argument, t!(auth_gateway_needs_a_terminal));
    }
    let Some(directory) = home::writable() else {
        return fail(Ending::Failed, t!(auth_gateway_no_home));
    };
    let path = keys::file(&directory);
    let Ok(stored) = Keys::read(&directory) else {
        return fail(
            Ending::Failed,
            t!(auth_gateway_keys_unreadable, path = path.display()),
        );
    };

    // Scoped, so the lock on stdin is released before the key is read from the terminal.
    let picked = {
        let mut asking = Prompting::new(std::io::stdin().lock(), std::io::stderr());
        which_gateway(&mut asking, &gateways, named, &stored)
    };
    let provider = match picked {
        Ok(Some(provider)) => provider,
        Ok(None) => return ExitCode::SUCCESS,
        Err(refused) => return fail(Ending::Argument, refused),
    };

    // BACKEND-16 reads a variable before a stored key, so one set here is what a session sends, and
    // that is said before the key is typed rather than after it is kept.
    if let Some(variable) = variable_in_force(provider, |name| std::env::var(name).ok()) {
        eprintln!("{}", t!(auth_gateway_variable_wins, variable = variable));
    }
    let question = t!(
        auth_gateway_key_question,
        id = printable(&provider.id),
        host = printable(provider.host())
    );
    let key = match bravebot_tui::hidden::read(&question) {
        Ok(Some(key)) => key,
        Ok(None) => {
            eprintln!("{}", t!(auth_gateway_nothing_stored));
            return ExitCode::SUCCESS;
        }
        Err(error) => {
            return fail(
                Ending::Failed,
                t!(auth_gateway_not_read, error = error.to_string()),
            );
        }
    };
    // Read again: the copy above is as old as the questions took to answer, and writing it back
    // would restore a key another command forgot in the meantime.
    let Ok(mut stored) = Keys::read(&directory) else {
        return fail(
            Ending::Failed,
            t!(auth_gateway_keys_unreadable, path = path.display()),
        );
    };
    stored.insert(&provider.id, key);
    if let Err(error) = store(&directory, &stored) {
        return fail(
            Ending::Failed,
            t!(
                auth_gateway_not_stored,
                path = path.display(),
                error = error.to_string()
            ),
        );
    }
    println!(
        "{}",
        t!(
            auth_gateway_stored,
            id = printable(&provider.id),
            path = path.display(),
            host = printable(provider.host())
        )
    );
    ExitCode::SUCCESS
}

/// The gateway the key is for, `None` where nothing was picked or a key held is to be kept, or the
/// refusal of an answer that names no gateway listed.
///
/// One configured, or one named, is not asked about. A key already stored for the one picked is
/// asked about before it is replaced, because the one it replaces is gone once the file is written.
fn which_gateway<'p, R: BufRead, W: Write>(
    asking: &mut Prompting<R, W>,
    gateways: &[&'p Provider],
    named: Option<&'p Provider>,
    stored: &Keys,
) -> Result<Option<&'p Provider>, String> {
    let provider = match (named, gateways) {
        (Some(named), _) => named,
        (None, [only]) => only,
        (None, _) => {
            asking.say(t!(auth_gateways_heading));
            // In characters, which is what the padding counts; an id is free text in settings.
            let width = gateways
                .iter()
                .map(|provider| provider.id.chars().count())
                .max();
            for (number, provider) in (1..).zip(gateways) {
                let held = match stored.get(&provider.id) {
                    Some(_) => format!(" ({})", t!(auth_gateway_key_stored)),
                    None => String::new(),
                };
                asking.say(&format!(
                    "  {number}. {:<width$}  {}{held}",
                    printable(&provider.id),
                    printable(provider.host()),
                    width = width.unwrap_or(0)
                ));
            }
            let answer = match asking.answer(t!(auth_which_gateway)) {
                Some(answer) if !answer.is_empty() => answer,
                _ => return Ok(None),
            };
            let picked = answer
                .parse::<usize>()
                .ok()
                .and_then(|number| number.checked_sub(1))
                .and_then(|index| gateways.get(index).copied())
                .or_else(|| {
                    gateways
                        .iter()
                        .copied()
                        .find(|provider| provider.id == answer)
                });
            match picked {
                Some(provider) => provider,
                None => {
                    return Err(t!(auth_not_a_listed_gateway, answer = printable(&answer)));
                }
            }
        }
    };
    if stored.get(&provider.id).is_some() {
        asking.say(&t!(auth_gateway_key_held, id = printable(&provider.id)));
        if asking.ask(&[], t!(auth_gateway_replace)) != Decision::Approve {
            return Ok(None);
        }
    }
    Ok(Some(provider))
}

/// The variable in the block's `env` a session would send instead of a stored key, as BACKEND-16
/// orders them: the first set to something other than space.
fn variable_in_force(provider: &Provider, lookup: impl Fn(&str) -> Option<String>) -> Option<&str> {
    provider.env.iter().map(String::as_str).find(|name| {
        lookup(name).is_some_and(|value| {
            let set = !value.trim().is_empty();
            let mut value = value.into_bytes();
            bravebot_config::scrub_bytes(&mut value);
            set
        })
    })
}

/// Write `stored` over the file of keys in `directory`, or remove the file where nothing is left.
///
/// Written beside the file and renamed over it, as the Leo batch is, so a write that stops part way
/// leaves the keys that were there. Created by the call that gives the Leo batch its protection,
/// 0600 on Unix and granted to this account alone on Windows (STATE-1), because a gateway key is a
/// bearer token too.
fn store(directory: &Path, stored: &Keys) -> std::io::Result<()> {
    let path = keys::file(directory);
    if stored.is_empty() {
        return match std::fs::remove_file(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            removed => removed,
        };
    }
    home::create_directory(directory)?;
    let temporary = path.with_file_name(format!("{}.{}.tmp", keys::FILE, std::process::id()));
    // The file is closed at the end of the closure, before the rename, which Windows refuses while
    // a handle is still open on either name.
    let written = bravebot_skus::store::create_private(&temporary)
        .and_then(|mut file| {
            file.write_all(stored.to_text().expose().as_bytes())?;
            // On disk before the rename, or a crash just after it can leave the name on an empty
            // file where a filesystem orders the rename first.
            file.sync_all()
        })
        .and_then(|()| std::fs::rename(&temporary, &path));
    if written.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    written
}

/// Several names on one line, in the order given.
fn listed<'n>(names: impl Iterator<Item = &'n str>) -> String {
    names.map(printable).collect::<Vec<_>>().join(", ")
}

/// `bravebot auth logout <way>`. Leo and a gateway keep a credential of their own, so they are the
/// ways signed out of here; the other two say where what they left is kept.
fn logout(args: &[String]) -> ExitCode {
    let Some((word, rest)) = args.split_first() else {
        return refused_with_the_forms(t!(auth_logout_needs_a_way));
    };
    let Some(way) = Way::named(word) else {
        return refused_with_the_forms(t!(auth_unknown_way, way = printable(word)));
    };
    let extra = match (way, rest) {
        (_, [flag, ..]) if flag.starts_with('-') => {
            return fail(
                Ending::Argument,
                t!(cli_unknown_option, flag = printable(option_name(flag))),
            );
        }
        (Way::Gateway, [_, extra, ..]) => Some(extra),
        (Way::Gateway, _) => None,
        (_, extra) => extra.first(),
    };
    if let Some(extra) = extra {
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
        // behind, not more. The gateway key on the same footing.
        Way::Leo => crate::import_leo_creds(&["--forget".to_string()]),
        Way::Bedrock => fail(Ending::Argument, t!(auth_logout_bedrock)),
        Way::Import => fail(Ending::Argument, t!(auth_logout_import)),
        Way::Gateway => forget_gateway(rest.first().map(String::as_str)),
    }
}

/// Forget the key stored for a gateway, which ends nothing at the service that issued it.
fn forget_gateway(named: Option<&str>) -> ExitCode {
    let Some(directory) = home::directory() else {
        return fail(Ending::Failed, t!(auth_logout_gateway_none));
    };
    let path = keys::file(&directory);
    let Ok(mut stored) = Keys::read(&directory) else {
        return fail(
            Ending::Failed,
            t!(auth_gateway_keys_unreadable, path = path.display()),
        );
    };
    let id = match forgotten(&stored, named) {
        Ok(id) => id,
        Err(refused) => return fail(Ending::Argument, refused),
    };
    stored.remove(&id);
    if let Err(error) = store(&directory, &stored) {
        return fail(
            Ending::Failed,
            t!(
                auth_logout_gateway_not_written,
                path = path.display(),
                error = error.to_string()
            ),
        );
    }
    // The host from the block that names the id, where one still does: the key is good there until
    // whoever issued it revokes it, and forgetting it here is not that.
    let host = Config::from_env().ok().and_then(|config| {
        config
            .providers
            .iter()
            .find(|provider| provider.bedrock.is_none() && provider.id == id)
            .map(|provider| printable(provider.host()))
    });
    let id = printable(&id);
    match host {
        Some(host) => println!(
            "{}",
            t!(auth_logout_gateway_forgotten, id = id, host = host)
        ),
        None => println!("{}", t!(auth_logout_gateway_forgotten_elsewhere, id = id)),
    }
    ExitCode::SUCCESS
}

/// The id whose key is forgotten: the one named, or the only one stored where none is.
///
/// Never a guess among several, because the one forgotten cannot be had back without going to the
/// service for a new key.
fn forgotten(stored: &Keys, named: Option<&str>) -> Result<String, String> {
    if stored.is_empty() {
        return Err(t!(auth_logout_gateway_none).to_string());
    }
    let mut ids = stored.ids();
    match (named, ids.next(), ids.next()) {
        (Some(id), _, _) if stored.get(id).is_some() => Ok(id.to_string()),
        (Some(id), _, _) => Err(t!(
            auth_logout_gateway_not_stored,
            id = printable(id),
            ids = listed(stored.ids())
        )),
        (None, Some(only), None) => Ok(only.to_string()),
        (None, _, _) => Err(t!(auth_logout_gateway_which, ids = listed(stored.ids()))),
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
            ("4\n", Some(Way::Gateway)),
            ("bedrock\n", Some(Way::Bedrock)),
            ("gateway\n", Some(Way::Gateway)),
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
        for refused in ["0\n", "5\n", "aws\n"] {
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

    /// CLI-18: the switch is recordable where nothing ranked above the person's own file turns it
    /// off, and the place that answers is the one `Config` ranks first: a machine-level pin over an
    /// export, and an export over a settings file, a blank one included.
    #[test]
    fn the_switch_is_recordable_where_nothing_ranked_above_the_persons_file_turns_it_off() {
        for (pinned, exported, recorded, expected) in [
            (None, None, None, Switch::Recordable),
            (None, Some("1"), None, Switch::Recordable),
            (None, Some(" 1 "), None, Switch::Recordable),
            (None, None, Some("1"), Switch::Recordable),
            (None, Some("1"), Some("1"), Switch::Recordable),
            (Some("1"), None, None, Switch::Pinned),
            (Some("1"), Some("0"), Some("0"), Switch::Pinned),
            (None, Some("1"), Some("0"), Switch::Overruled),
            (None, None, Some("0"), Switch::Off),
            (None, Some("0"), None, Switch::Off),
            (None, Some(""), Some("1"), Switch::Off),
            (None, Some("0"), Some("1"), Switch::Off),
            (Some("0"), Some("1"), Some("1"), Switch::PinnedOff),
            (Some("0"), None, None, Switch::PinnedOff),
        ] {
            assert_eq!(
                switch(pinned, exported, recorded),
                expected,
                "pinned {pinned:?}, exported {exported:?}, recorded {recorded:?}"
            );
        }
    }

    /// The gateways two provider blocks configure, and a Bedrock block, which is not one.
    fn two_gateways() -> Vec<Provider> {
        bravebot_config::Settings::parse(
            r#"{"provider": {
                "openrouter": {"env": ["OPENROUTER_API_KEY"],
                    "options": {"baseURL": "https://openrouter.example/api/v1"}},
                "work": {"options": {"baseURL": "https://gateway.work.example/v1"}}
            }}"#,
        )
        .providers()
        .to_vec()
    }

    /// What the gateway question said and which id was picked, for `answers` typed in turn.
    fn gateway_asked(
        answers: &str,
        gateways: &[Provider],
        named: Option<&str>,
        stored: &Keys,
    ) -> (String, Result<Option<String>, String>) {
        let gateways: Vec<&Provider> = gateways.iter().collect();
        let named = named.and_then(|id| gateways.iter().copied().find(|g| g.id == id));
        let mut said = Vec::new();
        let picked = {
            let mut asking = Prompting::new(answers.as_bytes(), &mut said);
            which_gateway(&mut asking, &gateways, named, stored)
        };
        let picked = picked.map(|picked| picked.map(|provider| provider.id.clone()));
        (String::from_utf8(said).expect("utf-8"), picked)
    }

    fn placeholder_keys(ids: &[&str]) -> Keys {
        let mut stored = Keys::default();
        for id in ids {
            stored.insert(id, bravebot_config::Secret::new("placeholder-gateway-key"));
        }
        stored
    }

    /// CLI-18: where several gateways are configured, each is listed by its id with the host its
    /// key goes to, and one is picked by its number or its id. An answer naming none is refused.
    #[test]
    fn a_gateway_is_picked_by_its_number_or_its_id() {
        let gateways = two_gateways();
        let nothing = Keys::default();
        for (answers, expected) in [
            ("1\n", Some("openrouter")),
            ("2\n", Some("work")),
            ("work\n", Some("work")),
            ("\n", None),
            ("", None),
        ] {
            let (said, picked) = gateway_asked(answers, &gateways, None, &nothing);
            assert_eq!(
                picked,
                Ok(expected.map(str::to_string)),
                "{answers:?}: {said}"
            );
            assert!(said.contains("openrouter.example"), "{said}");
            assert!(said.contains("gateway.work.example"), "{said}");
        }
        for refused in ["0\n", "3\n", "elsewhere\n"] {
            let (said, picked) = gateway_asked(refused, &gateways, None, &nothing);
            let complaint = picked.expect_err(refused);
            assert!(complaint.contains(refused.trim()), "{complaint} {said}");
        }
    }

    /// CLI-18: the hosts line up two spaces from the longest id whatever it is written in, since an
    /// id is free text in settings and a width counted in bytes pads past a character that takes
    /// more than one.
    #[test]
    fn the_padding_counts_an_id_in_characters() {
        let gateways = bravebot_config::Settings::parse(
            r#"{"provider": {
                "café": {"options": {"baseURL": "https://cafe.example/v1"}},
                "cafe": {"options": {"baseURL": "https://other.example/v1"}}
            }}"#,
        )
        .providers()
        .to_vec();
        let (said, _) = gateway_asked("", &gateways, None, &Keys::default());
        let column = |host: &str| {
            said.lines()
                .find_map(|line| line.find(host).map(|at| line[..at].chars().count()))
                .unwrap_or_else(|| panic!("{host} is not listed: {said}"))
        };
        assert_eq!(column("cafe.example"), column("other.example"), "{said}");
        assert!(
            said.lines()
                .any(|line| line.ends_with("café  cafe.example")),
            "{said}"
        );
    }

    /// CLI-18: one gateway configured, or one named on the command line, is not asked about.
    #[test]
    fn one_gateway_or_a_named_one_is_not_asked_about() {
        let gateways = two_gateways();
        let (said, picked) = gateway_asked("", &gateways[1..], None, &Keys::default());
        assert_eq!(picked, Ok(Some("work".to_string())));
        assert!(said.is_empty(), "one gateway was asked about: {said}");

        let (said, picked) = gateway_asked("", &gateways, Some("openrouter"), &Keys::default());
        assert_eq!(picked, Ok(Some("openrouter".to_string())));
        assert!(said.is_empty(), "a named gateway was asked about: {said}");
    }

    /// CLI-18: the key a gateway already holds is replaced only on a yes, since the file is
    /// rewritten and the one replaced is gone. The list says which hold one.
    #[test]
    fn a_stored_key_is_replaced_only_when_asked_to() {
        let gateways = two_gateways();
        let stored = placeholder_keys(&["work"]);

        let (said, picked) = gateway_asked("", &gateways, Some("work"), &stored);
        assert_eq!(picked, Ok(None), "{said}");
        let (said, picked) = gateway_asked("y\n", &gateways, Some("work"), &stored);
        assert_eq!(picked, Ok(Some("work".to_string())), "{said}");

        let (said, _) = gateway_asked("\n", &gateways, None, &stored);
        let listed = |id: &str| {
            said.lines()
                .find(|line| line.contains(id) && line.contains(". "))
                .unwrap_or_else(|| panic!("{id} is not listed: {said}"))
                .contains(t!(auth_gateway_key_stored))
        };
        assert!(listed("work"), "{said}");
        assert!(!listed("openrouter"), "{said}");
        assert!(!said.contains("placeholder-gateway-key"), "{said}");
    }

    /// CLI-18: logout forgets the key named, or the only one stored, and never guesses among
    /// several, since a key forgotten has to be issued again.
    #[test]
    fn the_key_forgotten_is_the_one_named_or_the_only_one() {
        let one = placeholder_keys(&["work"]);
        assert_eq!(forgotten(&one, None), Ok("work".to_string()));
        assert_eq!(forgotten(&one, Some("work")), Ok("work".to_string()));
        let refused = forgotten(&one, Some("openrouter")).expect_err("not stored");
        assert!(
            refused.contains("openrouter") && refused.contains("work"),
            "{refused}"
        );

        let two = placeholder_keys(&["openrouter", "work"]);
        assert_eq!(
            forgotten(&two, Some("openrouter")),
            Ok("openrouter".to_string())
        );
        let refused = forgotten(&two, None).expect_err("a guess among two");
        assert!(
            refused.contains("openrouter") && refused.contains("work"),
            "{refused}"
        );

        assert!(forgotten(&Keys::default(), None).is_err());
    }

    /// BACKEND-16: a variable the block names is sent before a stored key, so the sign-in names one
    /// that is set. A variable set to space is not, as it is not where the token is read.
    #[test]
    fn a_set_variable_is_named_as_the_one_sent_instead() {
        let gateways = two_gateways();
        let openrouter = &gateways[0];
        let set = |value: &'static str| {
            move |name: &str| (name == "OPENROUTER_API_KEY").then(|| value.to_string())
        };
        assert_eq!(
            variable_in_force(openrouter, set("a-placeholder")),
            Some("OPENROUTER_API_KEY")
        );
        assert_eq!(variable_in_force(openrouter, set("  ")), None);
        assert_eq!(variable_in_force(openrouter, |_| None), None);
        assert_eq!(variable_in_force(&gateways[1], set("a-placeholder")), None);
    }

    /// A directory of its own under the target directory, removed when the test ends.
    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/test-scratch")
                .join(name);
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("create scratch");
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// CLI-18 and STATE-1: the keys are written where the configuration reads them, readable by
    /// this account alone, with no temporary left beside them, and forgetting the last one removes
    /// the file.
    #[test]
    fn stored_keys_are_written_private_and_read_back() {
        let scratch = Scratch::new("cli-auth-gateway-keys");
        let directory = scratch.0.join("state");

        store(&directory, &placeholder_keys(&["work"])).expect("stored");
        let read = Keys::read(&directory).expect("a file of keys");
        assert_eq!(
            read.get("work").map(bravebot_config::Secret::expose),
            Some("placeholder-gateway-key")
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(keys::file(&directory))
                .expect("the file")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "{mode:o}");
        }
        let left: Vec<_> = std::fs::read_dir(&directory)
            .expect("the directory")
            .map(|entry| entry.expect("an entry").file_name())
            .collect();
        assert_eq!(left, [keys::FILE], "a temporary was left beside the keys");

        store(&directory, &Keys::default()).expect("forgotten");
        assert!(!keys::file(&directory).exists(), "an empty file was left");
        store(&directory, &Keys::default()).expect("nothing to remove is not a failure");
    }
}
