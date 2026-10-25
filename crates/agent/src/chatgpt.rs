//! The stored ChatGPT sign-in, as the source of a request's access token
//! ([CHATGPT-7](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-7),
//! [CHATGPT-10](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-10)).
//!
//! The adapter between the sign-in file and the Responses client, as [`crate::subscription`] is for
//! the Leo store. It lives here rather than in either because the client does not know where the
//! sign-in is kept and the crate that keeps it does not know what a request is.
//!
//! One renewer for the process. A renewal that could not be written is held in memory by the
//! renewer for the rest of the process, so a second one made for each request would drop it, and
//! the next request would present a refresh token the server has already replaced. The same holds
//! for a sign-in the server ended: the renewal that found it removed the file, and a second renewer
//! would find no file and say nothing was stored to the turn that asks after a picker's listing.

use crate::report::Reporter;
pub use bravebot_aichat::models::Model;
use bravebot_aichat::protocol::Effort;
pub use bravebot_aichat::responses::protocol::Refusal;
use bravebot_aichat::{AccountAccess, AccountFailure, ChatError, RenewalFailure};
use bravebot_config::provider::Provider;
use bravebot_config::{Config, Secret};
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::event::Sink;
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
use bravebot_i18n::t;
use bravebot_net::{Egress, EgressError, Transport};
use bravebot_skus::chatgpt::{Access, Account, Class, Hosts, Renewer, TokenError};
use std::sync::{Mutex, OnceLock, PoisonError};

/// The renewer, made at the first request that needs a token.
static RENEWER: Mutex<Option<Renewer>> = Mutex::new(None);

/// Whether the file holds the last renewal, and whether the person has been told where it does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Saving {
    /// No renewal has failed to be written, or the last one that did has since been written.
    Saved,
    /// A renewal could not be written and no turn has said so.
    Unsaid,
    /// A renewal could not be written and a turn has said so. It is not said again until a write
    /// succeeds and a later one fails.
    Said,
}

/// What the file holds of the renewals this process made, as the turns that follow tell it.
static SAVING: Mutex<Saving> = Mutex::new(Saving::Saved);

/// Record, in `saving`, that a request's token was had, where `unwritten` says the renewer holds a
/// renewal the file could not take.
///
/// Each request retries a write that failed, so a file that stays unwritable fails on every request
/// and the person is told once. A write that then succeeds clears it, so the next failure is told.
fn record_a_token(saving: &Mutex<Saving>, unwritten: bool) {
    let mut saving = saving.lock().unwrap_or_else(PoisonError::into_inner);
    *saving = match (unwritten, *saving) {
        (false, _) => Saving::Saved,
        (true, Saving::Saved) => Saving::Unsaid,
        (true, told) => told,
    };
}

/// The token of `access`, noting in `saving` whether the file took the renewal that made it.
fn token_of(saving: &Mutex<Saving>, access: &Access) -> Secret {
    record_a_token(saving, access.unwritten.is_some());
    Secret::new(access.token.expose())
}

/// Say, in the turn that made the request, that the sign-in was renewed and the file could not take
/// the renewal ([CHATGPT-7](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-7)).
///
/// Called after each round's request, whatever came of it, and when the turn ends, so a renewal made
/// by a call that is not a round's request is said in the turn it was made in. The renewed tokens are used from memory
/// for the rest of the process, so the turn goes on. The file still holds a refresh token the
/// server has replaced, so the sign-in ends at the next start unless it can be written by then,
/// which is what the sentence tells the person. It names no path and quotes nothing the file or the
/// operating system said.
pub(crate) fn say_an_unsaved_renewal(reporter: &mut (impl Reporter + ?Sized)) {
    say_from(&SAVING, reporter);
}

/// Note that the renewal behind a request's token could not be written, as having the token does.
///
/// For the test of what a turn says about it, which cannot make the renewer fail: the hosts it
/// renews at are compiled in. Nothing else calls it.
#[doc(hidden)]
pub fn note_a_renewal_the_file_could_not_take(unwritten: bool) {
    record_a_token(&SAVING, unwritten);
}

/// [`say_an_unsaved_renewal`] for a given record of what the file holds, so a test need not share
/// the process's.
fn say_from(saving: &Mutex<Saving>, reporter: &mut (impl Reporter + ?Sized)) {
    let mut saving = saving.lock().unwrap_or_else(PoisonError::into_inner);
    let say = *saving == Saving::Unsaid;
    if say {
        *saving = Saving::Said;
    }
    drop(saving);
    if say {
        reporter.notice(t!(chatgpt_renewal_unwritten).into());
    }
}

/// The service a stored sign-in reaches: the one built from constants, shared by every request.
pub(crate) fn account_provider() -> &'static Provider {
    #[cfg(test)]
    if let Some(host) = stand_in::host() {
        return host;
    }
    static PROVIDER: OnceLock<Provider> = OnceLock::new();
    PROVIDER.get_or_init(Provider::chatgpt_account)
}

/// A server a test of this crate runs in place of the serving host, and a token in place of the
/// stored sign-in, for the requests made on the thread that installed it.
///
/// Compiled into this crate's test build alone, so no flag, variable or file can reach it and the
/// serving host is the one constant in every other build. It is a mechanism of its own, apart from
/// the `test-hosts` feature, which points the built binary at a stand-in through a variable. It
/// reaches the thread that asked for it and no other, which is where a turn's own rounds and a
/// summary are made. It reads no stored sign-in and sends no real token anywhere.
#[cfg(test)]
pub(crate) mod stand_in {
    use super::{Provider, Secret};
    use std::cell::Cell;

    /// The token the stand-in hands out, which no stored sign-in holds.
    pub(super) const TOKEN: &str = "a-stand-in-token";

    thread_local! {
        static HOST: Cell<Option<&'static Provider>> = const { Cell::new(None) };
    }

    /// What ends the stand-in when it is dropped.
    pub(crate) struct Installed;

    impl Drop for Installed {
        fn drop(&mut self) {
            HOST.with(|host| host.set(None));
        }
    }

    /// Send this thread's requests to the account to the server at `endpoint`.
    pub(crate) fn at(endpoint: &str) -> Installed {
        let text = format!(
            r#"{{"provider": {{"chatgpt-stand-in": {{"name": "ChatGPT", "npm": "@ai-sdk/openai",
                "options": {{"baseURL": "{endpoint}/v1"}}}}}}}}"#
        );
        let serde_json::Value::Object(root) = serde_json::from_str(&text).expect("json") else {
            panic!("not an object");
        };
        let provider = Provider::all(&root).pop().expect("one provider");
        HOST.with(|host| host.set(Some(Box::leak(Box::new(provider)))));
        Installed
    }

    pub(super) fn host() -> Option<&'static Provider> {
        HOST.with(Cell::get)
    }

    pub(super) fn token() -> Option<Secret> {
        host().map(|_| Secret::new(TOKEN))
    }
}

