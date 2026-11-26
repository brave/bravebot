//! How each way of reaching a model service stands, and forgetting what one of them keeps, as typed
//! values: the CLI and the desktop each put them into their own words.
//!
//! Nothing here holds a message catalog or prints, and no value here carries a credential. A
//! detail the AWS CLI wrote is not kept either, since it can name an account or a role.

use crate::backend::{Backend, CredentialError};
use crate::home;
use bravebot_config::bedrock::Bedrock;
use bravebot_config::keys::{self, Keys};
use bravebot_config::provider::{Credential, GOOGLE_VERTEX_ID, Provider, Source, known_ids};
use bravebot_config::{Config, ConfigError, Managed, Settings, brave_key, env_var};
use bravebot_skus::chatgpt::AccountError;
use std::borrow::Cow;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, mpsc};
use std::time::{Duration, Instant};

/// One thing a row reports on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Way {
    /// The key for Brave's premium service.
    Brave,
    /// One AWS account, by the profile the AWS CLI signs it in to, `None` being its default chain.
    Bedrock { profile: Option<String> },
    /// One gateway, by the id of its provider block or of the known service.
    Gateway { id: String },
    /// The gateways as a whole, for a row about none of them in particular.
    Gateways,
    /// The ChatGPT account.
    ChatGpt,
    /// The xAI account.
    Xai,
}

/// How one way stands.
#[derive(Debug)]
pub enum Standing {
    /// Usable now.
    SignedIn(Held),
    /// Nothing is there, or what was there has lapsed, and signing in fixes it.
    NotSignedIn(Why),
    /// Something is there and signing in again would not make it usable.
    Unusable(Why),
    /// The configuration cannot be read, so the question has no answer.
    Misconfigured(ConfigError),
}

impl Standing {
    pub fn usable(&self) -> bool {
        matches!(self, Standing::SignedIn(_))
    }
}

/// What a usable way holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Held {
    /// A key bravebot stored.
    KeyStored,
    /// A key a variable the provider block names holds.
    KeyFromEnvironment,
    /// A key the file the provider block's `options.keyFile` names holds.
    KeyFromFile,
    /// The provider block names no key.
    KeyNotNeeded,
    /// An account whose access token expires at the instant stated, which a request renews.
    Account { expires: String },
    /// The AWS session gives credentials to sign a request with.
    AwsSession,
}

/// Why a way is not usable.
#[derive(Debug)]
pub enum Why {
    /// This machine has nowhere to keep a credential.
    Nowhere,
    /// Nothing is stored.
    NothingStored,
    /// The Brave key file at `path` cannot be read.
    BraveUnreadable { path: PathBuf },
    /// The stored ChatGPT account cannot be read.
    ChatGpt(AccountError),
    /// The stored xAI account cannot be read.
    Xai(AccountError),
    /// The AWS session has lapsed.
    BedrockLapsed,
    /// The AWS CLI has no profile by the name the account gives.
    BedrockNoProfile,
    /// The AWS CLI is not installed.
    BedrockNoCli,
    /// The AWS CLI answered with something that is not a credential.
    BedrockUndecodable,
    /// The AWS CLI did not answer in the time allowed.
    BedrockTimedOut,
    /// The configuration names no AWS account.
    NoAwsAccount,
    /// The machine-level layer names the gateways and names none.
    GatewayNoneConfigured,
    /// A known service no block names, with no key stored for it, or a Vertex AI express block with
    /// no key stored for it.
    GatewayNoneStored,
    /// A known service no block names, with a key stored that no session sends.
    GatewayUnrecorded,
    /// A configured gateway a turn would find no key for.
    GatewayAbsent,
    /// A configured gateway whose block names a file for its key that gives none, with no variable
    /// or stored key before it. Storing a key makes it usable, since a stored key is read first.
    GatewayKeyFileUnusable(bravebot_config::provider::KeyFileFault),
    /// The file of gateway keys cannot be read.
    KeysUnreadable,
    /// Not asked, because the caller said to skip it.
    NotChecked,
}

/// One way, how it stands, and whether bravebot keeps a credential for it on disk.
///
/// `stored` is never true for an AWS account, a key a variable holds, or a gateway that needs none.
#[derive(Debug)]
pub struct Row {
    pub way: Way,
    pub standing: Standing,
    pub stored: bool,
}

/// Whether to ask the AWS CLI about each account, which can take a while.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BedrockCheck {
    /// Ask, and say an account timed out where no answer came within `timeout`.
    Check { timeout: Duration },
    /// Report each account as not checked.
    Skip,
}

/// How a profile's AWS session is asked about, `None` being the AWS CLI's default chain.
pub type Session = Arc<dyn Fn(Option<&str>) -> Result<(), CredentialError> + Send + Sync>;

/// The profiles whose `aws` is still running from an earlier check, which are not asked again
/// until it ends, so a session that never answers starts one `aws` and not one per listing.
pub type InFlight = Arc<Mutex<HashSet<Option<String>>>>;

static IN_FLIGHT: LazyLock<InFlight> = LazyLock::new(InFlight::default);

/// Where a gateway's key variables are read from.
pub type Lookup = Box<dyn Fn(&str) -> Option<String>>;

/// A stored account as it loaded: its expiry, nothing, or why it could not be read.
pub type Loaded = Result<Option<String>, AccountError>;

/// Everything [`list`] reads, passed in rather than read from the process.
pub struct Inputs {
    /// What a session reads, for the gateways.
    pub config: Result<Config, ConfigError>,
    /// What the Bedrock sign-in signs in to ([`bedrock_config`]).
    pub bedrock_config: Result<Config, ConfigError>,
    pub managed: Managed,
    /// The state directory, where there is one.
    pub directory: Option<PathBuf>,
    /// The profile an `aws` started with no `--profile` uses ([`inherited_profile`]).
    pub inherited_profile: Option<String>,
    /// The environment a gateway's key variables are read from.
    pub lookup: Lookup,
    pub bedrock: BedrockCheck,
    pub session: Session,
    pub in_flight: InFlight,
    pub chatgpt: Loaded,
    pub xai: Loaded,
}

impl Inputs {
    /// What a turn would read now: the settings, the machine-level layer, the environment, the state
    /// directory and the stored accounts.
    pub fn read(bedrock: BedrockCheck) -> Self {
        let settings = Settings::load();
        let managed = Managed::load();
        Inputs {
            config: Config::from_env(),
            bedrock_config: bedrock_config(&settings, &managed),
            managed,
            directory: home::directory(),
            inherited_profile: inherited_profile(),
            lookup: Box::new(|name| std::env::var(name).ok()),
            bedrock,
            session: Arc::new(Backend::session_of),
            in_flight: Arc::clone(&IN_FLIGHT),
            chatgpt: chatgpt_loaded(),
            xai: xai_loaded(),
        }
    }
}

/// The stored ChatGPT account, read from where a turn reads it.
pub fn chatgpt_loaded() -> Loaded {
    bravebot_skus::chatgpt::Account::load().map(|account| account.map(|account| account.expires()))
}

/// The stored xAI account, read from where a turn reads it.
pub fn xai_loaded() -> Loaded {
    bravebot_skus::xai::Account::load().map(|account| account.map(|account| account.expires()))
}

/// Which ways to list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Which {
    /// Every way, in the order Brave, Bedrock, the gateways, ChatGPT, xAI.
    All,
    Brave,
    Bedrock,
    /// The gateways a block names and the known services a key is stored for, or the one named.
    Gateways(Option<String>),
    ChatGpt,
    Xai,
}

