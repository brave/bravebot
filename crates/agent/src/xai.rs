//! The words for what goes wrong with the xAI sign-in
//! ([XAI-8](../../../docs/specs/xai-sign-in.md#XAI-8),
//! [XAI-15](../../../docs/specs/xai-sign-in.md#XAI-15)).
//!
//! Each sentence is chosen from a closed value: a class of a failed request, the reason an access
//! token could not be had, or the failure of a turn's request ([`XaiFailure`]). None holds text taken
//! from a reply, a request or a file, so none quotes one. They are catalog messages, said in the
//! language the person chose, and the command that signs in and every front end that presents the
//! token say the same sentence for the same failure.

use crate::backend::BackendError;
use crate::chatgpt::{AccountFile, Remedy, Saving, say_from, token_of};
use crate::report::Reporter;
use bravebot_aichat::models::Model;
use bravebot_aichat::protocol::ChatRequest;
use bravebot_aichat::{AccountFailure, ChatError, RenewalFailure, XaiFailure};
use bravebot_config::provider::Provider;
use bravebot_config::{Config, Secret};
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::event::Sink;
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
use bravebot_i18n::t;
use bravebot_net::{Egress, Transport};
use bravebot_skus::xai::{Access, Account, AccountError, Class, Hosts, Renewer, TokenError};
use std::borrow::Cow;
use std::sync::{Mutex, OnceLock, PoisonError};

/// The sentence for the class of a request to the authorization server that did not succeed.
pub fn said_of_class(class: Class) -> &'static str {
    match class {
        Class::Refused => t!(xai_class_refused),
        Class::RateLimited => t!(xai_class_rate_limited),
        Class::Unavailable => t!(xai_class_unavailable),
        Class::Incomplete => t!(xai_class_incomplete),
        Class::Transport => t!(xai_class_transport),
    }
}

/// The failure a turn is told of, from the step of the renewal that failed.
///
/// A closed set chosen from the step and the class of a status: nothing the authorization server wrote
/// is in either.
pub(crate) fn failure_of(failure: &TokenError) -> XaiFailure {
    match failure {
        TokenError::NotSignedIn => XaiFailure::NotSignedIn,
        TokenError::Ended => XaiFailure::Ended,
        TokenError::Store(_) => XaiFailure::Unusable,
        TokenError::Refresh(class) => XaiFailure::Renewal(match *class {
            Class::Refused => RenewalFailure::Refused,
            Class::RateLimited => RenewalFailure::RateLimited,
            Class::Unavailable => RenewalFailure::Unavailable,
            Class::Incomplete => RenewalFailure::Incomplete,
            Class::Transport => RenewalFailure::Transport,
        }),
    }
}

fn class_of(failure: RenewalFailure) -> Class {
    match failure {
        RenewalFailure::Refused => Class::Refused,
        RenewalFailure::RateLimited => Class::RateLimited,
        RenewalFailure::Unavailable => Class::Unavailable,
        RenewalFailure::Incomplete => Class::Incomplete,
        RenewalFailure::Transport => Class::Transport,
    }
}

/// The sentence for a failure of the account, from the message catalog.
///
/// Where signing in again is the remedy it names the command: not signed in, a sign-in that has ended,
/// which a refresh the server refused as the refresh token no longer being valid is, and an access
/// token the serving host refused with a new one as well. A request the host refused with a 403 after a
/// renewal names none, since signing in again does not change a refusal that a new token did not mend.
/// A renewal that failed in another way names none, since the file is left as it is and the next request asks again. A file that
/// could not be read says where to find out why.
pub fn said_of_failure(failure: XaiFailure) -> String {
    match failure {
        XaiFailure::NotSignedIn => t!(xai_not_signed_in).into(),
        XaiFailure::Ended => t!(xai_ended).into(),
        XaiFailure::Rejected => t!(xai_token_refused).into(),
        XaiFailure::NotPermitted => t!(xai_not_permitted).into(),
        XaiFailure::Unusable => t!(xai_unusable).into(),
        XaiFailure::Renewal(class) => {
            t!(xai_renewal_failed, reason = said_of_class(class_of(class)))
        }
    }
}

/// The sentence for why no access token could be had.
pub fn said_of_renewal(failure: &TokenError) -> String {
    said_of_failure(failure_of(failure))
}

/// The renewer, made at the first request that needs a token. One for the process, for the reason
/// the ChatGPT account has one: a renewal the file could not take is held by it, and a second renewer
/// would present a refresh token the server has already replaced.
static RENEWER: Mutex<Option<Renewer>> = Mutex::new(None);

/// What the file holds of the renewals this process made, as the turns that follow tell it.
static SAVING: Mutex<Saving> = Mutex::new(Saving::Saved);

/// The service a stored sign-in reaches: the one built from constants, shared by every request
/// ([XAI-16](../../../docs/specs/xai-sign-in.md#XAI-16)).
pub(crate) fn serving_provider() -> &'static Provider {
    #[cfg(test)]
    if let Some(host) = stand_in::host() {
        return host;
    }
    static PROVIDER: OnceLock<Provider> = OnceLock::new();
    PROVIDER.get_or_init(Provider::xai_account)
}

/// Say, in the turn that made the request, that the sign-in was renewed and the file could not take
/// the renewal ([XAI-17](../../../docs/specs/xai-sign-in.md#XAI-17)).
///
/// Called where the ChatGPT account's is, after each round's request and when the turn ends. The
/// refresh token is replaced on use, so the file still holds one the server no longer accepts, and the
/// sign-in ends at the next start unless it can be written by then, which is what the sentence tells
/// the person. It names no path and quotes nothing the file or the operating system said.
pub(crate) fn say_an_unsaved_renewal(reporter: &mut (impl Reporter + ?Sized)) {
    say_from(&SAVING, reporter, t!(xai_renewal_unwritten));
}

/// Note that the renewal behind a request's token could not be written, as having the token does.
///
/// For the test of what a turn says about it, which cannot make the renewer fail: the hosts it
/// renews at are compiled in. Nothing else calls it.
#[doc(hidden)]
pub fn note_a_renewal_the_file_could_not_take(unwritten: bool) {
    crate::chatgpt::record_a_token(&SAVING, unwritten);
}

/// Where an access token comes from, one request at a time: the stored sign-in, renewed where it is
/// about to expire. A trait so the rule [`presenting`] states is tried without the file or the host.
pub(crate) trait Presented {
    /// The token to send the next request with: the stored one where it has more than a minute left,
    /// and otherwise a new one.
    fn access_token(&mut self) -> Result<Secret, XaiFailure>;

