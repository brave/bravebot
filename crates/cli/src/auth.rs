//! `bravebot auth`: every way this program signs in to a model service, from one command.
//!
//! Each way is a flow that already exists, run by the same function its own command runs, so the
//! two cannot drift apart. What this adds is the list: a person who has not read the documentation
//! learns from it which ways there are and which are already signed in.

use crate::exit::{Ending, fail};
use crate::plain::Prompting;
use crate::progress::printable;
use bravebot_agent::backend::{Backend, CredentialError};
use bravebot_agent::confirm::Decision;
use bravebot_agent::home;
use bravebot_config::bedrock::{Bedrock, Tier};
use bravebot_config::import::{Destination, Gateway, Unwritable};
use bravebot_config::keys::{self, Keys};
use bravebot_config::provider::{Credential, Provider, Source, known_ids};
use bravebot_config::{Config, ConfigError, Managed, Secret, Settings, env_var};
use bravebot_i18n::t;
use std::borrow::Cow;
use std::fmt::Display;
use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;
use std::process::ExitCode;

/// A way to sign in, in the order the list shows them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Way {
    Bedrock,
    Import,
    Gateway,
    ChatGpt,
    Xai,
}

impl Way {
    const ALL: [Way; 5] = [
        Way::Bedrock,
        Way::Import,
        Way::Gateway,
        Way::ChatGpt,
        Way::Xai,
    ];

    /// The word that names this way on the command line. Not translated, because a script types it.
    fn name(self) -> &'static str {
        match self {
            Way::Bedrock => "bedrock",
            Way::Import => "import",
            Way::Gateway => "gateway",
            Way::ChatGpt => "chatgpt",
            Way::Xai => "xai",
        }
    }

    fn named(word: &str) -> Option<Way> {
        Way::ALL
            .into_iter()
            .find(|way| way.name().eq_ignore_ascii_case(word))
    }

    /// What the list says this way is. The gateway line names the services bravebot knows, read
    /// from the same table a session reaches them by.
    fn description(self) -> String {
        match self {
            Way::Bedrock => t!(auth_way_bedrock).to_string(),
            Way::Import => t!(auth_way_import).to_string(),
            Way::Gateway => t!(auth_way_gateway, services = listed(known_ids())),
            Way::ChatGpt => t!(auth_way_chatgpt).to_string(),
            Way::Xai => t!(auth_way_xai).to_string(),
        }
    }
}

/// Run `bravebot auth <command>`.
pub(crate) fn command(args: &[String]) -> ExitCode {
    match args.split_first() {
        Some((command, rest)) if command == "login" => login(rest),
        Some((command, rest)) if command == "logout" => logout(rest),
        Some((command, rest)) if command == "status" => status(rest),
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
        "bravebot auth login bedrock",
        "bravebot auth login import",
        "bravebot auth login gateway [id]",
        "bravebot auth login chatgpt",
        "bravebot auth login xai",
        "bravebot auth logout gateway [id]",
        "bravebot auth logout chatgpt",
        "bravebot auth logout xai",
        "bravebot auth status [bedrock|gateway [id]|chatgpt|xai]",
    ] {
        said.push_str("\n  ");
        said.push_str(form);
    }
    fail(Ending::Argument, said)
}

/// `bravebot auth login [way]`: the way named, or the list and a question where none is.
fn login(args: &[String]) -> ExitCode {
    // No incognito check of its own: the import and a gateway key are refused by the checks
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
    let directory = home::directory();
    let picked = {
        let mut asking = Prompting::new(std::io::stdin().lock(), std::io::stderr());
        picked(&mut asking, &|way| held(way, directory.as_deref()))
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
/// What a way already holds is shown beside it. Repeating one is harmless: an AWS session that is
/// still good is left alone, and an import says when it has nothing new.
fn picked<R: BufRead, W: Write>(
    asking: &mut Prompting<R, W>,
    held: &dyn Fn(Way) -> Option<String>,
) -> Picked {
    asking.say(t!(auth_ways_heading));
    for (number, way) in (1..).zip(Way::ALL) {
        let description = match held(way) {
            Some(status) => t!(
                auth_way_held,
                description = way.description(),
                status = status
            ),
            None => way.description(),
        };
        asking.say(&format!("  {number}. {:<9}{description}", way.name()));
    }

    let answer = match asking.answer(t!(auth_which_way)) {
        Some(answer) if !answer.is_empty() => answer,
        _ => return Ok(None),
    };
    let way = crate::plain::numbered_row(&answer, &Way::ALL)
        .copied()
        .or_else(|| Way::named(&answer));
    match way {
        Some(way) => Ok(Some((way, Vec::new()))),
        None => Err(t!(auth_not_a_listed_way, answer = printable(&answer))),
    }
}

/// What a way already holds, as the list shows it, or `None` where it holds nothing.
///
/// Never the credential: an AWS account is reported only as signed in. `directory` is the state
/// directory the keys are kept in, where there is one.
fn held(way: Way, directory: Option<&Path>) -> Option<String> {
    match way {
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
        Way::ChatGpt => match crate::chatgpt::standing() {
            crate::chatgpt::Standing::SignedIn { .. } => Some(t!(auth_signed_in).to_string()),
            crate::chatgpt::Standing::Unusable(_) => Some(t!(chatgpt_held_unusable).to_string()),
            _ => None,
        },
        Way::Xai => match xai() {
            // What `status` says of it, so the list does not say less: when the access token expires
            // and the host each token is sent to.
            standing @ Standing::SignedIn(_) => Some(standing.said()),
            Standing::Unusable(_) => Some(t!(xai_held_unusable).to_string()),
            _ => None,
        },
        // The ids alone. Which host each key goes to is the provider block's to say.
        Way::Gateway => gateways_held(Keys::read(directory?)),
    }
}

/// What the gateway line of the list says of the keys stored: every id one is stored for, whether or
/// not a provider block names it.
fn gateways_held(stored: Result<Keys, keys::Unreadable>) -> Option<String> {
    match stored {
        Ok(stored) if stored.is_empty() => None,
        Ok(stored) => Some(t!(auth_gateway_held, ids = listed(stored.ids()))),
        Err(keys::Unreadable) => Some(t!(auth_gateway_held_unreadable).to_string()),
    }
}

/// Run one way with the arguments that followed its name.
fn sign_in(way: Way, rest: &[String]) -> ExitCode {
    match way {
        Way::Bedrock | Way::Import | Way::ChatGpt | Way::Xai if !rest.is_empty() => {
            unexpected(way, &rest[0])
        }
        Way::Bedrock => bedrock(),
        Way::Import => crate::import::providers(&[]),
        Way::ChatGpt => crate::chatgpt::login(),
        Way::Xai => crate::xai::login(),
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

/// The refusal of a word after a gateway's id. Not repeated, as for `login`: it is most likely
/// the key.
fn unexpected_gateway_word(verb: &str) -> ExitCode {
    fail(
        Ending::Argument,
        t!(
            auth_gateway_extra_word,
            command = format!("bravebot auth {verb} gateway")
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
                    path = managed_path(),
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

/// A service `auth login gateway` can store a key for.
struct Offer<'c> {
    provider: Cow<'c, Provider>,
    /// Whether no block of the configuration names the id. It is then one of the known names, and a
    /// session sends a key stored for it only once the person's settings name it.
    unconfigured: bool,
}

impl Offer<'_> {
    fn id(&self) -> &str {
        &self.provider.id
    }
}

/// What a key can be stored for: the gateways the configuration names, then each known id no block
/// of it names, each with the host a key for it is sent to.
///
/// No known id where the machine-level layer names the gateways. That layer replaces the blocks a
/// person wrote, so a block recorded for a known id in the person's file would not be read.
fn offered<'c>(providers: &'c [Provider], managed: &Managed) -> Vec<Offer<'c>> {
    let mut offers: Vec<Offer<'c>> = providers
        .iter()
        .filter(|provider| provider.bedrock.is_none())
        .map(|provider| Offer {
            provider: Cow::Borrowed(provider),
            unconfigured: false,
        })
        .collect();
    if managed.gateways().is_none() {
        offers.extend(
            known_ids()
                .filter(|id| !providers.iter().any(|provider| provider.id == *id))
                .filter_map(Provider::known)
                .map(|provider| Offer {
                    provider: Cow::Owned(provider),
                    unconfigured: true,
                }),
        );
    }
    offers
}

/// A path made printable, since a path may hold control characters a terminal would act on.
fn shown(path: &Path) -> String {
    printable(&path.display().to_string())
}

fn managed_path() -> String {
    shown(&bravebot_config::managed_file())
}

/// The refusal of an id that is not offered: the ids that are, and where the machine-level layer
/// names the gateways, the file that does, since that is why no known service is among them.
fn not_offered(offers: &[Offer<'_>], managed: &Managed) -> String {
    let ids = listed(offers.iter().map(Offer::id));
    match managed.gateways() {
        Some(_) => t!(
            auth_gateway_not_configured_managed,
            path = managed_path(),
            ids = ids
        ),
        None => t!(auth_gateway_not_configured, ids = ids),
    }
}

/// The line over the list of what a key can be stored for, which names the machine-level file where
/// that file names the gateways.
fn offers_heading(managed: &Managed) -> String {
    match managed.gateways() {
        Some(_) => t!(auth_gateways_heading_managed, path = managed_path()),
        None => t!(auth_gateways_heading).to_string(),
    }
}

/// What the gateway sign-in is handed besides the questions and the key.
struct Task<'a> {
    /// The gateways the configuration names, with the Bedrock accounts among them.
    providers: &'a [Provider],
    managed: &'a Managed,
    named: Option<&'a str>,
    /// Whether stdin and stderr are both terminals.
    terminal: bool,
    /// The state directory, `None` where nothing may be written.
    directory: Option<&'a Path>,
    lookup: &'a dyn Fn(&str) -> Option<String>,
}

/// Store a key for a service a key can be stored for, typed at the terminal with nothing drawn.
///
/// Never taken from the command line (CRED-24), where every process on the machine can read it and
/// the shell keeps it in its history. Every check that needs no answer is made before the key is
/// asked for, so nobody types a key only to be told it cannot be kept.
fn gateway(named: Option<&str>) -> ExitCode {
    // INCOG-7: a session that leaves nothing behind stores no key either.
    if bravebot_core::incognito::engaged() {
        return fail(Ending::Failed, t!(auth_gateway_not_while_incognito));
    }
    let providers = match gateways_configured(Config::from_env()) {
        Ok(providers) => providers,
        Err(problem) => return fail(Ending::Configuration, problem),
    };
    let managed = Managed::load();
    let directory = home::writable();
    let task = Task {
        providers: &providers,
        managed: &managed,
        named,
        // Both ends, as `import-providers` asks for them.
        terminal: std::io::stdin().is_terminal() && std::io::stderr().is_terminal(),
        directory: directory.as_deref(),
        lookup: &|name| std::env::var(name).ok(),
    };
    sign_in_gateway(
        &task,
        || Prompting::new(std::io::stdin().lock(), std::io::stderr()),
        bravebot_tui::hidden::read,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    )
    .code()
}

/// The gateways a configuration names, where it can be read.
///
/// A configuration that lacks only the Brave credentials is one that names no gateway and no AWS
/// account, since either makes them optional, so it names none here. Refusing it would stop the person
/// this sign-in is for, who holds a key and has configured nothing else.
fn gateways_configured(config: Result<Config, ConfigError>) -> Result<Vec<Provider>, ConfigError> {
    match config {
        Ok(config) => Ok(config.providers),
        Err(ConfigError::Missing(_) | ConfigError::Empty(_)) => Ok(Vec::new()),
        Err(other) => Err(other),
    }
}

/// One line to a stream. A failed write is ignored, since stdout and stderr are the only places one
/// could be reported.
fn tell(to: &mut impl Write, line: impl Display) {
    writeln!(to, "{line}").unwrap_or_default();
}

fn refused(err: &mut impl Write, ending: Ending, message: impl Display) -> Ending {
    ending.say(err, message);
    ending
}

/// What was answered about the service the key is for.
enum Asked<'o, 'c> {
    Nothing,
    /// A key is typed: none was stored, or it is to be replaced.
    Type(&'o Offer<'c>),
    /// The key stored is kept.
    Keep(&'o Offer<'c>),
}

/// What the checks that need no answer found.
struct Ready<'a> {
    managed: &'a Managed,
    offers: Vec<Offer<'a>>,
    named: Option<usize>,
    directory: &'a Path,
    stored: Keys,
}

/// The checks that need no answer, each of which refuses before anything is asked.
fn ready<'a>(task: &Task<'a>, err: &mut impl Write) -> Result<Ready<'a>, Ending> {
    let offers = offered(task.providers, task.managed);
    if offers.is_empty() {
        return Err(refused(
            err,
            Ending::Configuration,
            t!(auth_gateway_none_configured, path = managed_path()),
        ));
    }
    let named = match task.named {
        None => None,
        Some(id) => match offers.iter().position(|offer| offer.id() == id) {
            Some(at) => Some(at),
            // Not repeated back either: a word naming no gateway may be the key, typed where the id
            // goes by somebody who took it for the argument.
            None => {
                return Err(refused(
                    err,
                    Ending::Argument,
                    not_offered(&offers, task.managed),
                ));
            }
        },
    };
    if !task.terminal {
        return Err(refused(
            err,
            Ending::Argument,
            t!(auth_gateway_needs_a_terminal),
        ));
    }
    let Some(directory) = task.directory else {
        return Err(refused(err, Ending::Failed, t!(auth_gateway_no_home)));
    };
    let Ok(stored) = Keys::read(directory) else {
        return Err(refused(
            err,
            Ending::Failed,
            t!(
                auth_gateway_keys_unreadable,
                path = keys::file(directory).display()
            ),
        ));
    };
    Ok(Ready {
        managed: task.managed,
        offers,
        named,
        directory,
        stored,
    })
}

