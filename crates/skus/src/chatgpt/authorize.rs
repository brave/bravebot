//! The authorization request: the values one sign-in generates and the URL the browser is opened at
//! ([CHATGPT-3](../../../../docs/specs/chatgpt-sign-in.md#CHATGPT-3)).
//!
//! The URL is built from these values and from constants. Nothing in it comes from a settings file,
//! a model or a page.

use super::Hosts;
use crate::Secret;
use crate::oauth::encoding::{base64url, percent_encode, random_value};
use sha2::{Digest, Sha256};

/// The client a first sign-in presents, which asks OpenAI to register one for this program.
const PLACEHOLDER_CLIENT: &str = "dynamic_agent_client";

/// What the program calls itself when it asks for a client. The request sends no `originator` and
/// names no other program.
const AGENT_NAME: &str = "bravebot";

/// The scopes asked for. The last one allows this program to use the subscription's tokens, and a
/// reply that does not grant it is a sign-in this program cannot serve ([`USE_SCOPE`]).
const SCOPE: &str = "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";

/// The scope a reply has to grant.
pub(super) const USE_SCOPE: &str = "chatgpt.tokens.use.direct";

/// What every token request repeats, and the host that serves the models.
pub(super) const RESOURCE: &str = "https://api.openai.com/v1";

/// The values of one sign-in.
///
/// A second `auth login chatgpt` makes a new one, so a callback carrying an earlier flow's `state`
/// is not this one's. The verifier leaves the process once, in the code exchange, and is cleared
/// with the rest when this is dropped.
pub(super) struct Request {
    verifier: Secret,
    state: Secret,
    nonce: Secret,
    host_id: String,
    /// The client OpenAI issued, where the sign-in file holds one. `None` is the first sign-in.
    client: Option<String>,
}

impl Request {
    /// A request for a sign-in over `stored`, the client and host identifier a sign-in file holds,
    /// or for a first sign-in where there is none.
    pub(super) fn new(stored: Option<(&str, &str)>) -> Self {
        let (client, host_id) = match stored {
            Some((client, host_id)) => (Some(client.to_string()), host_id.to_string()),
            // Generated at a sign-in that has none stored, and kept in the sign-in file, so a
            // sign-in over a stored one names the same host.
            None => (None, format!("urn:uuid:{}", crate::new_request_id())),
        };
        Self {
            verifier: Secret::new(random_value()),
            state: Secret::new(random_value()),
            nonce: Secret::new(random_value()),
            host_id,
            client,
        }
    }

    pub(super) fn state(&self) -> &Secret {
        &self.state
    }

    pub(super) fn verifier(&self) -> &Secret {
        &self.verifier
    }

    /// The `nonce` the URL carries, which the ID token has to name.
    pub(super) fn nonce(&self) -> &Secret {
        &self.nonce
    }

    pub(super) fn client(&self) -> Option<&str> {
        self.client.as_deref()
    }

    pub(super) fn host_id(&self) -> &str {
        &self.host_id
    }

