//! A stand-in for OpenAI's authorization server, key set and serving host, and every value a run
//! against it handles, for the tests that hold each output of a run that signs in, renews and fails
//! to all of them ([CHATGPT-8](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-8)).
//!
//! The program under test reaches it through the `test-hosts` seam of `bravebot-skus` and
//! `bravebot-config`, which the dev-dependencies of the crate that includes this file turn on: the
//! variable [`STAND_IN`] names its address. The constants below are read from those crates, so a
//! crate where the seam is off does not compile this file.
//!
//! It issues each value from a placeholder of its own, so a value that reaches an output names
//! itself: the authorization code, the client, both pairs of tokens, the ID token with the claims it
//! carries, and the text each refusal says. Every refusal repeats every value the stand-in has seen or
//! made. The `state`, the `nonce`, the challenge, the host identifier and the verifier are made by the
//! program under test and are read from the authorization URL it announced and from the exchange.
//!
//! What it does at a request that a test scripts is the test's: [`Refresh`] says what the token
//! endpoint does with each refresh, [`Serving`] what the serving host does with each turn and each
//! listing, and [`Fault`] how a sign-in goes wrong.
#![allow(
    dead_code,
    reason = "each test crate that includes this file uses some of it"
)]

use bravebot_skus::chatgpt::stand_in_key::{header, key_set, signed};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Output;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

#[path = "../../aichat/test-support/refusal_codes.rs"]
mod refusal_codes;
/// The code of the closed list of CHATGPT-8 that says the account has reached its usage limit.
pub const USAGE_LIMIT: &str = refusal_codes::USAGE_LIMIT;
/// One of the six codes of the list that say the account may not be used this way.
pub const NOT_ELIGIBLE: &str = refusal_codes::NOT_ELIGIBLE;

#[path = "chatgpt_host.rs"]
mod chatgpt_host;
use chatgpt_host::{Request, call_events, read_request, respond, text_events};

/// The variable both seams read. Taken from the crate rather than written here, so a build where
/// the seam is off does not compile the tests that include this file.
pub const STAND_IN: &str = bravebot_skus::chatgpt::STAND_IN;
const _: () = assert!(const_eq(
    STAND_IN,
    bravebot_config::provider::CHATGPT_STAND_IN
));

const fn const_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut at = 0;
    while at < a.len() {
        if a[at] != b[at] {
            return false;
        }
        at += 1;
    }
    true
}

/// A distinct placeholder for each value the run handles that the stand-in chooses.
pub const CODE: &str = "PLACEHOLDER-CODE-c0de";
pub const CLIENT: &str = "placeholder-client-c11e";
pub const ACCESS_ONE: &str = "PLACEHOLDER-ACCESS-ONE-a1a1";
pub const REFRESH_ONE: &str = "PLACEHOLDER-REFRESH-ONE-r1r1";
pub const ACCESS_TWO: &str = "PLACEHOLDER-ACCESS-TWO-a2a2";
pub const REFRESH_TWO: &str = "PLACEHOLDER-REFRESH-TWO-r2r2";
pub const SUBJECT: &str = "PLACEHOLDER-SUBJECT-5b5b";
pub const EMAIL: &str = "placeholder-person@example.invalid";
/// In every body the stand-in refuses with, so a sentence that quoted a body is found even where it
/// quoted no secret.
pub const ECHO: &str = "PLACEHOLDER-REPLY-BODY-ec40";
pub const MODEL: &str = "chatgpt/gpt-placeholder";

/// How the stand-in goes wrong at a sign-in, for a run whose sign-in is meant to stop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fault {
    /// It does not: the sign-in completes.
    #[default]
    None,
    /// The token endpoint refuses the code, in a body that repeats what it has seen.
    ExchangeRefused,
    /// The ID token it issues carries another sign-in's nonce.
    IdTokenOfAnotherSignIn,
    /// The key set is unavailable, in a body that repeats what it has seen.
    KeySetUnavailable,
}

