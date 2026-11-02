//! The window's own entry of a Brave key or a gateway key, typed in a window the main process owns
//! ([PREM-16](../../../docs/specs/premium-credentials.md#PREM-16),
//! [CRED-24](../../../docs/specs/credential-protection.md#CRED-24)).
//!
//! `accounts.keyBefore` makes every check that needs no key, so nobody types one only to be told it
//! cannot be kept, and `accounts.storeKey` makes them again and stores it. The storing is
//! [`bravebot_agent::key_entry`]'s, which `bravebot auth login` uses too. No answer and no refusal
//! carries the key or any part of it.

use crate::protocol::{Failure, Request};
use bravebot_agent::accounts::{Offer, offered};
use bravebot_agent::home;
use bravebot_agent::key_entry::{self, StoreError};
use bravebot_config::keys::Keys;
use bravebot_config::{Config, ConfigError, Managed, Secret, brave_key, user_settings_file};
use bravebot_i18n::t;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// What a key is entered for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Target {
    Brave,
    Gateway(String),
}

fn target(request: &Request) -> Result<Target, Failure> {
    match request.string("way")?.as_str() {
        "brave" => Ok(Target::Brave),
        "gateway" => request
            .optional_string("id")
            .filter(|id| !id.is_empty())
            .map(Target::Gateway)
            .ok_or_else(|| Failure::bad_request("a gateway key needs the gateway's id")),
        other => Err(Failure::bad_request(format!(
            "the window enters a key only for brave or a gateway, not `{other}`"
        ))),
    }
}

/// Reads a variable from the environment the bridge was started with.
pub(crate) type Lookup = Box<dyn Fn(&str) -> Option<String>>;

/// What is read before a key is asked for or stored, so the refusals can be decided without the
/// process.
pub(crate) struct Before {
    pub(crate) incognito: bool,
    /// The state directory, `None` where nothing may be written.
    pub(crate) directory: Option<PathBuf>,
    pub(crate) config: Result<Config, ConfigError>,
    pub(crate) managed: Managed,
    pub(crate) lookup: Lookup,
}

impl Before {
    fn read(project: Option<&Path>, selected: Option<&Path>) -> Self {
        let managed = Managed::load();
        Self {
            incognito: bravebot_core::incognito::engaged(),
            directory: home::writable(),
            config: Config::from_env_and_settings(
                &crate::settings::layers(project, selected),
                &managed,
            ),
            managed,
            lookup: Box::new(|name| std::env::var(name).ok()),
        }
    }
}

/// What the checks that need no key found.
pub(crate) struct Ready<'b> {
    pub(crate) directory: &'b Path,
    /// Whether a key is stored already, which a new one replaces only where the person said so.
    pub(crate) stored: bool,
    /// For a gateway, the one it is for.
    pub(crate) offer: Option<Offer<'b>>,
}

impl Ready<'_> {
    fn host(&self) -> Option<String> {
        self.offer
            .as_ref()
            .map(|offer| offer.provider.host().to_string())
    }

    fn records(&self) -> bool {
        self.offer.as_ref().is_some_and(|offer| offer.unconfigured)
    }
}

/// The checks that need no key, in the order `bravebot auth login` makes them, each refusing with
/// a sentence of the window's own that names no path and repeats nothing a file holds.
pub(crate) fn ready<'b>(target: &Target, before: &'b Before) -> Result<Ready<'b>, &'static str> {
    // INCOG-7: a session that leaves nothing behind stores no key either.
    if before.incognito {
        return Err(t!(desktop_accounts_key_incognito));
    }
    match target {
        Target::Brave => {
            let directory = before
                .directory
                .as_deref()
                .ok_or(t!(desktop_accounts_key_nowhere))?;
            match brave_key::read(directory) {
                Err(brave_key::Unreadable) => Err(t!(desktop_accounts_key_brave_unreadable)),
                Ok(held) => Ok(Ready {
                    directory,
                    stored: held.is_some(),
                    offer: None,
                }),
            }
        }
        Target::Gateway(id) => {
            // A configuration that lacks only the Brave credentials names no gateway, as the
            // command reads it.
            let providers = match &before.config {
                Ok(config) => config.providers.as_slice(),
                Err(ConfigError::Missing(_) | ConfigError::Empty(_)) => &[],
                Err(_) => return Err(t!(desktop_accounts_key_misconfigured)),
            };
            let offer = offered(providers, &before.managed)
                .into_iter()
                .find(|offer| offer.id() == id)
                .ok_or(t!(desktop_accounts_key_not_offered))?;
            let directory = before
                .directory
                .as_deref()
                .ok_or(t!(desktop_accounts_key_nowhere))?;
            let stored = Keys::read(directory).map_err(|_| t!(desktop_accounts_keys_unreadable))?;
            // Before the key is typed, so nobody types one for a service no session would send it to.
            if offer.unconfigured {
                key_entry::with_block(&offer, &user_settings_file(directory))
                    .map_err(|_| t!(desktop_accounts_key_cannot_record))?;
            }
            Ok(Ready {
                directory,
                stored: stored.get(id).is_some(),
                offer: Some(offer),
            })
        }
    }
}

