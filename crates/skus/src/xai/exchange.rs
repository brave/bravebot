//! The requests to the authorization server: the request for a code to sign in with, the polling of
//! the token endpoint for it, and the refresh.
//!
//! The replies are bytes this program did not write. What is taken from them is a code and an
//! address to show a person, two tokens that are never read, a number of seconds, and one field of an
//! error body, `error`, compared with a closed list of codes. Nothing else of a reply, its error text
//! included, leaves this module ([XAI-4](../../../../docs/specs/xai-sign-in.md#XAI-4)).

use super::{
    CLIENT_ID, DEVICE_GRANT, Hosts, LONGEST_CODE_LIFETIME, LONGEST_INTERVAL,
    LONGEST_TOKEN_LIFETIME, REFERRER, SCOPE, SHORTEST_TOKEN_LIFETIME,
};
use crate::Secret;
use crate::oauth::encoding::base64url_decode;
use crate::oauth::{
    Class, INVALID_GRANT, Lifetime, REPLY_LIMIT, Refused, Renewed, class_of, ends_the_sign_in,
    error_code, form,
};
use crate::secret::Document;

/// What the program names itself as in `user-agent`, as the other requests it makes do.
const USER_AGENT: &str = "bravebot";

/// How long a code lasts when the reply states none.
const DEFAULT_CODE_LIFETIME: u64 = 300;

/// How often the token endpoint is asked when the reply states none.
const DEFAULT_INTERVAL: u64 = 5;

/// An access token's life when the reply states none and the token states none.
const DEFAULT_TOKEN_LIFETIME: u64 = 3600;

/// How long an access token may be before its payload is not read for an expiry.
const TOKEN_LIMIT: usize = 16 * 1024;

/// The longest code or address that is shown: a longer one is not what the service sends.
const SHOWN_LIMIT: usize = 512;

/// What the request for a code gave.
///
/// No equality, which the device code does not have: see [`Secret`].
pub(super) struct Code {
    /// What the token endpoint is asked with until the person has used the code.
    pub(super) device_code: Secret,
    /// What the person enters at the address.
    pub(super) user_code: String,
    /// Where the code is entered.
    pub(super) verification_uri: String,
    /// The same address with the code in it, where the reply gives one.
    pub(super) verification_uri_complete: Option<String>,
    /// Seconds the code lasts from the moment of the reply.
    pub(super) expires_in: u64,
    /// Seconds between two asks of the token endpoint.
    pub(super) interval: u64,
}

/// What the token endpoint granted.
///
/// No equality, which the tokens do not have: see [`Secret`].
pub(super) struct Granted {
    pub(super) access: Secret,
    /// The refresh token, where the reply holds one.
    pub(super) refresh: Option<Secret>,
    pub(super) lifetime: Lifetime,
}

/// What one ask of the token endpoint came to.
pub(super) enum Polled {
    /// The code was used, and the endpoint granted tokens.
    Granted(Granted),
    /// The code has not been used yet.
    Pending,
    /// The endpoint wants to be asked less often.
    SlowDown,
    /// The token endpoint answered `access_denied`.
    AccessDenied,
    /// The token endpoint answered `authorization_denied`.
    AuthorizationDenied,
    /// The code stopped working before it was used.
    Expired,
}

/// The agent that talks to the authorization server, from the transport the caller states. It
/// follows no redirect and reads a status of 400 or more rather than raising it.
pub(super) fn agent(
    transport: ureq::config::ConfigBuilder<ureq::typestate::AgentScope>,
) -> ureq::Agent {
    crate::oauth::agent(transport)
}

/// Ask for a code to sign in with.
pub(super) fn code(agent: &ureq::Agent, hosts: &Hosts) -> Result<Code, Class> {
    let body = Secret::new(form(&[
        ("client_id", CLIENT_ID),
        ("scope", SCOPE),
        ("referrer", REFERRER),
    ]));
    let mut response = send(agent, &hosts.device, &body)?;
    let text = success_text(&mut response)?;
    parse_code(text.expose())
}