/// What the token endpoint does with the next refresh it is given.
#[derive(Clone, Copy, Debug)]
pub enum Refresh {
    /// It grants a new access token and a new refresh token.
    Grants(&'static str, &'static str),
    /// It is busy: a 500 that repeats what it has seen.
    Busy,
    /// It says the refresh token is no longer valid: a 400 with `invalid_grant` that repeats what it
    /// has seen.
    Ended,
}

/// What the serving host does with the next turn, or the next listing, it is given.
#[derive(Clone, Copy, Debug)]
pub enum Serving {
    /// A turn is answered, in the text `all done`.
    Answers,
    /// A turn is answered, in this text.
    Says(&'static str),
    /// The token is turned away: a 401 that repeats what the stand-in has seen.
    RefusesTheToken,
    /// The request is refused with a status of its own and a body that repeats what it has seen.
    Refuses(u16),
    /// The request is refused with a status and an `error.code` of the closed list of
    /// [CHATGPT-8](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-8), in a body that repeats what it
    /// has seen.
    RefusesWithCode(u16, &'static str),
    /// A turn is answered with one call of the tool `name` with the JSON `arguments`, as the model
    /// would ask for it, and ends there.
    Calls(&'static str, &'static str),
    /// A listing is answered with a row for each of these names, each shown to a person and
    /// reachable through the API. A turn is answered in the text `all done`.
    Lists(&'static [&'static str]),
}

/// One request the stand-in took.
#[derive(Debug, Clone)]
pub struct Asked {
    pub method: String,
    pub path: String,
    pub bearer: Option<String>,
    pub form: Vec<(String, String)>,
    /// Every header the request carried, in the order they came.
    pub headers: Vec<(String, String)>,
    /// The body as the bytes that arrived, which for a request to the serving host is a zstd frame.
    pub body: Body,
}

impl Asked {
    /// The value of the header `name`, which is compared without regard to case.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(each, _)| each.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// The bytes of a request body, which a failure message names by their number and does not print.
#[derive(Clone)]
pub struct Body(pub Vec<u8>);

impl std::fmt::Debug for Body {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} bytes", self.0.len())
    }
}

/// What the stand-in knows and answers with.
pub struct State {
    /// The authorization URL of the sign-in, as the test read it from the program's terminal.
    pub url: Option<String>,
    /// A file the program under test writes that URL to, where the test cannot read its terminal.
    pub url_file: Option<PathBuf>,
    /// Each refresh token the token endpoint takes, and the access and refresh tokens it gives for
    /// it. One that is not listed is refused as no longer valid. A refresh that a script in
    /// [`State::refreshing`] answers is not looked for here.
    pub renewals: Vec<(&'static str, &'static str, &'static str)>,
    /// What each refresh is answered with, in order, until it is empty.
    pub refreshing: VecDeque<Refresh>,
    /// What each turn is answered with, in order, until it is empty. Then a turn is answered by the
    /// rules of [`State::refusing`] and [`ACCESS_ONE`].
    pub serving: VecDeque<Serving>,
    /// What each turn of a session is answered with, for the first session that makes a request, then
    /// for the second, and so on. A session with no script left is answered from [`State::serving`].
    pub session_scripts: Vec<VecDeque<Serving>>,
    /// What each listing is answered with, in order, until it is empty.
    pub listing: VecDeque<Serving>,
    /// Every request to the serving host is refused with a 401.
    pub refusing: bool,
    /// How the sign-in goes wrong.
    pub fault: Fault,
    /// How many seconds the first access token lasts.
    pub first_lifetime: u64,
    /// The ID token it issued.
    pub id_token: Option<String>,
    /// The `session-id` of each request that carried one, which each session makes for itself.
    pub sessions: Vec<String>,
    pub asked: Vec<Asked>,
}

impl State {
    /// The URL the program announced, from the test or from the file the program wrote it to.
    pub fn announced(&self) -> Option<String> {
        self.url.clone().or_else(|| {
            self.url_file
                .as_ref()
                .and_then(|file| std::fs::read_to_string(file).ok())
                .filter(|text| !text.is_empty())
        })
    }

