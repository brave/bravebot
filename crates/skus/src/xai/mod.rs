//! Signing in to xAI with a SuperGrok subscription, through a device code
//! ([xai-sign-in.md](../../../../docs/specs/xai-sign-in.md)).
//!
//! # Why this crate has no labels
//!
//! The reason is the one [the crate](crate) gives for the Leo import, and it is checked for this flow
//! in [XAI-4](../../../../docs/specs/xai-sign-in.md#XAI-4). A person types `bravebot auth login xai`
//! at a shell, so no planner exists and no model has a context. The bytes read from outside are the
//! replies of the authorization server. They are compared and parsed here. What leaves this module is
//! a code and an address to show the person, tokens that are never read, a number of seconds, and a
//! value from the closed set of [`Class`] and [`SignInError`], each chosen from a step and the class
//! of a status. No string taken from a reply is part of either.
//!
//! # What is here and what is not
//!
//! The sign-in, the file it is stored in, the forgetting of it, and the replacing of an access token
//! that is about to expire ([`Renewer`]). Nothing here sends a request that carries an access token.
//!
//! The hosts are constants ([`Hosts::compiled_in`]), and a test points the flow at a server of its own
//! through a constructor that exists only in test builds and, in a build with the `test-hosts`
//! feature, through [`XAI_STAND_IN`]. A release build refuses that feature.

mod account;
mod exchange;
mod renew;

pub use crate::oauth::{Access, AccountError, Class, Defect, TokenError};
pub use account::{Account, Xai, path};
pub use renew::Renewer;

use crate::oauth::encoding;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The client the sign-in presents. It is the `client_id` that xAI's open-source Grok Build sets in
/// its default OAuth2 provider configuration and that opencode presents, and the maintainer stated
/// that bravebot has permission to present it ([XAI-2](../../../../docs/specs/xai-sign-in.md#XAI-2)).
const CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";

/// What the sign-in asks to be allowed.
const SCOPE: &str = "openid profile email offline_access grok-cli:access api:access";

/// The name the request gives for the program that is signing in.
const REFERRER: &str = "bravebot";

/// The grant the token endpoint is asked under until the code has been used.
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// The most seconds a code is waited on, whatever the reply says it lasts
/// ([XAI-3](../../../../docs/specs/xai-sign-in.md#XAI-3)). The service stated 1800.
const LONGEST_CODE_LIFETIME: u64 = 3600;

/// The most seconds the interval between two asks may be, whatever the reply says and however often
/// it asks to slow down ([XAI-3](../../../../docs/specs/xai-sign-in.md#XAI-3)). The service stated 5.
const LONGEST_INTERVAL: u64 = 60;

/// The most polls in a row whose request does not complete that are asked again, where a fourth ends the
/// sign-in as the transport failure it is ([XAI-3](../../../../docs/specs/xai-sign-in.md#XAI-3)).
const MOST_CUT_POLLS: u32 = 3;

/// The fewest seconds an access token is taken to last, whatever the reply says
/// ([XAI-3](../../../../docs/specs/xai-sign-in.md#XAI-3)). The service stated 21600. A reply that stated
/// 1 would have the token replaced on every ask, and a refresh token spent on each.
const SHORTEST_TOKEN_LIFETIME: u64 = 300;

/// The most seconds an access token is taken to last, whatever the reply says
/// ([XAI-3](../../../../docs/specs/xai-sign-in.md#XAI-3)). A reply that stated 18446744073709551615 would
/// have a token never replaced by the clock.
const LONGEST_TOKEN_LIFETIME: u64 = 86_400;

/// The destinations of the sign-in. Compiled in: nothing a page, a reply, a model, a settings file
/// or an environment variable says names one
/// ([XAI-1](../../../../docs/specs/xai-sign-in.md#XAI-1)).
pub struct Hosts {
    device: String,
    token: String,
}

/// The variable that names a stand-in for every destination, in a build with the `test-hosts`
/// feature. A build without it holds no code that reads the variable, and a release build with it
/// does not compile, so no variable can name a destination in anything that ships
/// ([XAI-1](../../../../docs/specs/xai-sign-in.md#XAI-1)).
#[cfg(all(feature = "test-hosts", debug_assertions))]
pub const XAI_STAND_IN: &str = "BRAVEBOT_TEST_XAI_STAND_IN";

impl Hosts {
    /// The endpoint that gives a code to sign in with, and the token endpoint that is asked whether
    /// it has been used and that renews an access token.
    pub fn compiled_in() -> Self {
        #[cfg(all(feature = "test-hosts", debug_assertions))]
        if let Some(base) = std::env::var(XAI_STAND_IN)
            .ok()
            .filter(|base| !base.is_empty())
        {
            return Self::at(&base);
        }
        Self {
            device: "https://auth.x.ai/oauth2/device/code".to_string(),
            token: "https://auth.x.ai/oauth2/token".to_string(),
        }
    }

    /// Every destination under `base`, a server a test runs. Not in a release build, so no flag, no
    /// variable and no file can send a token anywhere but the hosts above.
    #[cfg(any(test, feature = "test-hosts"))]
    pub fn at(base: &str) -> Self {
        Self {
            device: format!("{base}/device/code"),
            token: format!("{base}/token"),
        }
    }
}

/// What the person is told before the sign-in waits for the code to be used
/// ([XAI-3](../../../../docs/specs/xai-sign-in.md#XAI-3)).
pub struct Announcement<'a> {
    /// The address to open: the one that holds the code where the reply gave one, and the address
    /// the code is entered at where it did not. It is made of printable characters and is `https`.
    pub url: &'a str,
    /// The code to enter, if the page asks for one.
    pub user_code: &'a str,
    /// Seconds the code lasts from the moment of the reply.
    pub expires_in: u64,
}