    /// A token other than `refused`, which the serving host turned away. The caller sends the request
    /// once more with it and does not ask again for the same request.
    fn replace_refused(&mut self, refused: &Secret) -> Result<Secret, XaiFailure>;
}

/// The access token of the stored sign-in, renewed where it is about to expire.
///
/// A handle on the process's renewer and nothing more. The lock is held for one call and not for the
/// request the token is sent with, so a reply that is slow to arrive holds up no other turn.
pub(crate) struct StoredSignIn;

impl Presented for StoredSignIn {
    fn access_token(&mut self) -> Result<Secret, XaiFailure> {
        #[cfg(test)]
        if let Some(token) = stand_in::next_token() {
            return token;
        }
        obtained(Renewer::access_token)
    }

    fn replace_refused(&mut self, refused: &Secret) -> Result<Secret, XaiFailure> {
        #[cfg(test)]
        if let Some(token) = stand_in::replacement_for(refused) {
            return token;
        }
        let refused = bravebot_skus::Secret::new(refused.expose());
        obtained(|renewer| renewer.replace_refused(&refused))
    }
}

/// Ask the process's renewer for a token, making it first where this is the first request.
fn obtained(
    ask: impl FnOnce(&mut Renewer) -> Result<Access, TokenError>,
) -> Result<Secret, XaiFailure> {
    obtained_from(&RENEWER, &SAVING, ask)
}

/// [`obtained`] over the renewer and the record of what the file holds that it is given, which are the
/// process's own except in a test.
fn obtained_from(
    renewer: &Mutex<Option<Renewer>>,
    saving: &Mutex<Saving>,
    ask: impl FnOnce(&mut Renewer) -> Result<Access, TokenError>,
) -> Result<Secret, XaiFailure> {
    let mut held = renewer.lock().unwrap_or_else(PoisonError::into_inner);
    if held.is_none() {
        let renewer = Renewer::new(
            Hosts::compiled_in(),
            Transport::shared().agent_config_builder(),
        )
        .map_err(|_| XaiFailure::Unusable)?;
        *held = Some(renewer);
    }
    let renewer = held.as_mut().ok_or(XaiFailure::Unusable)?;
    told_of(saving, ask(renewer))
}

/// What a request is given for the answer of the renewer: the token, with what the file holds of the
/// renewal that made it noted in `saving` so a turn says so once ([XAI-17]), or the failure of the
/// step that found none.
///
/// [XAI-17]: ../../../docs/specs/xai-sign-in.md#XAI-17
fn told_of(
    saving: &Mutex<Saving>,
    outcome: Result<Access, TokenError>,
) -> Result<Secret, XaiFailure> {
    outcome
        .map(|access| token_of(saving, &access))
        .map_err(|failure| failure_of(&failure))
}

/// The statuses that may say the service did not accept the access token it was sent. The service
/// answered an access token it could not validate with a 403, observed with a token whose signature or
/// payload was altered, and a bearer that is no token at all with a 400, and a request that is wrong in
/// another way is answered with a 400 as well, so none of the three is proof ([XAI-18]).
///
/// [XAI-18]: ../../../docs/specs/xai-sign-in.md#XAI-18
const MAY_BE_THE_TOKEN: [u16; 3] = [400, 401, 403];

/// The status that, where a token the authorization server has just issued is sent, says the host
/// refuses the sign-in. A 400 is not it: a token that was just issued and is refused as a bad request
/// is a bad request.
const REFUSES_THE_SIGN_IN: u16 = 401;

/// The status that, where a token the authorization server has just issued is sent, is told as a
/// refusal for this account. A token the service could not validate answers a 403 too, so a first 403
/// is a question about the token and is answered by a renewal. One that is still refused with a 403
/// after it is not an invalid token. No refusal for a plan was observed, so what the failure is made of
/// is the status alone ([XAI-18]).
///
/// [XAI-18]: ../../../docs/specs/xai-sign-in.md#XAI-18
const NOT_PERMITTED: u16 = 403;

/// The failure of a turn that found no token to present, before any request was made.
fn before_a_request(failure: XaiFailure, attempts: u32) -> BackendError {
    BackendError::from(ChatError::Account(AccountFailure::Xai(failure)))
        .counted(attempts, None, None)
}

/// Make a request that presents the account's access token, and make it again once with a new token
/// where the host may have refused the first
/// ([XAI-17](../../../docs/specs/xai-sign-in.md#XAI-17),
/// [XAI-18](../../../docs/specs/xai-sign-in.md#XAI-18)).
///
/// The token is asked for before anything is sent, so a turn with no usable sign-in ends before a
/// request is made, naming the command. Where the request fails with a 400, a 401 or a 403, which the
/// service uses to say a token was not accepted and a 400 is also what a request that is wrong is
/// answered with, the token is replaced once and the request is made once more with the new one. This
/// reads a status and nothing a reply wrote, so the two meanings of a 403 are not told apart. What it
/// concludes is:
///
/// - Where the token could not be replaced, the failure is the renewal's: the sign-in has ended, or
///   the renewal failed as a class of request.
/// - Where the new token is accepted, the first was the one at fault and the turn goes on.
/// - Where the new token is refused with a 401, the sign-in is refused.
/// - Where it is refused with a 403, the failure is the refusal for this account: a token the
///   authorization server has just issued is not an invalid one.
/// - Where it is refused with any other status, the request was at fault and not the token, and it is
///   the failure the status says, as for any gateway.
///
/// So a 400 is never concluded to be a refused sign-in on its own, and what a 400 or a 403 for a request
/// that was wrong costs is one renewal and one more request.
pub(crate) fn presenting<T>(
    access: &mut impl Presented,
    mut ask: impl FnMut(&Secret) -> Result<T, BackendError>,
) -> Result<T, BackendError> {
    let token = access
        .access_token()
        .map_err(|failure| before_a_request(failure, 0))?;
    let first = match ask(&token) {
        Ok(reply) => return Ok(reply),
        Err(first) => first,
    };
    let diagnosis = first.diagnosis();
    if !diagnosis
        .status
        .is_some_and(|status| MAY_BE_THE_TOKEN.contains(&status))
    {
        return Err(first);
    }
    let sent = diagnosis.attempts.unwrap_or(0);
    let replacement = access
        .replace_refused(&token)
        .map_err(|failure| before_a_request(failure, sent))?;
    match ask(&replacement) {
        Ok(reply) => Ok(reply),
        Err(second) => {
            let second = second.after_earlier_attempts(sent);
            let diagnosis = second.diagnosis();
            let refused = match diagnosis.status {
                Some(REFUSES_THE_SIGN_IN) => Some(XaiFailure::Rejected),
                Some(NOT_PERMITTED) => Some(XaiFailure::NotPermitted),
                _ => None,
            };
            if let Some(failure) = refused {
                return Err(before_a_request(failure, diagnosis.attempts.unwrap_or(0)));
            }
            Err(second)
        }
    }
}