    /// Where the session `id` comes among the sessions that have made a request, the first to appear
    /// being first: its place, or the next one where it has made none.
    fn place_of_session(&self, id: &str) -> usize {
        let mut seen: Vec<&str> = Vec::new();
        for each in &self.sessions {
            if !seen.contains(&each.as_str()) {
                seen.push(each);
            }
        }
        seen.iter()
            .position(|each| *each == id)
            .unwrap_or(seen.len())
    }

    /// One parameter of the announced URL.
    pub fn announced_parameter(&self, name: &str) -> Option<String> {
        let url = self.announced()?;
        let (_, query) = url.split_once('?')?;
        form(query)
            .into_iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    }
}

/// A stand-in for OpenAI at `base`: the token endpoint, the key set and the serving host under
/// `/v1`.
pub struct StandIn {
    pub base: String,
    pub state: Arc<Mutex<State>>,
}

impl StandIn {
    pub fn new(renewals: Vec<(&'static str, &'static str, &'static str)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let base = format!("http://{}", listener.local_addr().expect("an address"));
        let state = Arc::new(Mutex::new(State {
            url: None,
            url_file: None,
            renewals,
            refreshing: VecDeque::new(),
            serving: VecDeque::new(),
            session_scripts: Vec::new(),
            listing: VecDeque::new(),
            refusing: false,
            fault: Fault::None,
            first_lifetime: 3600,
            id_token: None,
            sessions: Vec::new(),
            asked: Vec::new(),
        }));
        std::thread::spawn({
            let state = Arc::clone(&state);
            move || {
                while let Ok((stream, _)) = listener.accept() {
                    let state = Arc::clone(&state);
                    std::thread::spawn(move || serve(stream, &state));
                }
            }
        });
        Self { base, state }
    }

    /// The state, to read or to change what the stand-in does next.
    pub fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn asked(&self) -> Vec<Asked> {
        self.state().asked.clone()
    }

    /// How many requests to the serving host each session made, for each session that made one, the
    /// first to appear first.
    pub fn requests_by_session(&self) -> Vec<(String, usize)> {
        let mut counted: Vec<(String, usize)> = Vec::new();
        for asked in self
            .asked()
            .iter()
            .filter(|asked| asked.path == "/v1/responses")
        {
            let Some(id) = asked.header("session-id") else {
                continue;
            };
            match counted.iter_mut().find(|(seen, _)| seen == id) {
                Some((_, count)) => *count += 1,
                None => counted.push((id.to_string(), 1)),
            }
        }
        counted
    }

    /// The refresh tokens the token endpoint has been given, in order.
    pub fn presented(&self) -> Vec<String> {
        self.asked()
            .iter()
            .filter_map(|asked| {
                asked
                    .form
                    .iter()
                    .find(|(name, _)| name == "refresh_token")
                    .map(|(_, value)| value.clone())
            })
            .collect()
    }

    /// Every value the run has handled so far. The ones the program under test made are read from
    /// the address it announced and from the exchange, so this is asked for after the sign-in.
    pub fn handled(&self) -> Handled {
        let state = self.state();
        let mut values: Vec<(String, String)> = [
            ("the authorization code", CODE),
            ("the client identifier", CLIENT),
            ("the first access token", ACCESS_ONE),
            ("the first refresh token", REFRESH_ONE),
            ("the second access token", ACCESS_TWO),
            ("the second refresh token", REFRESH_TWO),
            ("the subject claim", SUBJECT),
            ("the email claim", EMAIL),
            ("the text of every refusal", ECHO),
        ]
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .to_vec();
        for (name, parameter) in [
            ("the state", "state"),
            ("the nonce", "nonce"),
            ("the code challenge", "code_challenge"),
            ("the host identifier", "ext_agent_host_id"),
        ] {
            values.push((
                name.to_string(),
                state
                    .announced_parameter(parameter)
                    .unwrap_or_else(|| panic!("the sign-in announced no {parameter}")),
            ));
        }
        let verifier = state
            .asked
            .iter()
            .flat_map(|asked| asked.form.iter())
            .find(|(name, _)| name == "code_verifier")
            .map(|(_, value)| value.clone())
            .expect("the code was exchanged, so the verifier was presented");
        values.push(("the PKCE verifier".to_string(), verifier));
        if let Some(id_token) = state.id_token.clone() {
            // Each of the three dot-separated parts is a value of its own, so an output that wrote only
            // the claims, which hold the subject and the email, or only the signature, is found.
            for (name, part) in ["header", "payload", "signature"]
                .into_iter()
                .zip(id_token.split('.'))
            {
                values.push((format!("the ID token's {name}"), part.to_string()));
            }
            values.push(("the ID token".to_string(), id_token));
        }
        for (at, session) in state.sessions.iter().enumerate() {
            values.push((format!("a session identifier ({at})"), session.clone()));
        }
        // A value that was handled twice, as a session identifier is by every request, is one value.
        let mut distinct: Vec<(String, String)> = Vec::new();
        for (name, value) in values {
            if !distinct.iter().any(|(_, held)| *held == value) {
                distinct.push((name, value));
            }
        }
        let values = distinct;
        Handled {
            values,
            address: state.announced().unwrap_or_default(),
        }
    }
}

/// The values a run handled, each with what it is, and the authorization URL, which the clause lets
/// the sign-in write to the terminal once.
pub struct Handled {
    values: Vec<(String, String)>,
    address: String,
}

impl Handled {
    /// The values, each named by what it is.
    pub fn values(&self) -> &[(String, String)] {
        &self.values
    }

