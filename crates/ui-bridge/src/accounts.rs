//! The window's Accounts block: how each way of reaching a model service stands, signing out of the
//! ones bravebot keeps something for, and the ChatGPT and xAI sign-ins the window starts itself
//! ([CHATGPT-15](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-15),
//! [XAI-20](../../../docs/specs/xai-sign-in.md#XAI-20)).
//!
//! The standings are [`bravebot_agent::accounts`]'s, put into the window's words here. No row and no
//! event carries a key, a token, an account's identity or anything a service wrote.

use crate::emit::Emitter;
use crate::protocol::{ErrorCode, Event, Failure, Request};
use bravebot_agent::accounts::{self, BedrockCheck, Held, Inputs, Row, Standing, Way, Which, Why};
use bravebot_agent::home;
use bravebot_config::{Config, ConfigError, Managed};
use bravebot_i18n::t;
use bravebot_skus::chatgpt::{Class, Hosts as ChatGptHosts, Limits, Pages};
use bravebot_skus::xai::Hosts as XaiHosts;
use serde_json::{Value, json};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How long each AWS account's session is asked about before its row says it did not answer.
const BEDROCK_TIMEOUT: Duration = Duration::from_secs(5);

/// What a session in `project` reads, under the settings file the window selected.
fn config(project: Option<&Path>, selected: Option<&Path>) -> Result<Config, ConfigError> {
    Config::from_env_and_settings(
        &crate::settings::layers(project, selected),
        &Managed::load(),
    )
}

/// `accounts.list`: every row, in the window's words. `"bedrock": false` reports each AWS account
/// as not checked, for a caller that needs no AWS row's standing and should not wait on the AWS CLI.
pub(crate) fn list_asked(
    request: &Request,
    project: Option<&Path>,
    selected: Option<&Path>,
) -> Result<Value, Failure> {
    match request.params.get("bedrock") {
        None | Some(Value::Bool(true)) => list(project, selected),
        Some(Value::Bool(false)) => list_with(project, selected, BedrockCheck::Skip),
        Some(_) => Err(Failure::bad_request("`bedrock` must be true or false")),
    }
}

/// Every row, in the window's words, with each AWS account's session asked about.
pub(crate) fn list(project: Option<&Path>, selected: Option<&Path>) -> Result<Value, Failure> {
    list_with(
        project,
        selected,
        BedrockCheck::Check {
            timeout: BEDROCK_TIMEOUT,
        },
    )
}

fn list_with(
    project: Option<&Path>,
    selected: Option<&Path>,
    bedrock: BedrockCheck,
) -> Result<Value, Failure> {
    if let Some(path) = selected {
        crate::settings::validate(path)?;
    }
    let layers = crate::settings::layers(project, selected);
    let managed = Managed::load();
    let inputs = Inputs {
        config: config(project, selected),
        bedrock_config: accounts::bedrock_config(&layers, &managed),
        managed,
        ..Inputs::read(bedrock)
    };
    // `Which::All` names no gateway, and only a named one can be refused.
    let mut rows = accounts::list(inputs, Which::All)
        .map_err(|_| Failure::new(ErrorCode::Internal, "the accounts could not be listed"))?;
    let hosts = config(project, selected);
    with_known_first(&mut rows, &hosts, &Managed::load());
    let shown: Vec<Value> = rows
        .iter()
        .filter(|row| shown(row))
        .map(|row| worded(row, &hosts))
        .collect();
    Ok(json!({ "accounts": shown }))
}

/// The known services the window offers a key for even where nothing names them yet, so a person
/// can start with one from the window as `bravebot auth login gateway` lets them from a terminal.
const KNOWN_FIRST: &[&str] = &["openrouter"];