/// Why a gateway asked about by name has no row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListError {
    /// The machine-level layer names the gateways and names none.
    NoneConfigured,
    /// The id is not one a key can be stored for. `offered` are the ids that are, and `managed` says
    /// whether the machine-level layer chose them.
    NotOffered { offered: Vec<String>, managed: bool },
}

/// How each way `which` names stands.
pub fn list(inputs: Inputs, which: Which) -> Result<Vec<Row>, ListError> {
    let Inputs {
        config,
        bedrock_config,
        managed,
        directory,
        inherited_profile,
        lookup,
        bedrock: check,
        session,
        in_flight,
        chatgpt,
        xai,
    } = inputs;
    let all = which == Which::All;
    let with_chatgpt = all || which == Which::ChatGpt;
    let with_xai = all || which == Which::Xai;
    let mut rows = Vec::new();
    if all || which == Which::Brave {
        rows.push(brave(directory.as_deref()));
    }
    if all || which == Which::Bedrock {
        // Every account is asked at once and waited for until one deadline, so a listing takes at
        // most `timeout` however many accounts there are.
        let asking = match (check, &bedrock_config) {
            (BedrockCheck::Check { timeout }, Ok(config)) => {
                let deadline = Instant::now().checked_add(timeout);
                let asks = aws_accounts(config, inherited_profile.as_deref())
                    .into_iter()
                    .map(|(_, account)| {
                        let profile = account.profile.clone();
                        let ask = ask(&session, &in_flight, profile.clone());
                        (profile, ask)
                    })
                    .collect::<Vec<_>>();
                Some((deadline, asks))
            }
            _ => None,
        };
        rows.extend(bedrock(
            bedrock_config,
            inherited_profile.as_deref(),
            |profile| match &asking {
                Some((deadline, asks)) => asks
                    .iter()
                    .find(|(asked, _)| asked.as_deref() == profile)
                    .map_or(Asked::TimedOut, |(_, ask)| answer(ask.as_ref(), *deadline)),
                None => Asked::Skipped,
            },
        ));
    }
    let named = match which {
        Which::Gateways(named) => Some(named),
        Which::All => Some(None),
        _ => None,
    };
    if let Some(named) = named {
        let stored = directory.as_deref().map_or(Ok(Keys::default()), Keys::read);
        rows.extend(gateways(
            config,
            &managed,
            &stored,
            named.as_deref(),
            lookup,
        )?);
    }
    if with_chatgpt {
        rows.push(account(Way::ChatGpt, chatgpt));
    }
    if with_xai {
        rows.push(account(Way::Xai, xai));
    }
    Ok(rows)
}

/// The Brave key, from the file a turn reads in `directory`.
pub fn brave(directory: Option<&Path>) -> Row {
    let (standing, stored) = match directory.map(brave_key::read) {
        None => (Standing::NotSignedIn(Why::Nowhere), false),
        Some(Ok(Some(_))) => (Standing::SignedIn(Held::KeyStored), true),
        Some(Ok(None)) => (Standing::NotSignedIn(Why::NothingStored), false),
        Some(Err(brave_key::Unreadable)) => (
            Standing::Unusable(Why::BraveUnreadable {
                path: directory.map(brave_key::file).unwrap_or_default(),
            }),
            true,
        ),
    };
    Row {
        way: Way::Brave,
        standing,
        stored,
    }
}

/// The ChatGPT or the xAI account, as it loaded. One whose access token has expired is still signed
/// in, since the next request renews it.
pub fn account(way: Way, loaded: Loaded) -> Row {
    let unreadable = |failure| match way {
        Way::Xai => Why::Xai(failure),
        _ => Why::ChatGpt(failure),
    };
    let (standing, stored) = match loaded {
        Ok(Some(expires)) => (Standing::SignedIn(Held::Account { expires }), true),
        Ok(None) => (Standing::NotSignedIn(Why::NothingStored), false),
        Err(AccountError::Nowhere { .. }) => (Standing::NotSignedIn(Why::Nowhere), false),
        Err(failure) => (Standing::Unusable(unreadable(failure)), true),
    };
    Row {
        way,
        standing,
        stored,
    }
}

/// What asking about one AWS session came to.
#[derive(Debug)]
pub enum Asked {
    Answered(Result<(), CredentialError>),
    TimedOut,
    Skipped,
}

/// Ask on a thread of its own, so an `aws` that never answers holds up nothing past the deadline.
/// `None` is a profile whose `aws` from an earlier listing is still running, which is not started
/// again.
///
/// The `aws` is left to finish or not on that thread: nothing here can end the process it started.
fn ask(
    session: &Session,
    in_flight: &InFlight,
    profile: Option<String>,
) -> Option<mpsc::Receiver<Result<(), CredentialError>>> {
    let mut running = in_flight
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !running.insert(profile.clone()) {
        return None;
    }
    drop(running);
    let (answer, answered) = mpsc::channel();
    let session = Arc::clone(session);
    let in_flight = Arc::clone(in_flight);
    std::thread::spawn(move || {
        let found = session(profile.as_deref());
        in_flight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&profile);
        answer.send(found).unwrap_or_default();
    });
    Some(answered)
}

/// What an [`ask`] came to by `deadline`, `None` being one that never ends.
fn answer(
    answered: Option<&mpsc::Receiver<Result<(), CredentialError>>>,
    deadline: Option<Instant>,
) -> Asked {
    let Some(answered) = answered else {
        return Asked::TimedOut;
    };
    let found = match deadline {
        Some(deadline) => answered.recv_timeout(deadline.saturating_duration_since(Instant::now())),
        None => answered
            .recv()
            .map_err(|_| mpsc::RecvTimeoutError::Disconnected),
    };
    match found {
        Ok(found) => Asked::Answered(found),
        Err(mpsc::RecvTimeoutError::Timeout) => Asked::TimedOut,
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Asked::Answered(Err(CredentialError::Undecodable {
                detail: String::new(),
            }))
        }
    }
}

/// Each AWS account the configuration names, with how its session stands.
pub fn bedrock(
    config: Result<Config, ConfigError>,
    inherited: Option<&str>,
    session: impl Fn(Option<&str>) -> Asked,
) -> Vec<Row> {
    let row = |profile, standing| Row {
        way: Way::Bedrock { profile },
        standing,
        stored: false,
    };
    let config = match config {
        Ok(config) => config,
        Err(problem) => return vec![row(None, Standing::Misconfigured(problem))],
    };
    let accounts = aws_accounts(&config, inherited);
    if accounts.is_empty() {
        return vec![row(None, Standing::NotSignedIn(Why::NoAwsAccount))];
    }
    accounts
        .into_iter()
        .map(|(profile, account)| {
            let standing = match session(account.profile.as_deref()) {
                Asked::Answered(Ok(())) => Standing::SignedIn(Held::AwsSession),
                Asked::Answered(Err(CredentialError::Refused { .. })) => {
                    Standing::NotSignedIn(Why::BedrockLapsed)
                }
                Asked::Answered(Err(CredentialError::NoSuchProfile { .. })) => {
                    Standing::Unusable(Why::BedrockNoProfile)
                }
                Asked::Answered(Err(CredentialError::NotInstalled)) => {
                    Standing::Unusable(Why::BedrockNoCli)
                }
                Asked::Answered(Err(CredentialError::Undecodable { .. })) => {
                    Standing::Unusable(Why::BedrockUndecodable)
                }
                Asked::TimedOut => Standing::Unusable(Why::BedrockTimedOut),
                Asked::Skipped => Standing::NotSignedIn(Why::NotChecked),
            };
            row(profile, standing)
        })
        .collect()
}