/// What one session carries into the requests it makes to the ChatGPT account
/// ([CHATGPT-10](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-10)): the identifier those requests
/// carry as their session, thread and client request identifiers, and the effort level the session
/// began with.
///
/// Made when a session begins and held by whatever holds the session, so every request the session
/// makes carries the same identifier, and a second session of the same process carries another. The
/// service derives its prompt-cache affinity from the identifier, so one that changed between a
/// session's requests would be a cache that never hit, and one two sessions shared would give them
/// each other's cache. A delegate, a background session, a resumed session and a fork each begin
/// their own: nothing here is stored in a session's record.
///
/// The effort level is the one in force when the session began, and it is the one the account is
/// sent for the rest of it, whatever level is chosen afterwards: the service keeps the cache of a
/// session only while its level stays the same. What the person chose is kept apart and applies to
/// the sessions begun after it, and to every service but this one. A model that refuses a level
/// outranks it: the client sends that model none from then on, for every session of the process
/// ([BACKEND-22](../../../docs/specs/backends.md#BACKEND-22)).
///
/// Cloning gives another handle on the same session.
#[derive(Debug, Clone)]
pub struct AccountSession(std::sync::Arc<Begun>);

#[derive(Debug)]
struct Begun {
    identifier: String,
    effort: Option<Effort>,
}

impl AccountSession {
    /// A new session that began asking for no effort level, which is what a front end that offers no
    /// choice of one begins.
    pub fn begin() -> Self {
        Self::begin_with(None)
    }

    /// A new session that began with `effort` in force, where one was.
    ///
    /// The identifier is generated here and from the operating system's random source, so it holds
    /// nothing a reply, a model or a settings file chose and nothing about the person, the account,
    /// the project or what was asked.
    pub fn begin_with(effort: Option<Effort>) -> Self {
        Self(std::sync::Arc::new(Begun {
            identifier: bravebot_skus::new_request_id(),
            effort,
        }))
    }

    /// The effort level the session began with, which is the level the account is sent for all of
    /// it, except by a model that has refused a level and is sent none
    /// ([BACKEND-22](../../../docs/specs/backends.md#BACKEND-22)).
    pub fn effort(&self) -> Option<Effort> {
        self.0.effort
    }

    /// The identifier the session's requests carry.
    pub fn identifier(&self) -> &str {
        &self.0.identifier
    }
}

/// The access token of the stored sign-in, renewed where it is about to expire.
///
/// A handle on the process's renewer and nothing more. The lock is held for one call and not for
/// the request the token is sent with, so a reply that is slow to arrive holds up no other turn. A
/// renewal holds it for as long as it takes, which the refresh's own deadline bounds, and a turn
/// that needs the token meanwhile waits for it as it would wait for the claim on the file.
pub(crate) struct StoredSignIn;

impl AccountAccess for StoredSignIn {
    fn access_token(&mut self) -> Result<Secret, AccountFailure> {
        #[cfg(test)]
        if let Some(token) = stand_in::token() {
            return Ok(token);
        }
        obtained(Renewer::access_token)
    }

    fn replace_refused(&mut self, refused: &Secret) -> Result<Secret, AccountFailure> {
        #[cfg(test)]
        if let Some(token) = stand_in::token() {
            return Ok(token);
        }
        let refused = bravebot_skus::Secret::new(refused.expose());
        obtained(|renewer| renewer.replace_refused(&refused))
    }
}

/// Whether a sign-in is stored, apart from whether its token can be had.
///
/// For the picker that offers the models an entry names, which needs to know there is an account
/// and nothing of what it may reach, so its answer is the same offline and a read of it does not
/// rewrite the file. A stored sign-in that has ended and has not been found out yet is stored here,
/// and the first request says it has ended.
pub(crate) trait AccountStored {
    /// The file is read and nothing is renewed. One that cannot be read is no answer, and is not
    /// taken for an absent sign-in.
    fn is_stored(&mut self) -> Result<bool, AccountFailure>;
}

impl AccountStored for StoredSignIn {
    fn is_stored(&mut self) -> Result<bool, AccountFailure> {
        match Account::load() {
            Ok(stored) => Ok(stored.is_some()),
            Err(_) => Err(AccountFailure::Unusable),
        }
    }
}

/// What the file that keeps the ChatGPT sign-in holds, as far as telling a person what to configure
/// needs it ([BACKEND-39](../../../docs/specs/backends.md#BACKEND-39)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountFile {
    /// Nothing is stored, or this machine has nowhere to keep a sign-in.
    NotStored,
    /// A sign-in is stored and the file can be read. Its tokens are not looked at.
    Stored,
    /// The file exists and cannot be used, so it configures nothing and the person is told why.
    Unreadable,
}

/// The sign-in file as it is on this machine. It is read and nothing is renewed or asked of the
/// service, so it starts nothing.
pub(crate) fn account_file() -> AccountFile {
    file_of(&Account::load())
}

fn file_of(loaded: &Result<Option<Account>, bravebot_skus::chatgpt::AccountError>) -> AccountFile {
    match loaded {
        Ok(Some(_)) => AccountFile::Stored,
        Ok(None) | Err(bravebot_skus::chatgpt::AccountError::Nowhere { .. }) => {
            AccountFile::NotStored
        }
        Err(_) => AccountFile::Unreadable,
    }
}

/// What the ChatGPT account offers a picker
/// ([CHATGPT-9](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-9)).
#[derive(Debug, PartialEq, Eq)]
pub enum AccountModels {
    /// The account is not a roster to show: nothing usable is signed in, or a `provider` entry
    /// holds the id. Nothing was asked of the service, and the picker says nothing about it, since
    /// a person who never signed in is not owed a warning about a service they do not use.
    Absent,
    /// The models a `provider` entry named `chatgpt` names, where one does, and otherwise the
    /// models the listing offers, in the order it gave them. Empty for an account that reaches
    /// none, which the service answered and is not a failure.
    Offered(Vec<Model>),
    /// A `provider` entry named `chatgpt` names models, a sign-in is stored and its file cannot be
    /// read, so nothing can be said to be the account's. Nothing is offered, and the picker says the
    /// models could not be loaded.
    Unreadable,
    /// A sign-in is stored and the listing could not be had: the token could not be renewed, the
    /// request failed or was refused for its token or its status, or the reply was not a listing.
    /// The compiled list stands in for it
    /// ([CHATGPT-9](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-9)), and the picker says the
    /// listing could not be read and that a row may name a model the account cannot reach.
    Compiled(Vec<Model>),
    /// A sign-in is stored and the serving host refused the listing in a code that names the
    /// account ([CHATGPT-8](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-8)). Nothing is offered,
    /// and both pickers and the sign-in command say so in that clause's sentence, which does not send
    /// the person to sign in again.
    Refused(Refusal),
}