/// Each of [`KNOWN_FIRST`] that a key can be stored for and has no row, as a gateway with no key,
/// before the ChatGPT row.
fn with_known_first(rows: &mut Vec<Row>, config: &Result<Config, ConfigError>, managed: &Managed) {
    let Ok(providers) = bravebot_agent::key_entry::configured_gateways(config) else {
        return;
    };
    let offers = accounts::offered(providers, managed);
    for id in KNOWN_FIRST {
        let listed = rows
            .iter()
            .any(|row| matches!(&row.way, Way::Gateway { id: listed } if listed == id));
        if listed || !offers.iter().any(|offer| offer.id() == *id) {
            continue;
        }
        let at = rows
            .iter()
            .position(|row| matches!(row.way, Way::ChatGpt | Way::Xai))
            .unwrap_or(rows.len());
        rows.insert(
            at,
            Row {
                way: Way::Gateway { id: id.to_string() },
                standing: Standing::NotSignedIn(Why::GatewayNoneStored),
                stored: false,
            },
        );
    }
}

/// Whether the block draws a row. The row about the gateways as a whole says only that none is set
/// up, which is not a thing to sign in to, so it is drawn where something is wrong with them. A
/// configuration that names no AWS account has no Bedrock account to show.
fn shown(row: &Row) -> bool {
    !matches!(
        (&row.way, &row.standing),
        (Way::Gateways, Standing::NotSignedIn(_))
            | (
                Way::Bedrock { .. },
                Standing::NotSignedIn(Why::NoAwsAccount)
            )
    )
}

/// One row as the window draws it.
fn worded(row: &Row, config: &Result<Config, ConfigError>) -> Value {
    let (way, id) = match &row.way {
        Way::Brave => ("brave", None),
        Way::Bedrock { profile } => ("bedrock", profile.clone()),
        Way::Gateway { id } => ("gateway", Some(id.clone())),
        Way::Gateways => ("gateway", None),
        Way::ChatGpt => ("chatgpt", None),
        Way::Xai => ("xai", None),
    };
    let standing = match &row.standing {
        Standing::SignedIn(_) => "signedIn",
        Standing::NotSignedIn(_) => "notSignedIn",
        Standing::Unusable(_) => "unusable",
        Standing::Misconfigured(_) => "misconfigured",
    };
    let key_from = match &row.standing {
        Standing::SignedIn(Held::KeyStored) => Some("stored"),
        Standing::SignedIn(Held::KeyFromEnvironment) => Some("environment"),
        Standing::SignedIn(Held::KeyNotNeeded) => Some("none"),
        _ => None,
    };
    let host = match &row.way {
        Way::Gateway { id } => accounts::host_a_key_works_at(config, id),
        _ => None,
    };
    json!({
        "way": way,
        "id": id,
        "name": name(&row.way),
        "standing": standing,
        "keyFrom": key_from,
        "detail": detail(&row.standing),
        "command": command(&row.way, &row.standing),
        "canSignOut": can_sign_out(row),
        "canEnterKey": can_enter_key(row),
        "host": host,
    })
}

/// Whether the window offers its own key entry for the row: the Brave key and a gateway's, except
/// where nothing could be kept or the gateway takes no key.
fn can_enter_key(row: &Row) -> bool {
    match (&row.way, &row.standing) {
        (_, Standing::Misconfigured(_) | Standing::NotSignedIn(Why::Nowhere)) => false,
        (Way::Gateway { .. }, Standing::SignedIn(Held::KeyNotNeeded)) => false,
        (Way::Brave | Way::Gateway { .. }, _) => true,
        _ => false,
    }
}

/// Whether signing out of the row removes something: only what bravebot keeps, which an AWS session
/// never is.
fn can_sign_out(row: &Row) -> bool {
    row.stored && !matches!(row.way, Way::Bedrock { .. } | Way::Gateways)
}

fn name(way: &Way) -> String {
    match way {
        Way::Brave => t!(desktop_accounts_name_brave).into(),
        Way::Bedrock { profile: None } => t!(desktop_accounts_name_bedrock).into(),
        Way::Bedrock {
            profile: Some(profile),
        } => t!(
            desktop_accounts_name_bedrock_profile,
            profile = profile.clone()
        ),
        Way::Gateway { id } => id.clone(),
        Way::Gateways => t!(desktop_accounts_name_gateways).into(),
        Way::ChatGpt => t!(desktop_accounts_name_chatgpt).into(),
        Way::Xai => t!(desktop_accounts_name_xai).into(),
    }
}

