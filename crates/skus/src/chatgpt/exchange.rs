//! The requests to the token endpoint: the code exchange, which presents the authorization code and
//! the PKCE verifier, and the refresh, which presents the refresh token.
//!
//! The reply is bytes this program did not write. What is taken from it is two tokens that are never
//! read, the ID token of a code exchange, which [`super::id_token`] verifies and nothing stores, a
//! number of seconds, and whether the scope that allows this use was granted. The one field of an
//! error body that is read is the code, compared with the one value that says a refresh token is no
//! longer valid. The rest of the body, including every error text, is dropped in this module
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
    /// The ID token, where the reply holds one, to be verified and dropped.
    pub(super) id_token: Option<Secret>,
    /// Seconds the access token lasts from the moment of the reply.
    pub(super) lifetime: u64,
    /// Whether the scope that allows this program to use the subscription's tokens was granted.
    pub(super) allows_use: bool,
}

/// The error code that says a refresh token is no longer valid: RFC 6749 section 5.2's
/// `invalid_grant`. Not observed in a refused refresh from this server, which the spec lists as
/// unrecorded, so a refresh refused with any other code is a failure of the request and ends nothing.
const ENDED: &str = "invalid_grant";

/// The statuses a refused refresh is read for its error code at. RFC 6749 gives 400, and a server
/// answering a token as unauthorized gives 401.
const ENDED_STATUSES: [u16; 2] = [400, 401];

/// What a refresh gave.
///
/// No equality, which the tokens do not have: see [`Secret`].
pub(super) struct Renewed {
    pub(super) access: Secret,
    /// A new refresh token, where the server replaced the one presented. `None` keeps it.
    pub(super) refresh: Option<Secret>,
    /// Seconds the access token lasts from the moment of the reply.
    pub(super) lifetime: u64,
}

/// Why a refresh did not give tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Refused {
    /// The server says the refresh token is no longer valid: the sign-in is over.
    Ended,
    /// The request failed in some other way, which ends nothing.
    Failed(Class),
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
    let granted = granted(send(agent, &hosts.token, &body)?)?;
    Ok(Tokens {
        access: granted.access,
        refresh: granted.refresh.ok_or(Class::Incomplete)?,
        id_token: granted.id_token,
        lifetime: granted.lifetime,
        allows_use: granted.allows_use,
    })
}

/// Present the refresh token under the client, for a new access token.
///
/// The reply's scope is not read: the sign-in already checked the use was allowed, and a refresh
/// that states none changes nothing about that.
pub(super) fn refresh(
    agent: &ureq::Agent,
    hosts: &Hosts,
    client_id: &str,
    refresh_token: &Secret,
) -> Result<Renewed, Refused> {
    let body = Secret::new(form(&[
        ("grant_type", "refresh_token"),
        ("client_id", client_id),
        ("refresh_token", refresh_token.expose()),
        ("resource", RESOURCE),
    ]));
    let mut response = send(agent, &hosts.token, &body).map_err(Refused::Failed)?;
    let status = response.status().as_u16();
    if ENDED_STATUSES.contains(&status) && says_ended(&mut response) {
        return Err(Refused::Ended);
    }
    let granted = granted(response).map_err(Refused::Failed)?;
    Ok(Renewed {
        access: granted.access,
        refresh: granted.refresh,
        lifetime: granted.lifetime,
    })
}

/// Whether a refused refresh's body names the one error code that ends a sign-in.
///
/// A body that cannot be read or is not the shape is not that code: nothing is concluded from it.
fn says_ended(response: &mut ureq::http::Response<ureq::Body>) -> bool {
    let Ok(text) = response
        .body_mut()
        .with_config()
        .limit(REPLY_LIMIT)
        .read_to_string()
    else {
        return false;
    };
    let text = Secret::new(text);
    let Ok(parsed) = serde_json::from_str(text.expose()) else {
        return false;
    };
    let document = Document::of(parsed);
    document
        .read()
        .get("error")
        .and_then(serde_json::Value::as_str)
        == Some(ENDED)
}