/// The sign-in file as it is on this machine, as far as telling a person what to configure needs it
/// ([BACKEND-39](../../../docs/specs/backends.md#BACKEND-39)).
///
/// It is read and nothing is renewed or asked of the service, so it starts nothing. The enum is the
/// ChatGPT account's, since the question is the same for both files.
pub(crate) fn account_file() -> AccountFile {
    file_of(&Account::load())
}

fn file_of(loaded: &Result<Option<Account>, AccountError>) -> AccountFile {
    match loaded {
        Ok(Some(_)) => AccountFile::Stored,
        Ok(None) | Err(AccountError::Nowhere { .. }) => AccountFile::NotStored,
        Err(_) => AccountFile::Unreadable,
    }
}

/// What the xAI account offers a picker
/// ([XAI-19](../../../docs/specs/xai-sign-in.md#XAI-19)).
#[derive(Debug, PartialEq, Eq)]
pub enum AccountModels {
    /// The account is not a roster to show: nothing usable is signed in, or a `provider` entry holds the
    /// id. Nothing was asked of the service, and the picker says nothing about it, since a person who
    /// never signed in is not owed a warning about a service they do not use.
    Absent,
    /// The models the listing offers, in the order it gave them. Empty for an account that reaches
    /// none, which the service answered and is not a failure.
    Offered(Vec<Model>),
    /// A sign-in is stored and the listing could not be had: the token could not be renewed, the request
    /// failed or was refused, or the reply was not a listing. Nothing is offered, since no list built
    /// into this program stands in for it, and the picker says the listing could not be read.
    Unlisted,
}

/// The account's models for a picker, asked of the serving host with the stored sign-in.
///
/// One function for the terminal and the desktop pickers, so neither holds its own copy of when the
/// account is asked about, which token it is asked with and what a failed listing offers. `sink` is the
/// trail the listing's request is recorded in, as the other rosters are.
pub fn account_models<S: Sink>(config: &Config, sink: &mut S) -> AccountModels {
    roster_from(
        config,
        &mut StoredSignIn,
        serving_provider(),
        sink,
        &Egress::new(),
    )
}

/// [`account_models`] for a given source of the token and a given service, so a test can stand a server
/// of its own in for the compiled-in host.
///
/// The listing is asked for as a turn's request is: where the host refuses it with a 400, a 401 or a 403
/// the token is replaced once and the listing asked for again ([XAI-18](../../../docs/specs/xai-sign-in.md#XAI-18)),
/// so a token a clock made look valid does not hide the account's models. A listing refused again offers
/// nothing, as any listing that cannot be had does.
fn roster_from<S: Sink>(
    config: &Config,
    access: &mut impl Presented,
    provider: &Provider,
    sink: &mut S,
    egress: &Egress,
) -> AccountModels {
    // An entry with the id is a gateway, and a stored sign-in is not asked about for it.
    if !config.offers_xai_account() {
        return AccountModels::Absent;
    }
    let mut routing = Routing::new();
    routing.insert_trusted("models", provider.models_url());
    let Ok(mut policy) = Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        sink,
    ) else {
        return AccountModels::Unlisted;
    };
    let asked = presenting(access, |token| {
        bravebot_aichat::models::list_from_xai_account(
            &mut policy,
            provider,
            token.expose(),
            egress,
        )
        .map_err(BackendError::from)
    });
    match asked {
        Ok(models) => AccountModels::Offered(models),
        // Asked for before anything is sent, so a machine with no sign-in sends nothing. An ended
        // sign-in was removed by the renewal that found it so, and the renewer says it ended to every
        // later ask of the process.
        Err(error) => match error.diagnosis().account {
            Some(AccountFailure::Xai(XaiFailure::NotSignedIn | XaiFailure::Ended)) => {
                AccountModels::Absent
            }
            _ => AccountModels::Unlisted,
        },
    }
}

/// The request as the account is sent it: with no effort level. The service was not asked with one, so
/// none is sent ([XAI-16](../../../docs/specs/xai-sign-in.md#XAI-16)), and a session's own level stays
/// what the person chose.
pub(crate) fn without_effort(request: &ChatRequest) -> Cow<'_, ChatRequest> {
    match request.effort {
        None => Cow::Borrowed(request),
        Some(_) => Cow::Owned(request.clone().with_effort(None)),
    }
}

/// A server a test of this crate runs in place of the serving host, and the tokens the stored sign-in
/// would hand out, for the requests made on the thread that installed it.
///
/// Compiled into this crate's test build alone, so no flag, variable or file can reach it and the
/// serving host is the one constant in every other build. It reads no stored sign-in and sends no real
/// token anywhere.
#[cfg(test)]
pub(crate) mod stand_in {
    use super::{Provider, Secret, XaiFailure};
    use std::cell::{Cell, RefCell};

    thread_local! {
        static HOST: Cell<Option<&'static Provider>> = const { Cell::new(None) };
        /// Why the stored sign-in hands out no token at its first ask, where it does not.
        static NO_TOKEN: Cell<Option<XaiFailure>> = const { Cell::new(None) };
        /// Why it hands out no replacement for a token the host turned away, where it does not.
        static NO_REPLACEMENT: Cell<Option<XaiFailure>> = const { Cell::new(None) };
        /// The tokens to hand out, in order: the first for the first ask, and each after it for the
        /// ask that replaces a token the host turned away.
        static TOKENS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
        static HANDED: Cell<usize> = const { Cell::new(0) };
        static REFUSED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }

    /// What ends the stand-in when it is dropped.
    pub(crate) struct Installed;

    impl Drop for Installed {
        fn drop(&mut self) {
            HOST.with(|host| host.set(None));
            NO_TOKEN.with(|failure| failure.set(None));
            NO_REPLACEMENT.with(|failure| failure.set(None));
            TOKENS.with(|tokens| tokens.borrow_mut().clear());
            HANDED.with(|handed| handed.set(0));
            REFUSED.with(|refused| refused.borrow_mut().clear());
        }
    }

