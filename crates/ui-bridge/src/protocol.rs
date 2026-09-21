//! The shapes a front-end and this library exchange.
//!
//! Three of them, distinguished by which keys are present: a **request** carries an `id`
//! and a `method`, a **response** carries the same `id` and exactly one of `ok` or
//! `error`, and an **event** carries neither and arrives unasked.
//!
//! Nothing here does I/O. A transport reads a line and hands it to [`Request::parse`];
//! what comes back is a value it writes. That is the whole of the coupling, and it is why
//! the transport can be replaced without touching anything above it.

use serde_json::{Value, json};

/// Why a request could not be served.
///
/// A closed set, because a front-end switches on these and a free-text reason is not
/// something it can act on. The message beside it is for a person reading a log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// Malformed envelope, unknown method, missing or ill-typed parameters.
    BadRequest,
    /// Unknown session handle.
    NoSuchSession,
    /// Unknown or already-answered confirmation. An approval is single-use.
    NoSuchRequest,
    /// A turn is already running on that session.
    TurnInFlight,
    /// The path given to `session.new` is not a directory.
    NotADirectory,
    /// `~/.bravebot` could not be located or created.
    NoHome,
    /// The agent's configuration could not be read.
    Config,
    /// A bug here. The message is for a report, not for a user.
    Internal,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BadRequest => "bad_request",
            Self::NoSuchSession => "no_such_session",
            Self::NoSuchRequest => "no_such_request",
            Self::TurnInFlight => "turn_in_flight",
            Self::NotADirectory => "not_a_directory",
            Self::NoHome => "no_home",
            Self::Config => "config",
            Self::Internal => "internal",
        }
    }
}

/// A request that could not be served, with something to say about it.
#[derive(Debug, Clone)]
pub struct Failure {
    pub code: ErrorCode,
    pub message: String,
}

impl Failure {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::BadRequest, message)
    }

    pub fn no_such_session() -> Self {
        Self::new(ErrorCode::NoSuchSession, "unknown session handle")
    }
}

/// What a front-end asked for.
#[derive(Debug, Clone)]
pub struct Request {
    pub id: u64,
    pub method: String,
    pub params: Value,
}

/// A line that could not be read as a request.
///
/// Split by whether an `id` survived, because that decides whether anyone can be told.
/// A request whose id is unreadable has nobody to answer: the front-end is waiting on a
/// number we do not know.
#[derive(Debug, Clone)]
pub enum Unreadable {
    /// The id came through, so a failure can be addressed to it.
    Answerable { id: u64, failure: Failure },
    /// Nothing usable. All the transport can do is log it.
    Unanswerable { detail: String },
}

impl Request {
    /// Read one line.
    ///
    /// Every failure is described rather than thrown away, because a front-end that can
    /// produce one bad line will produce another and silence is the hardest version of
    /// that to debug.
    pub fn parse(line: &str) -> Result<Self, Unreadable> {
        let value: Value = serde_json::from_str(line).map_err(|error| Unreadable::Unanswerable {
            detail: format!("not JSON: {error}"),
        })?;

        let Some(id) = value.get("id").and_then(Value::as_u64) else {
            return Err(Unreadable::Unanswerable {
                detail: "no numeric `id`, so there is nobody to answer".into(),
            });
        };

        let Some(method) = value.get("method").and_then(Value::as_str) else {
            return Err(Unreadable::Answerable {
                id,
                failure: Failure::bad_request("no `method`"),
            });
        };

        Ok(Self {
            id,
            method: method.to_string(),
            // An absent `params` is an empty object rather than an error: several methods
            // take nothing, and making them send `{}` is ceremony.
            params: value.get("params").cloned().unwrap_or_else(|| json!({})),
        })
    }

    /// A required string parameter.
    pub fn string(&self, name: &str) -> Result<String, Failure> {
        self.params
            .get(name)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| Failure::bad_request(format!("`{name}` must be a string")))
    }

    /// A required unsigned parameter.
    /// One parameter, or `Null` when it is absent.
    ///
    /// No error case, because every caller is a decision reader and those are total by
    /// design: an absent answer and an unreadable one are both refusals, and `Null` is how
    /// they arrive at the same place. See [`crate::wire::decision`].
    pub fn param(&self, name: &str) -> &Value {
        self.params.get(name).unwrap_or(&Value::Null)
    }

    pub fn number(&self, name: &str) -> Result<u64, Failure> {
        self.params
            .get(name)
            .and_then(Value::as_u64)
            .ok_or_else(|| Failure::bad_request(format!("`{name}` must be a number")))
    }

    /// An optional string parameter. Absent and null are the same thing.
    pub fn optional_string(&self, name: &str) -> Option<String> {
        self.params.get(name).and_then(Value::as_str).map(str::to_string)
    }

    /// An optional boolean parameter, and what it means when it is not there.
    ///
    /// Anything that is not a boolean reads as the default rather than as a failure. That is the
    /// right shape for a flag whose whole purpose is that older callers, and callers that do not
    /// care, get the behaviour they had before it existed — a front-end that has never heard of
    /// the flag must not be able to trip over it by sending nothing.
    pub fn flag(&self, name: &str, default: bool) -> bool {
        self.params.get(name).and_then(Value::as_bool).unwrap_or(default)
    }
}

/// The answer to one request.
pub fn response(id: u64, outcome: Result<Value, Failure>) -> Value {
    match outcome {
        Ok(ok) => json!({ "id": id, "ok": ok }),
        Err(failure) => json!({
            "id": id,
            "error": { "code": failure.code.as_str(), "message": failure.message },
        }),
    }
}

/// Something that happened, addressed to nobody in particular.
///
/// `session` is absent only for `agent.ready`, which is emitted before any session exists.
#[derive(Debug, Clone)]
pub struct Event {
    pub name: &'static str,
    pub session: Option<String>,
    pub data: Value,
}

impl Event {
    pub fn new(name: &'static str, session: impl Into<String>, data: Value) -> Self {
        Self { name, session: Some(session.into()), data }
    }

    /// An event that belongs to no session.
    pub fn global(name: &'static str, data: Value) -> Self {
        Self { name, session: None, data }
    }

    pub fn to_value(&self) -> Value {
        match &self.session {
            Some(session) => json!({ "event": self.name, "session": session, "data": self.data }),
            None => json!({ "event": self.name, "data": self.data }),
        }
    }
}