/// The sentence under a row that is not signed in for a reason worth saying, or cannot be used. It
/// is chosen from the reason alone, so it repeats nothing a file, a service or the AWS CLI wrote.
fn detail(standing: &Standing) -> Option<String> {
    let why = match standing {
        Standing::SignedIn(_) => return None,
        Standing::Misconfigured(_) => return Some(t!(desktop_accounts_misconfigured).into()),
        Standing::NotSignedIn(why) | Standing::Unusable(why) => why,
    };
    let said: &str = match why {
        Why::NothingStored | Why::GatewayNoneStored | Why::NoAwsAccount => return None,
        Why::Nowhere => t!(desktop_accounts_nowhere),
        Why::BraveUnreadable { .. } => t!(desktop_accounts_brave_unreadable),
        Why::ChatGpt(_) => t!(desktop_accounts_chatgpt_unreadable),
        Why::Xai(_) => t!(desktop_accounts_xai_unreadable),
        Why::BedrockLapsed => t!(desktop_accounts_bedrock_lapsed),
        Why::BedrockNoProfile => t!(desktop_accounts_bedrock_no_profile),
        Why::BedrockNoCli => t!(desktop_accounts_bedrock_no_cli),
        Why::BedrockUndecodable => t!(desktop_accounts_bedrock_undecodable),
        Why::BedrockTimedOut => t!(desktop_accounts_bedrock_timed_out),
        Why::GatewayNoneConfigured => t!(desktop_accounts_gateway_none_configured),
        Why::GatewayUnrecorded => t!(desktop_accounts_gateway_unrecorded),
        Why::GatewayAbsent => t!(desktop_accounts_gateway_absent),
        Why::KeysUnreadable => t!(desktop_accounts_keys_unreadable),
        Why::NotChecked => t!(desktop_accounts_not_checked),
    };
    Some(said.to_string())
}

/// The command for the bravebot command-line tool that signs the row in, where signing in is the
/// remedy. None for an id a shell would read as more than one word, since the command would not work
/// as typed.
fn command(way: &Way, standing: &Standing) -> Option<String> {
    match standing {
        Standing::SignedIn(_) | Standing::Misconfigured(_) => return None,
        Standing::NotSignedIn(Why::Nowhere | Why::GatewayNoneConfigured)
        | Standing::Unusable(Why::BedrockNoCli | Why::BedrockNoProfile | Why::BedrockTimedOut) => {
            return None;
        }
        _ => {}
    }
    Some(match way {
        Way::Brave => "bravebot auth login brave".to_string(),
        Way::Bedrock { .. } => "bravebot auth login bedrock".to_string(),
        Way::Gateway { id } if typed_as_is(id) => format!("bravebot auth login gateway {id}"),
        Way::Gateway { .. } => return None,
        Way::Gateways => "bravebot auth login gateway".to_string(),
        Way::ChatGpt => "bravebot auth login chatgpt".to_string(),
        Way::Xai => "bravebot auth login xai".to_string(),
    })
}