    /// The authorization URL.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// Whether two names are of one token and one of its parts, which are text of the whole by
    /// design.
    fn one_token(name: &str, other: &str) -> bool {
        name.starts_with("the ID token") && other.starts_with("the ID token")
    }

    /// Check that no value is empty or part of another, so a value found in an output is that value
    /// and not a part of another.
    pub fn assert_distinct(&self) {
        for (name, value) in &self.values {
            assert!(value.len() > 8, "{name} is too short to be told from text");
            for (other_name, other) in &self.values {
                assert!(
                    name == other_name
                        || Self::one_token(name, other_name)
                        || !other.contains(value.as_str()),
                    "{name} is part of {other_name}, so a hit would not say which was found"
                );
            }
        }
    }

    /// The name of every value that `text` holds, as it is written or percent-encoded, which is how
    /// an address and a form body write the ones that are not URL-safe, and the authorization URL
    /// itself, which is one of them.
    pub fn found_in(&self, text: &str) -> Vec<String> {
        let mut found: Vec<String> = self
            .values
            .iter()
            .filter(|(_, value)| {
                text.contains(value.as_str()) || text.contains(&url_encoded(value))
            })
            .map(|(name, _)| name.clone())
            .collect();
        if !self.address.is_empty() && text.contains(self.address.as_str()) {
            found.push("the authorization URL".to_string());
        }
        found
    }

    /// Fail where `text`, the `output` of the run, holds a value the run handled, and where it is
    /// empty, since an output with nothing in it holds nothing and is not one that was searched.
    ///
    /// The authorization URL is a value too: it carries the `state`, the `nonce` and the challenge, and
    /// only the sign-in's terminal may hold it, once, which is
    /// [`Handled::assert_none_in_but_one_copy_of_the_address`]. Every other output is held whole.
    pub fn assert_none_in(&self, output: &str, text: &str) {
        assert!(
            !text.is_empty(),
            "{output} is empty, so nothing was searched"
        );
        assert!(
            self.address.is_empty() || !text.contains(self.address.as_str()),
            "{output} holds the authorization URL, which only the sign-in's terminal may, once: {text}"
        );
        let found = self.found_in(text);
        assert!(
            found.is_empty(),
            "{output} holds {found:?}, which the run handled: {text}"
        );
    }