/// Ask which service the key is for and, where one is stored for it, whether to replace it.
///
/// A service whose block has to be recorded is checked before the question about replacing a key,
/// so nobody answers it and is then told the sign-in cannot be finished.
fn asked<'o, 'a, R: BufRead, W: Write>(
    ready: &'o Ready<'a>,
    settings: &Path,
    open_questions: impl FnOnce() -> Prompting<R, W>,
    err: &mut impl Write,
) -> Result<Asked<'o, 'a>, Ending> {
    let named = ready.named.and_then(|at| ready.offers.get(at));
    let mut asking = open_questions();
    let heading = offers_heading(ready.managed);
    let offer = match which_gateway(&mut asking, &ready.offers, named, &ready.stored, &heading) {
        Ok(Some(offer)) => offer,
        Ok(None) => return Ok(Asked::Nothing),
        Err(refusal) => return Err(refused(err, Ending::Argument, refusal)),
    };
    if offer.unconfigured
        && let Err(problem) = with_block(offer, settings)
    {
        return Err(refused(
            err,
            Ending::Failed,
            t!(
                auth_gateway_cannot_record,
                id = printable(offer.id()),
                problem = problem
            ),
        ));
    }
    Ok(match replaced(&mut asking, offer, &ready.stored) {
        true => Asked::Type(offer),
        false => Asked::Keep(offer),
    })
}