/// The account's models for a picker, asked of the serving host with the stored sign-in.
///
/// One function for the terminal and the desktop pickers, so neither holds its own copy of when the
/// account is asked about, which token it is asked with and what a failed listing offers. `sink` is
/// the trail the listing's request is recorded in, as the other rosters are.
pub fn account_models<S: Sink>(config: &Config, sink: &mut S) -> AccountModels {
    account_models_from(
        config,
        &mut StoredSignIn,
        account_provider(),
        sink,
        &Egress::new(),
    )
}

/// [`account_models`] for a given source of the token and a given service, so a test can stand a
/// server of its own in for the compiled-in host.
fn account_models_from<S: Sink, A: AccountAccess + AccountStored>(
    config: &Config,
    access: &mut A,
    provider: &Provider,
    sink: &mut S,
    egress: &Egress,
) -> AccountModels {
    // An entry with the id is a gateway, and a stored sign-in is not asked about for it.
    if !config.offers_chatgpt_account() {
        return AccountModels::Absent;
    }
    // An entry that names models is taken at its word, as a gateway's is: nothing is asked of any
    // service and the access token is not renewed, so the roster is the same offline. It is offered
    // where a sign-in is stored, since none of the account's rows is offered without one.
    if !config.chatgpt_models.is_empty() {
        return match access.is_stored() {
            Ok(true) => AccountModels::Offered(bravebot_aichat::models::named_by_entry(
                provider,
                &config.chatgpt_models,
            )),
            Ok(false) => AccountModels::Absent,
            Err(_) => AccountModels::Unreadable,
        };
    }
    // Asked for before anything is sent, so a machine with no sign-in sends nothing. An ended
    // sign-in was removed by the renewal that found it so, so the picker offers nothing for it. The
    // renewer says it ended to every later ask of the process, which the next turn's is.
    let compiled =
        || AccountModels::Compiled(bravebot_aichat::models::compiled_for_account(provider));
    let mut token = match access.access_token() {
        Ok(token) => token,
        Err(AccountFailure::NotSignedIn | AccountFailure::Ended) => return AccountModels::Absent,
        Err(_) => return compiled(),
    };
    let mut routing = Routing::new();
    routing.insert_trusted("models", provider.models_url());
    let Ok(mut policy) = Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        sink,
    ) else {
        return compiled();
    };
    let mut replaced = false;
    loop {
        match bravebot_aichat::models::list_from_account(
            &mut policy,
            provider,
            token.expose(),
            egress,
        ) {
            Ok(models) => return AccountModels::Offered(models),
            // A token the host turned away is replaced once and the listing asked for again, as a
            // turn's request is, since a clock that made it look valid is no reason to hide the
            // account's models.
            Err(ChatError::Egress(EgressError::Status { status: 401, .. })) if !replaced => {
                replaced = true;
                token = match access.replace_refused(&token) {
                    Ok(token) => token,
                    Err(AccountFailure::NotSignedIn | AccountFailure::Ended) => {
                        return AccountModels::Absent;
                    }
                    Err(_) => return compiled(),
                };
            }
            Err(ChatError::Account(AccountFailure::Named { refusal, .. })) => {
                return AccountModels::Refused(refusal);
            }
            Err(_) => return compiled(),
        }
    }
}

/// The sentence for a failure of the account, from the message catalog
/// ([CHATGPT-8](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-8)).
///
/// The one place the failures are worded, for the terminal's line, the one-shot run's report, the line
/// for a failed goal check and the line for a summary that could not be made. It is chosen from the
/// condition alone, so it holds nothing a service, a file or a settings file said, and where signing in
/// again is the remedy it names `bravebot auth login chatgpt`. A refusal that came as a 429 with a wait
/// says the wait in it, in minutes rounded up. The status of a refusal is said after it by the layer that
/// words the line, as that layer says the status of any other failure.
pub fn said_of_failure(failure: AccountFailure) -> String {
    match failure {
        AccountFailure::NotSignedIn => t!(failure_account_not_signed_in).into(),
        AccountFailure::Ended => t!(failure_account_ended).into(),
        AccountFailure::Refused => t!(failure_account_token_refused).into(),
        AccountFailure::Unusable => t!(failure_account_unusable).into(),
        // The same words whatever the code, so both refusals are told when to ask again.
        AccountFailure::Named { refusal, .. } => match failure.wait() {
            Some(wait) => t!(
                failure_account_wait,
                what = said_of_refusal(refusal),
                minutes = crate::outcome::minutes_to_wait(wait)
            ),
            None => said_of_refusal(refusal).into(),
        },
        AccountFailure::Renewal(RenewalFailure::Refused) => {
            t!(failure_account_renewal_refused).into()
        }
        AccountFailure::Renewal(RenewalFailure::RateLimited) => {
            t!(failure_account_renewal_rate_limited).into()
        }
        AccountFailure::Renewal(RenewalFailure::Unavailable) => {
            t!(failure_account_renewal_unavailable).into()
        }
        AccountFailure::Renewal(RenewalFailure::Incomplete) => {
            t!(failure_account_renewal_incomplete).into()
        }
        AccountFailure::Renewal(RenewalFailure::Transport) => {
            t!(failure_account_renewal_transport).into()
        }
    }
}

/// [`said_of_failure`] with the status of a refusal that came with one after it, as the terminal's
/// line for a failed turn, the one-shot run's report and the record of a session say it
/// ([CHATGPT-8](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-8)). The lines for a summary that
/// could not be made and for a move to the fallback model say [`said_of_failure`] alone.
pub fn told_of_failure(failure: AccountFailure) -> String {
    let said = said_of_failure(failure);
    match failure.status() {
        Some(status) => t!(failure_with_status, what = said, status = status),
        None => said,
    }
}

/// The sentence for what a refusal that names the account says, from the message catalog
/// ([CHATGPT-8](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-8)).
///
/// Neither names signing in again as a remedy, since a new sign-in does not change what either says,
/// and neither names a cause the six codes of the second do not say.
pub fn said_of_refusal(refusal: Refusal) -> &'static str {
    match refusal {
        Refusal::UsageLimit => t!(failure_account_usage_limit),
        Refusal::NotPermitted => t!(failure_account_not_permitted),
    }
}

