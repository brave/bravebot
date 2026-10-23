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

use bravebot_aichat::{AccountAccess, AccountFailure, RenewalFailure};
use bravebot_config::Secret;
use bravebot_config::provider::Provider;
use bravebot_net::Transport;
use bravebot_skus::chatgpt::{Access, Class, Hosts, Renewer, TokenError};
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