/// The sign-in of `bravebot auth login gateway`, over the places it reads and writes, so a test
/// runs the whole of it: the pick, the key, where it is stored and the block that names its id.
///
/// `open_questions` is called for the questions and what it returns is let go before the key is
/// read, so the lock on stdin is released before the terminal is read for the key.
fn sign_in_gateway<R: BufRead, W: Write>(
    task: &Task<'_>,
    open_questions: impl FnOnce() -> Prompting<R, W>,
    read_key: impl FnOnce(&str) -> std::io::Result<Option<Secret>>,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Ending {
    let ready = match ready(task, err) {
        Ok(ready) => ready,
        Err(ending) => return ending,
    };
    let settings = bravebot_config::user_settings_file(ready.directory);
    let offer = match asked(&ready, &settings, open_questions, err) {
        Ok(Asked::Type(offer)) => offer,
        Ok(Asked::Keep(offer)) => {
            return record_kept(offer, ready.directory, &settings, out, err);
        }
        Ok(Asked::Nothing) => return Ending::Done,
        Err(ending) => return ending,
    };

    // BACKEND-16 reads a variable before a stored key, so one set here is what a session sends, and
    // that is said before the key is typed rather than after it is kept.
    if let Some(variable) = variable_in_force(&offer.provider, |name| (task.lookup)(name)) {
        tell(err, t!(auth_gateway_variable_wins, variable = variable));
    }
    let question = t!(
        auth_gateway_key_question,
        id = printable(offer.id()),
        host = printable(offer.provider.host())
    );
    let key = match read_key(&question) {
        Ok(Some(key)) => key,
        Ok(None) => {
            tell(err, t!(auth_gateway_nothing_stored));
            return record_kept(offer, ready.directory, &settings, out, err);
        }
        Err(error) => {
            return refused(
                err,
                Ending::Failed,
                t!(auth_gateway_not_read, error = error.to_string()),
            );
        }
    };
    store_key(offer, key, &ready, &settings, out, err)
}

/// Store the key typed for `offer`, then record the block that names its id where one is needed.
fn store_key(
    offer: &Offer<'_>,
    key: Secret,
    ready: &Ready<'_>,
    settings: &Path,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Ending {
    let path = keys::file(ready.directory);
    // Read again: the copy from before the questions is as old as they took to answer, and writing
    // it back would restore a key another command forgot in the meantime.
    let Ok(mut stored) = Keys::read(ready.directory) else {
        return refused(
            err,
            Ending::Failed,
            t!(auth_gateway_keys_unreadable, path = path.display()),
        );
    };
    stored.insert(offer.id(), key);
    if let Err(error) = store(ready.directory, &stored) {
        return refused(
            err,
            Ending::Failed,
            t!(
                auth_gateway_not_stored,
                path = path.display(),
                error = error.to_string()
            ),
        );
    }
    // Said once the block is recorded where one is needed, since until then no session sends the key.
    if offer.unconfigured
        && let Err(problem) = record_block(offer, settings)
    {
        return not_recorded(err, offer, settings, problem);
    }
    let (id, host) = (printable(offer.id()), printable(offer.provider.host()));
    tell(
        out,
        match offer.unconfigured {
            false => t!(
                auth_gateway_stored,
                id = id,
                path = path.display(),
                host = host
            ),
            true => t!(
                auth_gateway_stored_and_recorded,
                id = id,
                path = path.display(),
                file = shown(settings),
                host = host
            ),
        },
    );
    Ending::Done
}

/// The person's settings with a block naming `offer`'s id added, or what stops one being added.
///
/// A file that names the id already is not added to, whether it names a block or something a block
/// is not: the entry a gateway needs is the person's to write there.
fn with_block(offer: &Offer<'_>, settings: &Path) -> Result<Destination, String> {
    let mut destination = Destination::open(settings).map_err(|why| unrecordable(why, settings))?;
    if destination.holds_gateway(offer.id()) {
        return Err(t!(
            auth_gateway_entry_not_a_service,
            file = shown(settings),
            id = printable(offer.id())
        ));
    }
    destination.add_gateway(Gateway::bare(offer.id(), &offer.provider.base_url));
    // Past the size the settings reader reads, which a destination says only once it is asked.
    destination
        .text()
        .map_err(|why| unrecordable(why, settings))?;
    Ok(destination)
}

/// Record a block naming `offer`'s id, as `"provider": {"<id>": {}}`, which is the reading that
/// reaches the host compiled in for the id and names no variable and no key.
fn record_block(offer: &Offer<'_>, settings: &Path) -> Result<(), String> {
    crate::import::write(&with_block(offer, settings)?, unrecordable)
}

fn recorded(offer: &Offer<'_>, settings: &Path) -> String {
    t!(
        auth_gateway_recorded,
        file = shown(settings),
        id = printable(offer.id()),
        host = printable(offer.provider.host())
    )
}

/// The failure to record a block for a key that is stored, which leaves the key as it is, names the
/// entry to add by hand and never repeats the key.
fn not_recorded(
    err: &mut impl Write,
    offer: &Offer<'_>,
    settings: &Path,
    problem: String,
) -> Ending {
    refused(
        err,
        Ending::Failed,
        t!(
            auth_gateway_not_recorded,
            id = printable(offer.id()),
            entry = format!(r#""provider": {{"{}": {{}}}}"#, printable(offer.id())),
            file = shown(settings),
            problem = problem
        ),
    )
}

/// Record the block for a key that was already stored, where no block names its id.
///
/// A key stored for a known id that no block names is sent by no session, so keeping it, or
/// replacing it and not typing another, still records the block, which leaves nothing to add by
/// hand. Nothing is recorded for an id no key is stored for.
fn record_kept(
    offer: &Offer<'_>,
    directory: &Path,
    settings: &Path,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Ending {
    if !offer.unconfigured {
        return Ending::Done;
    }
    // Read again, since another command may have forgotten the key meanwhile.
    match Keys::read(directory) {
        Ok(stored) if stored.get(offer.id()).is_some() => {}
        Ok(_) => return Ending::Done,
        Err(keys::Unreadable) => {
            return refused(
                err,
                Ending::Failed,
                t!(
                    auth_gateway_keys_unreadable,
                    path = keys::file(directory).display()
                ),
            );
        }
    }
    match record_block(offer, settings) {
        Ok(()) => {
            tell(out, recorded(offer, settings));
            Ending::Done
        }
        Err(problem) => not_recorded(err, offer, settings, problem),
    }
}

/// The service the key is for, `None` where nothing was picked, or the refusal of an answer that
/// names none of those listed.
///
/// One offered, or one named, is not asked about.
fn which_gateway<'o, 'c, R: BufRead, W: Write>(
    asking: &mut Prompting<R, W>,
    offers: &'o [Offer<'c>],
    named: Option<&'o Offer<'c>>,
    stored: &Keys,
    heading: &str,
) -> Result<Option<&'o Offer<'c>>, String> {
    let offer = match (named, offers) {
        (Some(named), _) => named,
        (None, [only]) => only,
        (None, _) => {
            asking.say(heading);
            // In characters, which is what the padding counts; an id is free text in settings.
            let width = offers.iter().map(|offer| offer.id().chars().count()).max();
            for (number, offer) in (1..).zip(offers) {
                let held = match stored.get(offer.id()) {
                    Some(_) => format!(" ({})", t!(auth_gateway_key_stored)),
                    None => String::new(),
                };
                asking.say(&format!(
                    "  {number}. {:<width$}  {}{held}",
                    printable(offer.id()),
                    printable(offer.provider.host()),
                    width = width.unwrap_or(0)
                ));
            }
            let answer = match asking.answer(t!(auth_which_gateway)) {
                Some(answer) if !answer.is_empty() => answer,
                _ => return Ok(None),
            };
            let picked = crate::plain::numbered_row(&answer, offers)
                .or_else(|| offers.iter().find(|offer| offer.id() == answer));
            match picked {
                Some(offer) => offer,
                None => {
                    return Err(t!(auth_not_a_listed_gateway, answer = printable(&answer)));
                }
            }
        }
    };
    Ok(Some(offer))
}

/// Whether a key is to be typed for `offer`: where one is stored already, only on a yes, because
/// the one replaced is gone once the file is written.
fn replaced<R: BufRead, W: Write>(
    asking: &mut Prompting<R, W>,
    offer: &Offer<'_>,
    stored: &Keys,
) -> bool {
    if stored.get(offer.id()).is_none() {
        return true;
    }
    let id = printable(offer.id());
    asking.say(&match offer.unconfigured {
        false => t!(auth_gateway_key_held, id = id),
        true => t!(auth_gateway_key_held_unrecorded, id = id),
    });
    asking.ask(&[], t!(auth_gateway_replace)) == Decision::Approve
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
/// Written beside the file and renamed over it, as the sign-ins are, so a write that stops part way
/// leaves the keys that were there. Created by the call that gives them their protection,
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

/// How one sign-in stands, as `bravebot auth status` reports it.
///
/// Every text in it is a count or a fixed sentence: nothing here is built from a credential.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Standing {
    /// Usable now.
    SignedIn(String),
    /// Nothing is there, or what was there has lapsed, and signing in fixes it.
    NotSignedIn(String),
    /// Something is there and signing in again would not make it usable.
    Unusable(String),
    /// The configuration cannot be read, so the question has no answer.
    Misconfigured(String),
}

impl Standing {
    fn usable(&self) -> bool {
        matches!(self, Standing::SignedIn(_))
    }

    fn said(&self) -> String {
        match self {
            Standing::SignedIn(detail) => t!(auth_status_signed_in, detail = detail),
            Standing::NotSignedIn(detail) => t!(auth_status_not_signed_in, detail = detail),
            Standing::Unusable(detail) | Standing::Misconfigured(detail) => {
                t!(auth_status_unusable, detail = detail)
            }
        }
    }
}

/// One sign-in with the name it is reported under: a way, and the account or gateway under it.
type Line = (String, Standing);

/// `bravebot auth status [way]`: one line per sign-in, and an exit status that says whether it is
/// usable.
///
/// Reads what a turn reads and writes nothing. With a way named, every sign-in under it has to be
/// usable. With none named, one is enough, since nobody holds all of them and a script asking
/// whether bravebot can run is asking about the set.
fn status(args: &[String]) -> ExitCode {
    let Some((word, rest)) = args.split_first() else {
        return report(
            &[Way::Bedrock, Way::Gateway, Way::ChatGpt, Way::Xai],
            None,
            false,
        );
    };
    if word.starts_with('-') {
        return fail(
            Ending::Argument,
            t!(cli_unknown_option, flag = printable(option_name(word))),
        );
    }
    let Some(way) = Way::named(word) else {
        return refused_with_the_forms(t!(auth_unknown_way, way = printable(word)));
    };
    if way == Way::Import {
        return fail(Ending::Argument, t!(auth_status_import));
    }
    let takes = usize::from(way == Way::Gateway);
    if let Some(flag) = rest.iter().find(|word| word.starts_with('-')) {
        return fail(
            Ending::Argument,
            t!(cli_unknown_option, flag = printable(option_name(flag))),
        );
    }
    if way == Way::Gateway && rest.len() > takes {
        return unexpected_gateway_word("status");
    }
    if let Some(extra) = rest.get(takes) {
        return fail(
            Ending::Argument,
            t!(
                auth_unexpected_argument,
                command = format!("bravebot auth status {}", way.name()),
                argument = printable(extra)
            ),
        );
    }
    report(&[way], rest.first().map(String::as_str), true)
}

/// Print the standing of each of `ways`, then end as [`verdict`] says.
fn report(ways: &[Way], gateway_id: Option<&str>, named: bool) -> ExitCode {
    let mut lines: Vec<Line> = Vec::new();
    for way in ways {
        match way {
            Way::Bedrock => lines.extend(bedrock_standings(
                bedrock_config(&Settings::load(), &Managed::load()),
                inherited_profile().as_deref(),
                Backend::session_of,
            )),
            Way::Gateway => {
                let stored = home::directory().map_or(Ok(Keys::default()), |dir| Keys::read(&dir));
                match gateway_standings(
                    Config::from_env(),
                    &Managed::load(),
                    &stored,
                    gateway_id,
                    |name| std::env::var(name).ok(),
                ) {
                    Ok(found) => lines.extend(found),
                    Err((ending, refusal)) => return fail(ending, refusal),
                }
            }
            Way::ChatGpt => lines.push(("chatgpt".into(), chatgpt())),
            Way::Xai => lines.push(("xai".into(), xai())),
            Way::Import => {}
        }
    }
    for (name, standing) in &lines {
        println!("{name}: {}", standing.said());
    }
    match verdict(&lines, named) {
        Ending::Done => ExitCode::SUCCESS,
        ending => fail(
            ending,
            if named {
                t!(auth_status_not_all_usable)
            } else {
                t!(auth_status_none_usable)
            },
        ),
    }
}

/// How the run ends: done where the sign-ins asked about are usable, else the ending CLI-6 gives
/// the failure. A configuration that could not be read is that ending, and anything else is a
/// plain failure.
fn verdict(lines: &[Line], named: bool) -> Ending {
    let usable = if named {
        lines.iter().all(|(_, standing)| standing.usable())
    } else {
        lines.iter().any(|(_, standing)| standing.usable())
    };
    if usable {
        Ending::Done
    } else if lines
        .iter()
        .any(|(_, standing)| matches!(standing, Standing::Misconfigured(_)))
    {
        Ending::Configuration
    } else {
        Ending::Failed
    }
}

/// The stored ChatGPT sign-in, read from the file a turn would read.
fn chatgpt() -> Standing {
    use crate::chatgpt::Standing as Stored;
    match crate::chatgpt::standing() {
        Stored::SignedIn { expires } => {
            Standing::SignedIn(t!(auth_status_chatgpt_expires, expires = expires))
        }
        Stored::NotSignedIn => Standing::NotSignedIn(t!(auth_status_chatgpt_none).into()),
        Stored::Nowhere => Standing::NotSignedIn(t!(auth_status_chatgpt_nowhere).into()),
        Stored::Unusable(reason) => Standing::Unusable(reason),
    }
}

/// The stored xAI sign-in, read from the file a turn would read.
fn xai() -> Standing {
    use crate::xai::Standing as Stored;
    match crate::xai::standing() {
        Stored::SignedIn { expires } => {
            Standing::SignedIn(t!(auth_status_xai_expires, expires = expires))
        }
        Stored::NotSignedIn => Standing::NotSignedIn(t!(auth_status_xai_none).into()),
        Stored::Nowhere => Standing::NotSignedIn(t!(auth_status_xai_nowhere).into()),
        Stored::Unusable(reason) => Standing::Unusable(reason),
    }
}

/// Each AWS account the configuration names, asked whether its session gives credentials.
fn bedrock_standings(
    config: Result<Config, ConfigError>,
    inherited: Option<&str>,
    session: impl Fn(Option<&str>) -> Result<(), CredentialError>,
) -> Vec<Line> {
    let config = match config {
        Ok(config) => config,
        Err(problem) => {
            return vec![(
                "bedrock".into(),
                Standing::Misconfigured(printable(&problem.to_string())),
            )];
        }
    };
    let accounts = accounts(&config, inherited);
    if accounts.is_empty() {
        return vec![(
            "bedrock".into(),
            Standing::NotSignedIn(t!(
                auth_no_aws_account,
                region = env_var::AWS_REGION,
                tiers = listed(Tier::ALL.into_iter().map(Tier::env_var))
            )),
        )];
    }
    accounts
        .into_iter()
        .map(|(profile, account)| {
            let name = match profile {
                Some(profile) => format!("bedrock {}", printable(&profile)),
                None => "bedrock".to_string(),
            };
            (name, bedrock_standing(session(account.profile.as_deref())))
        })
        .collect()
}

/// An AWS session as a standing: a lapsed one is signed in again, and a missing CLI or a profile it
/// does not have is not fixed by signing in.
fn bedrock_standing(session: Result<(), CredentialError>) -> Standing {
    // Fixed sentences only: the AWS CLI's own text names accounts and roles, and the profiles it
    // lists can embed an account id, so neither is copied onto a line.
    match session {
        Ok(()) => Standing::SignedIn(t!(auth_status_bedrock_good).into()),
        Err(CredentialError::Refused { .. }) => {
            Standing::NotSignedIn(t!(auth_status_bedrock_lapsed).into())
        }
        Err(CredentialError::NoSuchProfile { .. }) => {
            Standing::Unusable(t!(auth_status_bedrock_no_profile).into())
        }
        Err(CredentialError::NotInstalled) => {
            Standing::Unusable(t!(auth_status_bedrock_no_cli).into())
        }
        Err(CredentialError::Undecodable { .. }) => {
            Standing::Unusable(t!(auth_status_bedrock_undecodable).into())
        }
    }
}

/// Each gateway a provider block names, or the one `named`, with whether a turn would find a key
/// for it, and where.
///
/// A known service no block names is asked about only where it is named, or where a key is stored for
/// it: a turn sends none, so it is not signed in, and signing in again records the block. Without a
/// name, every other known service is left out, since nobody holds a key for all of them.
///
/// A refusal where the id is one `auth login gateway` does not offer, as it refuses it.
fn gateway_standings(
    config: Result<Config, ConfigError>,
    managed: &Managed,
    stored: &Result<Keys, keys::Unreadable>,
    named: Option<&str>,
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<Vec<Line>, (Ending, String)> {
    let config = match config {
        Ok(config) => config,
        Err(problem) => {
            return Ok(vec![(
                "gateway".into(),
                Standing::Misconfigured(printable(&problem.to_string())),
            )]);
        }
    };
    let offers = offered(&config.providers, managed);
    if offers.is_empty() {
        let none = t!(auth_gateway_none_configured, path = managed_path());
        return match named {
            Some(_) => Err((Ending::Configuration, none)),
            None => Ok(vec![("gateway".into(), Standing::NotSignedIn(none))]),
        };
    }
    if let Some(id) = named
        && !offers.iter().any(|offer| offer.id() == id)
    {
        // Not repeated back, as in `auth login gateway`: the word may be a key.
        return Err((Ending::Argument, not_offered(&offers, managed)));
    }
    let key_stored = |offer: &Offer<'_>| {
        stored
            .as_ref()
            .is_ok_and(|stored| stored.get(offer.id()).is_some())
    };
    let asked_about: Vec<&Offer<'_>> = offers
        .iter()
        .filter(|offer| match named {
            Some(id) => offer.id() == id,
            None => !offer.unconfigured || key_stored(offer),
        })
        .collect();
    if asked_about.is_empty() {
        let standing = match stored {
            Err(keys::Unreadable) => Standing::Unusable(t!(auth_gateway_held_unreadable).into()),
            Ok(_) => Standing::NotSignedIn(t!(auth_status_gateway_none).into()),
        };
        return Ok(vec![("gateway".into(), standing)]);
    }
    Ok(asked_about
        .into_iter()
        .map(|offer| {
            let id = printable(offer.id());
            let standing = match offer.unconfigured {
                true => unconfigured_standing(&id, stored, key_stored(offer)),
                false => configured_standing(&offer.provider, stored.is_err(), &lookup),
            };
            (format!("gateway {id}"), standing)
        })
        .collect())
}

/// How a gateway a provider block names stands: whether a turn would find a key for it, and where.
fn configured_standing(
    provider: &Provider,
    keys_unreadable: bool,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Standing {
    match provider.credential_and_source(lookup) {
        (Credential::Token(_), Some(Source::Stored)) => {
            Standing::SignedIn(t!(doctor_gateway_token_stored).into())
        }
        (Credential::Token(_), _) => Standing::SignedIn(t!(doctor_gateway_token).into()),
        (Credential::NotNeeded, _) => {
            Standing::SignedIn(t!(doctor_gateway_token_not_needed).into())
        }
        (Credential::Absent, _) if keys_unreadable => {
            Standing::Unusable(t!(auth_gateway_held_unreadable).into())
        }
        (Credential::Absent, _) => Standing::NotSignedIn(t!(
            doctor_gateway_token_absent,
            id = printable(&provider.id)
        )),
    }
}

/// How a known service no block names stands. A turn sends no key to it, whether or not one is
/// stored, so it is never signed in, and signing in again is what records the block.
fn unconfigured_standing(
    id: &str,
    stored: &Result<Keys, keys::Unreadable>,
    key_stored: bool,
) -> Standing {
    match (stored, key_stored) {
        (Err(keys::Unreadable), _) => Standing::Unusable(t!(auth_gateway_held_unreadable).into()),
        (Ok(_), true) => Standing::NotSignedIn(t!(auth_status_gateway_unrecorded, id = id)),
        (Ok(_), false) => Standing::NotSignedIn(t!(auth_status_gateway_none_stored, id = id)),
    }
}

/// Several names on one line, in the order given.
fn listed<'n>(names: impl Iterator<Item = &'n str>) -> String {
    names.map(printable).collect::<Vec<_>>().join(", ")
}

/// `bravebot auth logout <way>`. A gateway, ChatGPT and xAI keep a credential of their own, so they
/// are the ways signed out of here; the other two say where what they left is kept.
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
        (Way::Gateway, [_, _, ..]) => return unexpected_gateway_word("logout"),
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
        Way::Bedrock => fail(Ending::Argument, t!(auth_logout_bedrock)),
        Way::Import => fail(Ending::Argument, t!(auth_logout_import)),
        Way::Gateway => forget_gateway(rest.first().map(String::as_str)),
        Way::ChatGpt => crate::chatgpt::logout(),
        Way::Xai => crate::xai::logout(),
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
    let host = host_a_key_works_at(&Config::from_env(), &id);
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

/// The host a key stored for `id` is good at until whoever issued it revokes it, which forgetting it
/// here is not: the host the block that names the id states, or for a known service no block names
/// the host compiled in for it.
///
/// Nothing for an id that is neither, such as a gateway whose block has since been taken out.
fn host_a_key_works_at(config: &Result<Config, ConfigError>, id: &str) -> Option<String> {
    let block = config.as_ref().ok().and_then(|config| {
        config
            .providers
            .iter()
            .find(|provider| provider.bedrock.is_none() && provider.id == id)
    });
    match block {
        Some(provider) => Some(printable(provider.host())),
        None => Provider::known(id).map(|provider| printable(provider.host())),
    }
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
    use std::path::PathBuf;

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

    fn gateway_held(way: Way) -> Option<String> {
        (way == Way::Gateway).then(|| "a key held for openai".to_string())
    }

    /// CLI-18: every way is on the list by the word that names it, and only one that holds
    /// something says what it holds.
    #[test]
    fn every_way_is_listed_and_only_a_held_one_says_what_it_holds() {
        let (said, _) = asked("\n", &gateway_held);

        let lines: Vec<&str> = said.lines().collect();
        for (number, way) in (1..).zip(Way::ALL) {
            let line = lines
                .iter()
                .find(|line| line.starts_with(&format!("  {number}. {}", way.name())))
                .unwrap_or_else(|| panic!("{} is not listed: {said}", way.name()));
            assert_eq!(
                line.contains("a key held for openai"),
                way == Way::Gateway,
                "{} says the wrong thing about what it holds: {line}",
                way.name()
            );
        }
    }

    /// CLI-18: the gateway line says a key is a way to sign in to the services bravebot knows, and
    /// names each from the table a session reaches them by, so a service added there is on the line.
    #[test]
    fn the_gateway_line_names_every_service_bravebot_knows() {
        let (said, _) = asked("\n", &nothing_held);
        let line = said
            .lines()
            .find(|line| line.starts_with("  3. gateway"))
            .unwrap_or_else(|| panic!("gateway is not listed: {said}"));
        assert!(known_ids().count() > 0);
        for id in known_ids() {
            assert!(line.contains(id), "{id} is not on the gateway line: {line}");
        }
        assert!(line.contains("provider block"), "{line}");
    }

    /// CLI-18: what the gateway line says is held is every id a key is stored for in the state
    /// directory, read from the file as the list reads it with no configuration consulted, so a key
    /// stored for a known service that no block names is shown, and nothing stored shows nothing.
    #[test]
    fn the_gateway_line_shows_a_key_stored_for_a_service_no_block_names() {
        let scratch = Scratch::new("cli-auth-held-line");
        let directory = scratch.0.join("state");
        let held_in = |directory: &Path| held(Way::Gateway, Some(directory));

        std::fs::create_dir_all(&directory).expect("a state directory");
        assert_eq!(held_in(&directory), None, "nothing is stored");
        assert_eq!(
            held(Way::Gateway, None),
            None,
            "there is no state directory"
        );

        store(&directory, &placeholder_keys(&["openai", "anthropic"])).expect("stored");
        let line = held_in(&directory).expect("keys are held");
        assert!(
            line.contains("openai") && line.contains("anthropic"),
            "{line}"
        );
        assert!(!line.contains("placeholder-gateway-key"), "{line}");
        assert!(
            !directory.join("settings.json").exists(),
            "the line was read with a block written"
        );

        std::fs::write(keys::file(&directory), "not a file of keys").expect("spoil the keys");
        assert!(
            held_in(&directory)
                .expect("said")
                .contains("cannot be read")
        );
    }

    /// CLI-18: a way is picked by its number or by its name, nothing typed picks nothing, and an
    /// answer naming no way listed is refused rather than guessed at.
    #[test]
    fn a_way_is_picked_by_its_number_or_its_name() {
        for (answers, expected) in [
            ("1\n", Some(Way::Bedrock)),
            ("2\n", Some(Way::Import)),
            ("3\n", Some(Way::Gateway)),
            ("4\n", Some(Way::ChatGpt)),
            ("5\n", Some(Way::Xai)),
            ("bedrock\n", Some(Way::Bedrock)),
            ("gateway\n", Some(Way::Gateway)),
            ("ChatGPT\n", Some(Way::ChatGpt)),
            ("XAI\n", Some(Way::Xai)),
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
        for refused in ["0\n", "6\n", "aws\n", "leo\n"] {
            let (said, picked) = asked(refused, &nothing_held);
            let complaint = picked.expect_err(refused);
            assert!(
                complaint.contains(refused.trim()),
                "{refused:?} was refused without being named: {complaint} {said}"
            );
        }
    }

    /// CLI-18: what a way holds is asked once, so the list shows the state of the sign-in it names
    /// and a line that looked twice could disagree with itself.
    #[test]
    fn what_each_way_holds_is_asked_once() {
        let looked = std::cell::Cell::new(0);
        let counted = |way| {
            looked.set(looked.get() + 1);
            gateway_held(way)
        };
        let (said, picked) = asked("\n", &counted);
        assert_eq!(picked, Ok(None), "{said}");
        assert_eq!(looked.get(), Way::ALL.len(), "a way was looked at twice");
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

    /// The gateways `providers` configure, as the sign-in offers them.
    fn blocks(providers: &[Provider]) -> Vec<Offer<'_>> {
        providers
            .iter()
            .map(|provider| Offer {
                provider: Cow::Borrowed(provider),
                unconfigured: false,
            })
            .collect()
    }

    /// What the gateway question said and which id a key is to be typed for, for `answers` typed in
    /// turn: `None` where nothing was picked or a key held is kept.
    fn gateway_asked(
        answers: &str,
        gateways: &[Provider],
        named: Option<&str>,
        stored: &Keys,
    ) -> (String, Result<Option<String>, String>) {
        let offers = blocks(gateways);
        let named = named.and_then(|id| offers.iter().find(|offer| offer.id() == id));
        let mut said = Vec::new();
        let picked = {
            let mut asking = Prompting::new(answers.as_bytes(), &mut said);
            which_gateway(
                &mut asking,
                &offers,
                named,
                stored,
                t!(auth_gateways_heading),
            )
            .map(|picked| picked.filter(|offer| replaced(&mut asking, offer, stored)))
        };
        let picked = picked.map(|picked| picked.map(|offer| offer.id().to_string()));
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

    /// CLI-18: a configuration that lacks only the Brave credentials names no gateway, which is the
    /// state of a person who holds a key and nothing else, so the sign-in offers the known services
    /// to it. Any other fault in the configuration is still refused.
    #[test]
    fn a_configuration_lacking_only_credentials_names_no_gateway_and_another_fault_is_refused() {
        assert!(
            matches!(Config::from_lookup(|_| None), Err(ConfigError::Missing(_))),
            "a configuration with nothing in it is no longer one that lacks credentials"
        );
        for lacking in [
            ConfigError::Missing("SERVICES_KEY_AICHAT"),
            ConfigError::Empty("SERVICES_KEY_AICHAT"),
        ] {
            assert_eq!(
                gateways_configured(Err(lacking)).map(|found| found.len()),
                Ok(0)
            );
        }
        let invalid = || ConfigError::InvalidEndpoint {
            value: "ftp://elsewhere".to_string(),
        };
        assert_eq!(
            gateways_configured(Err(invalid())).map(|found| found.len()),
            Err(invalid())
        );
        let config = configured(TWO_GATEWAYS_WITH_A_KNOWN_ID, &Managed::default());
        let found = gateways_configured(Ok(config)).expect("readable");
        let ids: Vec<&str> = found.iter().map(|provider| provider.id.as_str()).collect();
        assert_eq!(ids, ["openrouter", "work"]);
    }

    /// CLI-18: forgetting a key names the host it still works at: the one its block states, the one
    /// compiled in for a known service no block names, and none for an id that is neither.
    #[test]
    fn a_forgotten_key_names_the_host_it_still_works_at() {
        let configuration = Ok(configured(
            TWO_GATEWAYS_WITH_A_KNOWN_ID,
            &Managed::default(),
        ));
        let host = |id: &str| host_a_key_works_at(&configuration, id);
        assert_eq!(host("work").as_deref(), Some("gateway.work.example"));
        assert_eq!(host("openrouter").as_deref(), Some("openrouter.example"));
        assert_eq!(host("openai").as_deref(), Some("api.openai.com"));
        assert_eq!(host("anthropic").as_deref(), Some("api.anthropic.com"));
        assert_eq!(host("taken-out"), None);

        let unreadable = Err(ConfigError::Missing("SERVICES_KEY_AICHAT"));
        assert_eq!(
            host_a_key_works_at(&unreadable, "openai").as_deref(),
            Some("api.openai.com")
        );
        assert_eq!(host_a_key_works_at(&unreadable, "work"), None);
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

    /// The placeholder typed as a key. Never a real one.
    const A_TYPED_KEY: &str = "sk-test-placeholder-not-a-key";

    /// A placeholder already stored for an id, which a typed key replaces.
    const AN_OLD_KEY: &str = "sk-test-old-placeholder-not-a-key";

    /// A placeholder the process has exported under the name a block gives for its credential.
    const AN_EXPORTED_KEY: &str = "sk-test-exported-placeholder-not-a-key";

    /// What a run of the sign-in is given, besides the state directory it makes.
    #[derive(Default)]
    struct Given<'a> {
        /// The person's own settings file, which the configuration is read from too.
        settings: Option<&'a str>,
        /// The file of stored keys.
        keys: Option<&'a str>,
        /// The machine-level file.
        managed: Option<&'a str>,
        /// The id typed after `gateway`.
        named: Option<&'a str>,
        /// What is typed to the questions, a line each.
        answers: &'a str,
        /// What is typed when the key is asked for, `None` for Escape.
        typed: Option<&'a str>,
        /// Something that happens while the key is typed.
        meanwhile: Option<&'a dyn Fn(&Path)>,
        /// Variables the process has exported.
        exported: &'a [(&'a str, &'a str)],
        /// Stdin or stderr is not a terminal.
        no_terminal: bool,
    }

    /// What a run said and left behind. Holds the scratch directory until it is dropped.
    struct Run {
        ending: Ending,
        out: String,
        err: String,
        asked: String,
        /// The question the key was asked with, where it was asked.
        key_question: Option<String>,
        directory: PathBuf,
        _scratch: Scratch,
    }

    impl Run {
        fn settings(&self) -> PathBuf {
            bravebot_config::user_settings_file(&self.directory)
        }

        fn settings_text(&self) -> Option<String> {
            std::fs::read_to_string(self.settings()).ok()
        }

        fn keys(&self) -> Keys {
            Keys::read(&self.directory).expect("a file of keys")
        }

        /// Nothing the run printed, asked or wrote into settings holds a key typed or stored.
        fn holds_no_key(&self) {
            let settings = self.settings_text().unwrap_or_default();
            for (stream, text) in [
                ("stdout", &self.out),
                ("stderr", &self.err),
                ("questions", &self.asked),
                ("settings", &settings),
            ] {
                for key in [A_TYPED_KEY, AN_OLD_KEY, AN_EXPORTED_KEY] {
                    assert!(!text.contains(key), "{stream} holds a key: {text}");
                }
            }
            if let Some(question) = &self.key_question {
                assert!(!question.contains(A_TYPED_KEY), "{question}");
            }
        }
    }

    /// The configuration a session started with `settings` and `managed` would read, as far as the
    /// sign-in looks at it: the gateways, which the managed layer replaces where it names any.
    fn configured(settings: &str, managed: &Managed) -> Config {
        let mut config = Config::from_lookup(|name| match name {
            env_var::USE_BEDROCK => Some("1".into()),
            env_var::AWS_REGION => Some("us-east-1".into()),
            _ => None,
        })
        .expect("a configuration");
        config.providers = match managed.gateways() {
            Some(gateways) => gateways.to_vec(),
            None => Settings::parse(settings).providers().to_vec(),
        };
        config
    }

    /// Run the whole sign-in with no terminal: the pick, the key, where it is stored and the block
    /// that names its id.
    fn sign_in(name: &str, given: Given<'_>) -> Run {
        let scratch = Scratch::new(name);
        let directory = scratch.0.join("state");
        std::fs::create_dir_all(&directory).expect("a state directory");
        if let Some(text) = given.settings {
            std::fs::write(bravebot_config::user_settings_file(&directory), text)
                .expect("settings");
        }
        if let Some(text) = given.keys {
            std::fs::write(keys::file(&directory), text).expect("keys");
        }
        let managed = match given.managed {
            Some(text) => {
                let file = scratch.0.join("managed.json");
                std::fs::write(&file, text).expect("a machine-level file");
                Managed::at(&file)
            }
            None => Managed::default(),
        };
        let config = configured(given.settings.unwrap_or(""), &managed);
        let task = Task {
            providers: &config.providers,
            managed: &managed,
            named: given.named,
            terminal: !given.no_terminal,
            directory: Some(&directory),
            lookup: &|name| {
                given
                    .exported
                    .iter()
                    .find(|(exported, _)| *exported == name)
                    .map(|(_, value)| value.to_string())
            },
        };
        let (mut out, mut err, mut asked) = (Vec::new(), Vec::new(), Vec::new());
        let mut key_question = None;
        let ending = sign_in_gateway(
            &task,
            || Prompting::new(given.answers.as_bytes(), &mut asked),
            |question| {
                key_question = Some(question.to_string());
                if let Some(meanwhile) = given.meanwhile {
                    meanwhile(&directory);
                }
                Ok(given.typed.map(Secret::new))
            },
            &mut out,
            &mut err,
        );
        let text = |bytes: Vec<u8>| String::from_utf8(bytes).expect("utf-8");
        Run {
            ending,
            out: text(out),
            err: text(err),
            asked: text(asked),
            key_question,
            directory: directory.clone(),
            _scratch: scratch,
        }
    }

    /// The listed rows of a question, as `(id, host)`.
    fn rows(asked: &str) -> Vec<(String, String)> {
        asked
            .lines()
            .filter_map(|line| {
                let mut words = line.split_whitespace();
                words.next()?.strip_suffix('.')?.parse::<usize>().ok()?;
                Some((words.next()?.to_string(), words.next()?.to_string()))
            })
            .collect()
    }

    /// CLI-18 and BACKEND-17: a known service no block names is offered with the host a session
    /// sends it to, the key typed for it is stored under its id, and the settings file the sign-in
    /// leaves holds exactly the entry that makes a session send it. Picked by id, by number, or named.
    #[test]
    fn a_known_service_no_block_names_is_listed_and_its_key_is_stored_and_its_block_recorded() {
        let number = known_ids().position(|id| id == "openai").expect("known") + 1;
        let by_number = format!("{number}\n");
        for (how, named, answers) in [
            ("by id", None, "openai\n"),
            ("by number", None, by_number.as_str()),
            ("named", Some("openai"), ""),
        ] {
            let run = sign_in(
                "cli-auth-known-recorded",
                Given {
                    named,
                    answers,
                    typed: Some(A_TYPED_KEY),
                    ..Given::default()
                },
            );
            assert_eq!(run.ending, Ending::Done, "{how}: {}", run.err);
            run.holds_no_key();

            if named.is_none() {
                let listed = rows(&run.asked);
                let hosts: Vec<(String, String)> = known_ids()
                    .map(|id| {
                        let host = Provider::known(id).expect("known").host().to_string();
                        (id.to_string(), host)
                    })
                    .collect();
                assert_eq!(listed, hosts, "{how}: {}", run.asked);
            } else {
                assert!(run.asked.is_empty(), "a named service was asked about");
            }

            let stored = run.keys();
            assert_eq!(stored.ids().collect::<Vec<_>>(), ["openai"], "{how}");
            assert_eq!(
                stored.get("openai").map(Secret::expose),
                Some(A_TYPED_KEY),
                "{how}"
            );

            let written = run.settings_text().expect("the settings file was written");
            let value: serde_json::Value = serde_json::from_str(&written).expect("json");
            assert_eq!(
                value,
                serde_json::json!({"provider": {"openai": {}}}),
                "{how}: {written}"
            );
            // Through the reader a session uses: the entry is the service at the host compiled in,
            // and the key stored for its id is the credential it finds.
            let mut read = Settings::parse(&written).providers().to_vec();
            assert_eq!(read.len(), 1, "{how}: {written}");
            let mut openai = read.remove(0);
            assert_eq!(openai.id, "openai");
            assert_eq!(openai.host(), "api.openai.com", "{how}");
            assert_eq!(
                openai.base_url,
                Provider::known("openai").expect("known").base_url
            );
            openai.stored_key = stored.get("openai").cloned();
            match openai.credential_and_source(|_| None) {
                (Credential::Token(token), Some(Source::Stored)) => {
                    assert_eq!(token.expose(), A_TYPED_KEY, "{how}");
                }
                _ => panic!("{how}: the stored key is not the credential of the recorded block"),
            }

            assert!(run.out.contains("api.openai.com"), "{how}: {}", run.out);
            assert!(run.out.contains("settings.json"), "{how}: {}", run.out);
            assert_eq!(run.out.lines().count(), 1, "{how}: {}", run.out);
            let question = run.key_question.as_deref().expect("the key was asked for");
            assert!(
                question.contains("openai") && question.contains("api.openai.com"),
                "{question}"
            );
        }
    }

    /// CLI-18 and BACKEND-17: the gateways configured come first, a known service some block names
    /// is not listed again, and the one that is listed keeps the host its block states.
    #[test]
    fn the_configured_gateways_are_listed_first_and_a_known_id_is_listed_once_with_its_blocks_host()
    {
        let config = configured(TWO_GATEWAYS_WITH_A_KNOWN_ID, &Managed::default());
        let offers = offered(&config.providers, &Managed::default());
        let listed: Vec<(&str, &str, bool)> = offers
            .iter()
            .map(|offer| (offer.id(), offer.provider.host(), offer.unconfigured))
            .collect();
        let known: Vec<(String, String)> = known_ids()
            .filter(|id| *id != "openrouter")
            .map(|id| {
                let host = Provider::known(id).expect("known").host().to_string();
                (id.to_string(), host)
            })
            .collect();
        let mut expected = vec![
            ("openrouter", "openrouter.example", false),
            ("work", "gateway.work.example", false),
        ];
        expected.extend(
            known
                .iter()
                .map(|(id, host)| (id.as_str(), host.as_str(), true)),
        );
        assert_eq!(listed, expected);
    }

    /// Two gateways in a settings file, one of them under a known id and stating its own host.
    const TWO_GATEWAYS_WITH_A_KNOWN_ID: &str = r#"{"provider": {
        "openrouter": {"env": ["OPENROUTER_API_KEY"],
            "options": {"baseURL": "https://openrouter.example/api/v1"}},
        "work": {"options": {"baseURL": "https://gateway.work.example/v1"}}
    }}"#;

    /// CLI-18: a block with the id picked is the gateway, whatever the table says of the id: its key
    /// is stored, the settings file is left byte for byte as it was, and nothing is said of a block.
    #[test]
    fn a_block_with_the_same_id_keeps_its_host_and_nothing_is_recorded() {
        let settings = r#"{"provider": {"openai": {"options": {"baseURL": "https://proxy.example.invalid/v1"}}}}"#;
        let run = sign_in(
            "cli-auth-known-with-a-block",
            Given {
                settings: Some(settings),
                answers: "openai\n",
                typed: Some(A_TYPED_KEY),
                ..Given::default()
            },
        );
        assert_eq!(run.ending, Ending::Done, "{}", run.err);
        run.holds_no_key();

        let listed = rows(&run.asked);
        let openai: Vec<&(String, String)> =
            listed.iter().filter(|(id, _)| id == "openai").collect();
        assert_eq!(
            openai,
            [&("openai".to_string(), "proxy.example.invalid".to_string())],
            "{}",
            run.asked
        );
        assert_eq!(run.settings_text().as_deref(), Some(settings));
        assert_eq!(
            run.keys().get("openai").map(Secret::expose),
            Some(A_TYPED_KEY)
        );
        assert!(run.out.contains("proxy.example.invalid"), "{}", run.out);
        assert_eq!(run.out.lines().count(), 1, "{}", run.out);
    }

    /// CLI-18: a settings file that names the id as something that is not a block, or has a
    /// `provider` that is not a block of entries, cannot take the block, so the key is not asked
    /// for, nothing is stored and the file is left as it is. The words say what the file holds.
    #[test]
    fn a_file_that_names_the_id_as_no_service_is_refused_before_the_key_is_asked_for() {
        for (settings, id) in [
            (r#"{"provider": {"openai": null}}"#, "openai"),
            (
                r#"{"provider": {"openai": "https://api.openai.com/v1"}}"#,
                "openai",
            ),
            (r#"{"provider": []}"#, "openai"),
            (r#"{"provider": "none"}"#, "openrouter"),
            (
                r#"{"provider": {"anthropic": {"npm": "@ai-sdk/openai-compatible"}}}"#,
                "anthropic",
            ),
        ] {
            let run = sign_in(
                "cli-auth-known-not-a-service",
                Given {
                    settings: Some(settings),
                    named: Some(id),
                    typed: Some(A_TYPED_KEY),
                    ..Given::default()
                },
            );
            assert_eq!(run.ending, Ending::Failed, "{settings}: {}", run.err);
            assert_eq!(run.key_question, None, "{settings}: the key was asked for");
            assert!(
                !keys::file(&run.directory).exists(),
                "{settings}: a key was stored"
            );
            assert_eq!(run.settings_text().as_deref(), Some(settings));
            assert!(
                run.err.contains("settings.json") && run.err.contains(id),
                "{settings}: {}",
                run.err
            );
            assert!(run.out.is_empty(), "{settings}: {}", run.out);
            run.holds_no_key();
        }
    }

    /// CLI-18: a settings file that cannot be added to is refused before the key is asked for,
    /// writing nothing, where the id is one the file does not configure. Where it is, as a gateway
    /// the machine-level layer names is, the file is not needed and the key is stored.
    #[test]
    fn an_unwritable_settings_file_is_refused_before_the_key_is_asked_for() {
        let too_large = format!(r#"{{"theme": "{}"}}"#, "x".repeat(70 * 1024));
        for (settings, said) in [
            (
                "not a settings document",
                "does not hold a settings document",
            ),
            (too_large.as_str(), "past what bravebot reads"),
        ] {
            let run = sign_in(
                "cli-auth-known-unwritable",
                Given {
                    settings: Some(settings),
                    answers: "openai\n",
                    typed: Some(A_TYPED_KEY),
                    ..Given::default()
                },
            );
            assert_eq!(run.ending, Ending::Failed, "{}", run.err);
            assert_eq!(run.key_question, None, "the key was asked for");
            assert!(!keys::file(&run.directory).exists(), "a key was stored");
            assert_eq!(run.settings_text().as_deref(), Some(settings));
            assert!(run.err.contains(said), "{}", run.err);
            run.holds_no_key();
        }

        let run = sign_in(
            "cli-auth-managed-with-unwritable-settings",
            Given {
                settings: Some("not a settings document"),
                managed: Some(
                    r#"{"provider": {"corp": {"options": {"baseURL": "https://corp.example/v1"}}}}"#,
                ),
                typed: Some(A_TYPED_KEY),
                ..Given::default()
            },
        );
        assert_eq!(run.ending, Ending::Done, "{}", run.err);
        assert_eq!(
            run.keys().get("corp").map(Secret::expose),
            Some(A_TYPED_KEY)
        );
        assert_eq!(
            run.settings_text().as_deref(),
            Some("not a settings document")
        );
    }

    /// CLI-18 and BACKEND-38: where the machine-level layer names gateways only those are offered,
    /// since a block recorded in the person's file would not be read, so a known id is refused by
    /// the words of an id that is not offered. Where it names none, there is nothing to offer.
    #[test]
    fn no_known_service_is_offered_where_the_machine_level_layer_names_the_gateways() {
        let corp = r#"{"provider": {"corp": {"options": {"baseURL": "https://corp.example/v1"}}}}"#;
        let run = sign_in(
            "cli-auth-managed-names-gateways",
            Given {
                managed: Some(corp),
                named: Some("openai"),
                typed: Some(A_TYPED_KEY),
                ..Given::default()
            },
        );
        assert_eq!(run.ending, Ending::Argument, "{}", run.err);
        assert!(run.err.contains("corp"), "{}", run.err);
        assert!(
            !run.err.contains("openai"),
            "the id was repeated: {}",
            run.err
        );
        assert!(
            run.err
                .contains(&bravebot_config::managed_file().display().to_string()),
            "the refusal does not say the machine-level file names the gateways: {}",
            run.err
        );
        assert_eq!(run.key_question, None);
        assert!(!keys::file(&run.directory).exists());

        let scratch = Scratch::new("cli-auth-managed-offers");
        let file = scratch.0.join("managed.json");
        std::fs::write(&file, corp).expect("a machine-level file");
        let managed = Managed::at(&file);
        let config = configured("", &managed);
        let offers = offered(&config.providers, &managed);
        let ids: Vec<&str> = offers.iter().map(Offer::id).collect();
        assert_eq!(
            ids,
            ["corp"],
            "a known id was offered beside the pinned gateways"
        );

        let run = sign_in(
            "cli-auth-managed-names-none",
            Given {
                managed: Some(r#"{"provider": {}}"#),
                named: Some("openai"),
                typed: Some(A_TYPED_KEY),
                ..Given::default()
            },
        );
        assert_eq!(run.ending, Ending::Configuration, "{}", run.err);
        assert!(
            run.err
                .contains(&bravebot_config::managed_file().display().to_string()),
            "{}",
            run.err
        );
        assert_eq!(run.key_question, None);
        assert!(!keys::file(&run.directory).exists());
    }

    /// CLI-18: a key stored for a known id that no block names is sent by no session, so a sign-in
    /// that keeps it, replaces it or stops at the key still records the block, and a sign-in that
    /// picked nothing records nothing.
    #[test]
    fn a_stored_key_no_block_names_gets_its_block_whichever_way_replacing_it_is_answered() {
        let stored = r#"{"gateways": {"openai": "sk-test-old-placeholder-not-a-key"}}"#;
        let recorded = serde_json::json!({"provider": {"openai": {}}});
        for (how, answers, typed, kept) in [
            ("kept", "openai\nn\n", None, true),
            ("replaced", "openai\ny\n", Some(A_TYPED_KEY), false),
            ("stopped at the key", "openai\ny\n", None, true),
        ] {
            let run = sign_in(
                "cli-auth-known-orphan",
                Given {
                    keys: Some(stored),
                    answers,
                    typed,
                    ..Given::default()
                },
            );
            assert_eq!(run.ending, Ending::Done, "{how}: {}", run.err);
            run.holds_no_key();
            let expected = if kept { AN_OLD_KEY } else { A_TYPED_KEY };
            assert_eq!(
                run.keys().get("openai").map(Secret::expose),
                Some(expected),
                "{how}"
            );
            let written = run.settings_text().unwrap_or_default();
            let value: serde_json::Value = serde_json::from_str(&written).expect("json");
            assert_eq!(value, recorded, "{how}: {written}");
            assert!(run.out.contains("settings.json"), "{how}: {}", run.out);
            assert!(
                run.asked.contains("no provider block names it"),
                "{how}: the question did not say the key is sent by no session: {}",
                run.asked
            );
        }

        let run = sign_in(
            "cli-auth-known-orphan-named",
            Given {
                keys: Some(stored),
                named: Some("openai"),
                ..Given::default()
            },
        );
        assert_eq!(run.ending, Ending::Done, "{}", run.err);
        assert!(
            run.settings_text().is_some(),
            "the end of the input kept the key unrecorded"
        );

        let run = sign_in(
            "cli-auth-known-orphan-nothing-picked",
            Given {
                keys: Some(stored),
                answers: "\n",
                ..Given::default()
            },
        );
        assert_eq!(run.ending, Ending::Done, "{}", run.err);
        assert_eq!(
            run.settings_text(),
            None,
            "a block was recorded for no pick"
        );
    }

    /// CLI-18: a key stored with no block to send it is a sign-in the person has to finish by hand,
    /// so the failure names the entry to add, and is not told as a success.
    #[test]
    fn a_block_that_cannot_be_recorded_after_the_key_leaves_it_stored_and_names_the_entry() {
        for (how, written) in [
            ("not a settings document", "not json"),
            ("the id as no service", r#"{"provider": {"openai": null}}"#),
        ] {
            let spoil = |directory: &Path| {
                std::fs::write(bravebot_config::user_settings_file(directory), written)
                    .expect("spoil the settings");
            };
            let run = sign_in(
                "cli-auth-known-record-fails",
                Given {
                    named: Some("openai"),
                    typed: Some(A_TYPED_KEY),
                    meanwhile: Some(&spoil),
                    ..Given::default()
                },
            );
            assert_eq!(run.ending, Ending::Failed, "{how}: {}", run.err);
            assert_eq!(
                run.keys().get("openai").map(Secret::expose),
                Some(A_TYPED_KEY),
                "{how}: the key was not left stored"
            );
            assert!(
                run.err.contains(r#""provider": {"openai": {}}"#)
                    && run.err.contains("settings.json"),
                "{how}: {}",
                run.err
            );
            assert!(run.out.is_empty(), "{how}: said it worked: {}", run.out);
            run.holds_no_key();
        }
    }

    /// CRED-24: a word typed where the id goes may be the key, so refusing it must not repeat it. A
    /// missing terminal is found before anything is read or written, so nothing is left behind.
    #[test]
    fn a_word_naming_no_offered_service_is_not_repeated_and_no_terminal_writes_nothing() {
        let run = sign_in(
            "cli-auth-known-key-as-id",
            Given {
                named: Some(A_TYPED_KEY),
                typed: Some(A_TYPED_KEY),
                ..Given::default()
            },
        );
        assert_eq!(run.ending, Ending::Argument);
        for id in known_ids() {
            assert!(run.err.contains(id), "{id} is not named: {}", run.err);
        }
        assert!(
            !run.err
                .contains(&bravebot_config::managed_file().display().to_string()),
            "the refusal names a machine-level file where none names gateways: {}",
            run.err
        );
        run.holds_no_key();
        assert_eq!(run.key_question, None);

        let run = sign_in(
            "cli-auth-known-no-terminal",
            Given {
                named: Some("openai"),
                typed: Some(A_TYPED_KEY),
                no_terminal: true,
                ..Given::default()
            },
        );
        assert_eq!(run.ending, Ending::Argument, "{}", run.err);
        assert!(run.err.contains("terminal"), "{}", run.err);
        assert_eq!(run.key_question, None);
        assert!(!keys::file(&run.directory).exists());
        assert_eq!(run.settings_text(), None);
    }

    /// The standings `auth status gateway` prints, as `(name, said)`, for the keys `stored` and the
    /// provider blocks `settings` writes. A stored key is attached to the block of its id as the
    /// configuration does.
    fn standings(
        settings: &str,
        managed: &Managed,
        stored: &Result<Keys, keys::Unreadable>,
        named: Option<&str>,
    ) -> Result<Vec<(String, String)>, (Ending, String)> {
        let mut config = configured(settings, managed);
        for provider in &mut config.providers {
            provider.stored_key = stored
                .as_ref()
                .ok()
                .and_then(|stored| stored.get(&provider.id).cloned());
        }
        gateway_standings(Ok(config), managed, stored, named, |_| None).map(|lines| {
            lines
                .into_iter()
                .map(|(name, standing)| (name, standing.said()))
                .collect()
        })
    }

    /// CLI-23: a known service no block names is never signed in, since a turn sends it nothing,
    /// whether or not a key is stored for it.
    #[test]
    fn a_known_service_no_block_names_is_not_signed_in_whether_or_not_a_key_is_stored() {
        let none = Managed::default();
        let nothing = Ok(Keys::default());
        let lines = standings("", &none, &nothing, Some("openai")).expect("asked about");
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0].0, "gateway openai");
        assert!(
            lines[0]
                .1
                .starts_with("not signed in: no key is stored for openai")
                && lines[0].1.contains("bravebot auth login gateway openai"),
            "{lines:?}"
        );

        let held = Ok(placeholder_keys(&["openai"]));
        let lines = standings("", &none, &held, Some("openai")).expect("asked about");
        assert!(
            lines[0]
                .1
                .starts_with("not signed in: a key is stored for openai")
                && lines[0].1.contains("no provider block names it"),
            "{lines:?}"
        );
        assert!(!lines[0].1.contains("placeholder-gateway-key"), "{lines:?}");

        let lines = standings("", &none, &Err(keys::Unreadable), Some("openai")).expect("asked");
        assert!(lines[0].1.starts_with("unusable: "), "{lines:?}");

        // A block that names the id is the gateway, and the stored key is found for it.
        let with_a_block = r#"{"provider": {"openai": {"options": {"baseURL": "https://proxy.example.invalid/v1"}}}}"#;
        let lines = standings(with_a_block, &none, &held, Some("openai")).expect("asked");
        assert!(lines[0].1.starts_with("signed in: stored by"), "{lines:?}");
    }

    /// CLI-23: with no name, the gateways in force are asked about and so is a key stored for a known
    /// service no block names, which a turn does not send. The other known services are left out,
    /// since nobody holds a key for all of them and a named way has to be usable throughout.
    #[test]
    fn without_a_name_a_known_service_is_asked_about_only_where_a_key_is_stored_for_it() {
        let none = Managed::default();
        let names = |lines: Vec<(String, String)>| -> Vec<String> {
            lines.into_iter().map(|(name, _)| name).collect()
        };

        let lines = standings(
            TWO_GATEWAYS_WITH_A_KNOWN_ID,
            &none,
            &Ok(Keys::default()),
            None,
        )
        .expect("asked about");
        assert_eq!(names(lines), ["gateway openrouter", "gateway work"]);

        let lines = standings(
            TWO_GATEWAYS_WITH_A_KNOWN_ID,
            &none,
            &Ok(placeholder_keys(&["anthropic"])),
            None,
        )
        .expect("asked about");
        assert_eq!(
            names(lines),
            ["gateway openrouter", "gateway work", "gateway anthropic"]
        );

        let lines = standings("", &none, &Ok(Keys::default()), None).expect("asked about");
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0].0, "gateway");
        assert!(
            lines[0]
                .1
                .starts_with("not signed in: no gateway key is stored"),
            "{lines:?}"
        );
        let lines = standings("", &none, &Err(keys::Unreadable), None).expect("asked about");
        assert!(lines[0].1.starts_with("unusable: "), "{lines:?}");
    }

    /// CLI-23: an id the sign-in does not offer is refused with the ids it does, without being
    /// repeated. A known service is not one where the machine-level layer names the gateways, and
    /// where it names none there is no gateway to ask about.
    #[test]
    fn status_refuses_an_id_the_sign_in_does_not_offer_and_names_the_ids_it_does() {
        let nothing = Ok(Keys::default());
        let (ending, said) =
            standings("", &Managed::default(), &nothing, Some(A_TYPED_KEY)).expect_err("refused");
        assert_eq!(ending, Ending::Argument);
        for id in known_ids() {
            assert!(said.contains(id), "{id} is not named: {said}");
        }
        assert!(!said.contains(A_TYPED_KEY), "{said}");
        let file = bravebot_config::managed_file().display().to_string();
        assert!(
            !said.contains(&file),
            "the refusal names a machine-level file where none names gateways: {said}"
        );

        let scratch = Scratch::new("cli-auth-status-managed");
        let managed_of = |name: &str, text: &str| {
            let file = scratch.0.join(name);
            std::fs::write(&file, text).expect("a machine-level file");
            Managed::at(&file)
        };
        let corp = managed_of(
            "corp.json",
            r#"{"provider": {"corp": {"options": {"baseURL": "https://corp.example/v1"}}}}"#,
        );
        let (ending, said) = standings("", &corp, &nothing, Some("openai")).expect_err("refused");
        assert_eq!(ending, Ending::Argument);
        assert!(said.contains("corp") && !said.contains("openai"), "{said}");
        assert!(
            said.contains(&file),
            "the refusal does not say the machine-level file names the gateways: {said}"
        );
        let lines = standings("", &corp, &nothing, None).expect("asked about");
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0].0, "gateway corp");

        let empty = managed_of("empty.json", r#"{"provider": {}}"#);
        let (ending, said) = standings("", &empty, &nothing, Some("openai")).expect_err("refused");
        assert_eq!(ending, Ending::Configuration);
        assert!(
            said.contains(&bravebot_config::managed_file().display().to_string()),
            "{said}"
        );
        let lines = standings("", &empty, &nothing, None).expect("asked about");
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].1.starts_with("not signed in: "), "{lines:?}");
    }

    /// CLI-18: a settings file that cannot take the entry is refused before the question about
    /// replacing a key stored for the id, so nobody answers it and is then refused. The key stays
    /// as it was.
    #[test]
    fn a_file_that_cannot_take_the_entry_is_refused_before_replacing_a_key_is_asked_about() {
        let stored = r#"{"gateways": {"openai": "sk-test-old-placeholder-not-a-key"}}"#;
        for settings in [
            "not a settings document",
            r#"{"provider": {"openai": null}}"#,
        ] {
            let run = sign_in(
                "cli-auth-known-refused-before-replacing",
                Given {
                    settings: Some(settings),
                    keys: Some(stored),
                    answers: "openai\ny\n",
                    typed: Some(A_TYPED_KEY),
                    ..Given::default()
                },
            );
            assert_eq!(run.ending, Ending::Failed, "{settings}: {}", run.err);
            assert!(
                !run.asked.contains("Replace it?") && !run.asked.contains("already stored"),
                "{settings}: the question about replacing the key was asked: {}",
                run.asked
            );
            assert_eq!(run.key_question, None, "{settings}");
            assert_eq!(
                run.keys().get("openai").map(Secret::expose),
                Some(AN_OLD_KEY),
                "{settings}"
            );
            assert_eq!(run.settings_text().as_deref(), Some(settings));
            run.holds_no_key();
        }
    }

    /// CLI-18: where the file spoils between the question about replacing a key and the end, the
    /// key already stored is not changed and the person is told to add the entry, as for a new key.
    #[test]
    fn a_block_that_cannot_be_recorded_for_a_key_already_stored_fails_and_names_the_entry() {
        let stored = r#"{"gateways": {"openai": "sk-test-old-placeholder-not-a-key"}}"#;
        for (how, written) in [
            ("not a settings document", "not json"),
            ("the id as no service", r#"{"provider": {"openai": null}}"#),
        ] {
            let spoil = |directory: &Path| {
                std::fs::write(bravebot_config::user_settings_file(directory), written)
                    .expect("spoil the settings");
            };
            let run = sign_in(
                "cli-auth-known-kept-record-fails",
                Given {
                    keys: Some(stored),
                    answers: "openai\ny\n",
                    typed: None,
                    meanwhile: Some(&spoil),
                    ..Given::default()
                },
            );
            assert_eq!(run.ending, Ending::Failed, "{how}: {}", run.err);
            assert_eq!(
                run.keys().get("openai").map(Secret::expose),
                Some(AN_OLD_KEY),
                "{how}: the key stored was changed"
            );
            assert!(
                run.err.contains(r#""provider": {"openai": {}}"#)
                    && run.err.contains("settings.json"),
                "{how}: {}",
                run.err
            );
            assert!(run.out.is_empty(), "{how}: said it worked: {}", run.out);
            run.holds_no_key();
        }
    }

    /// CLI-18: a configured gateway's block is the person's, so keeping its key records nothing and
    /// touches no file.
    #[test]
    fn a_configured_gateway_with_a_kept_key_ends_well_and_records_nothing() {
        let settings = r#"{"provider": {"work": {"options": {"baseURL": "https://gateway.work.example/v1"}}}}"#;
        let stored = r#"{"gateways": {"work": "sk-test-old-placeholder-not-a-key"}}"#;
        for (how, answers) in [("kept", "n\n"), ("replaced, then nothing typed", "y\n")] {
            let run = sign_in(
                "cli-auth-configured-kept",
                Given {
                    settings: Some(settings),
                    keys: Some(stored),
                    named: Some("work"),
                    answers,
                    typed: None,
                    ..Given::default()
                },
            );
            assert_eq!(run.ending, Ending::Done, "{how}: {}", run.err);
            assert!(run.out.is_empty(), "{how}: {}", run.out);
            assert!(
                !run.err.contains("provider") && !run.err.contains("settings.json"),
                "{how}: said something of a block: {}",
                run.err
            );
            assert_eq!(run.settings_text().as_deref(), Some(settings), "{how}");
            assert_eq!(
                run.keys().get("work").map(Secret::expose),
                Some(AN_OLD_KEY),
                "{how}"
            );
            run.holds_no_key();
        }
    }

    /// BACKEND-16: a variable the block names outranks a stored key, so the person is told before
    /// typing one that it would not be sent. The variable is named and its value never shown.
    #[test]
    fn a_variable_the_block_names_is_said_to_win_before_the_key_is_typed() {
        let settings = r#"{"provider": {"work": {"env": ["WORK_API_KEY"],
            "options": {"baseURL": "https://gateway.work.example/v1"}}}}"#;
        let set = [("WORK_API_KEY", AN_EXPORTED_KEY)];
        for (exported, said) in [(&set[..], true), (&[][..], false)] {
            let run = sign_in(
                "cli-auth-variable-wins",
                Given {
                    settings: Some(settings),
                    named: Some("work"),
                    typed: Some(A_TYPED_KEY),
                    exported,
                    ..Given::default()
                },
            );
            assert_eq!(run.ending, Ending::Done, "{}", run.err);
            assert_eq!(
                run.err.contains("WORK_API_KEY is set"),
                said,
                "the notice is wrong where the variable is {}: {}",
                if said { "set" } else { "not set" },
                run.err
            );
            assert!(run.key_question.is_some(), "the key was not asked for");
            run.holds_no_key();
        }
    }

    /// BACKEND-38: a person who finds no known service on the list is told which file decided that.
    #[test]
    fn the_list_names_the_machine_level_file_where_it_names_the_gateways() {
        let file = bravebot_config::managed_file().display().to_string();
        let corp = r#"{"provider": {
            "corp": {"options": {"baseURL": "https://corp.example/v1"}},
            "other": {"options": {"baseURL": "https://other.example/v1"}}}}"#;

        let run = sign_in(
            "cli-auth-managed-list",
            Given {
                managed: Some(corp),
                answers: "\n",
                ..Given::default()
            },
        );
        assert_eq!(run.ending, Ending::Done, "{}", run.err);
        assert!(run.asked.contains(&file), "{}", run.asked);
        let ids: Vec<String> = rows(&run.asked).into_iter().map(|(id, _)| id).collect();
        assert_eq!(ids, ["corp", "other"], "{}", run.asked);

        let plain = sign_in(
            "cli-auth-plain-list",
            Given {
                answers: "\n",
                ..Given::default()
            },
        );
        assert!(
            plain.asked.contains("Services a key can be stored for:")
                && !plain.asked.contains(&file),
            "{}",
            plain.asked
        );
    }

    /// CLI-18: an entry is recorded for a key that is stored, so stopping at the key prompt with none
    /// stored leaves the settings as they were, or absent.
    #[test]
    fn stopping_at_the_key_with_none_stored_records_no_block() {
        for (how, settings) in [
            ("no settings file", None),
            ("a settings file", Some(r#"{"theme": "dark"}"#)),
        ] {
            let run = sign_in(
                "cli-auth-known-stopped-at-the-key",
                Given {
                    settings,
                    named: Some("openai"),
                    typed: None,
                    ..Given::default()
                },
            );
            assert_eq!(run.ending, Ending::Done, "{how}: {}", run.err);
            assert_eq!(run.settings_text().as_deref(), settings, "{how}");
            assert!(
                !keys::file(&run.directory).exists(),
                "{how}: a key was stored"
            );
            assert!(run.out.is_empty(), "{how}: {}", run.out);
            assert!(!run.err.contains("provider"), "{how}: {}", run.err);
        }
    }

    /// CLI-18: a file of keys that cannot be read once the key is not typed is a failure, as it is
    /// everywhere else the file is read, and not a sign-in that ended well.
    #[test]
    fn a_file_of_keys_that_cannot_be_read_when_no_key_is_typed_ends_as_a_failure() {
        let spoil = |directory: &Path| {
            std::fs::write(keys::file(directory), "not a file of keys").expect("spoil the keys");
        };
        let run = sign_in(
            "cli-auth-known-keys-spoiled",
            Given {
                named: Some("openai"),
                typed: None,
                meanwhile: Some(&spoil),
                ..Given::default()
            },
        );
        assert_eq!(run.ending, Ending::Failed, "{}", run.err);
        assert!(
            run.err.contains("is not a file of keys bravebot wrote"),
            "{}",
            run.err
        );
        assert_eq!(run.settings_text(), None, "a block was recorded");
        assert_eq!(
            std::fs::read_to_string(keys::file(&run.directory)).expect("the keys"),
            "not a file of keys"
        );
    }

    /// CLI-18: a settings file under the size bravebot reads can be taken past it by the entry, which
    /// is found before the key is asked for.
    #[test]
    fn a_file_the_entry_would_take_past_the_size_bravebot_reads_is_refused_before_the_key() {
        // The reader takes no file past 64 KiB, and the entry is written with the file's own
        // content re-indented, so this one is under the limit and the result is not.
        let limit = 64 * 1024;
        let settings = format!(r#"{{"pad": "{}"}}"#, "x".repeat(limit - 20));
        assert!(settings.len() < limit, "the file is already past the limit");
        let run = sign_in(
            "cli-auth-known-takes-the-file-past-the-limit",
            Given {
                settings: Some(&settings),
                named: Some("openai"),
                typed: Some(A_TYPED_KEY),
                ..Given::default()
            },
        );
        assert_eq!(run.ending, Ending::Failed, "{}", run.err);
        assert_eq!(run.key_question, None, "the key was asked for");
        assert!(!keys::file(&run.directory).exists(), "a key was stored");
        assert_eq!(run.settings_text().as_deref(), Some(settings.as_str()));
        assert!(run.err.contains("past what bravebot reads"), "{}", run.err);
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

    fn line(standing: Standing) -> Line {
        ("way".to_string(), standing)
    }

    /// CLI-23: a way named has to be usable throughout, and with none named one usable sign-in is
    /// enough. A configuration that could not be read is its own ending, but only where nothing
    /// asked about is usable.
    #[test]
    fn the_verdict_asks_all_of_a_named_way_and_any_of_the_rest() {
        let good = || line(Standing::SignedIn("good".into()));
        let absent = || line(Standing::NotSignedIn("absent".into()));
        let unusable = || line(Standing::Unusable("broken".into()));
        let misconfigured = || line(Standing::Misconfigured("unreadable".into()));

        assert_eq!(verdict(&[good(), absent()], false), Ending::Done);
        assert_eq!(verdict(&[good(), absent()], true), Ending::Failed);
        assert_eq!(verdict(&[good(), good()], true), Ending::Done);
        assert_eq!(verdict(&[absent(), unusable()], false), Ending::Failed);
        assert_eq!(verdict(&[], false), Ending::Failed);
        assert_eq!(
            verdict(&[absent(), misconfigured()], false),
            Ending::Configuration
        );
        assert_eq!(verdict(&[good(), misconfigured()], false), Ending::Done);
        assert_eq!(
            verdict(&[good(), misconfigured()], true),
            Ending::Configuration
        );
    }

    /// CLI-23: an AWS session that lapsed is one signing in fixes; a missing CLI or an unknown
    /// profile is not, and says so rather than sending somebody to sign in.
    #[test]
    fn a_lapsed_aws_session_is_not_signed_in_and_a_missing_cli_is_unusable() {
        assert!(bedrock_standing(Ok(())).usable());
        let lapsed = CredentialError::Refused {
            detail: "expired".into(),
        };
        assert!(matches!(
            bedrock_standing(Err(lapsed)),
            Standing::NotSignedIn(_)
        ));
        for other in [
            CredentialError::NotInstalled,
            CredentialError::NoSuchProfile {
                profile: "work".into(),
                available: Vec::new(),
            },
            CredentialError::Undecodable {
                detail: "odd".into(),
            },
        ] {
            assert!(
                matches!(bedrock_standing(Err(other)), Standing::Unusable(_)),
                "a fault signing in cannot fix was reported as one it can"
            );
        }
    }

    /// CLI-23: a line carries none of the text the AWS CLI chose, which names accounts and roles and
    /// lists profiles that embed an account id.
    #[test]
    fn a_bedrock_line_carries_nothing_the_aws_cli_wrote() {
        let account = "1234".repeat(3);
        let account = account.as_str();
        for failure in [
            CredentialError::Refused {
                detail: format!("AccessDenied: arn:aws:sts::{account}:assumed-role/x"),
            },
            CredentialError::NoSuchProfile {
                profile: "work".into(),
                available: vec![format!("AdministratorAccess-{account}")],
            },
            CredentialError::Undecodable {
                detail: account.into(),
            },
        ] {
            let line = match bedrock_standing(Err(failure)) {
                Standing::NotSignedIn(text) | Standing::Unusable(text) => text,
                _ => panic!("a failure was reported as usable"),
            };
            assert!(!line.contains(account), "{line}");
        }
    }
}