    /// Send this thread's requests to the account to the server at `endpoint`, with `tokens` as the
    /// tokens the stored sign-in hands out.
    pub(crate) fn at(endpoint: &str, tokens: &[&str]) -> Installed {
        let text = format!(
            r#"{{"provider": {{"xai": {{"name": "xAI",
                "options": {{"baseURL": "{endpoint}/v1"}}}}}}}}"#
        );
        let serde_json::Value::Object(root) = serde_json::from_str(&text).expect("json") else {
            panic!("not an object");
        };
        let provider = Provider::all(&root).pop().expect("one provider");
        HOST.with(|host| host.set(Some(Box::leak(Box::new(provider)))));
        TOKENS.with(|held| *held.borrow_mut() = tokens.iter().map(|t| (*t).to_string()).collect());
        HANDED.with(|handed| handed.set(0));
        REFUSED.with(|refused| refused.borrow_mut().clear());
        Installed
    }

    /// The same, with a stored sign-in that cannot hand out a token: nothing is stored, or it ended.
    pub(crate) fn at_without_a_token(endpoint: &str, failure: XaiFailure) -> Installed {
        let installed = at(endpoint, &[]);
        NO_TOKEN.with(|held| held.set(Some(failure)));
        installed
    }

    /// Make the renewal that replaces a token the host turned away fail as `failure`.
    pub(crate) fn fail_the_replacement(failure: XaiFailure) {
        NO_REPLACEMENT.with(|held| held.set(Some(failure)));
    }

    pub(super) fn host() -> Option<&'static Provider> {
        HOST.with(Cell::get)
    }

    /// The next token, where a stand-in is installed. The last one is handed out again where the list
    /// has run out.
    pub(super) fn next_token() -> Option<Result<Secret, XaiFailure>> {
        host()?;
        if let Some(failure) = NO_TOKEN.with(Cell::get) {
            return Some(Err(failure));
        }
        TOKENS.with(|tokens| {
            let tokens = tokens.borrow();
            let at = HANDED.with(|handed| {
                let at = handed.get();
                handed.set(at + 1);
                at
            });
            tokens
                .get(at.min(tokens.len().saturating_sub(1)))
                .map(|token| Ok(Secret::new(token)))
        })
    }

    /// The token that replaces `refused`, which is noted as having been turned away.
    pub(super) fn replacement_for(refused: &Secret) -> Option<Result<Secret, XaiFailure>> {
        host()?;
        REFUSED.with(|held| held.borrow_mut().push(refused.expose().to_string()));
        if let Some(failure) = NO_REPLACEMENT.with(Cell::get) {
            return Some(Err(failure));
        }
        next_token()
    }

    /// The tokens the host turned away, in order.
    pub(crate) fn refused() -> Vec<String> {
        REFUSED.with(|refused| refused.borrow().clone())
    }
}