/// Ask the renewer for a token, making it first where this is the first request.
fn obtained(
    ask: impl FnOnce(&mut Renewer) -> Result<Access, TokenError>,
) -> Result<Secret, AccountFailure> {
    let mut held = RENEWER.lock().unwrap_or_else(PoisonError::into_inner);
    if held.is_none() {
        let renewer = Renewer::new(
            Hosts::compiled_in(),
            Transport::shared().agent_config_builder(),
        )
        .map_err(|_| AccountFailure::Unusable)?;
        *held = Some(renewer);
    }
    let renewer = held.as_mut().ok_or(AccountFailure::Unusable)?;
    ask(renewer)
        .map(|access| token_of(&SAVING, &access))
        .map_err(|failure| failure_of(&failure))
}

/// The failure a turn is told of, from the step of the renewal that failed.
///
/// A closed set chosen from the step and the class of a status, as the renewal's own are: nothing
/// the authorization server wrote is in either.
pub(crate) fn failure_of(failure: &TokenError) -> AccountFailure {
    match failure {
        TokenError::NotSignedIn => AccountFailure::NotSignedIn,
        TokenError::Ended => AccountFailure::Ended,
        TokenError::Store(_) => AccountFailure::Unusable,
        TokenError::Refresh(class) => AccountFailure::Renewal(match *class {
            Class::Refused => RenewalFailure::Refused,
            Class::RateLimited => RenewalFailure::RateLimited,
            Class::Unavailable => RenewalFailure::Unavailable,
            Class::Incomplete => RenewalFailure::Incomplete,
            Class::Transport => RenewalFailure::Transport,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_core::event::RecordingSink;
    use bravebot_skus::chatgpt::AccountError;

    /// CHATGPT-8: each step of a renewal that failed becomes one failure of the closed set, and
    /// the account ending is told apart from a renewal that merely failed, since only the first
    /// removed the file and asks for a new sign-in.
    #[test]
    fn each_failed_step_of_a_renewal_is_its_own_failure_and_only_an_ended_account_is_ended() {
        let table = [
            (TokenError::NotSignedIn, AccountFailure::NotSignedIn),
            (TokenError::Ended, AccountFailure::Ended),
            (
                TokenError::Store(AccountError::Unusable {
                    detail: "a path".to_string(),
                }),
                AccountFailure::Unusable,
            ),
            (
                TokenError::Refresh(Class::Refused),
                AccountFailure::Renewal(RenewalFailure::Refused),
            ),
            (
                TokenError::Refresh(Class::RateLimited),
                AccountFailure::Renewal(RenewalFailure::RateLimited),
            ),
            (
                TokenError::Refresh(Class::Unavailable),
                AccountFailure::Renewal(RenewalFailure::Unavailable),
            ),
            (
                TokenError::Refresh(Class::Incomplete),
                AccountFailure::Renewal(RenewalFailure::Incomplete),
            ),
            (
                TokenError::Refresh(Class::Transport),
                AccountFailure::Renewal(RenewalFailure::Transport),
            ),
        ];
        for (step, expected) in table {
            let shown = step.to_string();
            assert_eq!(failure_of(&step), expected, "{shown}");
        }
    }

    /// BACKEND-39: a sign-in that is stored is the one state that configures a service. A machine
    /// with none, and one with nowhere to keep it, configure nothing and are told nothing, and a
    /// file that cannot be used configures nothing and is told apart from an absent one so that it
    /// is said. A read that took an unreadable file for a stored sign-in would send the person to
    /// name a model that no turn can reach, and one that took it for an absent one would not say
    /// why a sign-in they made is not used.
    #[test]
    fn a_stored_account_is_told_apart_from_an_absent_one_and_from_one_that_cannot_be_read() {
        let stored = Account {
            access_token: bravebot_skus::Secret::new("placeholder-access-token"),
            refresh_token: bravebot_skus::Secret::new("placeholder-refresh-token"),
            expires_at: 1,
            client_id: "client".to_string(),
            host_id: "urn:uuid:host".to_string(),
        };
        assert_eq!(file_of(&Ok(Some(stored))), AccountFile::Stored);
        assert_eq!(file_of(&Ok(None)), AccountFile::NotStored);
        assert_eq!(
            file_of(&Err(AccountError::Nowhere {
                detail: "a path".to_string()
            })),
            AccountFile::NotStored
        );
        for unusable in [
            AccountError::Unusable {
                detail: "a path".to_string(),
            },
            AccountError::Malformed(bravebot_skus::chatgpt::Defect::MissingField),
        ] {
            let shown = unusable.to_string();
            assert_eq!(file_of(&Err(unusable)), AccountFile::Unreadable, "{shown}");
        }
    }

    /// A refusal that names the account, with neither a status nor a wait, as one reported on a
    /// stream is.
    fn refused(refusal: Refusal) -> AccountFailure {
        AccountFailure::Named {
            refusal,
            status: None,
            retry_after: None,
        }
    }

    /// Every failure of the account, with the command it names where signing in again, or looking
    /// at the stored sign-in, is the remedy.
    fn every_failure() -> [(AccountFailure, Option<&'static str>); 11] {
        let login = Some("bravebot auth login chatgpt");
        [
            (AccountFailure::NotSignedIn, login),
            (AccountFailure::Ended, login),
            (AccountFailure::Refused, login),
            (
                AccountFailure::Unusable,
                Some("bravebot auth status chatgpt"),
            ),
            (refused(Refusal::UsageLimit), None),
            (refused(Refusal::NotPermitted), None),
            (AccountFailure::Renewal(RenewalFailure::Refused), None),
            (AccountFailure::Renewal(RenewalFailure::RateLimited), None),
            (AccountFailure::Renewal(RenewalFailure::Unavailable), None),
            (AccountFailure::Renewal(RenewalFailure::Incomplete), None),
            (AccountFailure::Renewal(RenewalFailure::Transport), None),
        ]
    }

    /// CHATGPT-8: each failure of the account has a sentence of its own, from the catalog and from
    /// the condition alone. Where signing in again is the remedy it names the command, and nowhere
    /// else does: a refusal that names the account sends nobody to sign in again, and only the usage
    /// limit says so. A usage limit and a refusal that says the account may not be used are told
    /// apart in the listing's sentence and in a turn's. The regressions it rejects are one sentence
    /// for every failure, a command named where it is no remedy, and a condition left without a
    /// sentence, which would show a person the category's line in its place.
    #[test]
    fn every_failure_of_the_account_has_its_own_sentence_and_names_the_command_only_as_a_remedy() {
        let mut sentences = Vec::new();
        for (failure, command) in every_failure() {
            let said = said_of_failure(failure);
            assert!(!said.trim().is_empty(), "{failure:?} has no sentence");
            assert!(
                !said.contains('{') && !said.contains('}'),
                "{failure:?}: {said}"
            );
            match command {
                Some(command) => assert!(said.contains(command), "{failure:?}: {said}"),
                None => assert!(!said.contains("auth login"), "{failure:?}: {said}"),
            }
            assert_eq!(
                said.contains("usage limit"),
                failure == refused(Refusal::UsageLimit),
                "{failure:?}: {said}"
            );
            if let AccountFailure::Renewal(_) = failure {
                assert!(said.contains("could not be renewed"), "{said}");
            }
            sentences.push(said);
        }
        let mut distinct = sentences.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!(distinct.len(), sentences.len(), "{sentences:#?}");
        for refusal in [Refusal::UsageLimit, Refusal::NotPermitted] {
            assert_eq!(
                said_of_refusal(refusal),
                said_of_failure(refused(refusal)),
                "a refusal is worded one way in the listing and in a turn"
            );
        }
    }

    /// CHATGPT-8: a refusal that came as a 429 with a wait says the wait in its sentence, in minutes
    /// rounded up and in the singular for one, whichever of the two refusals it is, and one that came
    /// with another status or on a stream says none. The regression it rejects is a wait said where
    /// the service did not ask for one, which would tell a person when to try a limit that has no
    /// date.
    #[test]
    fn a_refusal_that_asked_for_a_wait_says_it_in_minutes_and_one_that_did_not_says_none() {
        use std::time::Duration;
        let named = |refusal, status, seconds: u64| AccountFailure::Named {
            refusal,
            status,
            retry_after: Some(Duration::from_secs(seconds)),
        };
        let limit = said_of_refusal(Refusal::UsageLimit);
        assert_eq!(
            said_of_failure(named(Refusal::UsageLimit, Some(429), 601)),
            format!("{limit}, and the service asked for a wait of about 11 minutes")
        );
        assert_eq!(
            said_of_failure(named(Refusal::UsageLimit, Some(429), 5)),
            format!("{limit}, and the service asked for a wait of about 1 minute")
        );
        assert!(
            said_of_failure(named(Refusal::NotPermitted, Some(429), 60))
                .contains("a wait of about 1 minute")
        );
        for status in [Some(403), Some(401), None] {
            assert_eq!(
                said_of_failure(named(Refusal::UsageLimit, status, 600)),
                limit,
                "{status:?}"
            );
        }
        assert_eq!(said_of_failure(refused(Refusal::UsageLimit)), limit);
    }

    /// CHATGPT-8: everywhere a failed request is printed as the error that carries it, a turn, a goal
    /// check, a compaction and an advisor's answer included, the account's failure reads as the
    /// catalog's sentence and not as the name of its condition, and the status of a refusal that came
    /// with one follows it. The one-shot run's report, the goal run's `unchecked` line and the
    /// terminal's goal and manifest lines are made from these, and a layer that printed the name
    /// would show a person the identifier.
    #[test]
    fn a_turn_a_goal_check_a_compaction_and_an_advice_print_the_accounts_sentence() {
        let backend = |failure| crate::backend::BackendError::from(ChatError::Account(failure));
        for (failure, _) in every_failure() {
            let sentence = said_of_failure(failure);
            assert_eq!(
                crate::turn::TurnError::Chat(backend(failure)).to_string(),
                sentence
            );
            assert_eq!(
                crate::goal::GoalError::Chat(backend(failure)).to_string(),
                sentence
            );
            assert_eq!(
                crate::compact::CompactError::Chat(backend(failure)).to_string(),
                sentence
            );
            assert_eq!(
                crate::advisor::AdviceError::Chat(backend(failure)).to_string(),
                sentence
            );
            assert!(!sentence.contains("ChatGPT account:"), "{sentence}");
        }

        let with_status = AccountFailure::Named {
            refusal: Refusal::UsageLimit,
            status: Some(429),
            retry_after: None,
        };
        assert_eq!(
            crate::turn::TurnError::Chat(backend(with_status)).to_string(),
            format!("{} (HTTP 429)", said_of_failure(with_status))
        );
    }

    /// CHATGPT-10: a session's identifier is a fresh random value of the shape of a version 4
    /// identifier, and a handle on the session gives the identifier it was made with. The
    /// regressions it rejects are an identifier built from anything but the random source, and a
    /// clone that begins a session of its own.
    #[test]
    fn a_sessions_identifier_is_random_and_every_handle_on_it_gives_the_same_one() {
        let session = AccountSession::begin();
        let id = session.identifier();
        assert_eq!(id, session.clone().identifier());
        assert_ne!(id, AccountSession::begin().identifier());
        assert_ne!(id, bravebot_skus::new_request_id());
        let groups: Vec<usize> = id.split('-').map(str::len).collect();
        assert_eq!(groups, [8, 4, 4, 4, 12], "{id}");
        assert!(id.chars().all(|c| c == '-' || c.is_ascii_hexdigit()));
        assert_eq!(id.as_bytes()[14], b'4');
    }

    /// CHATGPT-7: a renewal the file could not take is said to the person once, after the request
    /// that renewed it. Each later request retries the write, so a file that stays unwritable would
    /// say it on every round if nothing remembered having said it. A write that succeeds clears it,
    /// and a failure after that is a new one and is said. Nothing is said while no write has failed.
    /// The regressions it rejects are a failure that goes unsaid, one repeated on every round of a
    /// turn, and one said only the first time a process ever failed.
    #[test]
    fn a_renewal_the_file_could_not_take_is_said_once_and_again_only_after_a_write_succeeds() {
        let saving = Mutex::new(Saving::Saved);
        let mut reporter = crate::report::RecordingReporter::default();

        say_from(&saving, &mut reporter);
        assert!(reporter.notices.is_empty(), "said with nothing to say");

        record_a_token(&saving, true);
        say_from(&saving, &mut reporter);
        assert_eq!(reporter.notices, [t!(chatgpt_renewal_unwritten)]);

        // The next request retries the write and it fails again.
        record_a_token(&saving, true);
        say_from(&saving, &mut reporter);
        say_from(&saving, &mut reporter);
        assert_eq!(reporter.notices.len(), 1, "{:?}", reporter.notices);

        // The write succeeds, and a later one fails.
        record_a_token(&saving, false);
        say_from(&saving, &mut reporter);
        assert_eq!(reporter.notices.len(), 1, "said after a write succeeded");
        record_a_token(&saving, true);
        say_from(&saving, &mut reporter);
        assert_eq!(reporter.notices.len(), 2, "a new failure was not said");

        // A failure no turn has said yet, which a later request's retry then wrote: nothing is left
        // to say, and a stale report would send the person to a file that holds the renewal.
        let saving = Mutex::new(Saving::Saved);
        let mut reporter = crate::report::RecordingReporter::default();
        record_a_token(&saving, true);
        record_a_token(&saving, false);
        say_from(&saving, &mut reporter);
        assert!(reporter.notices.is_empty(), "{:?}", reporter.notices);
    }

    /// CHATGPT-7: it is the renewer's report that the file could not take a renewal that is noted,
    /// and the token is the one it made either way, so the request goes on with it. A token whose
    /// renewal was written, or that needed none, is not a failure to say. The regression it rejects
    /// is the report read the other way round, which would say it for every request that went well
    /// and for none that did not.
    #[test]
    fn the_report_of_an_unwritten_renewal_is_what_is_noted_and_the_token_is_used_either_way() {
        let access = |unwritten| Access {
            token: bravebot_skus::Secret::new("a-token"),
            unwritten,
        };
        let said_after = |unwritten| {
            let saving = Mutex::new(Saving::Saved);
            let token = token_of(&saving, &access(unwritten));
            assert_eq!(token.expose(), "a-token");
            let mut reporter = crate::report::RecordingReporter::default();
            say_from(&saving, &mut reporter);
            reporter.notices.len()
        };

        let unwritable = AccountError::Unwritable {
            detail: "a path".to_string(),
        };
        assert_eq!(said_after(Some(unwritable)), 1);
        assert_eq!(said_after(None), 0);
    }

    /// CHATGPT-7: the sentence names no path and nothing a file or the system said, and tells the
    /// person what to do about it, which is to see that the folder has room and can be written to and,
    /// if the sign-in has ended by the next start, to sign in again. The regression it rejects is a sentence that carries the
    /// failure's detail, which is text about a file and not about the sign-in.
    #[test]
    fn the_sentence_for_a_renewal_the_file_could_not_take_quotes_nothing_and_names_the_remedy() {
        let said = t!(chatgpt_renewal_unwritten);
        assert!(said.contains("bravebot auth login chatgpt"), "{said}");
        assert!(said.contains("room"), "{said}");
        assert!(said.contains("written to"), "{said}");
        for quoted in ["/", "\\", "os error", "Permission denied"] {
            assert!(!said.contains(quoted), "{quoted}: {said}");
        }
    }

    /// A source of tokens that says what it holds, what it gives for a refused one, whether a
    /// sign-in is stored, and what it was asked.
    struct Held {
        outcome: Result<&'static str, AccountFailure>,
        replacement: Result<&'static str, AccountFailure>,
        stored: Result<bool, AccountFailure>,
        asked: usize,
        refused: Vec<String>,
    }

    impl Held {
        fn new(outcome: Result<&'static str, AccountFailure>) -> Self {
            Self {
                outcome,
                replacement: Err(AccountFailure::Refused),
                stored: Ok(true),
                asked: 0,
                refused: Vec::new(),
            }
        }

        fn replacing_with(mut self, token: &'static str) -> Self {
            self.replacement = Ok(token);
            self
        }

        fn stored(mut self, stored: Result<bool, AccountFailure>) -> Self {
            self.stored = stored;
            self
        }
    }

    impl AccountAccess for Held {
        fn access_token(&mut self) -> Result<Secret, AccountFailure> {
            self.asked += 1;
            self.outcome.map(Secret::new)
        }

        fn replace_refused(&mut self, refused: &Secret) -> Result<Secret, AccountFailure> {
            self.refused.push(refused.expose().to_string());
            self.replacement.map(Secret::new)
        }
    }

    impl AccountStored for Held {
        fn is_stored(&mut self) -> Result<bool, AccountFailure> {
            self.stored
        }
    }

    /// The service a test stands in for the serving host: the id and the protocol of the account,
    /// at the address of a server of its own.
    fn account_at(endpoint: &str) -> Provider {
        let text = format!(
            r#"{{"provider": {{"chatgpt": {{"name": "ChatGPT", "npm": "@ai-sdk/openai",
                "options": {{"baseURL": "{endpoint}/v1"}}}}}}}}"#
        );
        let serde_json::Value::Object(root) = serde_json::from_str(&text).expect("json") else {
            panic!("not an object");
        };
        Provider::all(&root).pop().expect("one provider")
    }

    /// A configuration with no gateway, or with the `provider` block `block` states.
    fn configured(block: &str) -> Config {
        let mut config = Config::from_lookup(|key| {
            match key {
                bravebot_config::env_var::SIGNING_KEY => Some("test-signing-key"),
                bravebot_config::env_var::KEY_ID => Some("test-key-id"),
                bravebot_config::env_var::ENDPOINT => Some("https://ai-chat.bsg.brave.com"),
                _ => None,
            }
            .map(str::to_string)
        })
        .expect("configured");
        let serde_json::Value::Object(root) = serde_json::from_str(block).expect("json") else {
            panic!("not an object");
        };
        config.providers = Provider::all(&root);
        config.chatgpt_models = bravebot_config::Settings::parse(block)
            .chatgpt_models()
            .to_vec();
        config
    }

    /// An entry named `chatgpt` that states no endpoint and names two models, neither of which the
    /// stand-in's listing offers.
    const PINNED: &str = r#"{"provider": {"chatgpt": {"models": {
        "gpt-pinned": {"limit": {"context": 400000, "output": 128000}},
        "gpt-plain": {}
    }}}}"#;

    /// What a request to the stand-in carried: its request line and its headers, lower-cased.
    type Seen = (String, Vec<(String, String)>);

    /// A server that answers one request per entry of `replies`, in order, each with its status
    /// and body, and tells what each request was.
    fn answering(replies: Vec<(u16, &'static str)>) -> (String, std::sync::mpsc::Receiver<Seen>) {
        use std::io::{BufRead, BufReader, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("address").port();
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for (status, body) in replies {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                let mut line = String::new();
                reader.read_line(&mut line).expect("request line");
                let mut headers = Vec::new();
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
                        break;
                    }
                    if let Some((name, value)) = header.split_once(':') {
                        headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
                    }
                }
                let _ = sender.send((line.trim().to_string(), headers));
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 {status} A Status\r\nContent-Type: application/json\r\n\
                         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                );
            }
        });
        (format!("http://127.0.0.1:{port}"), receiver)
    }

    /// A server that is listening and is never answered, so a connection made to it waits in the
    /// queue where [`connections`] counts it. A request that was made is there however long ago.
    fn silent() -> (String, std::net::TcpListener) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        listener.set_nonblocking(true).expect("nonblocking");
        let port = listener.local_addr().expect("address").port();
        (format!("http://127.0.0.1:{port}"), listener)
    }

    /// How many connections a server from [`silent`] was made.
    fn connections(listener: &std::net::TcpListener) -> usize {
        std::iter::from_fn(|| listener.accept().ok()).count()
    }

    const LISTING: &str = r#"{"models":[
        {"slug":"gpt-a","display_name":"GPT A","visibility":"list","supported_in_api":true},
        {"slug":"gpt-b","visibility":"hide","supported_in_api":true}]}"#;

    /// What a picker is given where the listing could not be read: the compiled list, which is the
    /// same whatever the service said or failed to say.
    fn compiled() -> AccountModels {
        AccountModels::Compiled(bravebot_aichat::models::compiled_for_account(
            &Provider::chatgpt_account(),
        ))
    }

    /// CHATGPT-9: where no usable sign-in is stored nothing is offered and nothing is asked of the
    /// service, and an ended sign-in is one that is no longer stored. A listing is a request the
    /// token goes with, so a machine that never signed in sends none.
    #[test]
    fn a_machine_nobody_signed_in_on_offers_nothing_and_asks_the_service_nothing() {
        for failure in [AccountFailure::NotSignedIn, AccountFailure::Ended] {
            let (endpoint, listener) = silent();
            let mut access = Held::new(Err(failure));
            let offered = account_models_from(
                &configured("{}"),
                &mut access,
                &account_at(&endpoint),
                &mut RecordingSink::new(),
                &Egress::new(),
            );
            assert_eq!(offered, AccountModels::Absent, "{failure:?}");
            assert_eq!(access.asked, 1);
            assert_eq!(connections(&listener), 0, "{failure:?} sent a request");
        }
    }

    /// CHATGPT-9: a sign-in that is stored and cannot be used is offered the compiled list and says
    /// so to the picker, which is not the silence of a machine that never signed in, and sends
    /// nothing either.
    #[test]
    fn an_account_whose_token_cannot_be_had_is_offered_the_compiled_list_and_asks_the_service_nothing()
     {
        for failure in [
            AccountFailure::Unusable,
            AccountFailure::Refused,
            AccountFailure::Renewal(RenewalFailure::Transport),
            AccountFailure::Renewal(RenewalFailure::Refused),
        ] {
            let (endpoint, listener) = silent();
            let offered = account_models_from(
                &configured("{}"),
                &mut Held::new(Err(failure)),
                &account_at(&endpoint),
                &mut RecordingSink::new(),
                &Egress::new(),
            );
            assert_eq!(offered, compiled(), "{failure:?}");
            assert_eq!(connections(&listener), 0, "{failure:?} sent a request");
        }
    }

    /// CHATGPT-9: a signed-in account is asked once, with its token, and what it lists is offered
    /// under the account's identifier.
    #[test]
    fn a_signed_in_account_is_offered_what_the_service_lists_under_its_own_identifier() {
        let (endpoint, received) = answering(vec![(200, LISTING)]);
        let mut access = Held::new(Ok("placeholder-token-8c1f"));
        let offered = account_models_from(
            &configured("{}"),
            &mut access,
            &account_at(&endpoint),
            &mut RecordingSink::new(),
            &Egress::new(),
        );

        let AccountModels::Offered(models) = offered else {
            panic!("{offered:?}");
        };
        let keys: Vec<&str> = models.iter().map(|model| model.key.as_str()).collect();
        assert_eq!(keys, ["chatgpt/gpt-a"]);
        assert_eq!(models[0].display_name, "GPT A");
        assert_eq!(models[0].provider.as_deref(), Some("ChatGPT"));
        let (line, headers) = received.recv().expect("the service was asked");
        assert_eq!(line, "GET /v1/models HTTP/1.1");
        let bearer = headers
            .iter()
            .find(|(name, _)| name == "authorization")
            .map(|(_, value)| value.as_str());
        assert_eq!(bearer, Some("Bearer placeholder-token-8c1f"));
        assert!(
            access.refused.is_empty(),
            "a token nobody refused was replaced"
        );
    }

    /// CHATGPT-9: a listing the service refused or answered in another shape offers the compiled
    /// list and says it could not be read, and one it answered empty offers nothing and says
    /// nothing: the account reaches no model, which the service said, and the compiled list is not
    /// used in its place.
    #[test]
    fn an_unreadable_listing_is_told_apart_from_an_empty_one() {
        for (status, body) in [
            (403, r#"{"error":"a refusal"}"#),
            (500, "unavailable"),
            (200, "not json"),
            (200, r#"{"data":[{"id":"gpt-a"}]}"#),
        ] {
            let (endpoint, _received) = answering(vec![(status, body)]);
            let mut access = Held::new(Ok("placeholder-token-8c1f"));
            let offered = account_models_from(
                &configured("{}"),
                &mut access,
                &account_at(&endpoint),
                &mut RecordingSink::new(),
                &Egress::new(),
            );
            assert_eq!(offered, compiled(), "{status} {body}");
            assert!(
                access.refused.is_empty(),
                "{status}: only a 401 is a refused token"
            );
        }

        let (endpoint, _received) = answering(vec![(200, r#"{"models":[]}"#)]);
        let offered = account_models_from(
            &configured("{}"),
            &mut Held::new(Ok("placeholder-token-8c1f")),
            &account_at(&endpoint),
            &mut RecordingSink::new(),
            &Egress::new(),
        );
        assert_eq!(offered, AccountModels::Offered(Vec::new()));
    }

    /// CHATGPT-9 and CHATGPT-7: a token the host turns away is replaced once and the listing asked
    /// for again with the new one, and a second refusal ends it, so a token a clock made look valid
    /// does not hide the account's models and a token that is dead is not tried again and again.
    #[test]
    fn a_listing_refused_for_its_token_is_asked_again_once_with_a_replacement() {
        let (endpoint, received) = answering(vec![(401, "{}"), (200, LISTING)]);
        let mut access =
            Held::new(Ok("placeholder-token-8c1f")).replacing_with("placeholder-token-2e90");
        let offered = account_models_from(
            &configured("{}"),
            &mut access,
            &account_at(&endpoint),
            &mut RecordingSink::new(),
            &Egress::new(),
        );

        assert!(matches!(offered, AccountModels::Offered(ref rows) if rows.len() == 1));
        assert_eq!(access.refused, ["placeholder-token-8c1f"]);
        let bearers: Vec<String> = [received.recv().unwrap(), received.recv().unwrap()]
            .into_iter()
            .map(|(_, headers)| {
                headers
                    .into_iter()
                    .find(|(name, _)| name == "authorization")
                    .map(|(_, value)| value)
                    .unwrap_or_default()
            })
            .collect();
        assert_eq!(
            bearers,
            [
                "Bearer placeholder-token-8c1f",
                "Bearer placeholder-token-2e90"
            ]
        );

        // The replacement refused as well: one replacement per listing, and the compiled list.
        let (endpoint, _received) = answering(vec![(401, "{}"), (401, "{}")]);
        let mut access =
            Held::new(Ok("placeholder-token-8c1f")).replacing_with("placeholder-token-2e90");
        let offered = account_models_from(
            &configured("{}"),
            &mut access,
            &account_at(&endpoint),
            &mut RecordingSink::new(),
            &Egress::new(),
        );
        assert_eq!(offered, compiled());
        assert_eq!(access.refused.len(), 1);

        // A replacement that cannot be made is a sign-in that cannot be used, or one that is gone.
        for (failure, expected) in [
            (AccountFailure::Ended, AccountModels::Absent),
            (AccountFailure::Refused, compiled()),
        ] {
            let (endpoint, _received) = answering(vec![(401, "{}")]);
            let mut access = Held::new(Ok("placeholder-token-8c1f"));
            access.replacement = Err(failure);
            let offered = account_models_from(
                &configured("{}"),
                &mut access,
                &account_at(&endpoint),
                &mut RecordingSink::new(),
                &Egress::new(),
            );
            assert_eq!(offered, expected, "{failure:?}");
        }
    }

    /// CHATGPT-9 and CHATGPT-8: a listing refused with a code that says the account may not be used
    /// this way is not asked for again with a replacement, which would not change it, and nothing is
    /// offered. The one replacement a picker makes is for a token the host turned away alone.
    #[test]
    fn a_listing_refused_for_a_listed_code_is_not_replaced_or_asked_again() {
        let (endpoint, received) = answering(vec![
            (
                401,
                r#"{"error":{"code":"subscription_sharing_invalid_user"}}"#,
            ),
            (200, LISTING),
        ]);
        let mut access =
            Held::new(Ok("placeholder-token-8c1f")).replacing_with("placeholder-token-2e90");

        let offered = account_models_from(
            &configured("{}"),
            &mut access,
            &account_at(&endpoint),
            &mut RecordingSink::new(),
            &Egress::new(),
        );

        assert_eq!(offered, AccountModels::Refused(Refusal::NotPermitted));
        assert!(access.refused.is_empty(), "a token was replaced");
        received.recv().expect("the listing was asked for");
        assert!(
            received
                .recv_timeout(std::time::Duration::from_millis(200))
                .is_err(),
            "the listing was asked for again"
        );
    }

    /// CHATGPT-9: an entry with the id keeps it, so no model of the account is offered and the
    /// stored sign-in is not so much as read for it.
    #[test]
    fn a_gateway_entry_named_chatgpt_keeps_the_name_and_the_account_is_not_asked_about() {
        let (endpoint, listener) = silent();
        let mut access = Held::new(Ok("placeholder-token-8c1f"));
        let offered = account_models_from(
            &configured(
                r#"{"provider": {"chatgpt": {
                    "options": {"baseURL": "https://gateway.example.invalid/v1"},
                    "models": {"gpt-5": {}}
                }}}"#,
            ),
            &mut access,
            &account_at(&endpoint),
            &mut RecordingSink::new(),
            &Egress::new(),
        );

        assert_eq!(offered, AccountModels::Absent);
        assert_eq!(access.asked, 0, "the sign-in was read for a gateway's name");
        assert_eq!(connections(&listener), 0);
    }

    /// CHATGPT-9: the models an entry names are what is offered, with the window each states, and
    /// the serving host is not asked for its listing, so a person pins a short list out of an
    /// account that lists many. The stand-in answers a listing at once, so a client that asked for
    /// it would offer that model and fail here rather than wait. The access token is not renewed to
    /// offer the rows either: a token that cannot be had, as it cannot offline, leaves the same
    /// rows, which is how a gateway's stated models behave without a network.
    #[test]
    fn the_models_an_entry_names_are_offered_in_place_of_the_listing_and_need_no_token() {
        for token in [
            Ok("placeholder-token-8c1f"),
            Err(AccountFailure::Renewal(RenewalFailure::Transport)),
        ] {
            let (endpoint, received) = answering(vec![(200, LISTING)]);
            let mut access = Held::new(token);
            let offered = account_models_from(
                &configured(PINNED),
                &mut access,
                &account_at(&endpoint),
                &mut RecordingSink::new(),
                &Egress::new(),
            );

            let AccountModels::Offered(mut models) = offered else {
                panic!("{offered:?}");
            };
            models.sort_by(|left, right| left.key.cmp(&right.key));
            let rows: Vec<(&str, Option<u64>)> = models
                .iter()
                .map(|model| (model.key.as_str(), model.conversation_tokens))
                .collect();
            assert_eq!(
                rows,
                [
                    ("chatgpt/gpt-pinned", Some(400_000)),
                    (
                        "chatgpt/gpt-plain",
                        Some(bravebot_config::provider::CONTEXT_WINDOW)
                    ),
                ]
            );
            assert_eq!(access.asked, 0, "the token was had for {token:?}");
            assert!(received.try_recv().is_err(), "the listing was asked for");
        }
    }

    /// CHATGPT-9: none of the account's rows is offered where no sign-in is stored, so an entry's
    /// models are offered only where one is. A file that cannot be read is told from an absent one
    /// as it is for the listing, and neither sends a request or has the token renewed.
    #[test]
    fn the_models_an_entry_names_are_offered_only_where_somebody_is_signed_in() {
        for (stored, expected) in [
            (Ok(false), AccountModels::Absent),
            (Err(AccountFailure::Unusable), AccountModels::Unreadable),
        ] {
            let (endpoint, listener) = silent();
            let mut access = Held::new(Ok("placeholder-token-8c1f")).stored(stored);
            let offered = account_models_from(
                &configured(PINNED),
                &mut access,
                &account_at(&endpoint),
                &mut RecordingSink::new(),
                &Egress::new(),
            );
            assert_eq!(offered, expected, "{stored:?}");
            assert_eq!(access.asked, 0, "{stored:?} had the token renewed");
            assert_eq!(connections(&listener), 0, "{stored:?} sent a request");
        }
    }

    /// CHATGPT-2: the service is one constant, reached in the Responses protocol, and the same
    /// value serves every request.
    #[test]
    fn the_account_is_one_provider_at_the_serving_host() {
        let provider = account_provider();
        assert!(std::ptr::eq(provider, account_provider()));
        assert_eq!(
            provider.completion_url(),
            "https://api.openai.com/v1/responses"
        );
    }
}
