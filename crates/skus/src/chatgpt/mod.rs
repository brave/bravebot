//! Signing in to OpenAI with a ChatGPT account, in a browser
//! ([chatgpt-sign-in.md](../../../../docs/specs/chatgpt-sign-in.md)).
//!
//! # Why this crate has no labels
//!
//! The reason is the one [the crate](crate) gives for the Leo import, and it is checked for this
//! flow in [CHATGPT-5](../../../../docs/specs/chatgpt-sign-in.md#CHATGPT-5). A person types
//! `bravebot auth login chatgpt` at a shell, so no planner exists and no model has a context. The
//! bytes read from outside are a callback from a browser and a reply from the authorization
//! server. They are compared and parsed here. What leaves this module is a token that is never
//! read, a number of seconds, and a value from the closed set of [`Class`] and [`SignInError`],
//! each chosen from a step and the class of a status. No string taken from a reply is part of
//! either.
//!
//! # What is here and what is not
//!
//! The sign-in, the file it is stored in, and the forgetting of it. The refresh of an expired
//! access token, the models the account offers and the requests to the serving host wait for the
//! Responses protocol and are not here. The ID token is not verified: that needs a signature
//! verifier this crate does not hold, and the reply's `id_token` is dropped unread.
//!
//! The listener binds `127.0.0.1` only, on a port the operating system chooses. The hosts are
//! constants ([`Hosts::compiled_in`]), and a test points the flow at a server of its own through a
//! constructor that exists only in test builds.

mod account;
mod authorize;
mod encoding;
mod exchange;
mod listener;
#[cfg(test)]
mod stand_in;

pub use account::{Account, AccountError, Defect, path};
pub use listener::Pages;

use std::path::Path;
use std::time::Duration;

/// The destinations of the sign-in. Compiled in: nothing a page, a reply, a model, a settings file
/// or an environment variable says names one
/// ([CHATGPT-2](../../../../docs/specs/chatgpt-sign-in.md#CHATGPT-2)).
pub struct Hosts {
    authorize: String,
    token: String,
}

impl Hosts {
    /// The authorization page the browser is opened at and the token endpoint that takes the code.
    pub fn compiled_in() -> Self {
        Self {
            authorize: "https://auth.openai.com/api/accounts/authorize".to_string(),
            token: "https://auth.openai.com/api/accounts/oauth/token".to_string(),
        }
    }

    /// Both destinations under `base`, a server a test runs. Not in a release build, so no flag, no
    /// variable and no file can send a token anywhere but the hosts above.
    #[cfg(test)]
    pub(crate) fn at(base: &str) -> Self {
        Self {
            authorize: format!("{base}/authorize"),
            token: format!("{base}/token"),
        }
    }
}

/// How long the sign-in waits.
pub struct Limits {
    /// For the browser to come back, after which the sign-in stops.
    pub callback: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            callback: Duration::from_secs(5 * 60),
        }
    }
}

/// What the person is told before the sign-in waits for the browser
/// ([CHATGPT-13](../../../../docs/specs/chatgpt-sign-in.md#CHATGPT-13)).
pub struct Announcement<'a> {
    /// The authorization URL. It holds the `state`, the `nonce`, the challenge and the identifiers,
    /// and it is shown once and goes nowhere else.
    pub url: &'a str,
    /// The loopback port the listener is bound to, which a browser on another machine has to be
    /// able to reach.
    pub port: u16,
}

/// How a request to the authorization server failed, in the categories the backends use for any
/// other service's failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// The server refused the request, or answered it with a redirect.
    Refused,
    /// The server is limiting requests.
    RateLimited,
    /// The server is busy or unavailable.
    Unavailable,
    /// The reply was not the shape the exchange records.
    Incomplete,
    /// The request did not complete.
    Transport,
}

/// A short name for the class, for a log or a test.
impl std::fmt::Display for Class {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Refused => "refused",
            Self::RateLimited => "rate-limited",
            Self::Unavailable => "unavailable",
            Self::Incomplete => "incomplete",
            Self::Transport => "transport",
        })
    }
}

