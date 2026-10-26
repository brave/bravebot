//! The requests to a token endpoint, and what is read of the reply of a refused one.
//!
//! The reply is bytes this program did not write. What a sign-in takes from it is two tokens that are
//! never read, a number of seconds and, for a refused refresh, the one error code that says the
//! refresh token is no longer valid, which is compared with that code and nothing else. The rest of
//! the body, including every error text, is dropped in this module.

use super::Class;
use crate::Secret;
use crate::secret::Document;

/// How long a reply may be before it is refused, so a body somebody sent cannot fill memory.
pub(crate) const REPLY_LIMIT: u64 = 1024 * 1024;

/// The error code that says a refresh token is no longer valid: RFC 6749 section 5.2's
/// `invalid_grant`, sent as a string in `{"error":"invalid_grant"}`. A refresh refused with any other
/// code is a failure of the request and ends nothing.
const ENDED: &str = "invalid_grant";

/// The statuses a refused refresh is read for its error code at. RFC 6749 gives 400, and a server
/// answering a token as unauthorized gives 401.
const ENDED_STATUSES: [u16; 2] = [400, 401];

/// How long an access token lasts, as a refresh reply or a sign-in states it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifetime {
    /// This many seconds from the moment of the reply.
    For(u64),
    /// Until this instant, in seconds since 1970-01-01, which the access token states of itself, held to
    /// no sooner than `shortest` seconds from the moment of the reply and no later than `longest`. The
    /// instant is a claim of bytes this program did not write, so one in the past would have the token
    /// replaced on every ask and one that never comes would have it never replaced.
    UntilWithin {
        instant: u64,
        shortest: u64,
        longest: u64,
    },
}

impl Lifetime {
    /// The instant the access token expires, for a reply that arrived at `now`.
    pub fn expiry(self, now: u64) -> u64 {
        match self {
            Self::For(seconds) => now.saturating_add(seconds),
            Self::UntilWithin {
                instant,
                shortest,
                longest,
            } => instant
                .max(now.saturating_add(shortest))
                .min(now.saturating_add(longest)),
        }
    }
}

/// What a refresh gave.
///
/// No equality, which the tokens do not have: see [`Secret`].
pub struct Renewed {
    pub access: Secret,
    /// A new refresh token, where the server replaced the one presented. `None` keeps it.
    pub refresh: Option<Secret>,
    pub lifetime: Lifetime,
}

/// Why a refresh did not give tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
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
///
/// It keeps no connection for a later request, as a hardening whose cause was not isolated. A sign-in
/// asks every eight seconds for up to an hour, so up to about 450 new connections, which the endpoint
/// can take. A connection kept between two asks is one the server or something in front of it may
/// have closed while the sign-in waited for a person to approve a code, which the next ask would
/// meet as a failure of the transport. The library probes a pooled connection before it reuses one,
/// so what is guarded against is a drop that the probe does not see.
pub(crate) fn agent(
    transport: ureq::config::ConfigBuilder<ureq::typestate::AgentScope>,
) -> ureq::Agent {
    transport
        .max_redirects(0)
        .max_idle_connections(0)
        .max_idle_age(std::time::Duration::ZERO)
        .http_status_as_error(false)
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .build()
        .into()
}

/// Whether a refused refresh is one the server says ends the sign-in: a status of 400 or 401 whose
/// body names the one error code that does.
///
/// A body that cannot be read or is not the shape is not that code: nothing is concluded from it.
pub(crate) fn ends_the_sign_in(response: &mut ureq::http::Response<ureq::Body>) -> bool {
    ENDED_STATUSES.contains(&response.status().as_u16()) && says_ended(response)
}

/// Whether a refused refresh's body names the one error code that ends a sign-in.
fn says_ended(response: &mut ureq::http::Response<ureq::Body>) -> bool {
    error_code(response).as_deref() == Some(ENDED)
}