/// Why the sign-in stopped.
///
/// Each variant is the step that failed and, for a failed request, the class of its status. None
/// holds text taken from a reply, a request or a settings file, so the sentence the caller chooses
/// for it quotes nothing ([XAI-4](../../../../docs/specs/xai-sign-in.md#XAI-4)). The words are the
/// caller's.
#[derive(Debug)]
pub enum SignInError {
    /// The request for a code failed.
    Code(Class),
    /// The token endpoint answered `access_denied`.
    AccessDenied,
    /// The token endpoint answered `authorization_denied`.
    AuthorizationDenied,
    /// The code stopped working before it was used.
    Expired,
    /// The time the reply gave for the code passed before it was used.
    TimedOut,
    /// A request to the token endpoint failed, or its reply could not be read.
    Exchange(Class),
    /// The clock is set before 1970, so no time can be compared with it. Nothing was stored.
    Clock,
    /// The sign-in could not be stored.
    Store(AccountError),
}

/// A short name for the step, for a log or a test.
impl std::fmt::Display for SignInError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Code(class) => write!(f, "code {class}"),
            Self::AccessDenied => f.write_str("access denied"),
            Self::AuthorizationDenied => f.write_str("authorization denied"),
            Self::Expired => f.write_str("expired"),
            Self::TimedOut => f.write_str("timed out"),
            Self::Exchange(class) => write!(f, "exchange {class}"),
            Self::Clock => f.write_str("clock"),
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
    pub stored_in: PathBuf,
}