/// The configuration the Bedrock sign-in signs in to: the one a session reads, with
/// [`env_var::USE_BEDROCK`] on where nothing sets it, so long as that names a model to use.
///
/// Without the model the tier variables' account is not one a session could use, and switching it
/// on would sign in to the default AWS profile for someone whose Bedrock is a provider block.
pub fn bedrock_config(settings: &Settings, managed: &Managed) -> Result<Config, ConfigError> {
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

/// The profile an `aws` started with no `--profile` uses: the one `AWS_PROFILE` names, which it
/// inherits.
pub fn inherited_profile() -> Option<String> {
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
pub fn aws_accounts<'c>(
    config: &'c Config,
    inherited: Option<&str>,
) -> Vec<(Option<String>, &'c Bedrock)> {
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

/// What the configuration already says about [`env_var::USE_BEDROCK`], as far as recording it goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BedrockSwitch {
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

/// [`BedrockSwitch`] from the three places that may set the name, in the order `Config` ranks
/// them: the machine-level pin, then the export, then the settings files.
///
/// An export of any other value turns Bedrock off, a blank one included: the name is not baked in,
/// so a blank is not passed over for a value under it. A settings file that turns it on leaves the
/// switch recordable: it is the person's own file or the file `--settings` names, since a
/// checkout's decides nothing (BACKEND-33), and [`record_bedrock`] writes nothing where the
/// person's own file holds it already.
pub fn bedrock_switch(
    pinned: Option<&str>,
    exported: Option<&str>,
    recorded: Option<&str>,
) -> BedrockSwitch {
    let on = |value: &str| value.trim() == "1";
    match (pinned, exported, recorded) {
        (Some(pin), _, _) if on(pin) => BedrockSwitch::Pinned,
        (Some(_), _, _) => BedrockSwitch::PinnedOff,
        (None, Some(export), _) if !on(export) => BedrockSwitch::Off,
        (None, _, Some(file)) if on(file) => BedrockSwitch::Recordable,
        (None, Some(_), Some(_)) => BedrockSwitch::Overruled,
        (None, None, Some(_)) => BedrockSwitch::Off,
        (None, _, None) => BedrockSwitch::Recordable,
    }
}

/// [`bedrock_switch`] as this process's environment and the given layers set it.
pub fn bedrock_switch_now(settings: &Settings, managed: &Managed) -> BedrockSwitch {
    bedrock_switch(
        managed.get(env_var::USE_BEDROCK),
        std::env::var(env_var::USE_BEDROCK).ok().as_deref(),
        settings.get(env_var::USE_BEDROCK),
    )
}

/// What [`record_bedrock`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BedrockRecorded {
    /// The person's own file already turns Bedrock on.
    Already,
    /// The person's own file names the switch with another value, which is theirs and was left.
    Left,
    Written,
}

/// Why [`record_bedrock`] wrote nothing.
#[derive(Debug)]
pub enum BedrockNotRecorded {
    Unwritable(bravebot_config::import::Unwritable),
    /// The file's `env` is there and is not a block of names.
    EnvNotABlock,
    /// The file system refused, in its own words.
    Refused(String),
}

/// Set [`env_var::USE_BEDROCK`] to 1 in the person's own settings file under `directory`, which is
/// what every session reads and the lowest layer, so a project file that turns Bedrock off still
/// does.
///
/// A value the person wrote there is theirs, so the file is written only where it names nothing.
pub fn record_bedrock(directory: &Path) -> Result<BedrockRecorded, BedrockNotRecorded> {
    let file = bravebot_config::user_settings_file(directory);
    let mut destination = bravebot_config::import::Destination::open(&file)
        .map_err(BedrockNotRecorded::Unwritable)?;
    let held = destination.env(env_var::USE_BEDROCK);
    if held.is_some_and(|held| held.trim() == "1") {
        return Ok(BedrockRecorded::Already);
    }
    if held.is_none() && destination.holds_env(env_var::USE_BEDROCK) {
        return Err(BedrockNotRecorded::EnvNotABlock);
    }
    if destination.names_env(env_var::USE_BEDROCK) {
        return Ok(BedrockRecorded::Left);
    }
    destination.add_env(env_var::USE_BEDROCK, "1");
    write_settings(&destination).map_err(|failure| match failure {
        SettingsNotWritten::Unwritable(why) => BedrockNotRecorded::Unwritable(why),
        SettingsNotWritten::Refused(why) => BedrockNotRecorded::Refused(why),
    })?;
    Ok(BedrockRecorded::Written)
}

/// Why [`write_settings`] left the file as it was.
#[derive(Debug)]
pub enum SettingsNotWritten {
    Unwritable(bravebot_config::import::Unwritable),
    /// The file system refused, in its own words.
    Refused(String),
}

/// Put the destination on disk, whole, in place of the file it was read from.
///
/// Written beside it under a name of its own at STATE-1's mode, then renamed over it, so a failed
/// write leaves the old file as it was and no copy of a key is readable by another user in between.
/// Through a link rather than over it, so a settings file kept among somebody's dotfiles stays
/// where they keep it.
pub fn write_settings(
    destination: &bravebot_config::import::Destination,
) -> Result<(), SettingsNotWritten> {
    use bravebot_config::import::Unwritable;
    let path = destination.path();
    // The questions take as long as the person does, and another program may write the file
    // meanwhile.
    if destination.changed() {
        return Err(SettingsNotWritten::Unwritable(Unwritable::Changed));
    }
    let text = destination.text().map_err(SettingsNotWritten::Unwritable)?;
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let Some(parent) = target.parent() else {
        return Err(SettingsNotWritten::Refused(target.display().to_string()));
    };
    home::create_directory(parent).map_err(|err| SettingsNotWritten::Refused(err.to_string()))?;
    let temporary = parent.join(format!(".settings.json.import-{}", std::process::id()));
    let written = home::write_file(&temporary, text.expose().as_bytes())
        .and_then(|()| std::fs::rename(&temporary, &target));
    if let Err(err) = written {
        std::fs::remove_file(&temporary).unwrap_or_default();
        return Err(SettingsNotWritten::Refused(err.to_string()));
    }
    Ok(())
}

/// A service a gateway key can be stored for.
#[derive(Clone)]
pub struct Offer<'c> {
    pub provider: Cow<'c, Provider>,
    /// Whether no block of the configuration names the id. It is then one of the known names or
    /// Vertex AI, and a session sends a key stored for it only once the person's settings name it.
    pub unconfigured: bool,
    /// The fields the block recorded for an unconfigured id states: none for a known name, whose
    /// endpoint is compiled in, and the project and location for Vertex AI, whose endpoint is built
    /// from them.
    pub entry: serde_json::Map<String, serde_json::Value>,
}

impl Offer<'_> {
    pub fn id(&self) -> &str {
        &self.provider.id
    }

    /// The entry recorded for this id as the JSON object it is written as, which is `{}` for a known
    /// name.
    pub fn entry_text(&self) -> String {
        serde_json::Value::Object(self.entry.clone()).to_string()
    }

    /// Whether the person is asked for a project and a location before a key: Vertex AI where no
    /// block names it, since the block recorded states them. A block that names it already does.
    pub fn takes_a_project(&self) -> bool {
        self.unconfigured && self.provider.is_google_vertex()
    }
}