/// The `error` of a refused reply's body, where the body is JSON that holds a string under it.
///
/// The one field of an error body that is read, and only to be compared with a closed list of codes
/// by the caller: the rest of the body, the error text included, is dropped here. A body that cannot
/// be read or is not the shape has no code.
pub(crate) fn error_code(response: &mut ureq::http::Response<ureq::Body>) -> Option<String> {
    let text = Secret::new(
        response
            .body_mut()
            .with_config()
            .limit(REPLY_LIMIT)
            .read_to_string()
            .ok()?,
    );
    let document = Document::of(serde_json::from_str(text.expose()).ok()?);
    document
        .read()
        .get("error")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

/// A form body, each name and value percent-encoded.
pub(crate) fn form(fields: &[(&str, &str)]) -> String {
    fields
        .iter()
        .map(|(name, value)| format!("{name}={}", super::encoding::percent_encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

/// Send the form and return the reply, whatever its status: the caller decides what a status
/// means, since a refresh reads a refusal's code and the exchange does not. `user_agent` is what the
/// request names the program as, where the sign-in's authorization server is told.
pub(crate) fn send(
    agent: &ureq::Agent,
    url: &str,
    body: &Secret,
    user_agent: Option<&str>,
) -> Result<ureq::http::Response<ureq::Body>, Class> {
    let mut request = agent
        .post(url)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json");
    if let Some(user_agent) = user_agent {
        request = request.header("user-agent", user_agent);
    }
    request
        .send(body.expose().as_bytes())
        .map_err(|_| Class::Transport)
}

/// The class of a status that is not a success.
///
/// A redirect is `Refused`, since none is followed.
pub(crate) fn class_of(status: u16) -> Class {
    match status {
        429 => Class::RateLimited,
        500..=599 => Class::Unavailable,
        _ => Class::Refused,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// Two requests made through one agent are made on two connections, so a connection the server has
    /// closed since the first reply, which a server that offered to keep it does when it has idled, is not
    /// the one the second meets. The stand-in offers the connection for reuse and hangs up on a request
    /// that comes on it, as a closed connection the client has not noticed does, and counts the
    /// connections it accepts. Five real runs of `auth login xai` ended with "xAI could not be reached"
    /// and nothing stored, none of them approved, after times that included about 18 seconds, 80
    /// seconds, 3.5 minutes and 5 minutes, and the sockets of one run showed a connection used for
    /// several polls. The cause was not isolated, and the library probes a pooled connection before it
    /// reuses one, so this stand-in models a drop the probe does not see or a race with it. The
    /// regression it rejects is a pooled connection, which would fail the second request.
    #[test]
    fn two_requests_through_one_agent_do_not_share_a_connection_the_server_may_have_closed() {
        use crate::oauth::stand_in::{Reply, serve};
        let server = serve(vec![
            Reply::json(200, "{\"first\":1}").offering_reuse_then_hanging_up(),
            Reply::json(200, "{\"second\":2}"),
        ]);
        let url = format!("{}/token", server.base);
        let agent = agent(ureq::Agent::config_builder());
        let body = Secret::new("a=b");

        // The first reply is read to its end, so its connection is the one a later request could reuse.
        let first = send(&agent, &url, &body, None).map(|mut reply| {
            reply
                .body_mut()
                .read_to_string()
                .expect("the first reply is read")
        });
        let second = send(&agent, &url, &body, None);

        assert!(first.is_ok(), "{:?}", first.err());
        assert!(
            second.is_ok(),
            "the second request met the connection the first left: {:?}",
            second.err()
        );
        assert_eq!(server.accepted(), 2, "the two requests shared a connection");
        assert_eq!(server.finish().len(), 2);
    }

    /// Every value is percent-encoded, so one with a `&` or an `=` in it cannot add a field or end
    /// one early.
    #[test]
    fn a_form_encodes_each_value() {
        assert_eq!(form(&[("a", "x&b=y"), ("c", "d e")]), "a=x%26b%3Dy&c=d%20e");
    }

    /// An expiry is the reply's instant plus the seconds it states, or the instant a token states held
    /// to between its shortest and longest distance from the reply, and a sum that would pass the end of
    /// the counter stops at it. The regressions it rejects are an instant in the past, which would have a
    /// token replaced on every ask, and one that is never reached, which would have it never replaced.
    #[test]
    fn a_lifetime_is_seconds_from_the_reply_or_an_instant_held_between_two_distances() {
        let within = |instant| Lifetime::UntilWithin {
            instant,
            shortest: 300,
            longest: 86_400,
        };
        assert_eq!(Lifetime::For(1800).expiry(1000), 2800);
        assert_eq!(Lifetime::For(u64::MAX).expiry(1000), u64::MAX);
        assert_eq!(within(5000).expiry(1000), 5000);
        assert_eq!(within(1300).expiry(1000), 1300);
        assert_eq!(within(1299).expiry(1000), 1300);
        assert_eq!(within(0).expiry(1000), 1300);
        assert_eq!(within(87_400).expiry(1000), 87_400);
        assert_eq!(within(87_401).expiry(1000), 87_400);
        assert_eq!(within(u64::MAX).expiry(1000), 87_400);
        assert_eq!(within(u64::MAX).expiry(u64::MAX), u64::MAX);
    }
}