/// Sign in, and store the account where [`path`] says.
///
/// `announce` is called once, with the address and the code, after the code is had and before the
/// token endpoint is asked anything. `transport` is what the caller has decided its clients trust and
/// go through, half-built as for [`crate::device::register`].
pub fn sign_in(
    hosts: &Hosts,
    transport: ureq::config::ConfigBuilder<ureq::typestate::AgentScope>,
    announce: impl FnOnce(&Announcement<'_>),
) -> Result<SignedIn, SignInError> {
    let destination = path().map_err(SignInError::Store)?;
    run(
        hosts,
        transport,
        &destination,
        encoding::clock,
        std::thread::sleep,
        announce,
    )
}

/// The sign-in, with `clock` as the seconds since 1970 or `None` where it reads before that, and
/// `wait` as what waits.
///
/// The code's lifetime and the access token's are compared with the clock, and a clock that reads no
/// time would put every instant after it. So it is read before anything is asked, and again at each
/// ask and where the grant arrives, and a reading of `None` at any of them ends the sign-in with
/// nothing stored.
///
/// The token endpoint is asked at once and then after each wait: the interval the reply gave, which
/// is five seconds where it gave none, and three seconds more. A reply that says to slow down adds
/// five seconds to the interval for every ask after it. The interval is never more than a minute, so a
/// wait is never more than 63 seconds, and the code is never waited on for more than an hour, whatever
/// the reply says. No wait passes the
/// instant the code stops working, so the sign-in ends there whatever interval the reply gave.
fn run(
    hosts: &Hosts,
    transport: ureq::config::ConfigBuilder<ureq::typestate::AgentScope>,
    destination: &Path,
    clock: impl Fn() -> Option<u64>,
    mut wait: impl FnMut(Duration),
    announce: impl FnOnce(&Announcement<'_>),
) -> Result<SignedIn, SignInError> {
    clock().ok_or(SignInError::Clock)?;
    let agent = exchange::agent(transport);
    let code = exchange::code(&agent, hosts).map_err(SignInError::Code)?;
    let deadline = clock()
        .ok_or(SignInError::Clock)?
        .saturating_add(code.expires_in);

    announce(&Announcement {
        url: code
            .verification_uri_complete
            .as_deref()
            .unwrap_or(&code.verification_uri),
        user_code: &code.user_code,
        expires_in: code.expires_in,
    });

    let mut interval = code.interval;
    let mut cut_polls = 0;
    let granted = loop {
        let at = clock().ok_or(SignInError::Clock)?;
        if at >= deadline {
            return Err(SignInError::TimedOut);
        }
        let polled = match exchange::poll(&agent, hosts, &code.device_code) {
            Ok(polled) => {
                cut_polls = 0;
                polled
            }
            // A poll whose request did not complete is asked again at the next interval, up to
            // `MOST_CUT_POLLS` in a row, since the sign-in is waiting on a person and a connection that
            // was cut says nothing of the code. Any other failure, and the request for the code, end it.
            Err(Class::Transport) if cut_polls < MOST_CUT_POLLS => {
                cut_polls += 1;
                exchange::Polled::Pending
            }
            Err(class) => return Err(SignInError::Exchange(class)),
        };
        match polled {
            exchange::Polled::Granted(granted) => break granted,
            exchange::Polled::Pending => {}
            exchange::Polled::SlowDown => {
                interval = interval.saturating_add(5).min(LONGEST_INTERVAL);
            }
            exchange::Polled::AccessDenied => return Err(SignInError::AccessDenied),
            exchange::Polled::AuthorizationDenied => return Err(SignInError::AuthorizationDenied),
            exchange::Polled::Expired => return Err(SignInError::Expired),
        }
        wait(Duration::from_secs(
            interval.saturating_add(3).min(deadline - at),
        ));
    };

    // A grant with no refresh token cannot be renewed, so a sign-in stored from it would end with
    // the access token.
    let refresh = granted
        .refresh
        .ok_or(SignInError::Exchange(Class::Incomplete))?;
    let now = clock().ok_or(SignInError::Clock)?;
    let signed_in = Account {
        access_token: granted.access,
        refresh_token: refresh,
        expires_at: granted.lifetime.expiry(now),
    };
    account::save_at(destination, &signed_in).map_err(SignInError::Store)?;
    Ok(SignedIn {
        expires: signed_in.expires(),
        stored_in: destination.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Secret;
    use crate::oauth::stand_in::{Reply, Server, named, serve};
    use std::cell::{Cell, RefCell};

    /// The instant every test starts at, and the one the fake clock reads until a wait moves it.
    const START: u64 = 1_800_000_000;

    fn transport() -> ureq::config::ConfigBuilder<ureq::typestate::AgentScope> {
        ureq::Agent::config_builder()
    }

    fn scratch(name: &str) -> PathBuf {
        let directory = crate::testutil::scratch_dir(&format!("bravebot-xai-flow-{name}"));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a scratch directory");
        directory.join("xai-account.json")
    }

    /// The reply of the endpoint that gives a code.
    fn a_code(expires_in: Option<u64>, interval: Option<u64>) -> Reply {
        let mut reply = serde_json::json!({
            "device_code": "PLACEHOLDER-DEVICE-CODE",
            "user_code": "ABCD-EFGH",
            "verification_uri": "https://accounts.example/oauth2/device",
            "verification_uri_complete": "https://accounts.example/oauth2/device?user_code=ABCD-EFGH",
        });
        if let Some(seconds) = expires_in {
            reply["expires_in"] = seconds.into();
        }
        if let Some(seconds) = interval {
            reply["interval"] = seconds.into();
        }
        Reply::json(200, &reply.to_string())
    }

    /// The reply of the token endpoint to a code that has been used.
    fn granted(access: &str, refresh: &str, expires_in: u64) -> Reply {
        Reply::json(
            200,
            &serde_json::json!({
                "access_token": access,
                "refresh_token": refresh,
                "expires_in": expires_in,
            })
            .to_string(),
        )
    }

    /// The reply of the token endpoint to a code that has not been used, with the status it came
    /// with and the text of an error a reply could hold.
    fn refusal(status: u16, error: &str) -> Reply {
        Reply::json(
            status,
            &serde_json::json!({
                "error": error,
                "error_description": "PLACEHOLDER-ERROR-TEXT-the-person-never-sees",
            })
            .to_string(),
        )
    }

    fn pending() -> Reply {
        refusal(400, "authorization_pending")
    }

    /// What a run did, with the waits it asked for in seconds, the address and code it showed, and
    /// what the stand-in was asked.
    struct Ran {
        result: Result<SignedIn, SignInError>,
        waits: Vec<u64>,
        shown: Option<(String, String, u64)>,
        asked: Vec<crate::oauth::stand_in::Received>,
    }

    /// Run the sign-in against a stand-in that gives `replies`, with a clock that reads [`START`]
    /// and moves only as a wait moves it.
    fn ran(path: &Path, replies: Vec<Reply>) -> Ran {
        let server = serve(replies);
        let outcome = ran_against(&server, path, || Some(START));
        Ran {
            asked: server.finish(),
            ..outcome
        }
    }

    /// [`ran`] against a server that is already running, reading the clock `from` and adding every
    /// wait to what it reads.
    fn ran_against(server: &Server, path: &Path, from: impl Fn() -> Option<u64>) -> Ran {
        let slept = Cell::new(0_u64);
        let waits = RefCell::new(Vec::new());
        let shown = RefCell::new(None);
        let result = run(
            &Hosts::at(&server.base),
            transport(),
            path,
            || from().map(|now| now + slept.get()),
            |waited| {
                waits.borrow_mut().push(waited.as_secs());
                slept.set(slept.get() + waited.as_secs());
            },
            |announcement| {
                *shown.borrow_mut() = Some((
                    announcement.url.to_string(),
                    announcement.user_code.to_string(),
                    announcement.expires_in,
                ));
            },
        );
        Ran {
            result,
            waits: waits.into_inner(),
            shown: shown.into_inner(),
            asked: Vec::new(),
        }
    }

    fn stored(path: &Path) -> Option<Account> {
        account::load_at(path).expect("a readable file")
    }

    /// The request for a code, from the first request to the file: the program asks for a code under
    /// its client, scope and referrer and no other field, with no credential, and the token endpoint
    /// is asked under the device grant with the code the first reply gave and no other field. The
    /// tokens, and the expiry the reply states, are stored.
    #[test]
    fn a_sign_in_asks_for_a_code_under_its_client_and_polls_with_the_code_it_was_given() {
        let path = scratch("first");

        let run = ran(
            &path,
            vec![
                a_code(Some(1800), Some(5)),
                granted("an-access", "a-refresh", 1800),
            ],
        );

        let signed = run.result.expect("a sign-in");
        assert_eq!(run.asked.len(), 2, "{:?}", run.asked);
        let asking = &run.asked[0];
        assert_eq!(
            (asking.method.as_str(), asking.path.as_str()),
            ("POST", "/device/code")
        );
        assert_eq!(
            asking.header("content-type"),
            Some("application/x-www-form-urlencoded")
        );
        assert_eq!(asking.header("accept"), Some("application/json"));
        assert_eq!(asking.header("user-agent"), Some("bravebot"));
        assert_eq!(asking.header("authorization"), None);
        let form = asking.form();
        let names: Vec<&str> = form.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["client_id", "scope", "referrer"]);
        assert_eq!(
            named(&form, "client_id"),
            "b1a00492-073a-47ea-816f-4c329264a828"
        );
        assert_eq!(
            named(&form, "scope"),
            "openid profile email offline_access grok-cli:access api:access"
        );
        assert_eq!(named(&form, "referrer"), "bravebot");

        let polling = &run.asked[1];
        assert_eq!(
            (polling.method.as_str(), polling.path.as_str()),
            ("POST", "/token")
        );
        assert_eq!(
            polling.header("content-type"),
            Some("application/x-www-form-urlencoded")
        );
        assert_eq!(polling.header("authorization"), None);
        let form = polling.form();
        let names: Vec<&str> = form.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["grant_type", "client_id", "device_code"]);
        assert_eq!(
            named(&form, "grant_type"),
            "urn:ietf:params:oauth:grant-type:device_code"
        );
        assert_eq!(
            named(&form, "client_id"),
            "b1a00492-073a-47ea-816f-4c329264a828"
        );
        assert_eq!(named(&form, "device_code"), "PLACEHOLDER-DEVICE-CODE");

        let account = stored(&path).expect("an account");
        assert_eq!(account.access_token.expose(), "an-access");
        assert_eq!(account.refresh_token.expose(), "a-refresh");
        assert_eq!(account.expires_at, START + 1800);
        assert_eq!(signed.expires, crate::oauth::encoding::utc(START + 1800));
        assert_eq!(signed.stored_in, path);
        assert!(run.waits.is_empty(), "{:?}", run.waits);
    }

    /// The address and the code are shown once, after the code is had and before the token endpoint
    /// is asked anything: at that moment the stand-in has been asked for the code and for nothing
    /// else. A sign-in that polled first, or showed them when it was over, would show a code that had
    /// already been used or had stopped working. The address is the one that holds the code where
    /// the reply gives one.
    #[test]
    fn the_address_and_the_code_are_shown_before_the_first_poll() {
        let path = scratch("shown");
        let server = serve(vec![a_code(Some(1800), Some(5)), granted("a", "r", 60)]);
        let asked_when_shown = Cell::new(0);
        let shown_times = Cell::new(0);

        let result = run(
            &Hosts::at(&server.base),
            transport(),
            &path,
            || Some(START),
            |_| {},
            |announcement| {
                shown_times.set(shown_times.get() + 1);
                asked_when_shown.set(server.served());
                assert_eq!(
                    announcement.url,
                    "https://accounts.example/oauth2/device?user_code=ABCD-EFGH"
                );
                assert_eq!(announcement.user_code, "ABCD-EFGH");
                assert_eq!(announcement.expires_in, 1800);
            },
        );

        result.expect("a sign-in");
        assert_eq!(shown_times.get(), 1, "shown more than once");
        assert_eq!(
            asked_when_shown.get(),
            1,
            "the token endpoint was asked before the code was shown"
        );
        let asked = server.finish();
        assert_eq!(asked.len(), 2, "{asked:?}");
        assert_eq!(asked[0].path, "/device/code");
        assert_eq!(asked[1].path, "/token");
    }

    /// Where the reply gives only the address the code is entered at, that is the address shown.
    #[test]
    fn an_address_with_no_code_in_it_is_the_one_shown_where_the_reply_gives_no_other() {
        let path = scratch("plain-address");
        let reply = Reply::json(
            200,
            "{\"device_code\":\"d\",\"user_code\":\"ABCD-EFGH\",\
             \"verification_uri\":\"https://accounts.example/oauth2/device\"}",
        );
        let server = serve(vec![reply, granted("a", "r", 60)]);

        let run = ran_against(&server, &path, || Some(START));
        server.finish();

        run.result.expect("a sign-in");
        assert_eq!(
            run.shown,
            Some((
                "https://accounts.example/oauth2/device".to_string(),
                "ABCD-EFGH".to_string(),
                300
            ))
        );
    }

    /// `authorization_pending` is asked again, after the interval the reply gave and three seconds
    /// more. The waits are recorded and no wait is real. An implementation that gave up at the first
    /// pending reply, asked again with no wait, or waited the interval alone would fail it.
    #[test]
    fn authorization_pending_is_asked_again_after_the_interval_and_three_seconds() {
        let path = scratch("pending");

        let run = ran(
            &path,
            vec![
                a_code(Some(1800), Some(5)),
                pending(),
                pending(),
                granted("a", "r", 60),
            ],
        );

        run.result.expect("a sign-in");
        assert_eq!(run.waits, [8, 8]);
        assert_eq!(run.asked.len(), 4, "{:?}", run.asked);
        assert!(run.asked[1..].iter().all(|asked| asked.path == "/token"));
    }

    /// XAI-3: a poll whose request does not complete, because the connection was cut, is asked again at
    /// the next interval, up to three in a row, and the sign-in goes on to be approved. The waits are
    /// the ordinary interval and three seconds. The regression it rejects is a sign-in that ends at the
    /// first poll the network drops, which ends one waiting on a person who has not yet approved the
    /// code.
    #[test]
    fn up_to_three_polls_in_a_row_that_do_not_complete_are_asked_again_at_the_next_interval() {
        let path = scratch("cut-polls");

        let run = ran(
            &path,
            vec![
                a_code(Some(1800), Some(5)),
                Reply::hang_up(),
                Reply::hang_up(),
                Reply::hang_up(),
                granted("a", "r", 60),
            ],
        );

        run.result
            .expect("a sign-in that went on past the cut polls");
        assert_eq!(run.waits, [8, 8, 8]);
        assert_eq!(run.asked.len(), 5, "{:?}", run.asked);
        assert_eq!(
            stored(&path).expect("an account").access_token.expose(),
            "a"
        );
    }

    /// XAI-3: the count is of polls in a row, so a poll that gets an answer starts it again, and a
    /// sign-in that is cut at every other poll is never ended for it. The regression it rejects is a
    /// count of all the polls that were cut, which would end a long wait at the fourth in an hour.
    #[test]
    fn a_poll_that_is_answered_starts_the_count_of_cut_polls_again() {
        let path = scratch("cut-polls-reset");
        let mut replies = vec![a_code(Some(1800), Some(5))];
        for _ in 0..4 {
            replies.push(Reply::hang_up());
            replies.push(pending());
        }
        replies.push(granted("a", "r", 60));

        let run = ran(&path, replies);

        run.result
            .expect("a sign-in that went on past four cut polls");
        assert_eq!(run.waits.len(), 8, "{:?}", run.waits);
    }

    /// XAI-3: a fourth poll in a row that does not complete ends the sign-in as the transport failure it
    /// is, with nothing stored, after waiting out the three before it. A sign-in that went on for ever
    /// would hold a machine that cannot reach the service until the code's time was up.
    #[test]
    fn a_fourth_poll_in_a_row_that_does_not_complete_ends_the_sign_in_as_a_transport_failure() {
        let path = scratch("cut-polls-fourth");

        let run = ran(
            &path,
            vec![
                a_code(Some(1800), Some(5)),
                Reply::hang_up(),
                Reply::hang_up(),
                Reply::hang_up(),
                Reply::hang_up(),
            ],
        );

        let failure = run.result.expect_err("a failed sign-in");
        assert!(
            matches!(failure, SignInError::Exchange(Class::Transport)),
            "{failure:?}"
        );
        assert_eq!(run.waits, [8, 8, 8]);
        assert_eq!(run.asked.len(), 5, "{:?}", run.asked);
        assert!(!path.exists());
    }

    /// XAI-3: the code's time still ends the wait. Three cut polls in a row are asked again and the
    /// last wait is the time that is left, so the sign-in ends as timed out where the code stops
    /// working, and not as a transport failure and not later. The regression it rejects is a cut poll
    /// that does not shorten its wait to the time the code has left.
    #[test]
    fn cut_polls_do_not_hold_a_sign_in_past_the_time_the_code_lasts() {
        let path = scratch("cut-polls-timeout");

        let run = ran(
            &path,
            vec![
                a_code(Some(20), Some(5)),
                Reply::hang_up(),
                Reply::hang_up(),
                Reply::hang_up(),
            ],
        );

        let failure = run.result.expect_err("a failed sign-in");
        assert!(matches!(failure, SignInError::TimedOut), "{failure:?}");
        assert_eq!(run.waits, [8, 8, 4]);
    }

    /// XAI-3: only a poll that does not complete is asked again. The request for the code ends the
    /// sign-in at the first failure, and so does a poll that is answered with a failure of another
    /// class. The regression it rejects is a retry of every failure, which would ask a service that is
    /// refusing or limiting requests again and again.
    #[test]
    fn nothing_but_a_poll_that_does_not_complete_is_asked_again() {
        let path = scratch("cut-code");
        let run = ran(&path, vec![Reply::hang_up()]);
        let failure = run.result.expect_err("a failed sign-in");
        assert!(
            matches!(failure, SignInError::Code(Class::Transport)),
            "{failure:?}"
        );
        assert_eq!(run.asked.len(), 1);
        assert!(run.waits.is_empty());

        for (reply, class) in [
            (Reply::json(503, ""), Class::Unavailable),
            (Reply::json(429, "{}"), Class::RateLimited),
            (refusal(400, "invalid_request"), Class::Refused),
        ] {
            let path = scratch("cut-other");
            let run = ran(&path, vec![a_code(Some(1800), Some(5)), reply, pending()]);
            let failure = run.result.expect_err("a failed sign-in");
            assert!(
                matches!(failure, SignInError::Exchange(found) if found == class),
                "{failure:?} where {class:?} was wanted"
            );
            assert_eq!(run.asked.len(), 2, "{:?}", run.asked);
            assert!(run.waits.is_empty(), "{:?}", run.waits);
        }
    }

    /// A reply that gives no interval is asked every five seconds, and three more.
    #[test]
    fn a_code_that_gives_no_interval_is_asked_about_every_eight_seconds() {
        let path = scratch("no-interval");

        let run = ran(
            &path,
            vec![a_code(Some(1800), None), pending(), granted("a", "r", 60)],
        );

        run.result.expect("a sign-in");
        assert_eq!(run.waits, [8]);
    }

    /// `slow_down` adds five seconds to the interval, and every wait after it is the longer one. The
    /// wait that follows the `slow_down` itself is already the longer one. An implementation that
    /// added the five seconds to one wait only, or after the wait, would fail it.
    #[test]
    fn slow_down_lengthens_the_wait_that_follows_it_and_every_one_after() {
        let path = scratch("slow-down");

        let run = ran(
            &path,
            vec![
                a_code(Some(1800), Some(5)),
                pending(),
                refusal(400, "slow_down"),
                pending(),
                refusal(400, "slow_down"),
                pending(),
                granted("a", "r", 60),
            ],
        );

        run.result.expect("a sign-in");
        assert_eq!(run.waits, [8, 13, 13, 18, 18]);
    }

    /// The sign-in a reply that is `error` ends, after one ask, with nothing stored and no wait.
    fn ended_by(error: &str) -> SignInError {
        let path = scratch(&format!("ends-{error}"));

        let run = ran(
            &path,
            vec![a_code(Some(1800), Some(5)), refusal(400, error)],
        );

        let failure = run.result.expect_err(error);
        assert_eq!(run.asked.len(), 2, "{error}");
        assert!(run.waits.is_empty(), "{error}: {:?}", run.waits);
        assert!(!path.exists(), "{error}: an account was stored");
        failure
    }

    /// `access_denied` ends the sign-in as `AccessDenied` and as nothing else, which is how it comes
    /// to have a sentence of its own. A sign-in that read it with `authorization_denied`, or as an
    /// expired code, or went on asking, would fail it.
    #[test]
    fn access_denied_ends_the_sign_in_as_its_own_ending_after_one_ask_and_stores_nothing() {
        let failure = ended_by("access_denied");
        assert!(matches!(failure, SignInError::AccessDenied), "{failure:?}");
    }

    /// `authorization_denied` ends the sign-in as `AuthorizationDenied`, apart from `access_denied`.
    #[test]
    fn authorization_denied_ends_the_sign_in_as_its_own_ending_after_one_ask_and_stores_nothing() {
        let failure = ended_by("authorization_denied");
        assert!(
            matches!(failure, SignInError::AuthorizationDenied),
            "{failure:?}"
        );
    }

    /// `expired_token` ends the sign-in as `Expired`, apart from the two that deny it.
    #[test]
    fn expired_token_ends_the_sign_in_as_its_own_ending_after_one_ask_and_stores_nothing() {
        let failure = ended_by("expired_token");
        assert!(matches!(failure, SignInError::Expired), "{failure:?}");
    }

    /// A code that nobody uses ends the sign-in at the time the reply gave for it, and the last wait
    /// is the time that is left rather than the interval, so a reply that gives an interval longer
    /// than the code lasts cannot keep the command waiting past it. Here the code lasts 20 seconds,
    /// the interval is 5, and the waits are 8, 8 and the 4 that are left.
    #[test]
    fn a_code_nobody_uses_ends_the_sign_in_at_the_time_it_states() {
        let path = scratch("times-out");

        let run = ran(
            &path,
            vec![a_code(Some(20), Some(5)), pending(), pending(), pending()],
        );

        assert!(
            matches!(run.result, Err(SignInError::TimedOut)),
            "{:?}",
            run.result
        );
        assert_eq!(run.waits, [8, 8, 4]);
        assert_eq!(run.asked.len(), 4, "{:?}", run.asked);
        assert!(!path.exists());
    }

    /// A reply that states no lifetime for the code is one that lasts five minutes, and an interval
    /// of an hour is a minute, so the waits are 63 seconds each and the last is the time that is left.
    #[test]
    fn a_code_that_states_no_lifetime_lasts_five_minutes_whatever_the_interval() {
        let path = scratch("five-minutes");
        let mut replies = vec![a_code(None, Some(3600))];
        replies.extend((0..5).map(|_| pending()));

        let run = ran(&path, replies);

        assert!(matches!(run.result, Err(SignInError::TimedOut)));
        assert_eq!(run.waits, [63, 63, 63, 63, 48]);
    }

    /// A reply that states a lifetime of a day and an interval of an hour is waited on for an hour at
    /// most and asked about every 63 seconds at most, and the person is told the hour. A sign-in that
    /// took the figures as stated would hold the command for a day, and wait an hour between asks.
    #[test]
    fn a_code_is_waited_on_for_an_hour_at_most_and_asked_about_every_minute_at_least() {
        let path = scratch("capped");
        let mut replies = vec![a_code(Some(86_400), Some(3600))];
        replies.extend((0..60).map(|_| pending()));

        let run = ran(&path, replies);

        assert!(matches!(run.result, Err(SignInError::TimedOut)));
        assert_eq!(run.waits.iter().sum::<u64>(), 3600, "{:?}", run.waits);
        assert_eq!(run.waits.iter().max(), Some(&63), "{:?}", run.waits);
        assert_eq!(run.shown.map(|(_, _, lasts)| lasts), Some(3600));
    }

    /// A reply that asks to slow down cannot take the interval past a minute: the wait is 63 seconds
    /// however often it asks. An interval that grew by five at each ask without a bound would wait
    /// 66 seconds after the first and 71 after the second.
    #[test]
    fn slow_down_never_lengthens_the_interval_past_a_minute() {
        let path = scratch("slow-down-capped");

        let run = ran(
            &path,
            vec![
                a_code(Some(1800), Some(58)),
                refusal(400, "slow_down"),
                refusal(400, "slow_down"),
                pending(),
                granted("a", "r", 60),
            ],
        );

        run.result.expect("a sign-in");
        assert_eq!(run.waits, [63, 63, 63]);
    }

    /// An error code that is not in the list ends the sign-in as the class of the status it came
    /// with, and quotes nothing: not the code, and not the text beside it. A sign-in that gave the
    /// error text in its own would put a string chosen by a server into a sentence.
    #[test]
    fn any_other_failure_ends_the_sign_in_as_the_class_of_its_status_and_quotes_nothing() {
        for (reply, expected) in [
            (refusal(400, "invalid_client"), Class::Refused),
            (refusal(400, "invalid_request"), Class::Refused),
            (
                refusal(401, "authorization_pending_but_not"),
                Class::Refused,
            ),
            (refusal(429, "invalid_client"), Class::RateLimited),
            (refusal(503, "temporarily_unavailable"), Class::Unavailable),
            (Reply::json(400, "not json"), Class::Refused),
            (Reply::json(400, "{\"error\":7}"), Class::Refused),
            (Reply::json(500, ""), Class::Unavailable),
            (Reply::json(200, "{\"nothing\":1}"), Class::Incomplete),
            (Reply::json(200, "not json"), Class::Incomplete),
        ] {
            let path = scratch("other");

            let run = ran(&path, vec![a_code(Some(1800), Some(5)), reply]);

            let failure = run.result.expect_err("a failed sign-in");
            assert!(
                matches!(failure, SignInError::Exchange(class) if class == expected),
                "{failure:?} where {expected:?} was wanted"
            );
            for printed in [format!("{failure}"), format!("{failure:?}")] {
                assert!(!printed.contains("PLACEHOLDER-ERROR-TEXT"), "{printed}");
                assert!(!printed.contains("invalid_client"), "{printed}");
            }
            assert!(!path.exists(), "an account was stored");
        }
    }

    /// A request for a code that fails ends the sign-in before anything is shown or asked of the
    /// token endpoint, as the class of its status. An unknown client is a 400 with a code of its own,
    /// which is not quoted.
    #[test]
    fn a_request_for_a_code_that_fails_shows_nothing_and_asks_the_token_endpoint_nothing() {
        for (reply, expected) in [
            (refusal(400, "invalid_client"), Class::Refused),
            (Reply::json(503, ""), Class::Unavailable),
            (Reply::json(429, "{}"), Class::RateLimited),
            (
                Reply::json(200, "{\"device_code\":\"d\"}"),
                Class::Incomplete,
            ),
            (Reply::json(200, "not json"), Class::Incomplete),
        ] {
            let path = scratch("no-code");

            let run = ran(&path, vec![reply]);

            let failure = run.result.expect_err("a failed sign-in");
            assert!(
                matches!(failure, SignInError::Code(class) if class == expected),
                "{failure:?} where {expected:?} was wanted"
            );
            assert!(run.shown.is_none(), "an address was shown");
            assert_eq!(run.asked.len(), 1, "{:?}", run.asked);
            assert!(!format!("{failure:?}").contains("PLACEHOLDER-ERROR-TEXT"));
            assert!(!path.exists());
        }
    }

    /// A grant with no refresh token ends the sign-in as a reply that cannot be read, and nothing is
    /// stored: an access token alone cannot be renewed, so a sign-in stored from it would end within
    /// the hour.
    #[test]
    fn a_grant_that_holds_no_refresh_token_stores_nothing() {
        let path = scratch("no-refresh");

        let run = ran(
            &path,
            vec![
                a_code(Some(1800), Some(5)),
                Reply::json(200, "{\"access_token\":\"an-access\",\"expires_in\":60}"),
            ],
        );

        assert!(
            matches!(run.result, Err(SignInError::Exchange(Class::Incomplete))),
            "{:?}",
            run.result
        );
        assert!(!path.exists());
    }

    /// The expiry stored is the instant the grant arrived plus the lifetime it states, and where it
    /// states none it is the `exp` claim of an access token that is a JWT, held to between five minutes
    /// and a day after the grant, and an hour after the grant where that is not there either. A
    /// lifetime stated as 0 is five minutes.
    #[test]
    fn the_expiry_stored_is_the_lifetimes_then_the_tokens_claim_then_an_hour() {
        use crate::oauth::encoding::base64url;
        let dated = |exp: &str| {
            format!(
                "{}.{}.signature",
                base64url(b"{\"alg\":\"none\"}"),
                base64url(format!("{{\"exp\":{exp}}}").as_bytes())
            )
        };
        let (dated, far, past) = (
            dated(&(START + 777).to_string()),
            dated("18446744073709551615"),
            dated("1"),
        );
        for (grant, expected) in [
            (
                serde_json::json!({"access_token": dated, "refresh_token": "r", "expires_in": 900}),
                START + 900,
            ),
            (
                serde_json::json!({"access_token": dated, "refresh_token": "r"}),
                START + 777,
            ),
            (
                serde_json::json!({"access_token": "opaque", "refresh_token": "r"}),
                START + 3600,
            ),
            (
                serde_json::json!({"access_token": far, "refresh_token": "r"}),
                START + 86_400,
            ),
            (
                serde_json::json!({"access_token": past, "refresh_token": "r"}),
                START + 300,
            ),
            (
                serde_json::json!({"access_token": "opaque", "refresh_token": "r", "expires_in": 0}),
                START + 300,
            ),
        ] {
            let path = scratch("expiry");

            let run = ran(
                &path,
                vec![
                    a_code(Some(1800), Some(5)),
                    Reply::json(200, &grant.to_string()),
                ],
            );

            run.result.expect("a sign-in");
            assert_eq!(
                stored(&path).expect("an account").expires_at,
                expected,
                "{grant}"
            );
        }
    }

    /// A failed sign-in leaves a stored one as it was, and a completed one replaces it. A sign-in
    /// that cleared the file before it knew it would succeed would sign a person out by failing.
    #[test]
    fn a_failed_sign_in_leaves_a_stored_one_and_a_completed_one_replaces_it() {
        let path = scratch("over");
        let before = Account {
            access_token: Secret::new("old-access"),
            refresh_token: Secret::new("old-refresh"),
            expires_at: 5,
        };
        account::save_at(&path, &before).expect("a stored account");

        let failed = ran(
            &path,
            vec![a_code(Some(1800), Some(5)), refusal(400, "access_denied")],
        );
        assert!(failed.result.is_err());
        assert_eq!(
            stored(&path).expect("the account").access_token.expose(),
            "old-access"
        );

        let done = ran(
            &path,
            vec![
                a_code(Some(1800), Some(5)),
                granted("new-access", "new-refresh", 60),
            ],
        );
        done.result.expect("a sign-in");
        let now = stored(&path).expect("an account");
        assert_eq!(now.access_token.expose(), "new-access");
        assert_eq!(now.refresh_token.expose(), "new-refresh");
    }

    /// A clock set before 1970 reads as no time, and a sign-in compares the code's lifetime and the
    /// token's with it, so it ends before anything is asked where the clock reads none at the start,
    /// and with nothing stored where it reads none at the grant.
    #[test]
    fn a_clock_set_before_1970_ends_the_sign_in_where_it_would_be_compared() {
        let path = scratch("clock-start");
        let server = serve(vec![]);
        let run = ran_against(&server, &path, || None);
        let asked = server.finish();
        assert!(
            matches!(run.result, Err(SignInError::Clock)),
            "{:?}",
            run.result
        );
        assert!(asked.is_empty(), "{asked:?}");

        let path = scratch("clock-grant");
        let server = serve(vec![a_code(Some(1800), Some(5)), granted("a", "r", 60)]);
        // Read well at the start, at the code and at the first ask, and not at the grant.
        let reads = Cell::new(0);
        let run = ran_against(&server, &path, || {
            reads.set(reads.get() + 1);
            (reads.get() <= 3).then_some(START)
        });
        server.finish();
        assert!(
            matches!(run.result, Err(SignInError::Clock)),
            "{:?}",
            run.result
        );
        assert!(!path.exists(), "an account was stored");
    }

    /// A redirect in answer to the request for a code or to a poll is a failure and is not followed: the
    /// redirect would carry the device code to a host nobody compiled in. The host a reply names is
    /// asked nothing. A redirect in answer to a refresh, which carries the refresh token, is held by
    /// `renew::a_redirect_in_answer_to_a_refresh_is_not_followed_and_the_sign_in_is_left_alone`.
    #[test]
    fn a_redirect_in_answer_to_any_request_is_not_followed() {
        for (replies, step) in [
            (vec!["redirect"], "the code"),
            (vec!["code", "redirect"], "a poll"),
        ] {
            let target = serve(vec![]);
            let to = format!("{}/elsewhere", target.base);
            let replies: Vec<Reply> = replies
                .into_iter()
                .map(|which| match which {
                    "redirect" => Reply::redirect(&to),
                    _ => a_code(Some(1800), Some(5)),
                })
                .collect();
            let path = scratch("redirect");

            let run = ran(&path, replies);

            assert!(
                matches!(
                    run.result,
                    Err(SignInError::Code(Class::Refused) | SignInError::Exchange(Class::Refused))
                ),
                "{step}: {:?}",
                run.result
            );
            let reached = target.finish();
            assert!(reached.is_empty(), "{step}: {reached:?}");
        }
    }

    /// Every value the sign-in handles goes where it belongs and nowhere else. The device code goes to
    /// the token endpoint, which is the one place that is asked about it. The refresh token goes to
    /// the token endpoint, in the refresh and in no other request. The request for a code carries no
    /// token, and no request carries an access token, since nothing in this module sends one.
    #[test]
    fn the_refresh_token_goes_to_the_token_endpoint_alone_and_no_request_carries_an_access_token() {
        let path = scratch("routing");
        let server = serve(vec![
            a_code(Some(1800), Some(5)),
            granted("PLACEHOLDER-ACCESS-ONE", "PLACEHOLDER-REFRESH-ONE", 1800),
            granted("PLACEHOLDER-ACCESS-TWO", "PLACEHOLDER-REFRESH-TWO", 1800),
        ]);
        ran_against(&server, &path, || Some(START))
            .result
            .expect("a sign-in");
        let mut renewing = Renewer::at(Hosts::at(&server.base), transport(), path.clone())
            .clocked(|| START + 1790);

        let access = renewing.access_token().expect("a renewal");
        let asked = server.finish();

        assert_eq!(access.token.expose(), "PLACEHOLDER-ACCESS-TWO");
        let paths: Vec<&str> = asked.iter().map(|asked| asked.path.as_str()).collect();
        assert_eq!(paths, ["/device/code", "/token", "/token"]);
        let carries = |asked: &crate::oauth::stand_in::Received, value: &str| {
            asked.body.contains(value)
                || asked
                    .headers
                    .iter()
                    .any(|(name, header)| name.contains(value) || header.contains(value))
        };
        let carriers = |value: &str| -> Vec<usize> {
            (0..asked.len())
                .filter(|at| carries(&asked[*at], value))
                .collect()
        };
        assert_eq!(carriers("PLACEHOLDER-REFRESH-ONE"), [2]);
        assert_eq!(carriers("PLACEHOLDER-DEVICE-CODE"), [1]);
        for value in [
            "PLACEHOLDER-ACCESS-ONE",
            "PLACEHOLDER-ACCESS-TWO",
            "PLACEHOLDER-REFRESH-TWO",
        ] {
            assert_eq!(carriers(value), Vec::<usize>::new(), "{value}");
        }
        assert!(
            asked
                .iter()
                .all(|asked| asked.header("authorization").is_none()),
            "a request carried an authorization header"
        );
    }

    /// The destinations are the issuer's two endpoints, as the constants of the source state them.
    /// A host that came from anywhere but the constants, or a path of another endpoint, would send
    /// the device code and the refresh token where nobody reviewed. With the variable that names a
    /// stand-in set the hosts are the stand-in's, so the test fails and says so, rather than passing
    /// on a comparison it did not make.
    #[test]
    fn the_destinations_are_the_two_endpoints_of_the_issuer() {
        assert!(
            std::env::var("BRAVEBOT_TEST_XAI_STAND_IN").is_err(),
            "BRAVEBOT_TEST_XAI_STAND_IN is set, so the compiled-in hosts cannot be read: unset it"
        );
        let hosts = Hosts::compiled_in();
        assert_eq!(hosts.device, "https://auth.x.ai/oauth2/device/code");
        assert_eq!(hosts.token, "https://auth.x.ai/oauth2/token");
    }
}