/// Whether `id` is one word to a shell, with nothing in it the shell would expand.
fn typed_as_is(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// `accounts.signOut`: forget what bravebot keeps for one way, and answer with the rows as they now
/// stand. Forgetting what is not there is not an error.
pub(crate) fn sign_out(
    request: &Request,
    project: Option<&Path>,
    selected: Option<&Path>,
) -> Result<Value, Failure> {
    let way = match request.string("way")?.as_str() {
        "brave" => Way::Brave,
        "chatgpt" => Way::ChatGpt,
        "xai" => Way::Xai,
        "gateway" => Way::Gateway {
            id: request
                .optional_string("id")
                .filter(|id| !id.is_empty())
                .ok_or_else(|| Failure::bad_request("signing out of a gateway needs its id"))?,
        },
        "bedrock" => {
            return Err(Failure::bad_request(
                "an AWS session is the AWS CLI's, so bravebot has nothing to sign out of",
            ));
        }
        other => return Err(Failure::bad_request(format!("unknown way `{other}`"))),
    };
    if let Err(failed) = accounts::forget(&way, home::directory().as_deref()) {
        let said: &str = match failed {
            accounts::ForgetError::NotOurs => {
                return Err(Failure::bad_request("bravebot keeps nothing for that way"));
            }
            accounts::ForgetError::KeysUnreadable { .. } => t!(desktop_accounts_keys_unreadable),
            accounts::ForgetError::NotRemoved { .. } | accounts::ForgetError::Account(_) => {
                t!(desktop_accounts_not_removed)
            }
        };
        return Err(Failure::new(ErrorCode::NoHome, said));
    }
    list(project, selected)
}

/// The sign-in running, if there is one. One at a time across both accounts, since each holds a
/// window the main process owns and a person finishes one at a time.
#[derive(Default)]
pub(crate) struct SignIns {
    running: Arc<Mutex<Option<Running>>>,
    next: u64,
}

struct Running {
    attempt: String,
    stop: Arc<AtomicBool>,
    /// The AWS CLI an AWS sign-in runs, which a stop ends at once.
    aws: Option<Arc<bravebot_agent::backend::AwsStop>>,
}

impl Running {
    fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(aws) = &self.aws {
            aws.stop();
        }
    }
}

/// Which account a sign-in is for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Account {
    ChatGpt,
    Xai,
}

impl Account {
    fn word(self) -> &'static str {
        match self {
            Account::ChatGpt => "chatgpt",
            Account::Xai => "xai",
        }
    }
}

/// What is read before a sign-in starts, so the refusals can be decided without the process.
struct Before {
    writable: bool,
    config: Result<Config, ConfigError>,
    stored: bool,
}

/// Why a sign-in is refused before anything binds or is asked, or `None` where it may start
/// ([CHATGPT-15](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-15),
/// [XAI-20](../../../docs/specs/xai-sign-in.md#XAI-20)).
fn refusal(account: Account, before: &Before, replace: bool) -> Option<&'static str> {
    if !before.writable {
        return Some(t!(desktop_accounts_sign_in_nowhere));
    }
    let Ok(config) = &before.config else {
        return Some(t!(desktop_accounts_sign_in_misconfigured));
    };
    match account {
        Account::ChatGpt if !config.offers_chatgpt_account() => {
            return Some(t!(desktop_accounts_sign_in_chatgpt_is_a_gateway));
        }
        Account::Xai if !config.offers_xai_account() => {
            return Some(t!(desktop_accounts_sign_in_xai_is_a_gateway));
        }
        _ => {}
    }
    if before.stored && !replace {
        return Some(t!(desktop_accounts_sign_in_stored));
    }
    None
}