impl Offer<'static> {
    /// Vertex AI for `project` and `location`, which are not asked for again once they are here, or
    /// `None` where either is a value its endpoint cannot be built from
    /// ([`bravebot_config::provider::vertex_entry`]).
    pub fn google_vertex(project: Option<&str>, location: Option<&str>) -> Option<Self> {
        let (provider, entry) = Provider::google_vertex(project, location)?;
        Some(Offer {
            provider: Cow::Owned(provider),
            unconfigured: true,
            entry,
        })
    }
}

/// What a key can be stored for: the gateways the configuration names, then each known id no block
/// of it names and Vertex AI where none does, each with the host a key for it is sent to.
///
/// No id that needs a block recorded where the machine-level layer names the gateways. That layer
/// replaces the blocks a person wrote, so a block recorded for it in the person's file would not be
/// read.
///
/// Vertex AI is offered in express mode, which is the service an entry that states no field reaches
/// ([BACKEND-58](../../../docs/specs/backends.md#BACKEND-58)). A caller that asks for a project
/// replaces the offer with [`Offer::google_vertex`].
pub fn offered<'c>(providers: &'c [Provider], managed: &Managed) -> Vec<Offer<'c>> {
    let mut offers: Vec<Offer<'c>> = providers
        .iter()
        .filter(|provider| provider.bedrock.is_none())
        .map(|provider| Offer {
            provider: Cow::Borrowed(provider),
            unconfigured: false,
            entry: serde_json::Map::new(),
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
                    entry: serde_json::Map::new(),
                }),
        );
        if !providers.iter().any(|provider| provider.is_google_vertex()) {
            offers.extend(Offer::google_vertex(None, None));
        }
    }
    offers
}

