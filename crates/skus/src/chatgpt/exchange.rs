//! The requests to the token endpoint: the code exchange, which presents the authorization code and
//! the PKCE verifier, and the refresh, which presents the refresh token.
//!
//! The reply is bytes this program did not write. What is taken from it is two tokens that are never
//! read, the ID token of a code exchange, which [`super::id_token`] verifies and nothing stores, a
//! number of seconds, and whether the scope that allows this use was granted. The one field of an
//! error body that is read is the code, compared with the six values that say a refresh token is no
//! longer valid. The rest of the body, including every error text, is dropped in this module
//! ([CHATGPT-5](../../../../docs/specs/chatgpt-sign-in.md#CHATGPT-5)).

use super::Hosts;
use super::authorize::{RESOURCE, USE_SCOPE};
use crate::Secret;
use crate::oauth::{
    Class, INVALID_GRANT, Lifetime, REPLY_LIMIT, Refused, Renewed, class_of, ends_the_sign_in,
    form, send,
};
use crate::secret::Document;

/// The codes that say a refresh token can no longer be used, which end the sign-in at a refresh
/// refused with a 400 or 401: RFC 6749's `invalid_grant`, which this server sent for a refresh token
/// it did not know and for one revoked at its revocation endpoint, and the five others OpenAI's
/// documentation lists for an unusable refresh token. OpenAI's SDK reads all six as ending the
/// sign-in and Codex three of the five
/// ([CHATGPT-7](../../../../docs/specs/chatgpt-sign-in.md#CHATGPT-7)). Each is compared as a whole
/// with the string an `error` field holds, so an `error` object, a code in another field and a code
/// that is only part of another end nothing. The xAI sign-in reads `invalid_grant` alone.
const ENDED: [&str; 6] = [
    INVALID_GRANT,
    "invalid_refresh_token",
    "token_expired",
    "refresh_token_expired",
    "refresh_token_invalidated",
    "refresh_token_reused",
];

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
    let granted = granted(send(agent, &hosts.token, &body, None)?)?;
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
    let mut response = send(agent, &hosts.token, &body, None).map_err(Refused::Failed)?;
    if ends_the_sign_in(&mut response, &ENDED) {
        return Err(Refused::Ended);
    }
    let granted = granted(response).map_err(Refused::Failed)?;
    Ok(Renewed {
        access: granted.access,
        refresh: granted.refresh,
        lifetime: Lifetime::For(granted.lifetime),
    })
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

    fn at(server: &crate::oauth::stand_in::Server) -> (ureq::Agent, Hosts) {
        (
            crate::oauth::agent(ureq::Agent::config_builder()),
            Hosts::at(&server.base),
        )
    }

    /// The code exchange needs a refresh token in the reply: an access token alone cannot be
    /// renewed, so a sign-in stored from it would end within the hour.
    #[test]
    fn the_exchange_refuses_a_reply_that_grants_no_refresh_token() {
        use crate::oauth::stand_in::{Reply, serve};
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
        use crate::oauth::stand_in::{Reply, named, serve};
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
        assert_eq!(renewed.lifetime, Lifetime::For(900));
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

    /// The six codes OpenAI's documentation lists for a refresh token that cannot be used, as the
    /// clause writes them and not as the table in the crate holds them.
    const REFRESH_CODES: [&str; 6] = [
        "invalid_grant",
        "invalid_refresh_token",
        "token_expired",
        "refresh_token_expired",
        "refresh_token_invalidated",
        "refresh_token_reused",
    ];

    /// What a refresh whose reply holds this body at this status comes to.
    fn refreshed_as(status: u16, body: &str) -> Option<Refused> {
        use crate::oauth::stand_in::{Reply, serve};
        let server = serve(vec![Reply::json(status, body)]);
        let (agent, hosts) = at(&server);
        let outcome = refresh(&agent, &hosts, "client", &Secret::new("r"));
        server.finish();
        outcome.err()
    }

    /// Only a 400 or 401 whose `error`, read as a string, is one of the six refresh codes ends the
    /// sign-in. The same codes at another status, another code at those statuses, and a body that is
    /// not the shape are each a failure of the request, so a busy server or a proxy's page cannot
    /// delete a person's sign-in. The token endpoint sends `error` as a string, with the other codes
    /// of RFC 6749 for the other faults of a request. The serving host sends `error` as an object with
    /// a `code`, and Codex's fixtures for its own endpoint write the same, and a body of that shape is
    /// not read as the token endpoint saying the sign-in is over. An implementation that ended the
    /// sign-in for `invalid_grant` alone would keep a sign-in whose refresh token is past its age and
    /// make every later turn try it, one that read `error.code`, or ended the sign-in for any 400 or
    /// any 401, would delete a sign-in that a retry or a corrected request would have kept.
    #[test]
    fn only_one_of_the_six_refresh_codes_at_a_refusing_status_ends_the_sign_in() {
        for code in REFRESH_CODES {
            let body = format!("{{\"error\":\"{code}\",\"error_description\":\"d\"}}");
            for (status, expected) in [
                (400, Refused::Ended),
                (401, Refused::Ended),
                (403, Refused::Failed(Class::Refused)),
                (404, Refused::Failed(Class::Refused)),
                (429, Refused::Failed(Class::RateLimited)),
                (500, Refused::Failed(Class::Unavailable)),
                (503, Refused::Failed(Class::Unavailable)),
            ] {
                assert_eq!(
                    refreshed_as(status, &body),
                    Some(expected),
                    "{status} {code}"
                );
            }
            // The form the serving host and Codex's fixtures use, and a code in another field.
            for body in [
                format!("{{\"error\":{{\"code\":\"{code}\"}}}}"),
                format!(
                    "{{\"error\":{{\"message\":\"m\",\"type\":\"t\",\"param\":null,\"code\":\"{code}\"}}}}"
                ),
                format!("{{\"detail\":\"{code}\"}}"),
                format!("{{\"code\":\"{code}\"}}"),
                format!("{{\"error\":[\"{code}\"]}}"),
                format!("{{\"error\":\"x\",\"error_description\":\"{code}\"}}"),
            ] {
                for status in [400, 401] {
                    assert_eq!(
                        refreshed_as(status, &body),
                        Some(Refused::Failed(Class::Refused)),
                        "{status} {body}"
                    );
                }
            }
        }
        for body in [
            "{\"error\":\"invalid_request\"}",
            "{\"error\":\"invalid_client\"}",
            "{\"error\":\"unsupported_grant_type\"}",
            "{\"error\":\"access_denied\"}",
            "{\"error\":\"invalid_token\"}",
            "{\"error\":\"invalid_api_key\"}",
            "{\"error\":\"token_revoked\"}",
            "{\"error\":\"refresh_token_expired_\"}",
            "{\"error\":\"_token_expired\"}",
            "{\"error\":\" token_expired\"}",
            "{\"error\":\"Token_Expired\"}",
            "{\"error\":\"INVALID_GRANT\"}",
            "{\"error\":\"refresh_token\"}",
            "{\"error\":\"expired\"}",
            "{\"error\":\"\"}",
            "{\"error\":7}",
            "{\"error\":null}",
            "{\"detail\":\"refresh_token_reused\"}",
            "not json",
            "",
        ] {
            for status in [400, 401] {
                assert_eq!(
                    refreshed_as(status, body),
                    Some(Refused::Failed(Class::Refused)),
                    "{status} {body}"
                );
            }
        }
        assert_eq!(
            refreshed_as(
                405,
                "{\"error\":{\"message\":\"m\",\"type\":\"t\",\"param\":null,\"code\":null}}"
            ),
            Some(Refused::Failed(Class::Refused))
        );
    }

    /// A refresh that did not complete, or that was answered by a server that is busy, ends nothing:
    /// the person's sign-in is kept for the next ask whichever code a busy server's body names. A
    /// reader that ended the sign-in on the six codes at any status, or on a connection that did not
    /// complete, would delete a sign-in that the next request would have renewed.
    #[test]
    fn a_refresh_that_did_not_complete_or_met_a_busy_server_ends_nothing() {
        use crate::oauth::stand_in::{Reply, serve};
        let server = serve(vec![Reply::hang_up()]);
        let (agent, hosts) = at(&server);

        let outcome = refresh(&agent, &hosts, "client", &Secret::new("r"));

        server.finish();
        assert_eq!(outcome.err(), Some(Refused::Failed(Class::Transport)));
        for code in REFRESH_CODES {
            for status in [500, 502, 503, 504] {
                assert_eq!(
                    refreshed_as(status, &format!("{{\"error\":\"{code}\"}}")),
                    Some(Refused::Failed(Class::Unavailable)),
                    "{status} {code}"
                );
            }
        }
    }
}
