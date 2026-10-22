//! The code exchange: the authorization code and the PKCE verifier, presented to the token
//! endpoint, for an access token and a refresh token.
//!
//! The reply is bytes this program did not write. What is taken from it is two tokens that are never
//! read, a number of seconds, and whether the scope that allows this use was granted. The rest of
//! the body, including the ID token and every error text, is dropped in this module
//! ([CHATGPT-5](../../../../docs/specs/chatgpt-sign-in.md#CHATGPT-5)).

use super::authorize::{RESOURCE, USE_SCOPE};
use super::encoding::percent_encode;
use super::{Class, Hosts};
use crate::Secret;
use crate::secret::Document;

/// How long a reply may be before it is refused, so a body somebody sent cannot fill memory.
const REPLY_LIMIT: u64 = 1024 * 1024;

/// An access token's life when the reply states none.
const DEFAULT_LIFETIME: u64 = 3600;

/// What the token endpoint granted.
///
/// No equality, which the tokens do not have: see [`Secret`].
pub(super) struct Tokens {
    pub(super) access: Secret,
    pub(super) refresh: Secret,
    /// Seconds the access token lasts from the moment of the reply.
    pub(super) lifetime: u64,
    /// Whether the scope that allows this program to use the subscription's tokens was granted.
    pub(super) allows_use: bool,
}

/// The agent that talks to the token endpoint, from the transport the caller states.
///
/// It follows no redirect: a redirect in answer to a request that carries a code, a verifier or a
/// token would send them to a host nobody compiled in, so one is a failure. A status of 400 or more
/// is read here rather than raised, so that the class of a reply is decided in one place.
pub(super) fn agent(
    transport: ureq::config::ConfigBuilder<ureq::typestate::AgentScope>,
) -> ureq::Agent {
    transport
        .max_redirects(0)
        .http_status_as_error(false)
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .build()
        .into()
}

/// Present the code and the verifier under the client, for tokens.
pub(super) fn exchange(
    agent: &ureq::Agent,
    hosts: &Hosts,
    client_id: &str,
    code: &Secret,
    verifier: &Secret,
    redirect_uri: &str,
) -> Result<Tokens, Class> {
    // The body is every secret of the exchange, so it is a buffer that clears itself.
    let body = Secret::new(form(&[
        ("grant_type", "authorization_code"),
        ("client_id", client_id),
        ("code", code.expose()),
        ("code_verifier", verifier.expose()),
        ("redirect_uri", redirect_uri),
        ("resource", RESOURCE),
    ]));
    post(agent, &hosts.token, &body)
}