    /// [`Handled::assert_none_in`] for the terminal a sign-in wrote to, which holds the authorization
    /// URL once and no more, the one line CHATGPT-13 lets through. A terminal with no copy of the
    /// address, or with two, fails, and so does any other value in it.
    pub fn assert_none_in_but_one_copy_of_the_address(&self, output: &str, text: &str) {
        assert!(
            !self.address.is_empty(),
            "the sign-in announced no address, so there is no line to let through"
        );
        let copies = text.matches(self.address.as_str()).count();
        assert_eq!(
            copies, 1,
            "{output} holds the authorization URL {copies} times, and the sign-in writes it once: {text}"
        );
        let without = text.replacen(self.address.as_str(), "", 1);
        let found = self.found_in(&without);
        assert!(
            found.is_empty(),
            "{output} holds {found:?}, which the run handled, besides the authorization URL: {without}"
        );
    }

    /// Values and an address made by hand, for a test of what holding an output against them does.
    pub fn of(values: Vec<(String, String)>, address: &str) -> Self {
        Self {
            values,
            address: address.to_string(),
        }
    }
}

/// The values the stand-in has seen or made, which a refusal's body repeats.
fn echoed(state: &State) -> String {
    let mut values = vec![
        ECHO.to_string(),
        CODE.to_string(),
        CLIENT.to_string(),
        ACCESS_ONE.to_string(),
        REFRESH_ONE.to_string(),
        ACCESS_TWO.to_string(),
        REFRESH_TWO.to_string(),
    ];
    for parameter in ["state", "nonce", "code_challenge", "ext_agent_host_id"] {
        values.extend(state.announced_parameter(parameter));
    }
    values.extend(state.id_token.clone());
    values.extend(state.sessions.clone());
    for asked in &state.asked {
        values.extend(asked.form.iter().map(|(_, value)| value.clone()));
    }
    values.join(" ")
}

fn refusal(state: &State, status: u16) -> (u16, &'static str, String) {
    let words = echoed(state);
    let body = serde_json::json!({
        "error": {"code": "placeholder_refusal", "message": words},
        "error_description": words,
        "detail": words,
    });
    (status, "application/json", body.to_string())
}

/// A refusal that names `code` as the account's error.
fn refusal_with_code(state: &State, status: u16, code: &str) -> (u16, &'static str, String) {
    let words = echoed(state);
    let body = serde_json::json!({
        "error": {"code": code, "message": words},
        "detail": words,
    });
    (status, "application/json", body.to_string())
}

impl Serving {
    /// The reply, as the status, the kind and the body.
    fn reply(self, state: &State) -> (u16, &'static str, String) {
        match self {
            Self::Answers => (200, "text/event-stream", text_events("all done")),
            Self::Says(words) => (200, "text/event-stream", text_events(words)),
            Self::Calls(name, arguments) => {
                (200, "text/event-stream", call_events(name, arguments))
            }
            Self::RefusesTheToken => refusal(state, 401),
            Self::Refuses(status) => refusal(state, status),
            Self::RefusesWithCode(status, code) => refusal_with_code(state, status, code),
            Self::Lists(names) => (200, "application/json", listing_of(names)),
        }
    }
}

/// The listing of the serving host with a row for each of `names`: the fields the program reads and
/// no others, with the window of a model that reads images and a level.
fn listing_of(names: &[&str]) -> String {
    let rows: Vec<serde_json::Value> = names
        .iter()
        .map(|name| {
            serde_json::json!({
                "slug": name,
                "display_name": format!("Named {name}"),
                "visibility": "list",
                "supported_in_api": true,
                "context_window": 272_000,
                "input_modalities": ["text", "image"],
                "supported_reasoning_levels": [{"effort": "low"}, {"effort": "high"}],
            })
        })
        .collect();
    serde_json::json!({ "models": rows }).to_string()
}

/// Answer one request.
fn serve(stream: TcpStream, state: &Mutex<State>) {
    let Some(request) = read_request(&stream) else {
        return;
    };
    let bearer = request
        .header("authorization")
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::to_string);
    let session = request.header("session-id").map(str::to_string);
    let Request {
        method,
        path,
        headers,
        body,
    } = request;
    // Only the token endpoint takes a form. A request to the serving host carries a zstd frame.
    let form = if path == "/token" {
        form(&String::from_utf8_lossy(&body))
    } else {
        Vec::new()
    };

