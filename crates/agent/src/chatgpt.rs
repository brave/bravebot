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
//! the next request would present a refresh token the server has already replaced.

use bravebot_aichat::models::Model;
use bravebot_aichat::responses::protocol::Refusal;
use bravebot_aichat::{AccountAccess, AccountFailure, ChatError, RenewalFailure};
use bravebot_config::provider::Provider;
use bravebot_config::{Config, Secret};
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::event::Sink;
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
use bravebot_net::{Egress, EgressError, Transport};
use bravebot_skus::chatgpt::{Access, Account, Class, Hosts, Renewer, TokenError};
use std::sync::{Mutex, OnceLock, PoisonError};

/// The renewer, made at the first request that needs a token.
static RENEWER: Mutex<Option<Renewer>> = Mutex::new(None);

/// The service a stored sign-in reaches: the one built from constants, shared by every request.
pub(crate) fn account_provider() -> &'static Provider {
    static PROVIDER: OnceLock<Provider> = OnceLock::new();
    PROVIDER.get_or_init(Provider::chatgpt_account)
}

/// The identifier every request of this process carries as its session, thread and client request
/// identifiers.
///
/// Generated here and from the operating system's random source, so it holds nothing a reply, a
/// model or a settings file chose and nothing about the person or the account. One for the process:
/// the service derives its prompt-cache affinity from it, so a value that changed between requests
/// would be a cache that never hit.
pub(crate) fn session_id() -> &'static str {
    static SESSION: OnceLock<String> = OnceLock::new();
    SESSION.get_or_init(bravebot_skus::new_request_id)
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
        obtained(Renewer::access_token)
    }

    fn replace_refused(&mut self, refused: &Secret) -> Result<Secret, AccountFailure> {
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
    /// A sign-in is stored and the listing could not be had: the token could not be renewed, the
    /// request failed or was refused for its token or its status, or the reply was not a listing.
    /// Nothing is offered, and the picker says the models could not be loaded.
    Unreadable,
    /// A sign-in is stored and the serving host refused the listing in a code that names the
    /// account ([CHATGPT-8](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-8)). Nothing is offered,
    /// and the desktop picker says so in that clause's sentence, which does not send the person to
    /// sign in again.
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
    // sign-in was removed by the renewal that found it so, and is a missing one from here on.
    let mut token = match access.access_token() {
        Ok(token) => token,
        Err(AccountFailure::NotSignedIn | AccountFailure::Ended) => return AccountModels::Absent,
        Err(_) => return AccountModels::Unreadable,
    };
    let mut routing = Routing::new();
    routing.insert_trusted("models", provider.models_url());
    let Ok(mut policy) = Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        sink,
    ) else {
        return AccountModels::Unreadable;
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
                    Err(_) => return AccountModels::Unreadable,
                };
            }
            Err(ChatError::Account(AccountFailure::Named(refusal))) => {
                return AccountModels::Refused(refusal);
            }
            Err(_) => return AccountModels::Unreadable,
        }
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
        .map(|access| Secret::new(access.token.expose()))
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

    /// CHATGPT-10: the three session headers carry one identifier for the whole process, so the
    /// service can tie requests to one conversation's cache, and it is a fresh random value of the
    /// shape of a version 4 identifier rather than anything about the person or the account.
    #[test]
    fn the_session_identifier_is_one_random_value_for_the_process() {
        let id = session_id();
        assert_eq!(id, session_id());
        assert_ne!(id, bravebot_skus::new_request_id());
        let groups: Vec<usize> = id.split('-').map(str::len).collect();
        assert_eq!(groups, [8, 4, 4, 4, 12], "{id}");
        assert!(id.chars().all(|c| c == '-' || c.is_ascii_hexdigit()));
        assert_eq!(id.as_bytes()[14], b'4');
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

    /// CHATGPT-9: a sign-in that is stored and cannot be used says so to the picker, which is not
    /// the silence of a machine that never signed in, and sends nothing either.
    #[test]
    fn an_account_whose_token_cannot_be_had_is_unreadable_and_asks_the_service_nothing() {
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
            assert_eq!(offered, AccountModels::Unreadable, "{failure:?}");
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

    /// CHATGPT-9: a listing the service refused or answered in another shape offers nothing and
    /// says it could not be read, and one it answered empty offers nothing and says nothing: the
    /// account reaches no model, which the service said.
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
            assert_eq!(offered, AccountModels::Unreadable, "{status} {body}");
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

        // The replacement refused as well: one replacement per listing, and nothing offered.
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
        assert_eq!(offered, AccountModels::Unreadable);
        assert_eq!(access.refused.len(), 1);

        // A replacement that cannot be made is a sign-in that cannot be used, or one that is gone.
        for (failure, expected) in [
            (AccountFailure::Ended, AccountModels::Absent),
            (AccountFailure::Refused, AccountModels::Unreadable),
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