/// Store `key`, typed for `target`, where every check still holds. `Ok` carries a sentence for a
/// key that is stored but not yet sent, `Err` the refusal.
pub(crate) fn store(
    target: &Target,
    before: &Before,
    key: Secret,
    replace: bool,
) -> Result<Option<&'static str>, &'static str> {
    let ready = ready(target, before)?;
    if key.expose().is_empty() {
        return Err(t!(desktop_accounts_key_empty));
    }
    // Replaced only on a yes, because the one replaced is gone once the file is written.
    if ready.stored && !replace {
        return Err(t!(desktop_accounts_key_stored_already));
    }
    let Some(offer) = &ready.offer else {
        return key_entry::store_brave_key(ready.directory, &key)
            .map(|()| None)
            .map_err(|_| t!(desktop_accounts_key_not_stored));
    };
    match key_entry::store_gateway_key(ready.directory, offer.id(), key) {
        Ok(()) => {}
        Err(StoreError::KeysUnreadable) => return Err(t!(desktop_accounts_keys_unreadable)),
        Err(StoreError::NotStored(_)) => return Err(t!(desktop_accounts_key_not_stored)),
    }
    // Recorded once the key is stored; a block that cannot be recorded now leaves the key stored.
    if offer.unconfigured
        && key_entry::record_block(offer, &user_settings_file(ready.directory)).is_err()
    {
        return Ok(Some(t!(desktop_accounts_key_not_recorded)));
    }
    Ok(None)
}

/// `accounts.keyBefore`: whether a key may be typed for the way named, and what the key window says
/// about it. A refusal is an answer, not an error, since it is a sentence for the person.
pub(crate) fn before(
    request: &Request,
    project: Option<&Path>,
    selected: Option<&Path>,
) -> Result<Value, Failure> {
    let target = target(request)?;
    if let Some(path) = selected {
        crate::settings::validate(path)?;
    }
    let before = Before::read(project, selected);
    Ok(answer_before(&target, &before))
}

pub(crate) fn answer_before(target: &Target, before: &Before) -> Value {
    match ready(target, before) {
        Err(refused) => json!({ "ready": false, "sentence": refused }),
        Ok(ready) => {
            let variable = ready.offer.as_ref().and_then(|offer| {
                key_entry::variable_in_force(&offer.provider, |name| (before.lookup)(name))
                    .map(str::to_string)
            });
            json!({
                "ready": true,
                "stored": ready.stored,
                "host": ready.host(),
                "variable": variable,
                "records": ready.records(),
            })
        }
    }
}

/// `accounts.storeKey`: store the key the main process read from its key window, and answer with
/// the rows as they now stand.
pub(crate) fn store_key(
    request: &Request,
    project: Option<&Path>,
    selected: Option<&Path>,
) -> Result<Value, Failure> {
    let target = target(request)?;
    let replace = request
        .params
        .get("replace")
        .and_then(Value::as_bool)
        .ok_or_else(|| Failure::bad_request("`replace` must be true or false"))?;
    // Held as a secret from here, trimmed as the terminal's hidden read trims it.
    let key = Secret::new(
        request
            .params
            .get("key")
            .and_then(Value::as_str)
            .ok_or_else(|| Failure::bad_request("`key` must be a string"))?
            .trim(),
    );
    if let Some(path) = selected {
        crate::settings::validate(path)?;
    }
    let before = Before::read(project, selected);
    match store(&target, &before, key, replace) {
        Err(refused) => Ok(json!({ "outcome": "refused", "sentence": refused })),
        Ok(said) => {
            let accounts = crate::accounts::list(project, selected)
                .ok()
                .and_then(|listed| listed.get("accounts").cloned());
            Ok(json!({ "outcome": "stored", "sentence": said, "accounts": accounts }))
        }
    }
}

#[cfg(test)]
#[path = "key_entry_tests.rs"]
mod tests;