impl SignIns {
    /// `accounts.signIn`: answer with the attempt at once, and say how it goes in
    /// `accounts.signIn.started` and `accounts.signIn.ended`. A refusal is an `ended` with no
    /// `started`, sent before the answer.
    pub(crate) fn start(
        &mut self,
        request: &Request,
        project: Option<&Path>,
        selected: Option<&Path>,
        emitter: &Emitter,
    ) -> Result<Value, Failure> {
        let account = match request.string("way")?.as_str() {
            "chatgpt" => Account::ChatGpt,
            "xai" => Account::Xai,
            "bedrock" => return self.start_bedrock(request, project, selected, emitter),
            other => {
                return Err(Failure::bad_request(format!(
                    "the window signs in only to chatgpt, xai or bedrock, not `{other}`"
                )));
            }
        };
        let replace = request
            .params
            .get("replace")
            .and_then(Value::as_bool)
            .ok_or_else(|| Failure::bad_request("`replace` must be true or false"))?;
        if let Some(path) = selected {
            crate::settings::validate(path)?;
        }
        let mut running = self
            .running
            .lock()
            .map_err(|_| Failure::new(ErrorCode::Internal, "the sign-in state is unavailable"))?;
        if running.is_some() {
            return Err(Failure::bad_request("a sign-in is already running"));
        }
        self.next += 1;
        let attempt = self.next.to_string();
        let loaded = match account {
            Account::ChatGpt => accounts::account(Way::ChatGpt, accounts::chatgpt_loaded()),
            Account::Xai => accounts::account(Way::Xai, accounts::xai_loaded()),
        };
        let before = Before {
            writable: home::writable().is_some(),
            config: config(project, selected),
            stored: loaded.stored,
        };
        if let Some(refused) = refusal(account, &before, replace) {
            emitter.send(ended(&attempt, account, "failed", Some(refused)));
            return Ok(json!({ "attempt": attempt }));
        }
        let stop = Arc::new(AtomicBool::new(false));
        *running = Some(Running {
            attempt: attempt.clone(),
            stop: Arc::clone(&stop),
            aws: None,
        });
        drop(running);
        let slot = Arc::clone(&self.running);
        let emitter = emitter.clone();
        let id = attempt.clone();
        // Not joined at shutdown: a process that is ending stores nothing it was waiting for, since
        // the flow stops before its store once the thread is gone
        // ([CHATGPT-17](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-17)).
        std::thread::spawn(move || {
            let (outcome, sentence) = run(account, &id, &stop, &emitter);
            if let Ok(mut running) = slot.lock()
                && running
                    .as_ref()
                    .is_some_and(|running| running.attempt == id)
            {
                *running = None;
            }
            emitter.send(ended(&id, account, outcome, sentence.as_deref()));
        });
        Ok(json!({ "attempt": attempt }))
    }

    /// `accounts.cancelSignIn`: stop the attempt named, if it is the one running. One that has ended
    /// is not an error, since the end may have crossed the request.
    pub(crate) fn cancel(&self, request: &Request) -> Result<Value, Failure> {
        let attempt = request.string("attempt")?;
        if let Ok(running) = self.running.lock()
            && let Some(running) = running.as_ref()
            && running.attempt == attempt
        {
            running.stop();
        }
        Ok(json!({}))
    }

    /// The bridge is ending: whatever sign-in runs is stopped, and nobody waits for it.
    pub(crate) fn stop_running(&self) {
        if let Ok(running) = self.running.lock()
            && let Some(running) = running.as_ref()
        {
            running.stop();
        }
    }
}

fn ended(attempt: &str, account: Account, outcome: &str, sentence: Option<&str>) -> Event {
    Event::global(
        "accounts.signIn.ended",
        json!({
            "attempt": attempt,
            "way": account.word(),
            "outcome": outcome,
            "sentence": sentence,
        }),
    )
}

/// The sign-in itself, over the transport the network crate resolved. What it announces goes to the
/// main process, which shows the address and the code only in the window it owns
/// ([CHATGPT-16](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-16),
/// [XAI-21](../../../docs/specs/xai-sign-in.md#XAI-21)).
fn run(
    account: Account,
    attempt: &str,
    stop: &AtomicBool,
    emitter: &Emitter,
) -> (&'static str, Option<String>) {
    let transport = || bravebot_net::transport::Transport::shared().agent_config_builder();
    let started = |data: Value| emitter.send(Event::global("accounts.signIn.started", data));
    let failed = |sentence: String| {
        if stop.load(Ordering::SeqCst) {
            ("cancelled", None)
        } else {
            ("failed", Some(sentence))
        }
    };
    match account {
        Account::ChatGpt => {
            let signed_in = bravebot_skus::chatgpt::sign_in(
                &ChatGptHosts::compiled_in(),
                transport(),
                &Limits::default(),
                &Pages {
                    done: t!(chatgpt_page_done),
                    not_found: t!(chatgpt_page_not_found),
                },
                stop,
                |announcement| {
                    started(json!({
                        "attempt": attempt,
                        "way": account.word(),
                        "url": announcement.url,
                        "port": announcement.port,
                    }))
                },
            );
            match signed_in {
                Ok(_) => ("signedIn", None),
                Err(bravebot_skus::chatgpt::SignInError::Cancelled) => ("cancelled", None),
                Err(stopped) => failed(said_of_chatgpt(&stopped)),
            }
        }
        Account::Xai => {
            let signed_in = bravebot_skus::xai::sign_in(
                &XaiHosts::compiled_in(),
                transport(),
                stop,
                |announcement| {
                    started(json!({
                        "attempt": attempt,
                        "way": account.word(),
                        "address": announcement.url,
                        "code": announcement.user_code,
                        "expiresIn": announcement.expires_in,
                    }))
                },
            );
            match signed_in {
                Ok(_) => ("signedIn", None),
                Err(bravebot_skus::xai::SignInError::Cancelled) => ("cancelled", None),
                Err(stopped) => failed(said_of_xai(&stopped)),
            }
        }
    }
}