    let mut state = state.lock().unwrap_or_else(PoisonError::into_inner);
    let place = session.as_deref().map(|id| state.place_of_session(id));
    state.sessions.extend(session);
    state.asked.push(Asked {
        method: method.clone(),
        path: path.clone(),
        bearer: bearer.clone(),
        form: form.clone(),
        headers,
        body: Body(body),
    });
    let named = |name: &str| {
        form.iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    };
    let scripted = match (method.as_str(), path.as_str()) {
        ("GET", "/v1/models") => state.listing.pop_front(),
        ("POST", "/v1/responses") => place
            .and_then(|at| state.session_scripts.get_mut(at))
            .and_then(VecDeque::pop_front)
            .or_else(|| state.serving.pop_front()),
        _ => None,
    };
    let (status, kind, reply) = if let Some(how) = scripted {
        how.reply(&state)
    } else {
        match (method.as_str(), path.as_str()) {
            ("GET", "/jwks") if state.fault == Fault::KeySetUnavailable => refusal(&state, 503),
            ("GET", "/jwks") => (200, "application/json", key_set()),
            ("POST", "/token") if named("grant_type").as_deref() == Some("authorization_code") => {
                if state.fault == Fault::ExchangeRefused {
                    let words = echoed(&state);
                    let body = serde_json::json!({
                        "error": "invalid_grant",
                        "error_description": words,
                    });
                    (400, "application/json", body.to_string())
                } else {
                    let nonce = if state.fault == Fault::IdTokenOfAnotherSignIn {
                        "placeholder-nonce-of-another-sign-in".to_string()
                    } else {
                        state.announced_parameter("nonce").unwrap_or_default()
                    };
                    let id_token = signed(
                        &header(),
                        &serde_json::json!({
                            "iss": "https://auth.openai.com",
                            "aud": CLIENT,
                            "exp": now() + 3600,
                            "nonce": nonce,
                            "sub": SUBJECT,
                            "email": EMAIL,
                        }),
                    );
                    state.id_token = Some(id_token.clone());
                    let reply = serde_json::json!({
                        "access_token": ACCESS_ONE,
                        "refresh_token": REFRESH_ONE,
                        "id_token": id_token,
                        "expires_in": state.first_lifetime,
                        "scope": "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct",
                    });
                    (200, "application/json", reply.to_string())
                }
            }
            ("POST", "/token") => {
                let presented = named("refresh_token").unwrap_or_default();
                // The tokens it grants, or the status it refuses with.
                let outcome = match state.refreshing.pop_front() {
                    Some(Refresh::Grants(access, refresh)) => Ok((access, refresh)),
                    Some(Refresh::Busy) => Err(500),
                    Some(Refresh::Ended) => Err(400),
                    None => state
                        .renewals
                        .iter()
                        .find(|(refresh, _, _)| *refresh == presented)
                        .map(|(_, access, refresh)| (*access, *refresh))
                        .ok_or(400),
                };
                match outcome {
                    Ok((access, refresh)) => (
                        200,
                        "application/json",
                        serde_json::json!({
                            "access_token": access,
                            "refresh_token": refresh,
                            "expires_in": 3600,
                        })
                        .to_string(),
                    ),
                    Err(status) => {
                        let words = echoed(&state);
                        let body = serde_json::json!({
                            "error": if status == 500 { "server_error" } else { "invalid_grant" },
                            "error_description": words,
                        });
                        (status, "application/json", body.to_string())
                    }
                }
            }
            _ if state.refusing || bearer.as_deref() == Some(ACCESS_ONE) => refusal(&state, 401),
            ("GET", "/v1/models") => refusal(&state, 500),
            ("POST", "/v1/responses") => Serving::Answers.reply(&state),
            _ => refusal(&state, 404),
        }
    };
    drop(state);
    respond(&stream, status, kind, &reply);
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock after 1970")
        .as_secs()
}

/// A form or a query, decoded.
pub fn form(text: &str) -> Vec<(String, String)> {
    text.split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(name, value)| (name.to_string(), decoded(value)))
        .collect()
}