/// The command that is the remedy for a failure of the account, where one is. Which failures have one
/// is decided here and nowhere else: the terminal's sentences name it, and the desktop bridge's are
/// passed it and send it beside the sentence.
pub fn remedy_of_failure(failure: XaiFailure) -> Option<Remedy> {
    match failure {
        XaiFailure::NotSignedIn | XaiFailure::Ended | XaiFailure::Rejected => {
            Some(Remedy::XaiSignIn)
        }
        XaiFailure::Unusable => Some(Remedy::XaiStatus),
        // Signing in again does not change a refusal for this account, so no command is its remedy.
        XaiFailure::NotPermitted | XaiFailure::Renewal(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_skus::xai::{AccountError, Defect};

    const CLASSES: [Class; 5] = [
        Class::Refused,
        Class::RateLimited,
        Class::Unavailable,
        Class::Incomplete,
        Class::Transport,
    ];

    /// XAI-17: what the file holds of a renewal is noted where the token that renewal made is handed out,
    /// so a turn says that it could not be saved, once, and a later write that succeeds clears it so a
    /// later failure is said again. A failure of a step that found no token notes nothing. The test goes
    /// through `obtained_from`, which `obtained` is made of, and so through `told_of` as the renewer's
    /// answer does, which the one test that sets the record by hand would not notice being left out. The
    /// renewer it holds is a placeholder that is asked nothing, since the closure gives the answer, so
    /// that `obtained` passes the process's own renewer and record to it is read and not tested here.
    #[test]
    fn a_token_whose_renewal_the_file_could_not_take_is_noted_and_said_once_where_it_is_handed_out()
    {
        use crate::report::RecordingReporter;
        let saving = Mutex::new(Saving::Saved);
        let placeholder = Mutex::new(Some(
            Renewer::new(
                Hosts::compiled_in(),
                Transport::shared().agent_config_builder(),
            )
            .expect("a renewer, which reads no file until it is asked"),
        ));
        let handed_out = |saving: &Mutex<Saving>, outcome: Result<Access, TokenError>| {
            obtained_from(&placeholder, saving, |_| outcome)
        };
        let access = |unwritten: bool| Access {
            token: bravebot_skus::Secret::new("a-token"),
            unwritten: unwritten.then(|| AccountError::Unwritable {
                detail: "a path".to_string(),
            }),
        };
        let said_now = |saving: &Mutex<Saving>| {
            let mut reporter = RecordingReporter::default();
            say_from(saving, &mut reporter, "the renewal was not saved");
            reporter.notices.len()
        };

        // A renewal the file took notes nothing, and a step that found no token notes nothing.
        assert!(handed_out(&saving, Ok(access(false))).is_ok());
        assert!(handed_out(&saving, Err(TokenError::Ended)).is_err());
        assert_eq!(said_now(&saving), 0);

        // One it could not take is said, once, and not again by the next request that has a token.
        assert_eq!(
            handed_out(&saving, Ok(access(true)))
                .expect("a token")
                .expose(),
            "a-token"
        );
        assert_eq!(said_now(&saving), 1);
        assert!(handed_out(&saving, Ok(access(true))).is_ok());
        assert_eq!(said_now(&saving), 0, "said again for the same failure");

        // A write that succeeds clears it, so a later failure is said again.
        assert!(handed_out(&saving, Ok(access(false))).is_ok());
        assert!(handed_out(&saving, Ok(access(true))).is_ok());
        assert_eq!(said_now(&saving), 1);
    }

    /// BACKEND-39: a stored sign-in is told apart from an absent one and from one that cannot be read,
    /// since an unreadable file configures nothing and is said, as the ChatGPT account's is. A read that
    /// took an unreadable file for a stored sign-in would send the person to name a model no turn can
    /// reach, and one that took it for an absent one would not say why a sign-in they made is not used.
    #[test]
    fn a_stored_sign_in_is_told_apart_from_an_absent_one_and_from_one_that_cannot_be_read() {
        let stored = Account {
            access_token: bravebot_skus::Secret::new("placeholder-access-token"),
            refresh_token: bravebot_skus::Secret::new("placeholder-refresh-token"),
            expires_at: 1,
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
            AccountError::Malformed(Defect::MissingField),
        ] {
            let shown = unusable.to_string();
            assert_eq!(file_of(&Err(unusable)), AccountFile::Unreadable, "{shown}");
        }
    }

    /// XAI-8: each class of a failed request has a sentence of its own, so a person told that xAI is
    /// limiting requests is not told it refused one, and no sentence is empty.
    #[test]
    fn each_class_of_a_failed_request_has_its_own_sentence() {
        let mut said: Vec<&str> = CLASSES.iter().map(|class| said_of_class(*class)).collect();
        said.sort_unstable();
        said.dedup();

        assert_eq!(said.len(), CLASSES.len(), "two classes share a sentence");
        assert!(said.iter().all(|sentence| !sentence.is_empty()));
    }

    /// XAI-8: a refresh the server refused as the refresh token no longer being valid, a sign-in
    /// that was never made, a renewal that failed and a file that could not be read are four
    /// different sentences. Signing in again is named where it is the remedy and not where it is not:
    /// a renewal that may pass is not told to be repeated as a new sign-in, and a file that cannot be
    /// read is sent to `auth status`, which says why. A sentence that quoted anything would put
    /// text chosen by a server in front of a person, so each of these is built from the value alone.
    #[test]
    fn each_reason_no_token_could_be_had_has_its_own_sentence_and_names_the_remedy_it_has() {
        let not_signed_in = said_of_renewal(&TokenError::NotSignedIn);
        let ended = said_of_renewal(&TokenError::Ended);
        let unusable = said_of_renewal(&TokenError::Store(AccountError::Malformed(Defect::Empty)));
        let refused = said_of_renewal(&TokenError::Refresh(Class::Refused));
        let limited = said_of_renewal(&TokenError::Refresh(Class::RateLimited));

        let mut all = vec![
            not_signed_in.clone(),
            ended.clone(),
            unusable.clone(),
            refused.clone(),
            limited.clone(),
        ];
        all.sort();
        all.dedup();
        assert_eq!(all.len(), 5, "two reasons share a sentence");
        for sentence in [&not_signed_in, &ended] {
            assert!(
                sentence.contains("`bravebot auth login xai`"),
                "no remedy: {sentence}"
            );
        }
        assert!(
            unusable.contains("`bravebot auth status xai`"),
            "{unusable}"
        );
        for sentence in [&refused, &limited, &unusable] {
            assert!(
                !sentence.contains("auth login"),
                "signing in again was offered for a failure it does not mend: {sentence}"
            );
        }
        assert!(refused.contains(said_of_class(Class::Refused)), "{refused}");
        assert!(
            limited.contains(said_of_class(Class::RateLimited)),
            "{limited}"
        );
    }

    /// Every failure of the account a turn can be told of.
    fn every_failure() -> [XaiFailure; 10] {
        [
            XaiFailure::NotSignedIn,
            XaiFailure::Ended,
            XaiFailure::Rejected,
            XaiFailure::NotPermitted,
            XaiFailure::Unusable,
            XaiFailure::Renewal(RenewalFailure::Refused),
            XaiFailure::Renewal(RenewalFailure::RateLimited),
            XaiFailure::Renewal(RenewalFailure::Unavailable),
            XaiFailure::Renewal(RenewalFailure::Incomplete),
            XaiFailure::Renewal(RenewalFailure::Transport),
        ]
    }

    /// XAI-15: each failure of a turn's request has a sentence of its own, and the sign-in the host
    /// refused with a new token as well is told apart from one that was never made and from one that
    /// ended, since the first is a refusal of the token and the others are about the sign-in. The
    /// sentence names signing in again where that is the remedy, and no other: a renewal that may pass
    /// is not told to be repeated as a new sign-in. None names ChatGPT or OpenAI. A turn that said the
    /// ChatGPT sentence for an xAI failure would send a person to sign in to the wrong service.
    #[test]
    fn each_failure_of_a_turns_request_has_its_own_sentence_and_names_the_remedy_it_has() {
        let mut said: Vec<String> = every_failure().into_iter().map(said_of_failure).collect();
        let all = said.clone();
        said.sort();
        said.dedup();
        assert_eq!(
            said.len(),
            all.len(),
            "two failures share a sentence: {all:?}"
        );

        for (failure, sentence) in every_failure().into_iter().zip(&all) {
            let remedy = remedy_of_failure(failure);
            match remedy {
                Some(Remedy::XaiSignIn) => assert!(
                    sentence.contains("`bravebot auth login xai`"),
                    "{failure:?}: {sentence}"
                ),
                Some(Remedy::XaiStatus) => assert!(
                    sentence.contains("`bravebot auth status xai`"),
                    "{failure:?}: {sentence}"
                ),
                Some(other) => panic!("{failure:?} has the remedy of another service: {other:?}"),
                None => assert!(
                    !sentence.contains("`bravebot auth"),
                    "{failure:?}: {sentence}"
                ),
            }
            assert!(
                !sentence.contains("ChatGPT") && !sentence.contains("OpenAI"),
                "{failure:?}: {sentence}"
            );
        }
        assert_eq!(
            remedy_of_failure(XaiFailure::Rejected),
            Some(Remedy::XaiSignIn)
        );
        // A refusal for this account is not mended by signing in again, so no command is its remedy and
        // its sentence says that signing in again will not change it and where to look.
        assert_eq!(remedy_of_failure(XaiFailure::NotPermitted), None);
        let plan = said_of_failure(XaiFailure::NotPermitted);
        assert!(
            plan.contains("will not change that")
                && plan.contains("Check the plan at xAI")
                && plan.contains("choose another model")
                && !plan.contains("sign-in has ended")
                && !plan.contains("auth login"),
            "{plan}"
        );
        assert_eq!(
            Remedy::XaiSignIn.command(),
            "bravebot auth login xai",
            "the command is what a person types"
        );
        assert_eq!(Remedy::XaiStatus.command(), "bravebot auth status xai");
    }

    /// XAI-15: each step of a renewal that failed becomes one failure, and the sign-in ending is told
    /// apart from a renewal that merely failed, since only the first removed the file and asks for a
    /// new sign-in. The class of a refused request is the one it was, so a rate limit is not told as a
    /// refusal.
    #[test]
    fn each_failed_step_of_a_renewal_is_its_own_failure_and_only_an_ended_sign_in_is_ended() {
        for (step, failure) in [
            (TokenError::NotSignedIn, XaiFailure::NotSignedIn),
            (TokenError::Ended, XaiFailure::Ended),
            (
                TokenError::Store(AccountError::Unusable {
                    detail: "a path".to_string(),
                }),
                XaiFailure::Unusable,
            ),
            (
                TokenError::Refresh(Class::Refused),
                XaiFailure::Renewal(RenewalFailure::Refused),
            ),
            (
                TokenError::Refresh(Class::RateLimited),
                XaiFailure::Renewal(RenewalFailure::RateLimited),
            ),
            (
                TokenError::Refresh(Class::Unavailable),
                XaiFailure::Renewal(RenewalFailure::Unavailable),
            ),
            (
                TokenError::Refresh(Class::Incomplete),
                XaiFailure::Renewal(RenewalFailure::Incomplete),
            ),
            (
                TokenError::Refresh(Class::Transport),
                XaiFailure::Renewal(RenewalFailure::Transport),
            ),
        ] {
            assert_eq!(failure_of(&step), failure, "{step:?}");
        }
    }

    /// A source of tokens that records what it was asked and hands out the ones it was given.
    struct Scripted {
        first: Result<&'static str, XaiFailure>,
        replacement: Result<&'static str, XaiFailure>,
        asked: usize,
        replaced: Vec<String>,
    }

    impl Presented for Scripted {
        fn access_token(&mut self) -> Result<Secret, XaiFailure> {
            self.asked += 1;
            self.first.map(Secret::new)
        }

        fn replace_refused(&mut self, refused: &Secret) -> Result<Secret, XaiFailure> {
            self.replaced.push(refused.expose().to_string());
            self.replacement.map(Secret::new)
        }
    }

    fn signed_in() -> Scripted {
        Scripted {
            first: Ok("first"),
            replacement: Ok("second"),
            asked: 0,
            replaced: Vec::new(),
        }
    }

    /// A request that failed with `status`, as a turn's request does, counted as one made.
    fn refused_with(status: u16) -> BackendError {
        BackendError::from(ChatError::Egress(bravebot_net::EgressError::Status {
            url: "https://api.example.invalid/v1/chat/completions".to_string(),
            status,
            retry_after: None,
            body: None,
        }))
        .counted(1, None, None)
    }

    /// XAI-18: the token is replaced for a 400 and a 401 and for no other failure, which is what the
    /// rule says and what keeps a refresh token from being spent on a model that is not there, a
    /// connection that broke or a stop the person asked for. The failure is returned as it was, with
    /// nothing asked of the authorization server. A 403 is among the three: the host refused an access
    /// token it could not validate with one, so a first 403 asks for a new token. The regression it
    /// rejects is a first 403 that renews nothing, which ends a turn for a token a renewal would mend.
    #[test]
    fn only_a_400_a_401_or_a_403_asks_for_a_replacement_token() {
        type Make = fn() -> BackendError;
        let cases: [(&str, Make, bool); 11] = [
            ("400", || refused_with(400), true),
            ("401", || refused_with(401), true),
            ("403", || refused_with(403), true),
            ("404", || refused_with(404), false),
            ("422", || refused_with(422), false),
            ("429", || refused_with(429), false),
            ("500", || refused_with(500), false),
            ("a redirect", || refused_with(307), false),
            ("a stop", || BackendError::from(ChatError::Cancelled), false),
            (
                "an empty reply",
                || BackendError::from(ChatError::NoContent),
                false,
            ),
            (
                "a reply that stopped",
                || BackendError::from(ChatError::Incomplete),
                false,
            ),
        ];
        for (name, make, replaces) in cases {
            let mut tokens = signed_in();
            let mut sent = Vec::new();

            let outcome = presenting(&mut tokens, |token| {
                sent.push(token.expose().to_string());
                match sent.len() {
                    1 => Err(make()),
                    _ => Ok(()),
                }
            });

            assert_eq!(tokens.replaced.len(), usize::from(replaces), "{name}");
            assert_eq!(sent.len(), 1 + usize::from(replaces), "{name}");
            assert_eq!(outcome.is_ok(), replaces, "{name}");
            if replaces {
                assert_eq!(sent, ["first", "second"], "{name}");
                assert_eq!(tokens.replaced, ["first"], "{name}");
            } else {
                assert_eq!(
                    outcome.expect_err(name).to_string(),
                    make().to_string(),
                    "{name}: the failure was changed"
                );
            }
        }
    }

    /// XAI-17: a sign-in with no token to give ends the turn before anything is sent, so the request
    /// is never made, and the failure is the sign-in's. A request made first and the failure read from
    /// its answer would be seen as a call.
    #[test]
    fn no_request_is_made_where_the_sign_in_has_no_token_to_give() {
        for failure in [
            XaiFailure::NotSignedIn,
            XaiFailure::Ended,
            XaiFailure::Unusable,
        ] {
            let mut tokens = Scripted {
                first: Err(failure),
                ..signed_in()
            };
            let mut made = 0;

            let outcome = presenting(&mut tokens, |_| {
                made += 1;
                Ok(())
            });

            assert_eq!(made, 0, "{failure:?}");
            assert_eq!(
                outcome.expect_err("no token").diagnosis().account,
                Some(AccountFailure::Xai(failure))
            );
        }
    }

    /// XAI-18: a replacement the host refuses with a 401 is the sign-in refused, with a 403 is the
    /// refusal for this account, and with a 400 is the request, as the second failure says, and a
    /// request that is accepted the second time goes on. The count of requests includes the first. Where
    /// the replacement could not be had the failure is the renewal's, and no second request is made.
    #[test]
    fn a_second_refusal_is_the_sign_in_with_a_401_the_account_with_a_403_and_the_request_with_a_400()
     {
        // A first 403 is how the host refused a token it could not validate, so it asks for a new token
        // as a 400 does, and both end the same way.
        for (first, second, rejected) in [
            (400, 401, true),
            (400, 403, false),
            (403, 403, false),
            (403, 401, true),
            (400, 400, false),
            (403, 400, false),
            (400, 404, false),
        ] {
            let mut tokens = signed_in();
            let mut sent = 0;

            let error = presenting::<()>(&mut tokens, |_| {
                sent += 1;
                Err(refused_with(if sent == 1 { first } else { second }))
            })
            .expect_err("refused twice");

            let diagnosis = error.diagnosis();
            assert_eq!(
                diagnosis.account == Some(AccountFailure::Xai(XaiFailure::Rejected)),
                rejected,
                "{first} then {second}"
            );
            assert_eq!(
                diagnosis.account == Some(AccountFailure::Xai(XaiFailure::NotPermitted)),
                second == 403,
                "{first} then {second}"
            );
            assert_eq!(
                diagnosis.attempts,
                Some(2),
                "{first} then {second}: both requests counted"
            );
            assert_eq!(tokens.replaced, ["first"], "{first} then {second}");
            assert_eq!(sent, 2, "{first} then {second}");
        }

        for (first, replacement) in [
            (400, XaiFailure::Ended),
            (400, XaiFailure::Unusable),
            (400, XaiFailure::Renewal(RenewalFailure::Unavailable)),
            // A first 403 whose renewal finds the sign-in ended is the sign-in ended.
            (403, XaiFailure::Ended),
            (403, XaiFailure::Renewal(RenewalFailure::Transport)),
        ] {
            let mut tokens = Scripted {
                replacement: Err(replacement),
                ..signed_in()
            };
            let mut sent = 0;

            let error = presenting::<()>(&mut tokens, |_| {
                sent += 1;
                Err(refused_with(first))
            })
            .expect_err("no replacement");

            assert_eq!(sent, 1, "{replacement:?}: a second request was made");
            assert_eq!(
                error.diagnosis().account,
                Some(AccountFailure::Xai(replacement)),
                "{first} then {replacement:?}"
            );
            assert_eq!(error.diagnosis().attempts, Some(1));
        }
    }

    /// XAI-16: the account is sent no effort level, and what it is sent is otherwise the request as it
    /// was built. The request the session holds keeps its level.
    #[test]
    fn the_request_the_account_is_sent_carries_no_level_and_is_otherwise_the_one_built() {
        use bravebot_aichat::protocol::{Effort, Message};
        let asked = ChatRequest::new("xai/grok-4.5", vec![Message::user("hi")])
            .with_effort(Some(Effort::High));

        let sent = without_effort(&asked);

        assert_eq!(sent.effort, None);
        assert_eq!(sent.model, asked.model);
        assert_eq!(
            serde_json::to_value(&*sent).expect("a body")["messages"],
            serde_json::to_value(&asked).expect("a body")["messages"]
        );
        assert_eq!(asked.effort, Some(Effort::High));
        let none = ChatRequest::new("xai/grok-4.5", vec![Message::user("hi")]);
        assert!(
            matches!(without_effort(&none), std::borrow::Cow::Borrowed(_)),
            "a request with no level was copied"
        );
    }

    /// A configuration with the gateways `block` configures, which a test varies and nothing else.
    fn configured(block: &str) -> Config {
        let mut config = Config::from_lookup(|key| match key {
            "BRAVE_AI_CHAT_ENDPOINT" => Some("https://example.invalid".into()),
            "BRAVE_SERVICES_KEY_ID" => Some("a-key-id".into()),
            "SERVICES_KEY_AICHAT" => Some("a-signing-key".into()),
            _ => None,
        })
        .expect("configured");
        let serde_json::Value::Object(root) = serde_json::from_str(block).expect("json") else {
            panic!("not an object");
        };
        config.providers = Provider::all(&root);
        config
    }

    /// The listing the serving host answered an account on 2026-10-26, cut to what the rule reads: three
    /// of the seven the chat route answered and three of the seven it refused.
    const LISTING: &str = r#"{"object":"list","data":[
        {"id":"grok-4.20-multi-agent-0309","object":"model","context_length":1000000},
        {"id":"grok-4.5","object":"model","context_length":500000},
        {"id":"grok-imagine-image","object":"model","context_length":16000},
        {"id":"grok-build-0.1","object":"model","context_length":256000},
        {"id":"grok-imagine-video","object":"model"},
        {"id":"grok-4.3","object":"model","context_length":1000000}]}"#;

    /// The models the account offers from a listing a stand-in host serves, with `tokens` as the stored
    /// sign-in and the answers the host gives in order.
    fn asked_for_the_models(
        config: &Config,
        access: &mut Scripted,
        answers: Vec<crate::testutil::XaiAnswer>,
    ) -> (AccountModels, crate::testutil::XaiHost) {
        let host = crate::testutil::xai_host(answers);
        let _at = stand_in::at(&host.endpoint, &[]);
        let models = roster_from(
            config,
            access,
            serving_provider(),
            &mut bravebot_core::event::RecordingSink::new(),
            &Egress::new(),
        );
        (models, host)
    }

    /// XAI-19: a machine nobody signed in on, or whose sign-in ended, offers nothing and says nothing, and
    /// asks the service nothing, since the token is asked for before anything is sent. A file that cannot
    /// be read, or a renewal that failed, is a listing that could not be had: nothing is offered for the
    /// account, and no list built into this program stands in for it.
    #[test]
    fn a_machine_nobody_signed_in_on_offers_nothing_and_asks_the_service_nothing() {
        for (failure, expected) in [
            (XaiFailure::NotSignedIn, AccountModels::Absent),
            (XaiFailure::Ended, AccountModels::Absent),
            (XaiFailure::Unusable, AccountModels::Unlisted),
            (
                XaiFailure::Renewal(RenewalFailure::Transport),
                AccountModels::Unlisted,
            ),
        ] {
            let mut access = Scripted {
                first: Err(failure),
                ..signed_in()
            };

            let (models, host) = asked_for_the_models(&configured("{}"), &mut access, vec![]);

            assert_eq!(models, expected, "{failure:?}");
            assert!(
                host.requests().is_empty(),
                "{failure:?}: the service was asked"
            );
        }
    }

    /// XAI-19 and XAI-14: a signed-in account is offered what the listing says, under the account's own
    /// identifier and by the written rule, asked for with the stored token as a bearer and nothing else,
    /// in the order the service listed the rows, with the window each states.
    #[test]
    fn a_signed_in_account_is_offered_what_the_service_lists_under_its_own_identifier() {
        use crate::testutil::XaiAnswer;
        let mut access = signed_in();

        let (models, host) = asked_for_the_models(
            &configured("{}"),
            &mut access,
            vec![XaiAnswer::Status(200, LISTING)],
        );

        let AccountModels::Offered(rows) = models else {
            panic!("the listing was not offered: {models:?}");
        };
        let keys: Vec<&str> = rows.iter().map(|row| row.key.as_str()).collect();
        assert_eq!(keys, ["xai/grok-4.5", "xai/grok-build-0.1", "xai/grok-4.3"]);
        assert_eq!(rows[0].conversation_tokens, Some(500_000));
        let taken = host.requests();
        assert_eq!(taken.len(), 1);
        assert_eq!(taken[0].path, "/v1/models");
        assert_eq!(taken[0].authorization.as_deref(), Some("Bearer first"));
        assert!(access.replaced.is_empty());
    }

    /// XAI-19 and XAI-18: a listing the host refuses with a 400, a 401 or a 403 is asked for again once
    /// with a replacement, as a turn's request is, so a token a clock made look valid does not hide the
    /// account's models. A listing refused again, or that is not a listing, offers nothing for the
    /// account and is told apart from one that was read and is empty, which is the service's answer.
    #[test]
    fn a_listing_refused_for_the_token_is_asked_again_once_with_a_replacement() {
        use crate::testutil::XaiAnswer;
        for status in [400, 401, 403] {
            let mut access = signed_in();

            let (models, host) = asked_for_the_models(
                &configured("{}"),
                &mut access,
                vec![
                    XaiAnswer::Status(status, "{}"),
                    XaiAnswer::Status(200, LISTING),
                ],
            );

            assert!(
                matches!(models, AccountModels::Offered(ref rows) if rows.len() == 3),
                "{status}: {models:?}"
            );
            assert_eq!(access.replaced, ["first"], "{status}");
            let bearers: Vec<Option<String>> = host
                .requests()
                .iter()
                .map(|request| request.authorization.clone())
                .collect();
            assert_eq!(
                bearers,
                [
                    Some("Bearer first".to_string()),
                    Some("Bearer second".to_string())
                ],
                "{status}"
            );
        }
    }

    /// XAI-19 and XAI-18: a listing the host refuses with a 403 and then with a 403 again offers nothing,
    /// after one replacement, as a turn's request refused twice with a 403 is the refusal for the account.
    /// The regression it rejects is a listing that asks again for ever, or that offers rows without a
    /// second answer.
    #[test]
    fn a_listing_refused_with_a_403_twice_replaces_the_token_once_and_offers_nothing() {
        use crate::testutil::XaiAnswer;
        let mut access = signed_in();

        let (models, host) = asked_for_the_models(
            &configured("{}"),
            &mut access,
            vec![
                XaiAnswer::Status(403, "{}"),
                XaiAnswer::Status(403, "{}"),
                XaiAnswer::Status(200, LISTING),
            ],
        );

        assert_eq!(models, AccountModels::Unlisted);
        assert_eq!(access.replaced, ["first"], "{:?}", access.replaced);
        assert_eq!(host.requests().len(), 2);
    }

    /// XAI-19: a listing that cannot be had offers nothing for the account, whatever the reason, and is
    /// told apart from an empty one and from a listing that held no row the rule keeps: those are the
    /// service's answer and offer nothing too, and are not a failure.
    #[test]
    fn a_listing_that_cannot_be_had_offers_nothing_and_is_told_apart_from_an_empty_one() {
        use crate::testutil::XaiAnswer;
        let refused = |status| {
            vec![
                XaiAnswer::Status(status, "{}"),
                XaiAnswer::Status(status, "{}"),
            ]
        };
        for (what, answers, expected) in [
            ("refused twice", refused(401), AccountModels::Unlisted),
            ("a bad request twice", refused(400), AccountModels::Unlisted),
            (
                "not found",
                vec![XaiAnswer::Status(404, "{}")],
                AccountModels::Unlisted,
            ),
            (
                "not a listing",
                vec![XaiAnswer::Status(200, "<html>not a listing</html>")],
                AccountModels::Unlisted,
            ),
            (
                "another shape",
                vec![XaiAnswer::Status(200, r#"{"models":[{"id":"grok-4.5"}]}"#)],
                AccountModels::Unlisted,
            ),
            (
                "empty",
                vec![XaiAnswer::Status(200, r#"{"object":"list","data":[]}"#)],
                AccountModels::Offered(Vec::new()),
            ),
            (
                "only models the rule leaves out",
                vec![XaiAnswer::Status(
                    200,
                    r#"{"data":[{"id":"grok-imagine-video"}]}"#,
                )],
                AccountModels::Offered(Vec::new()),
            ),
        ] {
            let mut access = signed_in();

            let (models, _host) = asked_for_the_models(&configured("{}"), &mut access, answers);

            assert_eq!(models, expected, "{what}");
        }
    }

    /// XAI-13 and XAI-19: an entry with the id is a gateway and keeps the name, so the account is not
    /// asked about, and a token is not so much as asked for. An entry that is a gateway of another id
    /// does not stop it.
    #[test]
    fn a_gateway_entry_named_xai_keeps_the_name_and_the_account_is_not_asked_about() {
        use crate::testutil::XaiAnswer;
        let entry = |id: &str| {
            configured(&format!(
                r#"{{"provider": {{"{id}": {{
                    "options": {{"baseURL": "https://gateway.example.invalid/v1"}},
                    "models": {{"grok-4.5": {{}}}}}}}}}}"#
            ))
        };

        let mut access = signed_in();
        let (models, host) = asked_for_the_models(&entry("xai"), &mut access, vec![]);
        assert_eq!(models, AccountModels::Absent);
        assert_eq!(access.asked, 0, "a token was asked for");
        assert!(host.requests().is_empty());

        let mut access = signed_in();
        let (models, _) = asked_for_the_models(
            &entry("litellm"),
            &mut access,
            vec![XaiAnswer::Status(200, LISTING)],
        );
        assert!(matches!(models, AccountModels::Offered(_)), "{models:?}");
    }

    /// A file that cannot be read says nothing of what the operating system said about it, which is
    /// a path and text the system chose.
    #[test]
    fn the_sentence_for_a_file_that_cannot_be_read_quotes_nothing_of_the_failure() {
        let said = said_of_renewal(&TokenError::Store(AccountError::Unusable {
            detail: "/home/a/.bravebot/xai-account.json: denied".to_string(),
        }));

        assert!(!said.contains("xai-account.json"), "{said}");
        assert!(!said.contains("denied"), "{said}");
    }
}