/// A form body, each name and value percent-encoded.
fn form(fields: &[(&str, &str)]) -> String {
    fields
        .iter()
        .map(|(name, value)| format!("{name}={}", percent_encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

/// Send the form and return the reply, whatever its status: the caller decides what a status
/// means, since a refresh reads a refusal's code and the exchange does not.
fn send(
    agent: &ureq::Agent,
    url: &str,
    body: &Secret,
) -> Result<ureq::http::Response<ureq::Body>, Class> {
    agent
        .post(url)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
        .send(body.expose().as_bytes())
        .map_err(|_| Class::Transport)
}

/// The tokens of a reply, or the class of its status where it is not a success.
fn granted(mut response: ureq::http::Response<ureq::Body>) -> Result<Granted, Class> {
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
pub(super) fn class_of(status: u16) -> Class {
    match status {
        429 => Class::RateLimited,
        500..=599 => Class::Unavailable,
        _ => Class::Refused,
    }
}

/// What a successful reply holds. The refresh token is optional because a refresh need not replace
/// it, and the exchange refuses a reply without one.
struct Granted {
    access: Secret,
    refresh: Option<Secret>,
    /// Read for the code exchange only. A refresh's is dropped with the reply.
    id_token: Option<Secret>,
    lifetime: u64,
    allows_use: bool,
}

/// The tokens of a reply, or `Incomplete` where it is not the shape the exchange records.
fn parse(text: &str) -> Result<Granted, Class> {
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
    // Absent or null is no new refresh token. One that is present must be a usable one.
    let refresh = match value.get("refresh_token") {
        None | Some(serde_json::Value::Null) => None,
        Some(_) => Some(token("refresh_token")?),
    };
    // Absent, or not a usable string, is no ID token, which the sign-in refuses.
    let id_token = token("id_token").ok();

    let lifetime = match value.get("expires_in") {
        None | Some(serde_json::Value::Null) => DEFAULT_LIFETIME,
        Some(stated) => stated.as_u64().ok_or(Class::Incomplete)?,
    };
    let allows_use = value
        .get("scope")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|granted| granted.split_whitespace().any(|scope| scope == USE_SCOPE));

    Ok(Granted {
        access,
        refresh,
        id_token,
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
        assert_eq!(
            tokens.refresh.as_ref().expect("a refresh token").expose(),
            "a-refresh"
        );
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
            "{\"refresh_token\":\"r\"}",
            "{\"access_token\":\"a\",\"refresh_token\":\"\"}",
            "{\"access_token\":\"a\",\"refresh_token\":1}",
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

    /// A refresh reply with no refresh token keeps the one presented, which is how a server that
    /// does not rotate answers. One that sends a token that is empty or not text is not read as
    /// keeping it.
    #[test]
    fn a_reply_without_a_refresh_token_keeps_the_one_presented() {
        let kept = parse("{\"access_token\":\"a\"}").expect("a reply");
        assert!(kept.refresh.is_none());
        let null = parse("{\"access_token\":\"a\",\"refresh_token\":null}").expect("a reply");
        assert!(null.refresh.is_none());
        for unusable in ["\"\"", "7", "[]"] {
            assert!(
                matches!(
                    parse(&format!(
                        "{{\"access_token\":\"a\",\"refresh_token\":{unusable}}}"
                    )),
                    Err(Class::Incomplete)
                ),
                "{unusable}"
            );
        }
    }

    fn at(server: &super::super::stand_in::Server) -> (ureq::Agent, Hosts) {
        (
            agent(ureq::Agent::config_builder()),
            Hosts::at(&server.base),
        )
    }

    /// The code exchange needs a refresh token in the reply: an access token alone cannot be
    /// renewed, so a sign-in stored from it would end within the hour.
    #[test]
    fn the_exchange_refuses_a_reply_that_grants_no_refresh_token() {
        use super::super::stand_in::{Reply, serve};
        let server = serve(vec![Reply::json(200, "{\"access_token\":\"a\"}")]);
        let (agent, hosts) = at(&server);

        let outcome = exchange(
            &agent,
            &hosts,
            "client",
            &Secret::new("code"),
            &Secret::new("verifier"),
            "http://127.0.0.1:1/auth/callback",
        );

        assert!(matches!(outcome, Err(Class::Incomplete)));
        server.finish();
    }

    /// A refresh presents the four fields the exchange records and no other, to the token
    /// endpoint, with no credential in a header.
    #[test]
    fn a_refresh_presents_the_refresh_token_under_the_client_and_the_resource() {
        use super::super::stand_in::{Reply, named, serve};
        let server = serve(vec![Reply::json(
            200,
            "{\"access_token\":\"new\",\"expires_in\":900}",
        )]);
        let (agent, hosts) = at(&server);

        let renewed = refresh(&agent, &hosts, "client", &Secret::new("old-refresh"))
            .unwrap_or_else(|e| panic!("a refresh, not {e:?}"));
        let asked = server.finish();

        assert_eq!(renewed.access.expose(), "new");
        assert!(renewed.refresh.is_none());
        assert_eq!(renewed.lifetime, 900);
        let form = asked[0].form();
        let names: Vec<&str> = form.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            ["grant_type", "client_id", "refresh_token", "resource"]
        );
        assert_eq!(named(&form, "grant_type"), "refresh_token");
        assert_eq!(named(&form, "client_id"), "client");
        assert_eq!(named(&form, "refresh_token"), "old-refresh");
        assert_eq!(named(&form, "resource"), "https://api.openai.com/v1");
        assert_eq!(asked[0].header("authorization"), None);
    }

    /// Only a 400 or 401 whose error code is `invalid_grant` ends the sign-in. The same code at
    /// another status, another code at those statuses, and a body that is not the shape are each a
    /// failure of the request, so a busy server or a proxy's page cannot delete a person's sign-in.
    #[test]
    fn only_the_recorded_code_at_a_refusing_status_ends_the_sign_in() {
        use super::super::stand_in::{Reply, serve};
        let cases: [(u16, &str, Refused); 8] = [
            (400, "{\"error\":\"invalid_grant\"}", Refused::Ended),
            (401, "{\"error\":\"invalid_grant\"}", Refused::Ended),
            (
                403,
                "{\"error\":\"invalid_grant\"}",
                Refused::Failed(Class::Refused),
            ),
            (
                500,
                "{\"error\":\"invalid_grant\"}",
                Refused::Failed(Class::Unavailable),
            ),
            (
                400,
                "{\"error\":\"invalid_request\"}",
                Refused::Failed(Class::Refused),
            ),
            (400, "not json", Refused::Failed(Class::Refused)),
            (400, "{\"error\":7}", Refused::Failed(Class::Refused)),
            (
                429,
                "{\"error\":\"invalid_grant\"}",
                Refused::Failed(Class::RateLimited),
            ),
        ];
        for (status, body, expected) in cases {
            let server = serve(vec![Reply::json(status, body)]);
            let (agent, hosts) = at(&server);
            let outcome = refresh(&agent, &hosts, "client", &Secret::new("r"));
            server.finish();
            assert_eq!(outcome.err(), Some(expected), "{status} {body}");
        }
    }
}