/// Why the ChatGPT sign-in stopped, chosen from the step and the class of a status, so it quotes
/// nothing a reply, a callback or a file held
/// ([CHATGPT-8](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-8)).
fn said_of_chatgpt(stopped: &bravebot_skus::chatgpt::SignInError) -> String {
    use bravebot_skus::chatgpt::SignInError as E;
    let class = |class: &Class| -> &'static str {
        match class {
            Class::Refused => t!(desktop_accounts_openai_refused),
            Class::RateLimited => t!(desktop_accounts_openai_rate_limited),
            Class::Unavailable => t!(desktop_accounts_openai_unavailable),
            Class::Incomplete => t!(desktop_accounts_openai_incomplete),
            Class::Transport => t!(desktop_accounts_openai_transport),
        }
    };
    match stopped {
        E::Listener => t!(desktop_accounts_chatgpt_listener).into(),
        E::TimedOut => t!(desktop_accounts_chatgpt_timed_out).into(),
        E::NoCode | E::NoClient | E::IdToken => t!(desktop_accounts_chatgpt_not_completed).into(),
        E::ClientChanged => t!(desktop_accounts_chatgpt_client_changed).into(),
        E::NoUseScope => t!(desktop_accounts_chatgpt_no_use_scope).into(),
        E::Exchange(failed) | E::KeySet(failed) => class(failed).into(),
        E::Clock => t!(desktop_accounts_clock).into(),
        E::Store(_) => t!(desktop_accounts_not_stored).into(),
        E::Cancelled => t!(desktop_accounts_cancelled).into(),
    }
}

/// Why the xAI sign-in stopped, chosen the same way
/// ([XAI-4](../../../docs/specs/xai-sign-in.md#XAI-4)).
fn said_of_xai(stopped: &bravebot_skus::xai::SignInError) -> String {
    use bravebot_skus::xai::SignInError as E;
    let class = |class: &Class| -> &'static str {
        match class {
            Class::Refused => t!(desktop_accounts_xai_refused),
            Class::RateLimited => t!(desktop_accounts_xai_rate_limited),
            Class::Unavailable => t!(desktop_accounts_xai_unavailable),
            Class::Incomplete => t!(desktop_accounts_xai_incomplete),
            Class::Transport => t!(desktop_accounts_xai_transport),
        }
    };
    match stopped {
        E::Code(failed) | E::Exchange(failed) => class(failed).into(),
        E::AccessDenied | E::AuthorizationDenied => t!(desktop_accounts_xai_denied).into(),
        E::Expired | E::TimedOut => t!(desktop_accounts_xai_expired).into(),
        E::Clock => t!(desktop_accounts_clock).into(),
        E::Store(_) => t!(desktop_accounts_not_stored).into(),
        E::Cancelled => t!(desktop_accounts_cancelled).into(),
    }
}

#[path = "accounts_bedrock.rs"]
mod bedrock;

#[cfg(test)]
#[path = "accounts_tests.rs"]
mod tests;