/// Ask the token endpoint whether the code has been used.
pub(super) fn poll(
    agent: &ureq::Agent,
    hosts: &Hosts,
    device_code: &Secret,
) -> Result<Polled, Class> {
    let body = Secret::new(form(&[
        ("grant_type", DEVICE_GRANT),
        ("client_id", CLIENT_ID),
        ("device_code", device_code.expose()),
    ]));
    let mut response = send(agent, &hosts.token, &body)?;
    let status = response.status().as_u16();
    if (200..300).contains(&status) {
        let text = read(&mut response)?;
        return parse_tokens(text.expose()).map(Polled::Granted);
    }
    // Only the one field of the body that names the error is read, and only to be compared with the
    // codes of the list. A body that cannot be read, or names none of them, is the class of its status.
    match error_code(&mut response).as_deref() {
        Some("authorization_pending") => Ok(Polled::Pending),
        Some("slow_down") => Ok(Polled::SlowDown),
        Some("access_denied") => Ok(Polled::AccessDenied),
        Some("authorization_denied") => Ok(Polled::AuthorizationDenied),
        Some("expired_token") => Ok(Polled::Expired),
        _ => Err(class_of(status)),
    }
}

/// Present the refresh token under the client, for a new access token.
pub(super) fn refresh(
    agent: &ureq::Agent,
    hosts: &Hosts,
    refresh_token: &Secret,
) -> Result<Renewed, Refused> {
    let body = Secret::new(form(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token.expose()),
        ("client_id", CLIENT_ID),
    ]));
    let mut response = send(agent, &hosts.token, &body).map_err(Refused::Failed)?;
    // The one code RFC 6749 gives for it: the xAI sign-in ends for no other
    // ([XAI-6](../../../../docs/specs/xai-sign-in.md#XAI-6)).
    if ends_the_sign_in(&mut response, &[INVALID_GRANT]) {
        return Err(Refused::Ended);
    }
    let text = success_text(&mut response).map_err(Refused::Failed)?;
    let granted = parse_tokens(text.expose()).map_err(Refused::Failed)?;
    Ok(Renewed {
        access: granted.access,
        refresh: granted.refresh,
        lifetime: granted.lifetime,
    })
}

/// Send the form and return the reply, whatever its status, naming the program in `user-agent`.
fn send(
    agent: &ureq::Agent,
    url: &str,
    body: &Secret,
) -> Result<ureq::http::Response<ureq::Body>, Class> {
    crate::oauth::send(agent, url, body, Some(USER_AGENT))
}

/// The text of a reply, or the class of its status where it is not a success.
fn success_text(response: &mut ureq::http::Response<ureq::Body>) -> Result<Secret, Class> {
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(class_of(status));
    }
    read(response)
}

/// The text of a reply, in a buffer that clears itself: the text of a good reply is a token.
fn read(response: &mut ureq::http::Response<ureq::Body>) -> Result<Secret, Class> {
    response
        .body_mut()
        .with_config()
        .limit(REPLY_LIMIT)
        .read_to_string()
        .map(Secret::new)
        .map_err(|_| Class::Incomplete)
}

/// A number of seconds a reply states: more than none, and not a fraction of one.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is finite and more than 0, and a cast of one to an integer saturates"
)]
fn seconds(value: &serde_json::Value) -> Option<u64> {
    let stated = value.as_f64()?;
    (stated.is_finite() && stated > 0.0).then(|| stated.ceil() as u64)
}

/// A field that may be absent, or null, and that is a number of seconds where it is there. Another
/// kind of value is a reply that is not the shape the exchange records.
fn optional_seconds(value: &serde_json::Value, name: &str, fallback: u64) -> Result<u64, Class> {
    match value.get(name) {
        None | Some(serde_json::Value::Null) => Ok(fallback),
        Some(stated) if stated.is_number() => Ok(seconds(stated).unwrap_or(fallback)),
        Some(_) => Err(Class::Incomplete),
    }
}

/// Whether `text` is something to show a person as written: not empty, not long, and made of
/// printable ASCII with no space, so that a reply cannot put a control sequence on a terminal.
fn shown(text: &str) -> bool {
    !text.is_empty() && text.len() <= SHOWN_LIMIT && text.bytes().all(|b| b.is_ascii_graphic())
}

/// An address to show: one that is shown as written and is `https`.
fn address(text: &str) -> bool {
    shown(text) && text.starts_with("https://")
}