/// Why the sign-in stopped.
///
/// Each variant is the step that failed and, for a failed request, the class of its status. None
/// holds text taken from a reply, a request, a callback or a settings file, so the sentence the
/// caller chooses for it quotes nothing
/// ([CHATGPT-8](../../../../docs/specs/chatgpt-sign-in.md#CHATGPT-8)). The words are the caller's.
#[derive(Debug)]
pub enum SignInError {
    /// The listener could not start. No browser was opened.
    Listener,
    /// The browser did not come back within the time allowed.
    TimedOut,
    /// The callback carried no authorization code.
    NoCode,
    /// A first sign-in's callback carried no usable client.
    NoClient,
    /// The callback's client is not the one stored.
    ClientChanged,
    /// The token request failed.
    Exchange(Class),
    /// The reply did not grant the scope that allows this program to use the subscription.
    NoUseScope,
    /// The sign-in could not be stored.
    Store(AccountError),
}

/// A short name for the step, for a log or a test.
impl std::fmt::Display for SignInError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Listener => f.write_str("listener"),
            Self::TimedOut => f.write_str("timed out"),
            Self::NoCode => f.write_str("no code"),
            Self::NoClient => f.write_str("no client"),
            Self::ClientChanged => f.write_str("client changed"),
            Self::Exchange(class) => write!(f, "exchange {class}"),
            Self::NoUseScope => f.write_str("scope not granted"),
            Self::Store(failure) => write!(f, "store {failure}"),
        }
    }
}

impl std::error::Error for SignInError {}

/// A completed sign-in. It holds nothing secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedIn {
    /// When the access token expires, as `2026-10-22T21:05:27Z`.
    pub expires: String,
    /// Where the sign-in was stored.
    pub stored_in: std::path::PathBuf,
}