/// A form body, each name and value percent-encoded.
fn form(fields: &[(&str, &str)]) -> String {
    fields
        .iter()
        .map(|(name, value)| format!("{name}={}", percent_encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn post(agent: &ureq::Agent, url: &str, body: &Secret) -> Result<Tokens, Class> {
    let mut response = agent
        .post(url)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
        .send(body.expose().as_bytes())
        .map_err(|_| Class::Transport)?;

    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(class_of(status));
    }
    let text = response
        .body_mut()
        .with_config()
        .limit(REPLY_LIMIT)
        .read_to_string()
        .map_err(|_| Class::Incomplete)?;
    // Held in a buffer that clears itself: the text of a good reply is both tokens.
    let text = Secret::new(text);
    parse(text.expose())
}

/// The class of a status that is not a success.
///
/// A redirect is `Refused`, since none is followed.
fn class_of(status: u16) -> Class {
    match status {
        429 => Class::RateLimited,
        500..=599 => Class::Unavailable,
        _ => Class::Refused,
    }
}

/// The tokens of a reply, or `Incomplete` where it is not the shape the exchange records.
fn parse(text: &str) -> Result<Tokens, Class> {
    let document = Document::of(serde_json::from_str(text).map_err(|_| Class::Incomplete)?);
    let value = document.read();

    let token = |name: &str| {
        value
            .get(name)
            .and_then(serde_json::Value::as_str)
            .filter(|token| !token.is_empty())
            .map(Secret::new)
            .ok_or(Class::Incomplete)
    };
    let access = token("access_token")?;
    let refresh = token("refresh_token")?;

    let lifetime = match value.get("expires_in") {
        None | Some(serde_json::Value::Null) => DEFAULT_LIFETIME,
        Some(stated) => stated.as_u64().ok_or(Class::Incomplete)?,
    };
    let allows_use = value
        .get("scope")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|granted| granted.split_whitespace().any(|scope| scope == USE_SCOPE));

    Ok(Tokens {
        access,
        refresh,
        lifetime,
        allows_use,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reply the exchange records, with the three fields that decide what the caller does.
    fn reply(extra: &str) -> String {
        format!(
            "{{\"access_token\":\"an-access\",\"refresh_token\":\"a-refresh\",\
             \"id_token\":\"an-id\",\"scope\":\"openid chatgpt.tokens.use.direct\"{extra}}}"
        )
    }

    /// A reply is read for the two tokens, the lifetime and the scope, and nothing else is kept.
    #[test]
    fn a_reply_gives_the_tokens_the_lifetime_and_whether_the_use_was_allowed() {
        let tokens = parse(&reply(",\"expires_in\":1800")).expect("a reply");

        assert_eq!(tokens.access.expose(), "an-access");
        assert_eq!(tokens.refresh.expose(), "a-refresh");
        assert_eq!(tokens.lifetime, 1800);
        assert!(tokens.allows_use);
    }

    /// A reply that states no lifetime lasts 3600 seconds, and one that states a lifetime that is
    /// not a count of seconds is not read as any: guessing would store an expiry nobody stated.
    #[test]
    fn a_missing_lifetime_is_an_hour_and_one_that_is_not_a_number_is_refused() {
        assert_eq!(parse(&reply("")).expect("a reply").lifetime, 3600);
        assert_eq!(
            parse(&reply(",\"expires_in\":null"))
                .expect("a reply")
                .lifetime,
            3600
        );
        for stated in ["\"soon\"", "-5", "1.5", "[1]"] {
            assert!(
                matches!(
                    parse(&reply(&format!(",\"expires_in\":{stated}"))),
                    Err(Class::Incomplete)
                ),
                "{stated}"
            );
        }
    }

    /// The scope is read as a list of words. A scope that holds the word as part of another, or
    /// that does not name it, does not allow the use, and a reply with no scope allows none.
    #[test]
    fn the_use_is_allowed_only_where_the_scope_lists_it_as_a_word() {
        let with = |scope: &str| {
            parse(&format!(
                "{{\"access_token\":\"a\",\"refresh_token\":\"r\",\"scope\":\"{scope}\"}}"
            ))
            .expect("a reply")
            .allows_use
        };

        assert!(with("openid  chatgpt.tokens.use.direct profile"));
        assert!(!with("openid profile email offline_access"));
        assert!(!with("chatgpt.tokens.use.directly"));
        assert!(!with("xchatgpt.tokens.use.direct"));
        assert!(
            !parse("{\"access_token\":\"a\",\"refresh_token\":\"r\"}")
                .expect("a reply")
                .allows_use
        );
    }

    /// A reply without a token, with an empty one, or that is not JSON is incomplete, and nothing
    /// of it is in the answer.
    #[test]
    fn a_reply_that_is_not_the_recorded_shape_is_incomplete() {
        for text in [
            "",
            "not json",
            "[]",
            "{}",
            "{\"access_token\":\"a\"}",
            "{\"refresh_token\":\"r\"}",
            "{\"access_token\":\"\",\"refresh_token\":\"r\"}",
            "{\"access_token\":1,\"refresh_token\":\"r\"}",
        ] {
            assert!(matches!(parse(text), Err(Class::Incomplete)), "{text}");
        }
    }

    /// The class is the status's: 429 is rate-limited and 5xx is unavailable, so a rate limit is
    /// waited out as it is elsewhere, and every other refusal, a redirect included, is refused.
    #[test]
    fn a_status_is_classed_as_the_backends_classify_it() {
        for (status, class) in [
            (429, Class::RateLimited),
            (500, Class::Unavailable),
            (503, Class::Unavailable),
            (599, Class::Unavailable),
            (400, Class::Refused),
            (401, Class::Refused),
            (403, Class::Refused),
            (404, Class::Refused),
            (302, Class::Refused),
            (307, Class::Refused),
        ] {
            assert_eq!(class_of(status), class, "{status}");
        }
    }

    /// Every value is percent-encoded, so one with a `&` or an `=` in it cannot add a field or end
    /// one early.
    #[test]
    fn a_form_encodes_each_value() {
        assert_eq!(form(&[("a", "x&b=y"), ("c", "d e")]), "a=x%26b%3Dy&c=d%20e");
    }
}