/// The code of a reply, or `Incomplete` where it is not the shape the exchange records.
fn parse_code(text: &str) -> Result<Code, Class> {
    let document = Document::of(serde_json::from_str(text).map_err(|_| Class::Incomplete)?);
    let value = document.read();
    let string = |name: &str| value.get(name).and_then(serde_json::Value::as_str);

    let device_code = string("device_code")
        .filter(|code| !code.is_empty())
        .map(Secret::new)
        .ok_or(Class::Incomplete)?;
    let user_code = string("user_code")
        .filter(|code| shown(code))
        .ok_or(Class::Incomplete)?
        .to_string();
    let verification_uri = string("verification_uri")
        .filter(|uri| address(uri))
        .ok_or(Class::Incomplete)?
        .to_string();
    // Absent or null is no address with the code in it. One that is there must be one to show.
    let verification_uri_complete = match value.get("verification_uri_complete") {
        None | Some(serde_json::Value::Null) => None,
        Some(stated) => Some(
            stated
                .as_str()
                .filter(|uri| address(uri))
                .ok_or(Class::Incomplete)?
                .to_string(),
        ),
    };
    Ok(Code {
        device_code,
        user_code,
        verification_uri,
        verification_uri_complete,
        expires_in: optional_seconds(value, "expires_in", DEFAULT_CODE_LIFETIME)?
            .min(LONGEST_CODE_LIFETIME),
        interval: optional_seconds(value, "interval", DEFAULT_INTERVAL)?.clamp(1, LONGEST_INTERVAL),
    })
}

/// The tokens of a reply, or `Incomplete` where it is not the shape the exchange records.
fn parse_tokens(text: &str) -> Result<Granted, Class> {
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
    // The lifetime the reply states, between the shortest and the longest an access token is taken to
    // last, else the instant the access token states, held to the same two distances from now, else an
    // hour. A lifetime that is present must be a number. A number that is not more than none is a
    // lifetime that has run out, so it is the shortest, as a 1 is.
    let stated = match value.get("expires_in") {
        None | Some(serde_json::Value::Null) => None,
        Some(stated) if stated.is_number() => Some(seconds(stated).unwrap_or(0)),
        Some(_) => return Err(Class::Incomplete),
    };
    let lifetime = match stated {
        Some(seconds) => {
            Lifetime::For(seconds.clamp(SHORTEST_TOKEN_LIFETIME, LONGEST_TOKEN_LIFETIME))
        }
        None => {
            token_expiry(access.expose()).map_or(Lifetime::For(DEFAULT_TOKEN_LIFETIME), |instant| {
                Lifetime::UntilWithin {
                    instant,
                    shortest: SHORTEST_TOKEN_LIFETIME,
                    longest: LONGEST_TOKEN_LIFETIME,
                }
            })
        }
    };
    Ok(Granted {
        access,
        refresh,
        lifetime,
    })
}