fn decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        match (bytes[at], bytes.get(at + 1..at + 3)) {
            (b'%', Some(pair)) => {
                let digits = std::str::from_utf8(pair).expect("ascii");
                out.push(u8::from_str_radix(digits, 16).expect("two hexadecimal digits"));
                at += 3;
            }
            (byte, _) => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8(out).expect("text")
}

pub fn url_encoded(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

/// What the person's browser does after the sign-in page: it returns to the listener on the port
/// the authorization URL names, with the code and the client OpenAI issued and the `state` the URL
/// carried. It runs on a thread of its own, since the sign-in is waiting for it.
pub fn browse(url: &str) {
    let (_, query) = url.split_once('?').expect("a query");
    let query = form(query);
    let value = |name: &str| {
        query
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
            .unwrap_or_else(|| panic!("no {name} in {url}"))
    };
    let port: u16 = value("redirect_uri")
        .trim_start_matches("http://127.0.0.1:")
        .trim_end_matches("/auth/callback")
        .parse()
        .expect("the listener's port");
    let target = format!(
        "/auth/callback?state={}&code={CODE}&client_id={CLIENT}",
        url_encoded(&value("state"))
    );
    std::thread::spawn(move || {
        let mut browser = TcpStream::connect(("127.0.0.1", port)).expect("the listener");
        browser
            .write_all(format!("GET {target} HTTP/1.1\r\nhost: x\r\n\r\n").as_bytes())
            .expect("the callback");
        let mut page = String::new();
        let _ = browser.read_to_string(&mut page);
    });
}

/// Every file under `directory`, with its bytes.
pub fn files_under(directory: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut found = Vec::new();
    let mut waiting = vec![directory.to_path_buf()];
    while let Some(next) = waiting.pop() {
        let Ok(entries) = std::fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => waiting.push(path),
                Ok(kind) if kind.is_file() => {
                    if let Ok(bytes) = std::fs::read(&path) {
                        found.push((path, bytes));
                    }
                }
                _ => {}
            }
        }
    }
    found.sort();
    found
}

pub fn holds(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

/// Run the test `test_path` of this binary again in a process of its own, with `environment`, and
/// return what it said on its terminal.
///
/// For a test that has to name the account's hosts, which the program reads from its environment, and
/// to set the state directory, where the code it reaches is a private function of a crate and only a
/// unit test can call it. The unit tests of a crate share one process, so that test does its work in
/// a child that runs nothing else, and the test that starts it holds what the child wrote.
#[allow(
    clippy::disallowed_methods,
    reason = "the test starts the binary it is already running"
)]
pub fn in_a_process_of_its_own(
    test_path: &str,
    environment: &[(&str, &std::ffi::OsStr)],
) -> Output {
    // The program started is this test binary, the one that is already running and is being tested,
    // so nothing here is decided by a path that anything else could have put in its place.
    // nosemgrep: rust.lang.security.current-exe.current-exe
    let running = std::env::current_exe().expect("the test binary");
    let mut child = std::process::Command::new(running);
    child
        .args([
            "--ignored",
            "--exact",
            "--nocapture",
            "--test-threads=1",
            test_path,
        ])
        .env_clear()
        .stdin(std::process::Stdio::null());
    // The program needs nothing of the person's environment, and what it inherited could name a
    // state directory, a home or a host.
    for name in ["PATH", "TMPDIR"] {
        if let Some(value) = std::env::var_os(name) {
            child.env(name, value);
        }
    }
    for (name, value) in environment {
        child.env(name, value);
    }
    child.output().expect("the child runs")
}