    /// The URL the browser is opened at, which redirects to `redirect_uri`.
    ///
    /// It holds the `state`, the `nonce`, the challenge and the identifiers, and nothing that can
    /// complete a sign-in: the challenge is public by construction, and a `code` obtained with
    /// another program's challenge does not exchange against this verifier.
    pub(super) fn url(&self, hosts: &Hosts, redirect_uri: &str) -> String {
        let challenge = base64url(&Sha256::digest(self.verifier.expose().as_bytes()));
        let mut parameters = vec![
            ("response_type", "code"),
            (
                "client_id",
                self.client.as_deref().unwrap_or(PLACEHOLDER_CLIENT),
            ),
            ("redirect_uri", redirect_uri),
            ("scope", SCOPE),
            ("resource", RESOURCE),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
            ("state", self.state.expose()),
            ("nonce", self.nonce.expose()),
        ];
        if self.client.is_none() {
            parameters.push(("agent_name_hint", AGENT_NAME));
        }
        parameters.push(("ext_agent_host_id", &self.host_id));

        let query: Vec<String> = parameters
            .into_iter()
            .map(|(name, value)| format!("{name}={}", percent_encode(value)))
            .collect();
        format!("{}?{}", hosts.authorize, query.join("&"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oauth::stand_in::{named, parameters};

    const REDIRECT: &str = "http://127.0.0.1:40000/auth/callback";

    /// CHATGPT-3 lists the parameters of the request and says there are no others. A request that
    /// added `originator` or `codex_cli_simplified_flow`, which the issue's earlier flow carried,
    /// would name another program, and one that left out `resource` would get a token for no
    /// service.
    #[test]
    fn a_first_sign_in_asks_for_a_client_and_carries_the_parameters_listed_and_no_other() {
        let request = Request::new(None);
        let url = request.url(&Hosts::compiled_in(), REDIRECT);

        assert!(
            url.starts_with("https://auth.openai.com/api/accounts/authorize?"),
            "{url}"
        );
        let found = parameters(&url);
        let mut names: Vec<&str> = found.iter().map(|(name, _)| name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(
            names,
            [
                "agent_name_hint",
                "client_id",
                "code_challenge",
                "code_challenge_method",
                "ext_agent_host_id",
                "nonce",
                "redirect_uri",
                "resource",
                "response_type",
                "scope",
                "state",
            ]
        );
        assert_eq!(named(&found, "response_type"), "code");
        assert_eq!(named(&found, "client_id"), "dynamic_agent_client");
        assert_eq!(named(&found, "agent_name_hint"), "bravebot");
        assert_eq!(named(&found, "redirect_uri"), REDIRECT);
        assert_eq!(
            named(&found, "scope"),
            "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct"
        );
        assert_eq!(named(&found, "resource"), "https://api.openai.com/v1");
        assert_eq!(named(&found, "code_challenge_method"), "S256");
        assert!(named(&found, "ext_agent_host_id").starts_with("urn:uuid:"));
    }

    /// A sign-in over a stored one presents the client OpenAI issued and no longer asks for one, and
    /// names the host it named before: a placeholder sent again would register a second client, and
    /// a new host identifier would make this machine two.
    #[test]
    fn a_sign_in_over_a_stored_one_presents_its_client_and_host_and_asks_for_no_client() {
        let request = Request::new(Some(("issued-client", "urn:uuid:stored-host")));
        let found = parameters(&request.url(&Hosts::compiled_in(), REDIRECT));

        assert_eq!(named(&found, "client_id"), "issued-client");
        assert_eq!(named(&found, "ext_agent_host_id"), "urn:uuid:stored-host");
        assert!(
            found.iter().all(|(name, _)| name != "agent_name_hint"),
            "{found:?}"
        );
        assert!(
            found
                .iter()
                .all(|(_, value)| value != "dynamic_agent_client"),
            "{found:?}"
        );
    }

    /// The challenge is the S256 of the verifier this request holds, which the exchange presents,
    /// and the verifier itself is not in the URL. A `plain` method, or a challenge of some other
    /// value, would let a program that read the URL complete the sign-in.
    #[test]
    fn the_challenge_is_the_hash_of_the_verifier_and_the_verifier_is_not_in_the_url() {
        let request = Request::new(None);
        let url = request.url(&Hosts::compiled_in(), REDIRECT);
        let found = parameters(&url);

        let expected = base64url(&Sha256::digest(request.verifier().expose().as_bytes()));
        assert_eq!(named(&found, "code_challenge"), expected);
        assert!(!url.contains(request.verifier().expose()), "{url}");
        assert_ne!(named(&found, "code_challenge"), request.verifier().expose());
    }

    /// Each sign-in makes values of its own. A `state` or a verifier that two sign-ins shared
    /// would let a callback meant for the first complete the second, and one that was a constant
    /// is one another program can send.
    #[test]
    fn each_sign_in_generates_its_own_verifier_state_and_nonce() {
        let first = Request::new(None);
        let second = Request::new(None);

        assert_ne!(first.verifier().expose(), second.verifier().expose());
        assert_ne!(first.state().expose(), second.state().expose());
        assert_ne!(first.host_id(), second.host_id());
        let a = parameters(&first.url(&Hosts::compiled_in(), REDIRECT));
        let b = parameters(&second.url(&Hosts::compiled_in(), REDIRECT));
        assert_ne!(named(&a, "nonce"), named(&b, "nonce"));
        assert_ne!(named(&a, "state"), named(&b, "state"));
        assert_ne!(named(&a, "nonce"), named(&a, "state"));
        assert_ne!(named(&a, "state"), first.verifier().expose());
    }

    /// The query is encoded, so a value with a character that ends a parameter is read back whole.
    /// The redirect carries a `:` and `/`, and the scope a space.
    #[test]
    fn the_values_are_percent_encoded_in_the_url() {
        let url = Request::new(None).url(&Hosts::compiled_in(), REDIRECT);

        assert!(url.contains("scope=openid%20profile%20email"), "{url}");
        assert!(
            url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A40000%2Fauth%2Fcallback"),
            "{url}"
        );
        assert!(!url.contains(' '), "{url}");
    }
}