/// The `exp` claim of an access token that is a JWT, in seconds since 1970-01-01.
///
/// Read to say when the token needs replacing and for nothing else: the signature is not checked,
/// no other claim is read and none is kept. The instant is held to the same distances from now as a
/// lifetime a reply states, by the caller, so a token that states a time that is not its own only
/// changes when this program asks for another, and not by more than a day.
fn token_expiry(access_token: &str) -> Option<u64> {
    if access_token.len() > TOKEN_LIMIT {
        return None;
    }
    let payload = access_token.split('.').nth(1)?;
    let bytes = Secret::new(String::from_utf8(base64url_decode(payload)?).ok()?);
    let claims = Document::of(serde_json::from_str(bytes.expose()).ok()?);
    let stated = claims.read().get("exp")?;
    // A number that is not more than none is an instant that has passed.
    stated.is_number().then(|| seconds(stated).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oauth::encoding::base64url;

    /// An access token that is a JWT whose payload is `claims`.
    fn jwt(claims: &str) -> String {
        format!(
            "{}.{}.signature",
            base64url(b"{\"alg\":\"none\"}"),
            base64url(claims.as_bytes())
        )
    }

    /// The reply the exchange records for a code, with `extra` after the fields it requires.
    fn code_reply(extra: &str) -> String {
        format!(
            "{{\"device_code\":\"a-device-code\",\"user_code\":\"ABCD-EFGH\",\
             \"verification_uri\":\"https://accounts.example/oauth2/device\"{extra}}}"
        )
    }

    /// A reply is read for the device code, the two things shown to a person and the two numbers,
    /// and nothing else is kept.
    #[test]
    fn a_code_reply_gives_what_is_shown_and_how_often_to_ask() {
        let code = parse_code(&code_reply(
            ",\"verification_uri_complete\":\"https://accounts.example/oauth2/device?user_code=ABCD-EFGH\",\
             \"expires_in\":1800,\"interval\":7",
        ))
        .expect("a reply");

        assert_eq!(code.device_code.expose(), "a-device-code");
        assert_eq!(code.user_code, "ABCD-EFGH");
        assert_eq!(
            code.verification_uri,
            "https://accounts.example/oauth2/device"
        );
        assert_eq!(
            code.verification_uri_complete.as_deref(),
            Some("https://accounts.example/oauth2/device?user_code=ABCD-EFGH")
        );
        assert_eq!((code.expires_in, code.interval), (1800, 7));
    }

    /// A reply that states no lifetime or no interval is a code that lasts five minutes and is asked
    /// about every five seconds, and one that states none that is more than none is the same. An
    /// interval of a fraction of a second is a second, so the endpoint is never asked in a loop with
    /// no wait.
    #[test]
    fn a_code_with_no_lifetime_or_interval_lasts_five_minutes_and_is_asked_every_five_seconds() {
        for extra in [
            "",
            ",\"expires_in\":null,\"interval\":null",
            ",\"expires_in\":0,\"interval\":0",
            ",\"expires_in\":-1,\"interval\":-1",
        ] {
            let code = parse_code(&code_reply(extra)).expect("a reply");
            assert_eq!((code.expires_in, code.interval), (300, 5), "{extra}");
            assert!(code.verification_uri_complete.is_none(), "{extra}");
        }
        let fraction = parse_code(&code_reply(",\"interval\":0.2")).expect("a reply");
        assert_eq!(fraction.interval, 1);
    }

    /// A reply that states a lifetime of more than an hour is an hour, and an interval of more than a
    /// minute is a minute, so a reply cannot hold the command for longer or ask it to wait longer
    /// between asks. The figures at the caps, and those the service stated (1800 and 5), are kept as
    /// they are.
    #[test]
    fn a_code_reply_that_states_more_than_the_caps_allow_is_held_to_them() {
        for (extra, expected) in [
            (",\"expires_in\":86400,\"interval\":3600", (3600, 60)),
            (",\"expires_in\":3601,\"interval\":61", (3600, 60)),
            (",\"expires_in\":3600,\"interval\":60", (3600, 60)),
            (",\"expires_in\":1800,\"interval\":5", (1800, 5)),
            (",\"expires_in\":1e300,\"interval\":1e300", (3600, 60)),
        ] {
            let code = parse_code(&code_reply(extra)).expect("a reply");
            assert_eq!((code.expires_in, code.interval), expected, "{extra}");
        }
    }

    /// A reply without a device code, a user code or an address, with one that is empty, that is not
    /// text, or that is not JSON is incomplete. So is a number that is some other kind of value.
    #[test]
    fn a_code_reply_that_is_not_the_recorded_shape_is_incomplete() {
        for text in [
            "",
            "not json",
            "[]",
            "{}",
            "{\"user_code\":\"A\",\"verification_uri\":\"https://x.example/d\"}",
            "{\"device_code\":\"d\",\"verification_uri\":\"https://x.example/d\"}",
            "{\"device_code\":\"d\",\"user_code\":\"A\"}",
            "{\"device_code\":\"\",\"user_code\":\"A\",\"verification_uri\":\"https://x.example/d\"}",
            "{\"device_code\":1,\"user_code\":\"A\",\"verification_uri\":\"https://x.example/d\"}",
        ] {
            assert!(matches!(parse_code(text), Err(Class::Incomplete)), "{text}");
        }
        for extra in [
            ",\"expires_in\":\"soon\"",
            ",\"interval\":[1]",
            ",\"interval\":true",
        ] {
            assert!(
                matches!(parse_code(&code_reply(extra)), Err(Class::Incomplete)),
                "{extra}"
            );
        }
    }

    /// What is shown to a person is shown as written, so a reply cannot draw on the terminal that
    /// shows it: an address that is not `https`, one with a space or a control character in it, and a
    /// code with one, are not shown and end the sign-in as a reply that cannot be read.
    #[test]
    fn what_is_shown_to_a_person_is_one_printable_word_and_an_address_is_https() {
        for uri in [
            "http://accounts.example/oauth2/device",
            "javascript:alert(1)",
            "https://accounts.example/\u{1b}[2J",
            "https://accounts.example/ device",
            "https://accounts.example/\nx",
            "https://accounts.example/\u{e9}",
            "",
        ] {
            let reply = format!(
                "{{\"device_code\":\"d\",\"user_code\":\"A\",\"verification_uri\":{}}}",
                serde_json::json!(uri)
            );
            assert!(
                matches!(parse_code(&reply), Err(Class::Incomplete)),
                "{uri}"
            );
            let complete = code_reply(&format!(
                ",\"verification_uri_complete\":{}",
                serde_json::json!(uri)
            ));
            assert!(
                matches!(parse_code(&complete), Err(Class::Incomplete)),
                "{uri}"
            );
        }
        for user_code in ["AB CD", "AB\u{1b}[2JCD", "AB\nCD", ""] {
            let reply = format!(
                "{{\"device_code\":\"d\",\"user_code\":{},\"verification_uri\":\"https://x.example/d\"}}",
                serde_json::json!(user_code)
            );
            assert!(
                matches!(parse_code(&reply), Err(Class::Incomplete)),
                "{user_code:?}"
            );
        }
        let long = "a".repeat(SHOWN_LIMIT + 1);
        let reply = format!(
            "{{\"device_code\":\"d\",\"user_code\":\"{long}\",\"verification_uri\":\"https://x.example/d\"}}"
        );
        assert!(matches!(parse_code(&reply), Err(Class::Incomplete)));
    }

    /// A reply is read for the access token, the refresh token where there is one, and the lifetime
    /// it states.
    #[test]
    fn a_token_reply_gives_the_tokens_and_the_lifetime_it_states() {
        let granted = parse_tokens(
            "{\"access_token\":\"an-access\",\"refresh_token\":\"a-refresh\",\"expires_in\":1800,\
             \"id_token\":\"not-kept\",\"scope\":\"not kept\"}",
        )
        .expect("a reply");

        assert_eq!(granted.access.expose(), "an-access");
        assert_eq!(
            granted.refresh.as_ref().expect("a refresh token").expose(),
            "a-refresh"
        );
        assert_eq!(granted.lifetime, Lifetime::For(1800));
    }

    /// A lifetime the reply states wins over the one the token states. A reply that states none, or
    /// null, is read from the `exp` claim of an access token that is a JWT, and from there an hour. An
    /// implementation that took the claim first would expire a token by a time the service did not give
    /// for the reply. A lifetime stated as 0 is one that has run out and is the shortest, and is not none.
    #[test]
    fn the_expiry_is_the_replys_lifetime_then_the_tokens_claim_then_an_hour() {
        let within = |instant| Lifetime::UntilWithin {
            instant,
            shortest: 300,
            longest: 86_400,
        };
        let with = |access: &str, extra: &str| {
            parse_tokens(&format!(
                "{{\"access_token\":{}{extra}}}",
                serde_json::json!(access)
            ))
            .expect("a reply")
            .lifetime
        };
        let dated = jwt("{\"exp\":1792703127,\"sub\":\"somebody\"}");

        assert_eq!(with(&dated, ",\"expires_in\":900"), Lifetime::For(900));
        assert_eq!(with(&dated, ""), within(1_792_703_127));
        assert_eq!(with(&dated, ",\"expires_in\":null"), within(1_792_703_127));
        assert_eq!(with(&dated, ",\"expires_in\":0"), Lifetime::For(300));
        assert_eq!(with("an-opaque-token", ""), Lifetime::For(3600));
        assert_eq!(
            with("an-opaque-token", ",\"expires_in\":0"),
            Lifetime::For(300)
        );
        assert_eq!(with(&jwt("{\"exp\":-5}"), ""), within(0));
        for undated in [
            jwt("{\"sub\":\"somebody\"}"),
            jwt("not json"),
            jwt("{\"exp\":\"tomorrow\"}"),
            "a.%%%.c".to_string(),
            "a..c".to_string(),
        ] {
            assert_eq!(with(&undated, ""), Lifetime::For(3600), "{undated}");
        }
    }

    /// XAI-3: the lifetime a reply states is taken between 300 seconds and a day, which the grant and a
    /// refresh both read. The service stated 21600, which is left as it is. The regressions it rejects are
    /// a lifetime of 18446744073709551615 that makes an access token the clock never replaces, and one of
    /// 1 that makes a refresh token spent on every ask, and a bound that moves what the service stated.
    #[test]
    fn a_lifetime_a_reply_states_is_taken_between_five_minutes_and_a_day() {
        let lifetime = |stated: &str| {
            parse_tokens(&format!(
                "{{\"access_token\":\"a-token\",\"expires_in\":{stated}}}"
            ))
            .expect("a reply")
            .lifetime
        };

        assert_eq!(lifetime("18446744073709551615"), Lifetime::For(86_400));
        assert_eq!(lifetime("86401"), Lifetime::For(86_400));
        assert_eq!(lifetime("86400"), Lifetime::For(86_400));
        assert_eq!(lifetime("21600"), Lifetime::For(21_600));
        assert_eq!(lifetime("300"), Lifetime::For(300));
        assert_eq!(lifetime("299"), Lifetime::For(300));
        assert_eq!(lifetime("1"), Lifetime::For(300));
        assert_eq!(lifetime("0.5"), Lifetime::For(300));
        assert_eq!(lifetime("0"), Lifetime::For(300));
        assert_eq!(lifetime("-5"), Lifetime::For(300));
        assert_eq!(lifetime("-0.5"), Lifetime::For(300));
    }

    /// XAI-3: where the reply states no lifetime, the instant the access token states of itself is held
    /// to the same distances from the reply, which the grant and a refresh both read. The regressions it
    /// rejects are an `exp` of 18446744073709551615, which makes an access token the clock never
    /// replaces, and an `exp` of 1 or 0, which makes a refresh token spent on every ask, and a bound that
    /// moves an instant within them.
    #[test]
    fn an_expiry_the_token_states_of_itself_is_held_to_five_minutes_and_a_day_from_the_reply() {
        let expiry = |exp: &str| {
            parse_tokens(&format!(
                "{{\"access_token\":{}}}",
                serde_json::json!(jwt(&format!("{{\"exp\":{exp}}}")))
            ))
            .expect("a reply")
            .lifetime
            .expiry(1_000_000)
        };

        assert_eq!(expiry("18446744073709551615"), 1_086_400);
        assert_eq!(expiry("1e30"), 1_086_400);
        assert_eq!(expiry("1086401"), 1_086_400);
        assert_eq!(expiry("1086400"), 1_086_400);
        assert_eq!(expiry("1021600"), 1_021_600);
        assert_eq!(expiry("1000300"), 1_000_300);
        assert_eq!(expiry("1000299"), 1_000_300);
        assert_eq!(expiry("1"), 1_000_300);
        assert_eq!(expiry("0"), 1_000_300);
        assert_eq!(expiry("-5"), 1_000_300);
    }

    /// A lifetime that is not a number is a reply that cannot be read, since guessing would store an
    /// expiry nobody stated.
    #[test]
    fn a_lifetime_that_is_not_a_number_is_refused() {
        for stated in ["\"soon\"", "[1]", "true", "{}"] {
            assert!(
                matches!(
                    parse_tokens(&format!(
                        "{{\"access_token\":\"a\",\"expires_in\":{stated}}}"
                    )),
                    Err(Class::Incomplete)
                ),
                "{stated}"
            );
        }
    }

    /// A reply without an access token, with an empty one, or that is not JSON is incomplete, and
    /// nothing of it is in the answer. A refresh token that is there and is empty or not text is
    /// refused rather than read as absent.
    #[test]
    fn a_token_reply_that_is_not_the_recorded_shape_is_incomplete() {
        for text in [
            "",
            "not json",
            "[]",
            "{}",
            "{\"refresh_token\":\"r\"}",
            "{\"access_token\":\"\",\"refresh_token\":\"r\"}",
            "{\"access_token\":1,\"refresh_token\":\"r\"}",
            "{\"access_token\":\"a\",\"refresh_token\":\"\"}",
            "{\"access_token\":\"a\",\"refresh_token\":1}",
        ] {
            assert!(
                matches!(parse_tokens(text), Err(Class::Incomplete)),
                "{text}"
            );
        }
    }

    /// A reply that holds no refresh token keeps the one presented, which is how a server that does
    /// not replace it answers. A `null` is the same as absent.
    #[test]
    fn a_reply_without_a_refresh_token_names_none() {
        for text in [
            "{\"access_token\":\"a\"}",
            "{\"access_token\":\"a\",\"refresh_token\":null}",
        ] {
            assert!(parse_tokens(text).expect("a reply").refresh.is_none());
        }
    }
}