/// Sign in, and store the account where [`path`] says.
///
/// `announce` is called once, with the authorization URL and the port, after the listener is bound
/// and before anything waits. It shows the URL and asks for the browser, and the sign-in carries on
/// whether or not a browser opened. `transport` is what the caller has decided its clients trust and
/// go through, half-built as for [`crate::device::register`]. `pages` is the words the browser is
/// shown when it returns.
pub fn sign_in(
    hosts: &Hosts,
    transport: ureq::config::ConfigBuilder<ureq::typestate::AgentScope>,
    limits: &Limits,
    pages: &Pages<'_>,
    announce: impl FnOnce(&Announcement<'_>),
) -> Result<SignedIn, SignInError> {
    let destination = path().map_err(SignInError::Store)?;
    run(hosts, transport, limits, pages, &destination, announce)
}

fn run(
    hosts: &Hosts,
    transport: ureq::config::ConfigBuilder<ureq::typestate::AgentScope>,
    limits: &Limits,
    pages: &Pages<'_>,
    destination: &Path,
    announce: impl FnOnce(&Announcement<'_>),
) -> Result<SignedIn, SignInError> {
    // A file that cannot be read is signed in over, so the remedy the command names for it works.
    let stored = account::load_at(destination).ok().flatten();
    let request = authorize::Request::new(
        stored
            .as_ref()
            .map(|stored| (stored.client_id.as_str(), stored.host_id.as_str())),
    );

    let listener = listener::Listener::bind().map_err(|_| SignInError::Listener)?;
    let port = listener.port().map_err(|_| SignInError::Listener)?;
    let redirect_uri = format!("http://127.0.0.1:{port}/auth/callback");
    announce(&Announcement {
        url: &request.url(hosts, &redirect_uri),
        port,
    });

    let callback = listener
        .wait(request.state(), limits.callback, pages)
        .map_err(|listener::Waited::TimedOut| SignInError::TimedOut)?;
    let code = callback.code.ok_or(SignInError::NoCode)?;
    // The client OpenAI issued is used from a first sign-in's callback and from the file after it.
    let client = match (request.client(), callback.client_id) {
        (Some(stored), Some(sent)) if stored != sent => return Err(SignInError::ClientChanged),
        (Some(stored), _) => stored.to_string(),
        (None, Some(sent)) => sent,
        (None, None) => return Err(SignInError::NoClient),
    };

    let agent = exchange::agent(transport);
    let tokens = exchange::exchange(
        &agent,
        hosts,
        &client,
        &code,
        request.verifier(),
        &redirect_uri,
    )
    .map_err(SignInError::Exchange)?;
    if !tokens.allows_use {
        return Err(SignInError::NoUseScope);
    }

    let signed_in = Account {
        access_token: tokens.access,
        refresh_token: tokens.refresh,
        expires_at: encoding::now().saturating_add(tokens.lifetime),
        client_id: client,
        host_id: request.host_id().to_string(),
    };
    account::save_at(destination, &signed_in).map_err(SignInError::Store)?;
    Ok(SignedIn {
        expires: signed_in.expires(),
        stored_in: destination.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::encoding::{base64url, utc};
    use super::stand_in::{Reply, named, parameters, serve, visit};
    use super::*;
    use sha2::{Digest, Sha256};

    const TOKENS: &str = "{\"access_token\":\"an-access\",\"refresh_token\":\"a-refresh\",\
        \"id_token\":\"an-id\",\"expires_in\":1800,\
        \"scope\":\"openid profile email offline_access resource.invoke chatgpt.tokens.use.direct\"}";

    fn transport() -> ureq::config::ConfigBuilder<ureq::typestate::AgentScope> {
        ureq::Agent::config_builder()
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let directory = crate::testutil::scratch_dir(&format!("bravebot-chatgpt-flow-{name}"));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a scratch directory");
        directory.join("chatgpt-account.json")
    }

    const PAGES: Pages<'static> = Pages {
        done: "Return to the program.",
        not_found: "Nothing here.",
    };

    fn brief() -> Limits {
        Limits {
            callback: Duration::from_secs(30),
        }
    }

    /// What the browser returns to, given the URL it was opened at.
    enum Returns<'a> {
        /// The callback the server would send, with this query after the `state`.
        With(&'a str),
        /// Nothing: the person never finishes.
        Nothing,
    }

    /// CHATGPT-4: the browser has five minutes to come back, which is what the command waits for
    /// unless a test states a shorter time.
    #[test]
    fn the_browser_has_five_minutes_to_return_by_default() {
        assert_eq!(Limits::default().callback, Duration::from_secs(300));
    }

    /// What a sign-in said, what it announced and what the token server was asked.
    type Outcome = (
        Result<SignedIn, SignInError>,
        Option<(String, u16)>,
        Vec<stand_in::Received>,
    );

    /// Run the sign-in with a browser that behaves as `returns` says, against a token server that
    /// gives `replies`, and return what the sign-in said, the URL it announced and what the token
    /// server was asked.
    fn signed_in(
        path: &std::path::Path,
        replies: Vec<Reply>,
        returns: Returns<'_>,
        limits: &Limits,
    ) -> Outcome {
        let server = serve(replies);
        let mut announced = None;
        let result = run(
            &Hosts::at(&server.base),
            transport(),
            limits,
            &PAGES,
            path,
            |announcement| {
                announced = Some((announcement.url.to_string(), announcement.port));
                if let Returns::With(query) = returns {
                    let state = named(&parameters(announcement.url), "state").to_string();
                    let target = format!("/auth/callback?state={state}&{query}");
                    let port = announcement.port;
                    std::thread::spawn(move || visit(port, &target));
                }
            },
        );
        (result, announced, server.finish())
    }

    /// A first sign-in, from the first request to the file: the person is sent to the
    /// authorization page, the browser returns with a code and the client OpenAI issued, the code
    /// is exchanged under that client with the verifier whose hash the URL carried, and the tokens,
    /// the expiry, the client and the host identifier are stored.
    #[test]
    fn a_first_sign_in_exchanges_the_code_under_the_issued_client_and_stores_the_account() {
        let path = scratch("first");

        let (result, announced, asked) = signed_in(
            &path,
            vec![Reply::json(200, TOKENS)],
            Returns::With("code=the-code&client_id=issued-client"),
            &brief(),
        );

        let signed = result.expect("a sign-in");
        let (url, port) = announced.expect("an announcement");
        let found = parameters(&url);

        assert_eq!(asked.len(), 1, "{asked:?}");
        let request = &asked[0];
        assert_eq!(
            (request.method.as_str(), request.path.as_str()),
            ("POST", "/token")
        );
        assert_eq!(
            request.header("content-type"),
            Some("application/x-www-form-urlencoded")
        );
        assert_eq!(request.header("authorization"), None);
        let form = request.form();
        let names: Vec<&str> = form.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            [
                "grant_type",
                "client_id",
                "code",
                "code_verifier",
                "redirect_uri",
                "resource"
            ]
        );
        assert_eq!(named(&form, "grant_type"), "authorization_code");
        assert_eq!(named(&form, "client_id"), "issued-client");
        assert_eq!(named(&form, "code"), "the-code");
        assert_eq!(
            named(&form, "redirect_uri"),
            format!("http://127.0.0.1:{port}/auth/callback")
        );
        assert_eq!(named(&form, "resource"), "https://api.openai.com/v1");
        // The verifier the exchange presents is the one the URL's challenge was made from.
        assert_eq!(
            base64url(&Sha256::digest(named(&form, "code_verifier").as_bytes())),
            named(&found, "code_challenge")
        );

        let stored = account::load_at(&path)
            .expect("a read")
            .expect("an account");
        assert_eq!(stored.access_token.expose(), "an-access");
        assert_eq!(stored.refresh_token.expose(), "a-refresh");
        assert_eq!(stored.client_id, "issued-client");
        assert_eq!(
            stored.host_id,
            named(&found, "ext_agent_host_id"),
            "the host identifier stored is not the one the URL named"
        );
        let now = encoding::now();
        assert!(
            (now + 1790..=now + 1810).contains(&stored.expires_at),
            "{} against {now}",
            stored.expires_at
        );
        assert_eq!(signed.expires, utc(stored.expires_at));
    }

    /// A sign-in over a stored one asks under the client OpenAI issued and names the host it named,
    /// and its callback needs no client of its own. The tokens are replaced and the client and host
    /// are the same.
    #[test]
    fn a_sign_in_over_a_stored_one_keeps_its_client_and_host_and_replaces_the_tokens() {
        let path = scratch("over");
        let before = Account {
            access_token: crate::Secret::new("old-access"),
            refresh_token: crate::Secret::new("old-refresh"),
            expires_at: 1,
            client_id: "issued-client".to_string(),
            host_id: "urn:uuid:stored-host".to_string(),
        };
        account::save_at(&path, &before).expect("a stored account");

        let (result, announced, asked) = signed_in(
            &path,
            vec![Reply::json(200, TOKENS)],
            Returns::With("code=the-code"),
            &brief(),
        );

        result.expect("a sign-in");
        let found = parameters(&announced.expect("an announcement").0);
        assert_eq!(named(&found, "client_id"), "issued-client");
        assert_eq!(named(&found, "ext_agent_host_id"), "urn:uuid:stored-host");
        assert!(found.iter().all(|(name, _)| name != "agent_name_hint"));
        assert_eq!(named(&asked[0].form(), "client_id"), "issued-client");
        let stored = account::load_at(&path)
            .expect("a read")
            .expect("an account");
        assert_eq!(stored.access_token.expose(), "an-access");
        assert_eq!(stored.refresh_token.expose(), "a-refresh");
        assert_eq!(stored.client_id, "issued-client");
        assert_eq!(stored.host_id, "urn:uuid:stored-host");
    }

    /// A callback that names another client than the stored one ends the sign-in before anything is
    /// sent to the token endpoint and leaves the stored account as it was. The code would otherwise
    /// be presented under a client it was not issued to.
    #[test]
    fn a_callback_naming_another_client_than_the_stored_one_ends_the_sign_in_unexchanged() {
        let path = scratch("changed");
        let before = Account {
            access_token: crate::Secret::new("old-access"),
            refresh_token: crate::Secret::new("old-refresh"),
            expires_at: 1,
            client_id: "issued-client".to_string(),
            host_id: "urn:uuid:stored-host".to_string(),
        };
        account::save_at(&path, &before).expect("a stored account");

        let (result, _, asked) = signed_in(
            &path,
            vec![],
            Returns::With("code=c&client_id=another-client"),
            &brief(),
        );

        assert!(
            matches!(result, Err(SignInError::ClientChanged)),
            "{result:?}"
        );
        assert!(asked.is_empty(), "{asked:?}");
        let stored = account::load_at(&path)
            .expect("a read")
            .expect("an account");
        assert_eq!(stored.access_token.expose(), "old-access");
    }

    /// A first sign-in whose callback has no client of the shape one has ends it before the
    /// exchange, and so does a callback with no code, whatever else it carries.
    #[test]
    fn a_callback_without_a_code_or_without_a_usable_first_client_stops_before_the_exchange() {
        for (query, no_client) in [
            ("code=c", true),
            ("code=c&client_id=not%20a%20client", true),
            ("client_id=issued-client", false),
            ("error=access_denied", false),
            ("code=&client_id=issued-client", false),
        ] {
            let path = scratch("stops");

            let (result, _, asked) = signed_in(&path, vec![], Returns::With(query), &brief());

            let failure = result.expect_err(query);
            assert_eq!(
                matches!(failure, SignInError::NoClient),
                no_client,
                "{query}: {failure:?}"
            );
            assert!(
                matches!(failure, SignInError::NoClient | SignInError::NoCode),
                "{query}: {failure:?}"
            );
            assert!(asked.is_empty(), "{query}: {asked:?}");
            assert!(!path.exists(), "{query}: an account was stored");
        }
    }

    /// A reply that does not grant the scope that allows the use is a failed sign-in, and nothing is
    /// stored: the tokens would be good for nothing this program does, and a file holding them would
    /// make `auth status` say signed in.
    #[test]
    fn a_reply_that_does_not_allow_the_use_stores_nothing() {
        let path = scratch("scope");
        let without = "{\"access_token\":\"a\",\"refresh_token\":\"r\",\
                       \"scope\":\"openid profile email offline_access\"}";

        let (result, _, asked) = signed_in(
            &path,
            vec![Reply::json(200, without)],
            Returns::With("code=c&client_id=issued-client"),
            &brief(),
        );

        assert!(matches!(result, Err(SignInError::NoUseScope)), "{result:?}");
        assert_eq!(asked.len(), 1);
        assert!(!path.exists(), "an account was stored");
    }

    /// A failed exchange stores nothing, is reported in the category of the status, and quotes
    /// nothing the server said: its error text is a value it chose, and a log of it would carry the
    /// value for as long as the log.
    #[test]
    fn a_failed_exchange_is_classed_by_its_status_and_quotes_nothing() {
        for (status, class) in [
            (400, Class::Refused),
            (403, Class::Refused),
            (429, Class::RateLimited),
            (500, Class::Unavailable),
            (503, Class::Unavailable),
        ] {
            let path = scratch("failed");
            let body = "{\"error\":\"invalid_grant\",\"error_description\":\"a-secret-detail\"}";

            let (result, _, _) = signed_in(
                &path,
                vec![Reply::json(status, body)],
                Returns::With("code=the-code&client_id=issued-client"),
                &brief(),
            );

            let failure = result.expect_err("a failure");
            assert!(
                matches!(failure, SignInError::Exchange(found) if found == class),
                "{status}: {failure:?}"
            );
            for said in [failure.to_string(), format!("{failure:?}")] {
                for secret in ["a-secret-detail", "invalid_grant", "the-code"] {
                    assert!(!said.contains(secret), "{status}: {said}");
                }
            }
            assert!(!path.exists(), "{status}: an account was stored");
        }
    }

    /// A redirect in answer to the exchange is a failure and is not followed. The request carries
    /// the code and the verifier, and a followed redirect would send both to a host nobody compiled
    /// in.
    #[test]
    fn a_redirect_in_answer_to_the_exchange_is_not_followed() {
        let path = scratch("redirect");
        let elsewhere = serve(vec![Reply::json(200, TOKENS)]);

        let (result, _, asked) = signed_in(
            &path,
            vec![Reply::redirect(&format!("{}/token", elsewhere.base))],
            Returns::With("code=the-code&client_id=issued-client"),
            &brief(),
        );

        let reached = elsewhere.finish();
        assert!(reached.is_empty(), "the redirect was followed: {reached:?}");
        assert_eq!(asked.len(), 1);
        assert!(
            matches!(result, Err(SignInError::Exchange(Class::Refused))),
            "{result:?}"
        );
        assert!(!path.exists());
    }

    /// A server that cannot be reached is a transport failure and a reply that is not the recorded
    /// shape is incomplete, and neither stores anything.
    #[test]
    fn an_unreachable_server_and_a_reply_of_another_shape_are_reported_as_such() {
        let path = scratch("shapes");

        let (result, _, _) = signed_in(
            &path,
            vec![Reply::json(200, "{\"unexpected\":true}")],
            Returns::With("code=c&client_id=issued-client"),
            &brief(),
        );
        assert!(
            matches!(result, Err(SignInError::Exchange(Class::Incomplete))),
            "{result:?}"
        );

        // A port nothing listens on: the stand-in is bound and then dropped.
        let gone = serve(vec![]);
        let base = gone.base.clone();
        gone.finish();
        let result = run(
            &Hosts::at(&base),
            transport(),
            &brief(),
            &PAGES,
            &path,
            |announcement| {
                let state = named(&parameters(announcement.url), "state").to_string();
                let port = announcement.port;
                std::thread::spawn(move || {
                    visit(
                        port,
                        &format!("/auth/callback?state={state}&code=c&client_id=issued"),
                    )
                });
            },
        );
        assert!(
            matches!(result, Err(SignInError::Exchange(Class::Transport))),
            "{result:?}"
        );
        assert!(!path.exists());
    }

    /// A browser that never comes back ends the sign-in at the bound, and nothing is stored or sent. The listener has been announced first,
    /// so the person was told where to go.
    #[test]
    fn a_browser_that_never_returns_ends_the_sign_in_at_the_bound() {
        let path = scratch("timeout");

        let (result, announced, asked) = signed_in(
            &path,
            vec![],
            Returns::Nothing,
            &Limits {
                callback: Duration::from_millis(300),
            },
        );

        assert!(announced.is_some(), "the person was not told where to go");
        assert!(matches!(result, Err(SignInError::TimedOut)), "{result:?}");
        assert!(asked.is_empty());
        assert!(!path.exists());
    }

    /// The URL is announced after the listener is bound and before the wait, with the port the
    /// redirect names: the person has the port to forward and the browser has somewhere to
    /// return to when the announcement is made.
    #[test]
    fn the_announcement_names_the_port_the_listener_is_already_bound_to() {
        let path = scratch("announce");
        let (_, announced, _) = signed_in(
            &path,
            vec![],
            Returns::Nothing,
            &Limits {
                callback: Duration::from_millis(100),
            },
        );

        let (url, port) = announced.expect("an announcement");
        assert!(url.contains("/authorize?"), "{url}");
        assert_eq!(
            named(&parameters(&url), "redirect_uri"),
            format!("http://127.0.0.1:{port}/auth/callback")
        );
    }

    /// A file that cannot be read is signed in over: the remedy the command names for it is to sign
    /// in again, and a sign-in that refused to start over it would make that remedy circular.
    #[test]
    fn a_stored_file_that_cannot_be_read_is_signed_in_over() {
        let path = scratch("garbled");
        std::fs::write(&path, "{ not json").expect("a file");

        let (result, announced, _) = signed_in(
            &path,
            vec![Reply::json(200, TOKENS)],
            Returns::With("code=c&client_id=issued-client"),
            &brief(),
        );

        result.expect("a sign-in");
        let found = parameters(&announced.expect("an announcement").0);
        assert_eq!(named(&found, "client_id"), "dynamic_agent_client");
        assert!(account::load_at(&path).expect("a read").is_some());
    }

    /// No sentence of an error holds a value any step handled: the state, the code, a token or the
    /// path of a request. Each is fixed for the step and its class.
    #[test]
    fn no_error_of_a_sign_in_holds_a_value_the_flow_handled() {
        for failure in [
            SignInError::Listener,
            SignInError::TimedOut,
            SignInError::NoCode,
            SignInError::NoClient,
            SignInError::ClientChanged,
            SignInError::NoUseScope,
            SignInError::Exchange(Class::Refused),
            SignInError::Exchange(Class::RateLimited),
            SignInError::Exchange(Class::Unavailable),
            SignInError::Exchange(Class::Incomplete),
            SignInError::Exchange(Class::Transport),
        ] {
            for said in [failure.to_string(), format!("{failure:?}")] {
                assert!(!said.contains("http"), "{said}");
                assert!(!said.contains("access_token"), "{said}");
                assert!(said.len() < 60, "{said}");
            }
        }
    }
}