/// Each gateway a provider block names, or the one `named`, with whether a turn would find a key
/// for it, and where.
///
/// A known service no block names is listed only where it is named, or where a key is stored for
/// it: a turn sends none, so it is not signed in, and signing in again records the block. Without a
/// name, every other known service is left out, since nobody holds a key for all of them.
pub fn gateways(
    config: Result<Config, ConfigError>,
    managed: &Managed,
    stored: &Result<Keys, keys::Unreadable>,
    named: Option<&str>,
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<Vec<Row>, ListError> {
    let all = |standing| {
        vec![Row {
            way: Way::Gateways,
            standing,
            stored: false,
        }]
    };
    let config = match config {
        Ok(config) => config,
        Err(problem) => return Ok(all(Standing::Misconfigured(problem))),
    };
    let offers = offered(&config.providers, managed);
    if offers.is_empty() {
        return match named {
            Some(_) => Err(ListError::NoneConfigured),
            None => Ok(all(Standing::NotSignedIn(Why::GatewayNoneConfigured))),
        };
    }
    if let Some(id) = named
        && !offers.iter().any(|offer| offer.id() == id)
    {
        return Err(ListError::NotOffered {
            offered: offers.iter().map(|offer| offer.id().to_string()).collect(),
            managed: managed.gateways().is_some(),
        });
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
        return Ok(all(match stored {
            Err(keys::Unreadable) => Standing::Unusable(Why::KeysUnreadable),
            Ok(_) => Standing::NotSignedIn(Why::NothingStored),
        }));
    }
    Ok(asked_about
        .into_iter()
        .map(|offer| {
            let held = key_stored(offer);
            let standing = match (offer.unconfigured, stored) {
                (_, Err(_)) if offer.unconfigured => Standing::Unusable(Why::KeysUnreadable),
                (true, _) if held => Standing::NotSignedIn(Why::GatewayUnrecorded),
                (true, _) => Standing::NotSignedIn(Why::GatewayNoneStored),
                (false, stored) => configured(&offer.provider, stored.is_err(), &lookup),
            };
            Row {
                way: Way::Gateway {
                    id: offer.id().to_string(),
                },
                standing,
                stored: held,
            }
        })
        .collect())
}

/// How a gateway a provider block names stands: whether a turn would find a key for it, and where.
///
/// Express mode names no credential in its block and reaches a service that refuses a request with
/// no key, so a block of it holding no key is not signed in, as a known service's is, although the
/// block names no variable ([BACKEND-58](../../../docs/specs/backends.md#BACKEND-58)).
fn configured(
    provider: &Provider,
    keys_unreadable: bool,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Standing {
    match provider.credential_and_source(lookup) {
        (Credential::Token(_), Some(Source::Stored)) => Standing::SignedIn(Held::KeyStored),
        (Credential::Token(_), Some(Source::KeyFile)) => Standing::SignedIn(Held::KeyFromFile),
        (Credential::Token(_), _) => Standing::SignedIn(Held::KeyFromEnvironment),
        (Credential::NotNeeded, _) if provider.is_vertex_express() => {
            Standing::NotSignedIn(Why::GatewayNoneStored)
        }
        (Credential::NotNeeded, _) => Standing::SignedIn(Held::KeyNotNeeded),
        (Credential::Absent | Credential::KeyFileUnusable(_), _) if keys_unreadable => {
            Standing::Unusable(Why::KeysUnreadable)
        }
        (Credential::Absent, _) => Standing::NotSignedIn(Why::GatewayAbsent),
        (Credential::KeyFileUnusable(fault), _) => {
            Standing::NotSignedIn(Why::GatewayKeyFileUnusable(fault))
        }
    }
}

/// What forgetting found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Forgot {
    /// Whether there was anything to forget.
    pub was_there: bool,
    /// For a gateway, the host its key is good at until whoever issued it revokes it.
    pub host: Option<String>,
}

/// Why nothing was forgotten.
#[derive(Debug)]
pub enum ForgetError {
    /// bravebot keeps nothing for this way: the AWS CLI keeps an AWS session.
    NotOurs,
    /// The file of gateway keys at `path` cannot be read, so it is left as it is.
    KeysUnreadable { path: PathBuf },
    /// The file at `path` could not be written or removed.
    NotRemoved {
        path: PathBuf,
        error: std::io::Error,
    },
    /// The stored ChatGPT or xAI account could not be removed.
    Account(AccountError),
}

/// Forget what bravebot keeps for `way` in `directory`, which ends nothing at the service that
/// issued it. Forgetting what is not there is not an error, so it can be repeated.
///
/// The ChatGPT and xAI accounts are kept where their own module says, whatever `directory` is.
pub fn forget(way: &Way, directory: Option<&Path>) -> Result<Forgot, ForgetError> {
    let forgot = |was_there| Forgot {
        was_there,
        host: None,
    };
    match way {
        Way::Bedrock { .. } | Way::Gateways => Err(ForgetError::NotOurs),
        Way::Brave => {
            let Some(directory) = directory else {
                return Ok(forgot(false));
            };
            let path = brave_key::file(directory);
            match std::fs::remove_file(&path) {
                Ok(()) => Ok(forgot(true)),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(forgot(false)),
                Err(error) => Err(ForgetError::NotRemoved { path, error }),
            }
        }
        Way::Gateway { id } => {
            let host = host_a_key_works_at(&Config::from_env(), id);
            let Some(directory) = directory else {
                return Ok(Forgot {
                    was_there: false,
                    host,
                });
            };
            let path = keys::file(directory);
            let mut stored = Keys::read(directory)
                .map_err(|keys::Unreadable| ForgetError::KeysUnreadable { path: path.clone() })?;
            let was_there = stored.get(id).is_some();
            if was_there {
                stored.remove(id);
                write_keys(directory, &stored)
                    .map_err(|error| ForgetError::NotRemoved { path, error })?;
            }
            Ok(Forgot { was_there, host })
        }
        Way::ChatGpt => bravebot_skus::chatgpt::Account::forget()
            .map(forgot)
            .map_err(ForgetError::Account),
        Way::Xai => bravebot_skus::xai::Account::forget()
            .map(forgot)
            .map_err(ForgetError::Account),
    }
}

/// The host a key stored for `id` is good at until whoever issued it revokes it: the host the block
/// that names the id states, for a known service no block names the host compiled in for it, and
/// for Vertex AI with no block the host of express mode, which is the host of the global location
/// as well.
///
/// Nothing for an id that is none of these, such as a gateway whose block has since been taken out.
pub fn host_a_key_works_at(config: &Result<Config, ConfigError>, id: &str) -> Option<String> {
    let block = config.as_ref().ok().and_then(|config| {
        config
            .providers
            .iter()
            .find(|provider| provider.bedrock.is_none() && provider.id == id)
    });
    match block {
        Some(provider) => Some(provider.host().to_string()),
        None => Provider::known(id)
            .or_else(|| {
                (id == GOOGLE_VERTEX_ID)
                    .then(|| Provider::google_vertex(None, None))
                    .flatten()
                    .map(|(provider, _)| provider)
            })
            .map(|provider| provider.host().to_string()),
    }
}

/// Write the gateway keys over the file in `directory`, or remove it where none is left.
///
/// Written beside the file and renamed over it, so a write that stops part way leaves the keys that
/// were there, and created private to this account (STATE-1), because each key is a bearer token.
pub fn write_keys(directory: &Path, stored: &Keys) -> std::io::Result<()> {
    let path = keys::file(directory);
    if stored.is_empty() {
        return match std::fs::remove_file(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            removed => removed,
        };
    }
    home::replace_private(directory, keys::FILE, stored.to_text().expose())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_config::Secret;
    use bravebot_skus::oauth::Defect;

    fn scratch(name: &str) -> PathBuf {
        let path = crate::testutil::scratch_dir("accounts").join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("a scratch directory");
        path
    }

    fn keys_for(ids: &[&str]) -> Keys {
        let mut stored = Keys::default();
        for id in ids {
            stored.insert(id, Secret::new("placeholder-gateway-key".to_string()));
        }
        stored
    }

    fn store_brave(directory: &Path) {
        let key = Secret::new("placeholder-brave-key".to_string());
        std::fs::write(
            brave_key::file(directory),
            brave_key::to_text(&key).expose(),
        )
        .expect("a Brave key");
    }

    fn configured(settings: &str, stored: &Result<Keys, keys::Unreadable>) -> Config {
        let mut config = Config::from_lookup(|name| match name {
            env_var::USE_BEDROCK => Some("1".into()),
            env_var::AWS_REGION => Some("us-east-1".into()),
            env_var::AWS_PROFILE => Some("work".into()),
            _ => None,
        })
        .expect("a configuration");
        config.providers = Settings::parse(settings).providers().to_vec();
        for provider in &mut config.providers {
            provider.stored_key = stored
                .as_ref()
                .ok()
                .and_then(|stored| stored.get(&provider.id).cloned());
        }
        config
    }

    const GATEWAYS: &str = r#"{"provider": {
        "openrouter": {"env": ["OPENROUTER_API_KEY"],
            "options": {"baseURL": "https://openrouter.example/api/v1"}},
        "work": {"env": ["WORK_KEY"], "options": {"baseURL": "https://gateway.work.example/v1"}},
        "local": {"options": {"baseURL": "http://127.0.0.1:8080/v1"}}
    }}"#;

    const TWO_AWS_ACCOUNTS: &str = r#"{"provider": {"amazon-bedrock": {"options": {"region": "us-west-2", "profile": "other"}}}}"#;

    fn inputs(directory: Option<PathBuf>, bedrock: BedrockCheck, session: Session) -> Inputs {
        Inputs {
            config: Ok(configured(GATEWAYS, &Ok(Keys::default()))),
            bedrock_config: Ok(configured(TWO_AWS_ACCOUNTS, &Ok(Keys::default()))),
            managed: Managed::default(),
            directory,
            inherited_profile: Some("work".into()),
            lookup: Box::new(|name| (name == "OPENROUTER_API_KEY").then(|| "sk-env".into())),
            bedrock,
            session,
            in_flight: InFlight::default(),
            chatgpt: Ok(None),
            xai: Ok(None),
        }
    }

    fn good() -> Session {
        Arc::new(|_| Ok(()))
    }

    /// CLI-23 and PREM-16: the Brave key is signed in where its file reads, not signed in where
    /// there is none or nowhere to keep one, and unusable where the file cannot be read; `stored`
    /// follows the file, and nothing reported carries the key.
    #[test]
    fn the_brave_key_stands_as_its_file_does() {
        let directory = scratch("brave");
        let stood = |directory: Option<&Path>| {
            let row = brave(directory);
            assert_eq!(row.way, Way::Brave);
            (row.standing, row.stored)
        };
        assert!(matches!(
            stood(None),
            (Standing::NotSignedIn(Why::Nowhere), false)
        ));
        assert!(matches!(
            stood(Some(&directory)),
            (Standing::NotSignedIn(Why::NothingStored), false)
        ));
        store_brave(&directory);
        assert!(matches!(
            stood(Some(&directory)),
            (Standing::SignedIn(Held::KeyStored), true)
        ));
        std::fs::write(brave_key::file(&directory), "not a key").expect("spoil");
        match stood(Some(&directory)) {
            (Standing::Unusable(Why::BraveUnreadable { path }), true) => {
                assert_eq!(path, brave_key::file(&directory));
            }
            other => panic!("{other:?}"),
        }
    }

    /// A stored account is signed in by its expiry, nothing stored or nowhere to keep one is not
    /// signed in, and a file that cannot be read is unusable under its own way and still stored.
    #[test]
    fn an_account_stands_as_it_loaded() {
        let held = account(Way::ChatGpt, Ok(Some("2026-10-22T21:05:27Z".into())));
        assert!(held.stored);
        assert!(
            matches!(&held.standing, Standing::SignedIn(Held::Account { expires }) if expires == "2026-10-22T21:05:27Z")
        );
        let none = account(Way::Xai, Ok(None));
        assert!(!none.stored);
        assert!(matches!(
            none.standing,
            Standing::NotSignedIn(Why::NothingStored)
        ));
        let nowhere = account(
            Way::ChatGpt,
            Err(AccountError::Nowhere {
                detail: "no home".into(),
            }),
        );
        assert!(!nowhere.stored);
        assert!(matches!(
            nowhere.standing,
            Standing::NotSignedIn(Why::Nowhere)
        ));
        let spoilt = account(Way::Xai, Err(AccountError::Malformed(Defect::NotJson)));
        assert!(spoilt.stored);
        assert!(matches!(
            spoilt.standing,
            Standing::Unusable(Why::Xai(AccountError::Malformed(Defect::NotJson)))
        ));
        let spoilt = account(Way::ChatGpt, Err(AccountError::Malformed(Defect::Empty)));
        assert!(matches!(
            spoilt.standing,
            Standing::Unusable(Why::ChatGpt(_))
        ));
    }

    /// Each AWS session answer is its own standing, an account is listed once per profile, nothing
    /// configured and a configuration that cannot be read say so, and none is ever stored.
    #[test]
    fn each_aws_account_stands_as_its_session_answers() {
        let standings = |answer: fn() -> Asked| {
            bedrock(
                Ok(configured(TWO_AWS_ACCOUNTS, &Ok(Keys::default()))),
                Some("work"),
                |profile| match profile {
                    Some("other") => answer(),
                    _ => Asked::Answered(Ok(())),
                },
            )
        };
        let other = |answer| {
            let rows = standings(answer);
            assert_eq!(rows.len(), 2);
            assert_eq!(
                rows[0].way,
                Way::Bedrock {
                    profile: Some("work".into())
                }
            );
            assert!(matches!(
                rows[0].standing,
                Standing::SignedIn(Held::AwsSession)
            ));
            assert!(rows.iter().all(|row| !row.stored));
            let row = rows.into_iter().nth(1).expect("the second account");
            assert_eq!(
                row.way,
                Way::Bedrock {
                    profile: Some("other".into())
                }
            );
            row.standing
        };
        assert!(matches!(
            other(|| Asked::Answered(Err(CredentialError::Refused { detail: "x".into() }))),
            Standing::NotSignedIn(Why::BedrockLapsed)
        ));
        assert!(matches!(
            other(|| Asked::Answered(Err(CredentialError::NotInstalled))),
            Standing::Unusable(Why::BedrockNoCli)
        ));
        assert!(matches!(
            other(|| Asked::Answered(Err(CredentialError::NoSuchProfile {
                profile: "other".into(),
                available: Vec::new(),
            }))),
            Standing::Unusable(Why::BedrockNoProfile)
        ));
        assert!(matches!(
            other(|| Asked::Answered(Err(CredentialError::Undecodable { detail: "x".into() }))),
            Standing::Unusable(Why::BedrockUndecodable)
        ));
        assert!(matches!(
            other(|| Asked::TimedOut),
            Standing::Unusable(Why::BedrockTimedOut)
        ));
        assert!(matches!(
            other(|| Asked::Skipped),
            Standing::NotSignedIn(Why::NotChecked)
        ));

        let mut none = configured("", &Ok(Keys::default()));
        none.bedrock = None;
        let rows = bedrock(Ok(none), None, |_| Asked::Answered(Ok(())));
        assert!(matches!(
            rows.as_slice(),
            [Row {
                way: Way::Bedrock { profile: None },
                standing: Standing::NotSignedIn(Why::NoAwsAccount),
                stored: false
            }]
        ));
        let rows = bedrock(Err(ConfigError::Empty("X")), None, |_| {
            Asked::Answered(Ok(()))
        });
        assert!(matches!(
            rows.as_slice(),
            [Row {
                standing: Standing::Misconfigured(ConfigError::Empty("X")),
                ..
            }]
        ));
    }

    /// `auth status` and the window: a gateway whose key is read from the file its block names is
    /// signed in from the file, and one whose file gives no key is not signed in and says why, with a
    /// key stored for it winning because a stored key is read before the file. The regressions it
    /// rejects are reading the file as `options.apiKey` is read (reported as an environment key), and
    /// reporting a key file that gives no key as a gateway that names no credential, which would read
    /// as signed in.
    #[test]
    fn a_key_file_that_gives_no_key_is_not_signed_in_and_one_that_does_is() {
        let directory = scratch("key-file-standing");
        let file = directory.join("key.txt");
        let block = format!(
            r#"{{"provider": {{"gw": {{"options": {{"baseURL": "https://gw.example/v1",
                "keyFile": {}}}}}}}}}"#,
            serde_json::json!(file.to_str().expect("a path"))
        );
        let standing = |stored: Result<Keys, keys::Unreadable>| {
            let rows = gateways(
                Ok(configured(&block, &stored)),
                &Managed::default(),
                &stored,
                Some("gw"),
                |_: &str| None,
            )
            .expect("rows");
            rows.into_iter().next().expect("a row").standing
        };

        // No file: not signed in, with the fault named and no other source standing in.
        assert!(matches!(
            standing(Ok(Keys::default())),
            Standing::NotSignedIn(Why::GatewayKeyFileUnusable(
                bravebot_config::provider::KeyFileFault::Unreadable
            ))
        ));
        // A key stored for the id is read before the file, so it makes the gateway usable.
        assert!(matches!(
            standing(Ok(keys_for(&["gw"]))),
            Standing::SignedIn(Held::KeyStored)
        ));
        // A file that holds a key: signed in from the file.
        std::fs::write(&file, "placeholder-file-key\n").expect("a file");
        assert!(matches!(
            standing(Ok(Keys::default())),
            Standing::SignedIn(Held::KeyFromFile)
        ));
        // A file that holds the wrong thing is the same not-signed-in, with its own fault.
        std::fs::write(&file, "two words").expect("a file");
        assert!(matches!(
            standing(Ok(Keys::default())),
            Standing::NotSignedIn(Why::GatewayKeyFileUnusable(
                bravebot_config::provider::KeyFileFault::NotAKey
            ))
        ));
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// A gateway's key is stored, from a variable or not needed; a known service no block names is
    /// unrecorded where a key is stored for it, and `stored` follows the file of keys alone.
    #[test]
    fn each_gateway_stands_as_its_key_is_found() {
        let lookup = |name: &str| (name == "OPENROUTER_API_KEY").then(|| "sk-env".to_string());
        let rows = |stored: Result<Keys, keys::Unreadable>, named| {
            gateways(
                Ok(configured(GATEWAYS, &stored)),
                &Managed::default(),
                &stored,
                named,
                lookup,
            )
            .expect("rows")
        };
        let found = rows(Ok(keys_for(&["work", "anthropic"])), None);
        let summary: Vec<(String, bool, bool)> = found
            .iter()
            .map(|row| match &row.way {
                Way::Gateway { id } => (id.clone(), row.standing.usable(), row.stored),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("local".to_string(), true, false),
                ("openrouter".to_string(), true, false),
                ("work".to_string(), true, true),
                ("anthropic".to_string(), false, true),
            ]
        );
        assert!(matches!(
            found[0].standing,
            Standing::SignedIn(Held::KeyNotNeeded)
        ));
        assert!(matches!(
            found[1].standing,
            Standing::SignedIn(Held::KeyFromEnvironment)
        ));
        assert!(matches!(
            found[2].standing,
            Standing::SignedIn(Held::KeyStored)
        ));
        assert!(matches!(
            found[3].standing,
            Standing::NotSignedIn(Why::GatewayUnrecorded)
        ));

        let absent = rows(Ok(Keys::default()), Some("work"));
        assert!(matches!(
            absent.as_slice(),
            [Row {
                standing: Standing::NotSignedIn(Why::GatewayAbsent),
                stored: false,
                ..
            }]
        ));
        let unrecorded = rows(Ok(Keys::default()), Some("openai"));
        assert!(matches!(
            unrecorded.as_slice(),
            [Row {
                standing: Standing::NotSignedIn(Why::GatewayNoneStored),
                stored: false,
                ..
            }]
        ));
        let unreadable = rows(Err(keys::Unreadable), Some("work"));
        assert!(matches!(
            unreadable.as_slice(),
            [Row {
                standing: Standing::Unusable(Why::KeysUnreadable),
                ..
            }]
        ));

        let nothing = gateways(
            Ok(configured("", &Ok(Keys::default()))),
            &Managed::default(),
            &Ok(Keys::default()),
            None,
            lookup,
        )
        .expect("a row");
        assert!(matches!(
            nothing.as_slice(),
            [Row {
                way: Way::Gateways,
                standing: Standing::NotSignedIn(Why::NothingStored),
                stored: false
            }]
        ));
        let refused = gateways(
            Ok(configured(GATEWAYS, &Ok(Keys::default()))),
            &Managed::default(),
            &Ok(Keys::default()),
            Some("nosuch"),
            lookup,
        );
        match refused {
            Err(ListError::NotOffered { offered, managed }) => {
                assert!(!managed);
                assert!(offered.starts_with(&["local".into(), "openrouter".into(), "work".into()]));
            }
            other => panic!("{other:?}"),
        }
    }

    /// The list follows what is on disk in the directory it is given, in the order Brave, Bedrock,
    /// the gateways, ChatGPT, xAI.
    #[test]
    fn the_list_reads_the_directory_it_is_given() {
        let directory = scratch("list");
        store_brave(&directory);
        write_keys(&directory, &keys_for(&["anthropic"])).expect("keys");
        let rows = list(
            inputs(Some(directory.clone()), BedrockCheck::Skip, good()),
            Which::All,
        )
        .expect("rows");
        let ways: Vec<(&Way, bool)> = rows.iter().map(|row| (&row.way, row.stored)).collect();
        assert_eq!(
            ways,
            [
                (&Way::Brave, true),
                (
                    &Way::Bedrock {
                        profile: Some("work".into())
                    },
                    false
                ),
                (
                    &Way::Bedrock {
                        profile: Some("other".into())
                    },
                    false
                ),
                (&Way::Gateway { id: "local".into() }, false),
                (
                    &Way::Gateway {
                        id: "openrouter".into()
                    },
                    false
                ),
                (&Way::Gateway { id: "work".into() }, false),
                (
                    &Way::Gateway {
                        id: "anthropic".into()
                    },
                    true
                ),
                (&Way::ChatGpt, false),
                (&Way::Xai, false),
            ]
        );
        let brave_alone = list(
            inputs(Some(directory), BedrockCheck::Skip, good()),
            Which::Brave,
        )
        .expect("a row");
        assert!(matches!(
            brave_alone.as_slice(),
            [Row {
                way: Way::Brave,
                standing: Standing::SignedIn(Held::KeyStored),
                stored: true
            }]
        ));
    }

    /// An `aws` that never answers is reported as timed out once the time allowed has passed, rather
    /// than holding up the list.
    #[test]
    fn a_hanging_aws_session_times_out() {
        let hangs: Session = Arc::new(|_| {
            std::thread::sleep(Duration::from_secs(30));
            Ok(())
        });
        let started = std::time::Instant::now();
        let rows = list(
            inputs(
                None,
                BedrockCheck::Check {
                    timeout: Duration::from_millis(50),
                },
                hangs,
            ),
            Which::Bedrock,
        )
        .expect("rows");
        assert!(started.elapsed() < Duration::from_secs(10));
        assert_eq!(rows.len(), 2);
        assert!(
            rows.iter()
                .all(|row| matches!(row.standing, Standing::Unusable(Why::BedrockTimedOut)))
        );
        let answered = list(
            inputs(
                None,
                BedrockCheck::Check {
                    timeout: Duration::MAX,
                },
                good(),
            ),
            Which::Bedrock,
        )
        .expect("rows");
        assert!(answered.iter().all(|row| row.standing.usable()));
    }

    /// Asked one after another, a few slow profiles would push the listing past its deadline.
    #[test]
    fn every_aws_account_is_asked_at_once() {
        let slow: Session = Arc::new(|_| {
            std::thread::sleep(Duration::from_millis(600));
            Ok(())
        });
        let started = std::time::Instant::now();
        let rows = list(
            inputs(
                None,
                BedrockCheck::Check {
                    timeout: Duration::from_secs(5),
                },
                slow,
            ),
            Which::Bedrock,
        )
        .expect("rows");
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| row.standing.usable()));
        assert!(started.elapsed() < Duration::from_millis(1100));
    }

    /// A hung `aws` restarted on every listing would pile up processes until the app quits.
    #[test]
    fn an_aws_check_still_running_is_not_started_again() {
        let asked = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = Arc::clone(&asked);
        let hangs: Session = Arc::new(move |_| {
            counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::thread::sleep(Duration::from_secs(30));
            Ok(())
        });
        let in_flight = InFlight::default();
        let listing = || {
            let mut inputs = inputs(
                None,
                BedrockCheck::Check {
                    timeout: Duration::from_millis(50),
                },
                Arc::clone(&hangs),
            );
            inputs.in_flight = Arc::clone(&in_flight);
            list(inputs, Which::Bedrock).expect("rows")
        };
        let first = listing();
        let second = listing();
        assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 2);
        for rows in [first, second] {
            assert!(
                rows.iter()
                    .all(|row| matches!(row.standing, Standing::Unusable(Why::BedrockTimedOut)))
            );
        }
    }

    /// CLI-18 and PREM-16: forgetting removes the one credential named and nothing beside it, says
    /// whether it was there, can be repeated, and refuses an AWS account, whose session the AWS CLI
    /// keeps.
    #[test]
    fn forgetting_removes_only_the_credential_named() {
        let directory = scratch("forget");
        store_brave(&directory);
        write_keys(&directory, &keys_for(&["openai", "work"])).expect("keys");

        let forgot = forget(
            &Way::Gateway {
                id: "openai".into(),
            },
            Some(&directory),
        )
        .expect("forgot");
        assert!(forgot.was_there);
        assert_eq!(forgot.host.as_deref(), Some("api.openai.com"));
        let left = Keys::read(&directory).expect("keys");
        assert_eq!(left.ids().collect::<Vec<_>>(), ["work"]);
        assert!(brave_key::file(&directory).exists());
        assert!(
            !forget(
                &Way::Gateway {
                    id: "openai".into()
                },
                Some(&directory)
            )
            .expect("again")
            .was_there
        );

        assert!(
            forget(&Way::Brave, Some(&directory))
                .expect("forgot")
                .was_there
        );
        assert!(!brave_key::file(&directory).exists());
        assert!(keys::file(&directory).exists());
        assert!(
            !forget(&Way::Brave, Some(&directory))
                .expect("again")
                .was_there
        );
        assert!(!forget(&Way::Brave, None).expect("nowhere").was_there);

        forget(&Way::Gateway { id: "work".into() }, Some(&directory)).expect("the last");
        assert!(!keys::file(&directory).exists());

        assert!(matches!(
            forget(&Way::Bedrock { profile: None }, Some(&directory)),
            Err(ForgetError::NotOurs)
        ));
        std::fs::write(keys::file(&directory), "not keys").expect("spoil");
        assert!(matches!(
            forget(&Way::Gateway { id: "work".into() }, Some(&directory)),
            Err(ForgetError::KeysUnreadable { .. })
        ));
        assert_eq!(
            std::fs::read_to_string(keys::file(&directory)).expect("left"),
            "not keys"
        );
    }

    const VERTEX_PROJECT: &str = r#"{"provider": {"google-vertex": {"options":
        {"project": "example-project-1", "location": "us-east5"}}}}"#;

    /// A Vertex AI offer as `(not named by a block, host, states no field)`.
    fn vertex_offers(offers: &[Offer<'_>]) -> Vec<(bool, String, bool)> {
        offers
            .iter()
            .filter(|offer| offer.id() == "google-vertex")
            .map(|offer| {
                (
                    offer.unconfigured,
                    offer.provider.host().to_string(),
                    offer.entry.is_empty(),
                )
            })
            .collect()
    }

    /// CLI-18 and BACKEND-17: Vertex AI is offered once, in express mode, where nothing names it, as
    /// the configured gateway where a block or the environment does, again where an entry names it and
    /// configures nothing, and never where the machine-level layer names the gateways, since a block
    /// recorded in the person's file would not be read there. The regressions it rejects are offering
    /// it beside a managed list, and offering it twice beside the service the environment names, which
    /// would record a block that replaces that route.
    #[test]
    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    fn google_vertex_is_offered_once_in_express_mode_and_not_under_a_machine_level_list() {
        let express = (true, "aiplatform.googleapis.com".to_string(), true);
        assert_eq!(
            vertex_offers(&offered(&[], &Managed::default())),
            std::slice::from_ref(&express)
        );

        let block = Settings::parse(VERTEX_PROJECT);
        assert_eq!(
            vertex_offers(&offered(block.providers(), &Managed::default())),
            [(
                false,
                "us-east5-aiplatform.googleapis.com".to_string(),
                true
            )]
        );

        let nothing = Settings::parse("");
        let key = |name: &str| (name == "GOOGLE_API_KEY").then(|| "placeholder-key".to_string());
        let from_environment = bravebot_config::gateways(&nothing, &Managed::default(), &key);
        assert_eq!(
            vertex_offers(&offered(&from_environment, &Managed::default())),
            [(false, "aiplatform.googleapis.com".to_string(), true)]
        );

        let configures_nothing =
            Settings::parse(r#"{"provider": {"google-vertex": {"options": {"project": "a/b"}}}}"#);
        assert_eq!(
            vertex_offers(&offered(
                configures_nothing.providers(),
                &Managed::default()
            )),
            [express]
        );

        let directory = scratch("vertex-managed");
        let file = directory.join("managed.json");
        std::fs::write(
            &file,
            r#"{"provider": {"corp": {"options": {"baseURL": "https://corp.example/v1"}}}}"#,
        )
        .expect("a machine-level file");
        let managed = Managed::at(&file);
        for lookup in [
            Box::new(|_: &str| None) as Box<dyn Fn(&str) -> Option<String>>,
            Box::new(key),
        ] {
            let pinned = bravebot_config::gateways(&nothing, &managed, &lookup);
            let ids: Vec<String> = offered(&pinned, &managed)
                .iter()
                .map(|offer| offer.id().to_string())
                .collect();
            assert_eq!(ids, ["corp"]);
        }
        std::fs::remove_dir_all(&directory).expect("the scratch directory removed");
    }

    /// CLI-23, BACKEND-58, BACKEND-65 and BACKEND-50: how `auth status` stands for Vertex AI. An
    /// express block holding no key is not signed in, since the service refuses a request with none,
    /// where a block of a gateway that names no credential needs none. A key stored for it is found,
    /// a variable the environment names is read before a stored key, and a project with no key is not
    /// signed in where its block or the environment names `GOOGLE_API_KEY` and it is unset, although
    /// the Google Cloud CLI would sign the request. The regression it rejects is reporting the
    /// express block as signed in with a key not needed, which stays true after `auth logout`.
    #[test]
    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    fn google_vertex_stands_as_its_key_is_found() {
        const EXPRESS: &str = r#"{"provider": {"google-vertex": {}}}"#;
        const PROJECT_NAMING_NO_KEY: &str = VERTEX_PROJECT;
        const PROJECT_NAMING_THE_VARIABLE: &str = r#"{"provider": {"google-vertex": {
            "env": ["GOOGLE_API_KEY"], "options": {"project": "example-project-1"}}}}"#;
        let standing = |settings: &str, stored: &[&str], exported: &[(&str, &str)]| {
            let lookup = |name: &str| {
                exported
                    .iter()
                    .find(|(exported, _)| *exported == name)
                    .map(|(_, value)| value.to_string())
            };
            let keys = keys_for(stored);
            let mut config = configured("", &Ok(Keys::default()));
            config.providers =
                bravebot_config::gateways(&Settings::parse(settings), &Managed::default(), &lookup);
            for provider in &mut config.providers {
                provider.stored_key = keys.get(&provider.id).cloned();
            }
            let rows = gateways(
                Ok(config),
                &Managed::default(),
                &Ok(keys),
                Some("google-vertex"),
                lookup,
            )
            .expect("a row");
            assert_eq!(rows.len(), 1);
            rows.into_iter().next().expect("a row").standing
        };
        let key = [("GOOGLE_API_KEY", "placeholder-exported-key")];
        let project = [("GOOGLE_CLOUD_PROJECT", "example-project-1")];
        let nothing: [(&str, &str); 0] = [];

        assert!(matches!(
            standing(EXPRESS, &[], &nothing),
            Standing::NotSignedIn(Why::GatewayNoneStored)
        ));
        assert!(matches!(
            standing(EXPRESS, &["google-vertex"], &nothing),
            Standing::SignedIn(Held::KeyStored)
        ));
        // An entry that authenticates by a header of its own is not express mode, so it names no
        // credential and needs none, as a block of any gateway does.
        assert!(matches!(
            standing(
                r#"{"provider": {"google-vertex": {"options": {"headers": {"x-goog-api-key": "a-value"}}}}}"#,
                &[],
                &nothing
            ),
            Standing::SignedIn(Held::KeyNotNeeded)
        ));
        assert!(matches!(
            standing("", &[], &key),
            Standing::SignedIn(Held::KeyFromEnvironment)
        ));
        assert!(matches!(
            standing("", &["google-vertex"], &key),
            Standing::SignedIn(Held::KeyFromEnvironment)
        ));
        assert!(matches!(
            standing("", &["google-vertex"], &project),
            Standing::SignedIn(Held::KeyStored)
        ));
        assert!(matches!(
            standing("", &[], &project),
            Standing::NotSignedIn(Why::GatewayAbsent)
        ));
        assert!(matches!(
            standing(PROJECT_NAMING_THE_VARIABLE, &[], &nothing),
            Standing::NotSignedIn(Why::GatewayAbsent)
        ));
        assert!(matches!(
            standing(PROJECT_NAMING_NO_KEY, &[], &nothing),
            Standing::SignedIn(Held::KeyNotNeeded)
        ));
        assert!(matches!(
            standing("", &[], &nothing),
            Standing::NotSignedIn(Why::GatewayNoneStored)
        ));
        assert!(matches!(
            standing("", &["google-vertex"], &nothing),
            Standing::NotSignedIn(Why::GatewayUnrecorded)
        ));
    }

    /// CLI-18: forgetting a key stored for Vertex AI names the host the key still works at, which is
    /// the one its block states, and the express host where no block names it, so the sentence names
    /// a host as it does for every service the program can sign in to. The regression it rejects is
    /// the sentence for a key whose issuer is unknown, which says nothing about where it works.
    #[test]
    fn a_key_stored_for_google_vertex_is_said_to_work_at_the_host_its_block_or_express_mode_names()
    {
        let host = |settings: &str| {
            host_a_key_works_at(
                &Ok(configured(settings, &Ok(Keys::default()))),
                "google-vertex",
            )
        };
        assert_eq!(host("").as_deref(), Some("aiplatform.googleapis.com"));
        assert_eq!(
            host(VERTEX_PROJECT).as_deref(),
            Some("us-east5-aiplatform.googleapis.com")
        );
        assert_eq!(
            host_a_key_works_at(&Err(ConfigError::Missing("a variable")), "google-vertex")
                .as_deref(),
            Some("aiplatform.googleapis.com")
        );
        assert_eq!(
            host_a_key_works_at(&Ok(configured("", &Ok(Keys::default()))), "nobody"),
            None
        );
    }
}
