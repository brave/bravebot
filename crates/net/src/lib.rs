//! The single network egress path.
//!
//! Every outbound request carrying labelled content goes through [`Egress::fetch`], and the HTTP
//! client behind it is private to this module, so nothing outside it can use that client to reach
//! the network without passing the policy gate. That is deliberate: in the design this replaces,
//! two of three fetchers bypassed the redirect check because using the hardened helper was
//! optional.
//!
//! One other crate opens a socket, and it is a recorded exception rather than an oversight.
//! `bravebot-skus` builds its own client for the subscription service, over the transport this
//! module resolved, and carries a credential and an order id rather than workspace content or
//! model output. What that costs, including the revalidation below, which it does not get, is
//! under `## Known costs` in `docs/specs/network-egress.md`. `make check-security` holds every
//! manifest under `crates/` to that list, so a third client fails a build rather than arriving
//! with a paragraph here saying it cannot exist.
//!
//! Redirects are followed manually and **revalidated on every hop**. Otherwise a
//! permitted host could redirect to a denied one and the gate would only ever have
//! seen the first URL.
//!
//! Timeouts bound each phase of a request separately rather than the call as a whole, so a
//! reply that takes a long time to write is not confused with a connection that has died. See
//! [`Timeouts`].
//!
//! Responses are size-capped and content-type filtered. That is resource hygiene, not
//! content inspection: the bytes are never parsed to decide anything, they are handed
//! back for the caller to label.

#![forbid(unsafe_code)]

mod address;
pub mod transport;

pub use transport::{Transport, TrustError, TrustRoots};

use bravebot_core::cancel::Cancel;
use bravebot_core::event::Sink;
use bravebot_core::label::Label;
use bravebot_core::policy::{Denial, Policy};
use bravebot_core::value::Labelled;
use std::fmt;
use std::time::Duration;

/// Response bodies are truncated past this size.
pub const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

/// Redirect hops followed before giving up.
pub const MAX_REDIRECTS: usize = 5;

/// How long each part of a request may take.
///
/// Deliberately not one end-to-end bound. A single global timeout counts the time the model
/// spends writing its reply, so a long answer is indistinguishable from a stalled connection and
/// is cut off for being long. Split up, the two questions can be asked separately: a reply is
/// allowed to take a while, and a connection that has stopped delivering it is not.
#[derive(Debug, Clone, Copy)]
pub struct Timeouts {
    /// Resolving the host name.
    ///
    /// The transport bounds connecting by this as well, so what it is given is this or
    /// [`Timeouts::connect`], whichever is larger, and a lookup is allowed that long. A lookup
    /// bound under the connection bound would be ending connections rather than lookups.
    pub resolve: Duration,
    /// Opening the socket, including the TLS handshake, from the name having resolved.
    pub connect: Duration,
    /// Sending the request, from the connection having opened.
    ///
    /// A request body is bounded by this and [`Timeouts::reply`] together rather than by this
    /// alone. The transport carries a send bound into the wait for the reply, so a bound tight
    /// enough to time the body precisely would cut the reply short as well, and the reply is the
    /// one that must not be cut short.
    pub send: Duration,
    /// The reply: the wait for it to begin, and then the whole of it.
    ///
    /// Generous, because this is where a model thinking and then writing a long answer spends
    /// its time, and none of that is a fault. It is the only bound on the wait before the reply
    /// starts, since nothing has arrived yet for a gap to be measured between, and it bounds the
    /// body again from the moment the headers arrive rather than counting the two together.
    ///
    /// A request that knows how long its reply can run states its own in place of this one
    /// ([`Request::reply_within`], [`Request::stream_within`]), since no one figure fits a reply
    /// of every length.
    pub reply: Duration,
    /// The longest gap between two pieces of a reply that is still arriving.
    ///
    /// This is the one that catches a dead connection, which is what a closed laptop lid leaves
    /// behind: the socket is gone and nothing on it says so, so a read would otherwise wait for
    /// bytes that are never coming.
    pub idle: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            resolve: Duration::from_secs(15),
            connect: Duration::from_secs(30),
            send: Duration::from_secs(60),
            reply: Duration::from_secs(600),
            idle: Duration::from_secs(120),
        }
    }
}

#[derive(Debug)]
pub enum EgressError {
    /// The policy refused this request.
    Denied(Denial),
    /// A redirect chain exceeded [`MAX_REDIRECTS`].
    TooManyRedirects { url: String },
    /// A redirect response carried no usable target.
    MissingLocation { url: String },
    /// The URL could not be parsed, or was not http(s).
    InvalidUrl { url: String, detail: String },
    /// A redirect would have continued an https chain over cleartext http.
    InsecureRedirect { url: String },
    /// The host resolved to an address that is not public: this machine, a private network, a link
    /// local address or a metadata service.
    ///
    /// Carries the URL and nothing of the address. Where it was found is a fact about the network
    /// the answer came from, and the request was never sent.
    AddressRefused { url: String },
    /// Transport failure.
    Transport {
        url: String,
        detail: String,
        /// Whether sending the same request again could get past it.
        ///
        /// Decided from what the transport reported, never from anything a server sent: a
        /// timeout, a reset, or a name that did not resolve are facts about the connection.
        transient: bool,
    },
    /// The reply was still arriving when the time it was given ran out.
    ///
    /// Not a transport failure, because the two are answered differently: a connection that went
    /// quiet may be dead and another attempt may get past it, where a reply cut at its deadline was
    /// being written, and another attempt writes it as long and is billed for it again. Decided
    /// from which of the transport's bounds ended the read, never from anything a server sent.
    OutOfTime { url: String },
    /// The server returned a non-success status.
    Status { url: String, status: u16 },
    /// The caller asked to stop while the request was still being waited on.
    Stopped { url: String },
}

impl fmt::Display for EgressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Denied(d) => write!(f, "{d}"),
            Self::TooManyRedirects { url } => {
                write!(f, "too many redirects starting from {url}")
            }
            Self::MissingLocation { url } => {
                write!(f, "{url} returned a redirect with no location")
            }
            Self::InvalidUrl { url, detail } => write!(f, "invalid url {url}: {detail}"),
            Self::InsecureRedirect { url } => {
                write!(f, "{url} was redirected out of https to cleartext http")
            }
            Self::AddressRefused { url } => {
                write!(
                    f,
                    "{url} was not fetched: its host is not at a public address"
                )
            }
            Self::Transport { url, detail, .. } => {
                write!(f, "request to {url} failed: {detail}")
            }
            Self::OutOfTime { url } => {
                write!(f, "the reply from {url} ran past the time it was given")
            }
            Self::Status { url, status } => write!(f, "{url} returned HTTP {status}"),
            Self::Stopped { url } => write!(f, "the request to {url} was stopped"),
        }
    }
}

impl EgressError {
    /// Whether sending the same request again is worth doing.
    ///
    /// A connection that died and an overloaded server are both temporary, and a request that
    /// never arrived has changed nothing, so sending it again is the same request rather than a
    /// second one. Everything else is a decision that will be made the same way twice.
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Transport { transient, .. } => *transient,
            Self::Status { status, .. } => RETRYABLE_STATUSES.contains(status),
            Self::Denied(_)
            | Self::TooManyRedirects { .. }
            | Self::MissingLocation { .. }
            | Self::InvalidUrl { .. }
            // The same chain answers the same way, so another attempt is the same downgrade.
            | Self::InsecureRedirect { .. }
            // A name answers where it pointed a moment ago, and a person is not told to try again
            // at something that was refused for where it is.
            | Self::AddressRefused { .. }
            // The same reply takes as long again and is billed again.
            | Self::OutOfTime { .. }
            // The one error that says the reply is not wanted. Sending it again would be
            // answering a request somebody withdrew.
            | Self::Stopped { .. } => false,
        }
    }

    /// The same failure, reported as a failure of `url` and carrying nothing of where it
    /// actually happened.
    ///
    /// Everything a failure says about a place comes from the URL it happened on, down to the
    /// transport quoting back the string it was handed. Past the first hop that string is one a
    /// server wrote into a `Location` header, and an error's text is the part of a failure a
    /// caller formats into whatever it is building, including a message the planner reads. So a
    /// redirect's URL is not carried out of the crate that followed it: the caller is told which
    /// of its own requests failed and how, which is the whole of what it asked.
    fn into_a_failure_of(self, url: &str) -> Self {
        let url = url.to_string();
        match self {
            // The policy's own words about a gate it applied, naming what a person approved.
            Self::Denied(denial) => Self::Denied(denial),
            Self::TooManyRedirects { .. } => Self::TooManyRedirects { url },
            Self::MissingLocation { .. } => Self::MissingLocation { url },
            Self::Stopped { .. } => Self::Stopped { url },
            Self::OutOfTime { .. } => Self::OutOfTime { url },
            Self::Status { status, .. } => Self::Status { url, status },
            // The detail here is this crate's own sentence about a shape, so it survives.
            Self::InvalidUrl { detail, .. } => Self::InvalidUrl { url, detail },
            Self::InsecureRedirect { .. } => Self::InsecureRedirect { url },
            Self::AddressRefused { .. } => Self::AddressRefused { url },
            // The detail here is the transport's, and a transport reports a URL it could not use
            // by quoting it: `ureq::Error::BadUri` is "bad uri: <the whole string>". Keeping the
            // ones that do not quote it would mean reading the detail to decide, which is a
            // branch on a string a server chose. So the cost is paid instead: a timeout or a
            // refused connection on a hop reads as this sentence, and `transient` still carries
            // the part of it anything decides on.
            Self::Transport { transient, .. } => Self::Transport {
                url,
                detail: "a request after a redirect did not complete".to_string(),
                transient,
            },
        }
    }
}

/// Statuses a server uses to say "not now" rather than "no".
///
/// 408 and 504 are timeouts, 429 is a rate limit, and 500, 502 and 503 are a server that is
/// unwell rather than a request that is wrong.
const RETRYABLE_STATUSES: [u16; 6] = [408, 429, 500, 502, 503, 504];

impl std::error::Error for EgressError {}

impl From<Denial> for EgressError {
    fn from(value: Denial) -> Self {
        Self::Denied(value)
    }
}

/// A fetched response. The body is labelled, so a caller receives untrusted bytes it
/// cannot inspect without going through the policy.
///
/// Nothing here names where a redirect chain ended. Past the first hop the URL a request is on is
/// a string a server wrote into a `Location` header, and a caller able to read it could put it in
/// a result the planner is sent as the driver's own words, so that detail stops in this crate.
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub content_type: Option<String>,
    pub body: Labelled<Vec<u8>>,
    /// Whether the body hit [`MAX_RESPONSE_BYTES`].
    pub truncated: bool,
}

/// A response whose body is read in pieces as it arrives.
///
/// Same gates as [`Response`], and the same cap: what changes is when the caller sees the bytes,
/// not whether anything checked them. The label is fixed before the first byte is read, so a
/// stream cannot acquire a better one partway through.
pub struct Streamed<'r> {
    pub status: u16,
    pub content_type: Option<String>,
    /// The URL the caller asked for, which is the only URL this hands back: it is what a failure
    /// part-way through the body names, and where a redirect chain ended stays in this crate.
    requested: String,
    label: Label,
    /// `Send`, so a caller can read the body on a thread it is able to walk away from.
    ///
    /// Reading is the one part of a request that blocks for as long as the other end is quiet,
    /// and there is no way to interrupt a read in progress. A caller that has to answer a person
    /// promptly moves the reading somewhere it can abandon; nothing about a read needs to happen
    /// on any particular thread, since it applies nothing and decides nothing.
    reader: Box<dyn std::io::Read + Send + 'r>,
    read: usize,
    truncated: bool,
    /// The most of the body this will hand back, [`MAX_RESPONSE_BYTES`] unless the caller moved it.
    cap: usize,
    /// How much is asked of the reader at a time.
    chunk: usize,
}

impl Streamed<'_> {
    /// Raise or lower the cap this body is read under, for a caller that saves it to a file
    /// instead of holding it in a conversation.
    ///
    /// Only the size changes. The label was fixed before the first byte and nothing here can
    /// change it, and the cap is still enforced across the whole stream. Reads are larger too,
    /// since a body worth raising the cap for is not one that has to look like it is being typed.
    #[must_use]
    pub fn capped_at(mut self, cap: usize) -> Self {
        self.cap = cap;
        self.chunk = DOWNLOAD_CHUNK_BYTES;
        self
    }

    /// The label every piece of this body carries.
    pub fn label(&self) -> Label {
        self.label
    }

    /// Read the next piece, or `None` at the end of the body.
    ///
    /// Each piece comes back labelled, exactly as a whole body would: a caller that wants to look
    /// at one still has to go through the policy. The cap is enforced across the whole stream, so
    /// an endless response is cut off rather than read forever.
    ///
    /// A byte past the cap is read but never handed back, the same way the buffered path does it:
    /// reaching the cap and being cut by it are different outcomes, and a body that ends exactly
    /// at the cap was not cut.
    pub fn next_chunk(&mut self) -> Result<Option<Labelled<Vec<u8>>>, EgressError> {
        if self.truncated {
            return Ok(None);
        }

        let remaining = self.cap + 1 - self.read;
        let mut buffer = vec![0u8; self.chunk.min(remaining)];
        match self.reader.read(&mut buffer) {
            Ok(0) => Ok(None),
            Ok(n) => {
                self.read += n;
                buffer.truncate(n);
                if self.read > self.cap {
                    self.truncated = true;
                    buffer.truncate(n - (self.read - self.cap));
                    if buffer.is_empty() {
                        return Ok(None);
                    }
                }
                Ok(Some(Labelled::new(buffer, self.label)))
            }
            Err(e) => Err(body_failure(&self.requested, e)),
        }
    }

    /// Whether the cap stopped the read before the body ended.
    pub fn truncated(&self) -> bool {
        self.truncated
    }
}

impl fmt::Debug for Streamed<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // No body: it has not all arrived, and printing what has would expose labelled bytes.
        f.debug_struct("Streamed")
            .field("status", &self.status)
            .field("url", &self.requested)
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

/// How much is read from a streaming body at a time.
///
/// Small enough that a reply appears to arrive as it is written rather than in visible jumps.
const STREAM_CHUNK_BYTES: usize = 1024;

/// How much is read at a time from a body that is going to a file.
const DOWNLOAD_CHUNK_BYTES: usize = 64 * 1024;

/// A request to send.
#[derive(Debug, Clone)]
pub struct Request {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    /// How long the reply may take, where the request says. Otherwise [`Timeouts::reply`].
    pub reply: Option<ReplyBound>,
    /// Whether a reply from this machine is waited on for as long as it takes. See
    /// [`Request::patient_on_this_machine`].
    pub patient_on_this_machine: bool,
}

/// How long a reply may take, stated by the request that asks for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyBound {
    /// The wait for the reply to begin, and then the whole of it: a reply written in full before
    /// any of it is sent.
    Whole(Duration),
    /// The reply from the moment it begins: a stream, whose first bytes are sent at once. The wait
    /// for them stays [`Timeouts::reply`], so a server that never answers is given up on no later
    /// than any other.
    Begun(Duration),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

impl Request {
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            method: Method::Get,
            url: url.into(),
            headers: Vec::new(),
            body: None,
            reply: None,
            patient_on_this_machine: false,
        }
    }

    pub fn post(url: impl Into<String>, body: Vec<u8>) -> Self {
        Self {
            method: Method::Post,
            url: url.into(),
            headers: Vec::new(),
            body: Some(body),
            reply: None,
            patient_on_this_machine: false,
        }
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// Give the reply `bound` rather than [`Timeouts::reply`], and no longer.
    ///
    /// For a caller that knows how long its reply can run. The gap allowed between two pieces of
    /// it stays [`Timeouts::idle`], because what that bound catches is the same connection
    /// whatever was asked for.
    pub fn reply_within(mut self, bound: Duration) -> Self {
        self.reply = Some(ReplyBound::Whole(bound));
        self
    }

    /// As [`Request::reply_within`], for a reply that begins at once and may then run for `bound`.
    pub fn stream_within(mut self, bound: Duration) -> Self {
        self.reply = Some(ReplyBound::Begun(bound));
        self
    }

    /// Where the URL names this machine and no proxy carries it, wait on the reply for as long as
    /// it takes: no bound on its start, its length, or the gaps in it. Resolving and connecting
    /// keep theirs, and so does every hop past a redirect.
    ///
    /// Honoured only by [`Egress::fetch_streaming`] given a [`Cancel`], since a server here that
    /// never answers is then waited on until somebody stops it. The caller reading the stream has
    /// to look at the token between pieces for that to be true.
    pub fn patient_on_this_machine(mut self) -> Self {
        self.patient_on_this_machine = true;
        self
    }
}

/// The one way out of the process.
pub struct Egress {
    agent: ureq::Agent,
    /// The same configuration, resolving through [`address::GuardedResolver`]. A fetch in flight is
    /// sent with this one and everything else with `agent`, so a local model endpoint on loopback
    /// is untouched.
    guarded: ureq::Agent,
    /// What the agent was configured with, so a request stating its own reply bound can have the
    /// phases that carry it worked out again the same way.
    timeouts: Timeouts,
}

impl Default for Egress {
    fn default() -> Self {
        Self::new()
    }
}

impl Egress {
    pub fn new() -> Self {
        Self::with_timeouts(Timeouts::default())
    }

    /// As [`Egress::new`], with different bounds on how long a request may take.
    ///
    /// Exists so the bounds can be exercised in a test at a scale a test can wait for, and for the
    /// one request a start waits on before it has said anything: asking a local Ollama what it
    /// serves (`bravebot_aichat::ollama`). Every other request takes the defaults.
    pub fn with_timeouts(timeouts: Timeouts) -> Self {
        Self::with_transport(timeouts, Transport::shared())
    }

    /// As [`Egress::with_timeouts`], against trust roots and a proxy the caller states.
    ///
    /// Private, because the product has one answer to both and it is the environment's. A test
    /// states them so that what the environment resolves to can be pinned without setting a
    /// variable for every other test in the process.
    fn with_transport(timeouts: Timeouts, transport: &Transport) -> Self {
        // ureq gives a phase the earliest of its own deadline and the deadlines of the phases
        // before it (`Timeout::preceeding`, ureq 3.4 src/timings.rs), which its configuration
        // does not say. A number handed over here therefore bounds the phase it names *and* the
        // phases after it, so each one below covers every phase whose deadline it sets. Passing
        // `send` straight to the send phases instead would bound the wait for the reply by
        // `send`, and an endpoint that took longer than that to start answering would be
        // reported as a failed send and the request sent again.
        let config = transport
            .agent_config_builder()
            // Redirects are handled here so each hop can be revalidated; letting the
            // client follow them silently would defeat the gate.
            .max_redirects(0)
            // A status is a reply, not a failure to get one. Left as ureq has it, every non-2xx
            // comes back as a transport error carrying the status in its text, so the status
            // check below never runs and nothing downstream can tell 403 from a dead socket:
            // a refused credential loses the sign-in that fixes it, and a 429 stops counting as
            // worth another attempt.
            .http_status_as_error(false)
            // Bounds connecting too, from the moment the name resolved.
            .timeout_resolve(Some(timeouts.resolve.max(timeouts.connect)))
            // Bounds the request going out too, from the moment the connection opened.
            .timeout_connect(Some(timeouts.connect.max(timeouts.send)))
            // Bounds the request body and then the wait for the reply too, both from the
            // moment the request headers went out.
            .timeout_send_request(Some(timeouts.send + timeouts.reply))
            // Bounds the wait for the reply too, from the moment the body went out, which is
            // the moment that wait begins.
            .timeout_send_body(Some(timeouts.reply))
            // ureq keeps checking this one while the body arrives, so it bounds the whole
            // reply rather than only its headers.
            .timeout_recv_response(Some(timeouts.reply))
            // Recomputed on every read, which is what makes it a gap rather than a total.
            .timeout_recv_body(Some(timeouts.idle))
            .build();
        let guarded = ureq::Agent::with_parts(
            config.clone(),
            ureq::unversioned::transport::DefaultConnector::default(),
            address::GuardedResolver::new(),
        );
        Self {
            agent: config.into(),
            guarded,
            timeouts,
        }
    }

    /// As built, with the lookup a fetch in flight makes replaced, so a test can state what a name
    /// resolves to without a name server.
    #[cfg(test)]
    fn guarded_by(mut self, resolver: address::GuardedResolver) -> Self {
        self.guarded = ureq::Agent::with_parts(
            self.agent.config().clone(),
            ureq::unversioned::transport::DefaultConnector::default(),
            resolver,
        );
        self
    }

    /// The agent for one request: the guarded one for a fetch in flight that no proxy stands in
    /// front of.
    ///
    /// A proxy resolves the target itself, and ureq resolves the proxy's own host through the
    /// resolver instead, so guarding that lookup would refuse a proxy on this machine and check
    /// nothing about the target. That cost is recorded in `docs/specs/tools/fetch-url.md`.
    fn agent_for(&self, fetching: bool, url: &str) -> &ureq::Agent {
        if fetching && !self.proxied(url) {
            &self.guarded
        } else {
            &self.agent
        }
    }

    /// Whether a proxy stands between this machine and `url`'s host.
    fn proxied(&self, url: &str) -> bool {
        let Ok(uri) = url.parse::<ureq::http::Uri>() else {
            return false;
        };
        self.agent
            .config()
            .proxy()
            .is_some_and(|proxy| !proxy.is_no_proxy(&uri))
    }

    /// Send a request, checking the policy before the initial URL and before every
    /// redirect hop.
    ///
    /// `label` is the label the response body carries. It comes from the caller's
    /// capability, not from anything the server says.
    pub fn fetch<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        request: Request,
        label: Label,
    ) -> Result<Response, EgressError> {
        self.fetch_watching(policy, request, label, None)
    }

    /// As [`Egress::fetch`], but abandonable while it waits.
    ///
    /// For a fetch a person is waiting on inside a turn they can stop. Every gate is the same one
    /// at the same point: the token decides how long this waits, never where the request may go or
    /// what its body is labelled.
    pub fn fetch_watching<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        request: Request,
        label: Label,
        cancel: Option<&Cancel>,
    ) -> Result<Response, EgressError> {
        // Never stoppable: the body is read to its end below, where no token is looked at.
        let (status, content_type, reader) = self.fetch_checked(policy, &request, cancel, false)?;
        // The URL the caller asked for, not the one the body is arriving from: a redirect chain
        // ends somewhere a server chose, and this failure is reported to whoever asked.
        let (body, truncated) = read_capped(reader).map_err(|e| body_failure(&request.url, e))?;

        Ok(Response {
            status,
            content_type,
            body: Labelled::new(body, label),
            truncated,
        })
    }

    /// As [`Egress::fetch`], but handing back the body to read as it arrives.
    ///
    /// Every gate is the same and runs at the same point: the policy is checked before the initial
    /// URL and before every redirect hop, before any body exists. What differs is only that the
    /// caller reads the body in pieces, so a long reply can be shown while it is still being
    /// written. The label is fixed here, from the caller's capability, so no piece of the stream
    /// can arrive better labelled than the whole would have been.
    pub fn fetch_streaming<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        request: Request,
        label: Label,
        cancel: Option<&Cancel>,
    ) -> Result<Streamed<'static>, EgressError> {
        let (status, content_type, reader) =
            self.fetch_checked(policy, &request, cancel, cancel.is_some())?;

        Ok(Streamed {
            status,
            content_type,
            requested: request.url.clone(),
            label,
            reader,
            read: 0,
            truncated: false,
            cap: MAX_RESPONSE_BYTES,
            chunk: STREAM_CHUNK_BYTES,
        })
    }

    /// Send, following and revalidating redirects, and return the body reader unread.
    ///
    /// The single place the gate is applied, so a streamed request cannot take a different path
    /// through the checks than a buffered one.
    ///
    /// Also the single place a failure past the first hop is reported from. Once a redirect has
    /// been followed, every URL the loop below holds is one a server wrote into a `Location`
    /// header, so each of the errors it can build names a place the caller never asked about.
    /// Rewriting them here rather than at each construction is what keeps that true: a new arm in
    /// there cannot forget a discipline it does not have to apply.
    #[allow(clippy::type_complexity)]
    fn fetch_checked<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        request: &Request,
        cancel: Option<&Cancel>,
        stoppable: bool,
    ) -> Result<(u16, Option<String>, Box<dyn std::io::Read + Send>), EgressError> {
        let started = std::time::Instant::now();
        let mut redirected = false;
        let outcome = match self.follow(policy, request, cancel, stoppable, &mut redirected) {
            Err(error) if redirected => Err(error.into_a_failure_of(&request.url)),
            outcome => outcome,
        };
        if let Err(error) = &outcome {
            log_failure(&request.url, error, started.elapsed());
        }
        outcome
    }

    /// Whether a connection to `url` would end on this machine: its host is one, and no proxy
    /// stands between.
    fn reaches_here(&self, url: &str) -> bool {
        let Ok(uri) = url.parse::<ureq::http::Uri>() else {
            return false;
        };
        !self.proxied(url) && uri.host().is_some_and(names_this_machine)
    }

    /// The redirect loop itself: send, revalidate, follow, and hand back the body reader unread.
    #[allow(clippy::type_complexity)]
    fn follow<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        request: &Request,
        cancel: Option<&Cancel>,
        stoppable: bool,
        redirected: &mut bool,
    ) -> Result<(u16, Option<String>, Box<dyn std::io::Read + Send>), EgressError> {
        let mut url = request.url.clone();
        let mut hops = 0;
        let fetching = policy.fetch_in_flight();

        loop {
            require_http_scheme(&url)?;
            policy.before_network(&url)?;

            // The caller's URL only. A hop past it is somewhere a server named, and how long to
            // wait on it is not the server's to lengthen. And only a caller that can walk away from
            // the wait and the body both, since with no bound nothing else ends a server that hung.
            let patient = request.patient_on_this_machine
                && stoppable
                && hops == 0
                && self.reaches_here(&url);
            let response = match cancel {
                Some(cancel) => self.send_watching(request, &url, patient, fetching, cancel)?,
                None => send(
                    self.agent_for(fetching, &url),
                    self.timeouts,
                    request,
                    &url,
                    patient,
                )?,
            };
            let status = response.0;

            if is_redirect(status) {
                if hops >= MAX_REDIRECTS {
                    return Err(EgressError::TooManyRedirects {
                        url: request.url.clone(),
                    });
                }
                let location = response
                    .1
                    .ok_or_else(|| EgressError::MissingLocation { url: url.clone() })?;
                // Resolved against the current URL so a relative Location is checked
                // as the absolute URL it will actually resolve to.
                url = resolve(&url, &location)?;
                hops += 1;
                *redirected = true;
                continue;
            }

            if !(200..300).contains(&status) {
                return Err(EgressError::Status { url, status });
            }

            return Ok((status, response.2, response.3));
        }
    }

    /// One hop, sent on a thread this one can walk away from when the caller says to stop.
    ///
    /// The wait before a reply begins is the longest one in a turn and the least interruptible:
    /// name resolution, the connection, the request going out and the endpoint's first byte all
    /// happen inside a single call that cannot be asked to return. Left on this thread, a stop
    /// pressed while an endpoint is still quiet could not be noticed until it answered or the
    /// bound on the reply ran out, which is ten minutes or more.
    ///
    /// Nothing on the other thread holds a policy or a workspace: every gate has been passed
    /// before it starts, and it sends bytes and hands back a reader. So a request walked away
    /// from leaves a socket to be closed when the far end finishes or the connection times out,
    /// and nothing else.
    fn send_watching(
        &self,
        request: &Request,
        url: &str,
        patient: bool,
        fetching: bool,
        cancel: &Cancel,
    ) -> Result<Sent, EgressError> {
        let (answered, waiting) = std::sync::mpsc::channel();
        let (agent, hop, target) = (
            self.agent_for(fetching, url).clone(),
            request.clone(),
            url.to_string(),
        );
        let timeouts = self.timeouts;
        std::thread::spawn(move || {
            // A send that fails means the caller stopped, so there is nobody left to answer.
            let _ = answered.send(send(&agent, timeouts, &hop, &target, patient));
        });

        loop {
            if cancel.is_cancelled() {
                return Err(EgressError::Stopped {
                    url: url.to_string(),
                });
            }
            match waiting.recv_timeout(STOP_CHECK) {
                Ok(sent) => return sent,
                // Nothing has arrived yet, which is the whole point of waiting with a limit.
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                // The thread is gone without having answered, which only a panic leaves behind.
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(EgressError::Transport {
                        url: url.to_string(),
                        detail: "the request ended without a reply".to_string(),
                        transient: false,
                    });
                }
            }
        }
    }
}

/// What one hop answers with: status, location, content-type, and the body reader.
type Sent = (
    u16,
    Option<String>,
    Option<String>,
    Box<dyn std::io::Read + Send>,
);

/// One hop, on whichever thread calls it.
///
/// Owns nothing of the caller's, so the whole of it can be handed to a thread that is allowed to
/// outlive the wait for it.
fn send(
    agent: &ureq::Agent,
    timeouts: Timeouts,
    request: &Request,
    url: &str,
    patient: bool,
) -> Result<Sent, EgressError> {
    // GET and POST builders have different types in ureq, so the header loop is
    // repeated rather than abstracted over them.
    let result = match request.method {
        Method::Get => {
            let mut builder = agent.get(url);
            for (name, value) in &request.headers {
                builder = builder.header(name, value);
            }
            within(builder, timeouts, request.reply, patient).call()
        }
        Method::Post => {
            let mut builder = agent.post(url);
            for (name, value) in &request.headers {
                builder = builder.header(name, value);
            }
            let builder = within(builder, timeouts, request.reply, patient);
            match &request.body {
                Some(bytes) => builder.send(&bytes[..]),
                None => builder.send_empty(),
            }
        }
    };

    let response = match result {
        Ok(r) => r,
        // A redirect with max_redirects(0) is returned as a response, not an
        // error, so anything here is a genuine transport failure.
        // A reply asked for whole is written before any of it is sent, so its deadline can pass
        // while its headers are still awaited, and it was being written then as surely as one cut
        // part way. Any other reply that never began is the connection's.
        Err(ureq::Error::Timeout(ureq::Timeout::RecvResponse))
            if matches!(request.reply, Some(ReplyBound::Whole(_))) =>
        {
            return Err(EgressError::OutOfTime {
                url: url.to_string(),
            });
        }
        Err(e) if address::is_refusal(&e) => {
            return Err(EgressError::AddressRefused {
                url: url.to_string(),
            });
        }
        Err(e) => {
            return Err(EgressError::Transport {
                url: url.to_string(),
                detail: e.to_string(),
                transient: is_transient_call(&e),
            });
        }
    };

    let status = response.status().as_u16();
    let header = |name: &str| {
        response
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    };
    let location = header("location");
    let content_type = header("content-type");

    Ok((
        status,
        location,
        content_type,
        Box::new(response.into_body().into_reader()),
    ))
}

/// `builder` with the bound the request states on its reply, where it states one.
///
/// A whole reply sets the same three phases [`Egress::with_transport`] sets from
/// [`Timeouts::reply`], for the reason given there: each bounds the phases after it, so leaving one
/// at the agent's figure would cut the reply off there. The body's is `send` longer than the
/// reply's, so the wait for the reply ends on the reply's own bound and is named for it, which is
/// what [`send`] tells a reply out of time from a request that did not get through by. A begun one
/// sets only the last, which ureq counts from the headers arriving, and the two before it keep the
/// wait for them at the agent's.
///
/// A patient one lifts every bound from the request going out onwards, since each carries into the
/// phases after it. Connecting keeps its bound, which carries only into sending the headers.
fn within<B>(
    builder: ureq::RequestBuilder<B>,
    timeouts: Timeouts,
    reply: Option<ReplyBound>,
    patient: bool,
) -> ureq::RequestBuilder<B> {
    if patient {
        return builder
            .config()
            .timeout_send_request(None)
            .timeout_send_body(None)
            .timeout_recv_response(None)
            .timeout_recv_body(None)
            .build();
    }
    match reply {
        None => builder,
        Some(ReplyBound::Whole(bound)) => builder
            .config()
            .timeout_send_request(Some(timeouts.send + bound))
            .timeout_send_body(Some(timeouts.send + bound))
            .timeout_recv_response(Some(bound))
            .build(),
        Some(ReplyBound::Begun(bound)) => {
            builder.config().timeout_recv_response(Some(bound)).build()
        }
    }
}

/// Writes a failed request to the diagnostic log: the host asked, what kind of failure, the status
/// where there was one, and how long it took. The URL is the caller's, never a redirect target, and
/// only its host is kept; the failure's own text can carry what a server said, so it is not.
fn log_failure(url: &str, error: &EgressError, elapsed: Duration) {
    if let Some(fields) = failure_fields(url, error, elapsed) {
        bravebot_diag::error("net.fetch", &fields);
    }
}

/// What a failed request is recorded as, or `None` for a stop: a person ending a run is not a
/// failure, and logging it would leave a file for a run that went wrong nowhere.
fn failure_fields(
    url: &str,
    error: &EgressError,
    elapsed: Duration,
) -> Option<Vec<(&'static str, bravebot_diag::Field)>> {
    let (kind, status) = match error {
        EgressError::Denied(_) => ("denied", None),
        EgressError::TooManyRedirects { .. } => ("too_many_redirects", None),
        EgressError::MissingLocation { .. } => ("missing_location", None),
        EgressError::InvalidUrl { .. } => ("invalid_url", None),
        EgressError::InsecureRedirect { .. } => ("insecure_redirect", None),
        EgressError::AddressRefused { .. } => ("address_refused", None),
        EgressError::Transport { .. } => ("transport", None),
        EgressError::OutOfTime { .. } => ("out_of_time", None),
        EgressError::Status { status, .. } => ("status", Some(*status)),
        EgressError::Stopped { .. } => return None,
    };
    let mut fields = vec![
        ("host", bravebot_diag::Field::host(url)),
        ("kind", bravebot_diag::Field::word(kind)),
        ("transient", bravebot_diag::Field::num(error.is_transient())),
        ("elapsed_ms", bravebot_diag::Field::num(elapsed.as_millis())),
    ];
    if let Some(status) = status {
        fields.push(("status", bravebot_diag::Field::num(status)));
    }
    Some(fields)
}

/// How often a thread waiting on a reply looks at whether the caller has stopped.
const STOP_CHECK: Duration = Duration::from_millis(50);

/// Whether a failure to send or to read the reply is worth another attempt.
///
/// Everything here is the connection giving out: a timeout, a socket that went away, a name
/// that did not resolve because the machine's network was not up yet. A protocol error or a
/// malformed URL is the request being wrong, and sending it again would be wrong again.
fn is_transient_call(error: &ureq::Error) -> bool {
    match error {
        ureq::Error::Timeout(_) | ureq::Error::ConnectionFailed | ureq::Error::HostNotFound => true,
        ureq::Error::Io(e) => is_transient_io(e),
        _ => false,
    }
}

/// What a read of a reply's body that failed is reported as.
///
/// ureq names the reply's own deadline, [`Timeouts::reply`] or the bound its request stated, as
/// the wait for the response, which it keeps counting from the headers while the body arrives. Any
/// other failure, the gap bound among them, is the connection's.
fn body_failure(url: &str, error: std::io::Error) -> EgressError {
    let cause = error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<ureq::Error>());
    if matches!(
        cause,
        Some(ureq::Error::Timeout(ureq::Timeout::RecvResponse))
    ) {
        return EgressError::OutOfTime {
            url: url.to_string(),
        };
    }
    EgressError::Transport {
        url: url.to_string(),
        detail: error.to_string(),
        transient: is_transient_io(&error),
    }
}

fn is_transient_io(error: &std::io::Error) -> bool {
    use std::io::ErrorKind::*;
    matches!(
        error.kind(),
        TimedOut
            | Interrupted
            | UnexpectedEof
            | ConnectionReset
            | ConnectionAborted
            | BrokenPipe
            | NotConnected
            | WouldBlock
    )
}

fn is_redirect(status: u16) -> bool {
    (300..400).contains(&status)
}

/// Whether a URL's host is this machine: `localhost` or a loopback address.
///
/// No other name, since what one resolves to is up to whoever answers the lookup.
fn names_this_machine(host: &str) -> bool {
    let host = host
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(host);
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.to_canonical().is_loopback())
}

/// Reject anything that is not http(s) before it reaches the client, so a `file://` or
/// similar cannot be used to read local data through the network path.
fn require_http_scheme(url: &str) -> Result<(), EgressError> {
    if url.starts_with("https://") || url.starts_with("http://") {
        return Ok(());
    }
    Err(EgressError::InvalidUrl {
        url: url.to_string(),
        detail: "only http and https are permitted".into(),
    })
}

/// Resolve a `Location` value against the URL it came from, refusing one that leaves TLS.
///
/// The one way a hop is computed, so a chain cannot advance without being held to the transport the
/// hop before it used. `send` re-sends the whole request on every hop, headers and body alike, so a
/// hop out of https would put what the one before it carried on the wire in the clear.
///
/// The refusal reads the URL that was produced rather than the `Location` that produced it, because
/// each of the four forms below yields a scheme by a different route and only one of them states one
/// at all. Checking the result holds every form, including any added later.
fn resolve(base: &str, location: &str) -> Result<String, EgressError> {
    let next = join(base, location)?;
    if base.starts_with("https://") && !next.starts_with("https://") {
        return Err(EgressError::InsecureRedirect {
            // The URL the request is on, never the one a server named: past the first hop that
            // string would be a server's own bytes.
            url: base.to_string(),
        });
    }
    Ok(next)
}

/// A `Location` value as the absolute URL it resolves to.
///
/// Handles absolute, scheme-relative, path-absolute, and relative forms, because a
/// server can use any of them and each must be checked as the absolute URL it becomes.
fn join(base: &str, location: &str) -> Result<String, EgressError> {
    if location.starts_with("http://") || location.starts_with("https://") {
        return Ok(location.to_string());
    }

    let separator = base.find("://").ok_or_else(|| EgressError::InvalidUrl {
        url: base.to_string(),
        detail: "no scheme".into(),
    })?;
    // Scheme without its "://", then everything after it.
    let scheme = &base[..separator];
    let rest = &base[separator + 3..];
    let authority_end = rest.find('/').unwrap_or(rest.len());
    let authority = &rest[..authority_end];

    // "//host/path" keeps the current scheme but replaces the authority.
    if let Some(host_and_path) = location.strip_prefix("//") {
        return Ok(format!("{scheme}://{host_and_path}"));
    }

    if location.starts_with('/') {
        return Ok(format!("{scheme}://{authority}{location}"));
    }

    let path = &rest[authority_end..];
    let parent = match path.rfind('/') {
        Some(index) => &path[..=index],
        None => "/",
    };
    Ok(format!("{scheme}://{authority}{parent}{location}"))
}

/// Read at most [`MAX_RESPONSE_BYTES`], reporting whether the cap was hit.
///
/// Truncation is size hygiene, not filtering: nothing is inspected, and the caller
/// still receives the bytes labelled.
///
/// A read that fails is an error rather than a short body. The two are indistinguishable once
/// the bytes are handed back, and treating a cut-off reply as a complete one turns a transport
/// failure into whatever the truncated bytes happen to parse as.
fn read_capped(mut reader: Box<dyn std::io::Read>) -> Result<(Vec<u8>, bool), std::io::Error> {
    use std::io::Read;
    let mut buffer = Vec::new();
    let mut limited = reader.by_ref().take(MAX_RESPONSE_BYTES as u64 + 1);
    limited.read_to_end(&mut buffer)?;

    if buffer.len() > MAX_RESPONSE_BYTES {
        buffer.truncate(MAX_RESPONSE_BYTES);
        return Ok((buffer, true));
    }
    Ok((buffer, false))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one way out is built from the shared transport, so the certificate authorities and the
    /// proxy a machine states reach the gated path rather than only whichever client happened to
    /// be configured with them. A client built from the library's own defaults here would leave
    /// every agent request on a different trust set from the rest of the process.
    #[test]
    fn the_one_way_out_is_built_against_the_stated_transport_rather_than_a_default_client() {
        let egress = Egress::with_transport(
            Timeouts::default(),
            &Transport::stated(TrustRoots::Bundled, Some("http://proxy.corp:3128"), None),
        );

        let config = egress.agent.config();
        assert_eq!(config.proxy().map(|proxy| proxy.port()), Some(3128));
        assert!(matches!(
            config.tls_config().root_certs(),
            ureq::tls::RootCerts::WebPki
        ));
    }

    /// Only a connection that ends here is waited on without bounds. A name that merely sounds
    /// local, or an address on the local network, is another machine that can go quiet for good.
    #[test]
    fn only_this_machine_is_this_machine() {
        let direct = Egress::with_transport(
            Timeouts::default(),
            &Transport::stated(TrustRoots::Bundled, None, None),
        );
        for here in [
            "http://localhost:11434/v1/chat/completions",
            "http://LOCALHOST:11434",
            "http://127.0.0.1:11434",
            "http://127.1.2.3",
            "http://[::1]:11434",
            "http://[::ffff:127.0.0.1]:11434",
            "https://localhost/v1",
        ] {
            assert!(direct.reaches_here(here), "{here}");
        }
        for elsewhere in [
            "https://api.example.com/v1",
            "http://192.168.1.20:11434",
            "http://10.0.0.1",
            "http://localhost.example.com",
            "http://mylocalhost:11434",
            "http://0.0.0.0:11434",
            "http://[::]:11434",
            "not a url",
        ] {
            assert!(!direct.reaches_here(elsewhere), "{elsewhere}");
        }

        // Through a proxy the connection ends at the proxy, wherever the URL points.
        let proxied = Egress::with_transport(
            Timeouts::default(),
            &Transport::stated(TrustRoots::Bundled, Some("http://proxy.corp:3128"), None),
        );
        assert!(!proxied.reaches_here("http://localhost:11434/v1"));
        assert!(!proxied.reaches_here("http://127.0.0.1:11434/v1"));
    }

    /// A URL naming this machine does not make a request patient when a proxy carries it, because
    /// the proxy is another machine that can go quiet for good. The proxy here answers after a
    /// silence longer than the reply bound, so the ask being honoured lets that answer through.
    #[test]
    fn a_request_through_a_proxy_keeps_its_bounds_when_it_asks_to_be_patient() {
        use std::io::{BufRead, BufReader, Read, Write};

        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind loopback");
        let proxy = format!(
            "http://127.0.0.1:{}",
            listener.local_addr().expect("addr").port()
        );
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            // The client tunnels through the proxy: it asks for a CONNECT, is told it has one, and
            // then sends the request it means.
            loop {
                let mut line = String::new();
                let mut length = 0;
                let mut first = None;
                while reader.read_line(&mut line).unwrap_or(0) > 0 {
                    if line == "\r\n" || line == "\n" {
                        break;
                    }
                    if first.is_none() {
                        first = Some(line.clone());
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap_or(0);
                    }
                    line.clear();
                }
                let Some(first) = first else {
                    return;
                };
                let _ = reader.read_exact(&mut vec![0; length]);
                if first.starts_with("CONNECT") {
                    let _ = stream.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n");
                    continue;
                }
                std::thread::sleep(Duration::from_millis(800));
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                );
                return;
            }
        });

        let egress = Egress::with_transport(
            Timeouts {
                reply: Duration::from_millis(300),
                idle: Duration::from_millis(300),
                ..Timeouts::default()
            },
            &Transport::stated(TrustRoots::Bundled, Some(&proxy), None),
        );
        let mut routing = bravebot_core::policy::Routing::new();
        routing.insert_trusted("task", "fetch a page");
        let mut sink = bravebot_core::event::NullSink;
        let mut policy = bravebot_core::policy::Policy::begin(
            routing,
            bravebot_core::policy::ReleasePlan::new(),
            bravebot_core::capability::CapabilitySet::from_iter([
                bravebot_core::capability::Capability::WebFetch,
            ]),
            &mut sink,
        )
        .expect("policy begins");

        let outcome = egress
            .fetch_streaming(
                &mut policy,
                Request::post("http://localhost:9/v1", b"{}".to_vec()).patient_on_this_machine(),
                Label::untrusted_public(),
                Some(&Cancel::new()),
            )
            .map(|_| ());
        assert!(
            matches!(outcome, Err(EgressError::Transport { .. })),
            "the silence from the proxy was waited through: {outcome:?}"
        );
    }

    /// The classification a retry rests on. Getting it wrong in one direction repeats a request
    /// that will fail identically, and in the other abandons one that would have worked.
    #[test]
    fn a_connection_that_gave_out_is_worth_another_attempt_and_a_refusal_is_not() {
        let dead = EgressError::Transport {
            url: "https://example.com".into(),
            detail: "timeout".into(),
            transient: true,
        };
        assert!(dead.is_transient());

        let wrong = EgressError::Transport {
            url: "https://example.com".into(),
            detail: "malformed http".into(),
            transient: false,
        };
        assert!(!wrong.is_transient());

        assert!(
            !EgressError::InvalidUrl {
                url: "gopher://example.com".into(),
                detail: "scheme".into(),
            }
            .is_transient()
        );

        // The same chain redirects the same way, so a second attempt is refused a second time.
        assert!(
            !EgressError::InsecureRedirect {
                url: "https://example.com".into(),
            }
            .is_transient()
        );
    }

    /// A server saying "not now" is temporary; a server saying "no" is not.
    #[test]
    fn only_the_statuses_that_mean_not_now_are_worth_another_attempt() {
        let at = |status| {
            EgressError::Status {
                url: "https://example.com".into(),
                status,
            }
            .is_transient()
        };

        // Every status a server can send, so a status added to the set or dropped from it is
        // named here: 501 and 505 are server statuses that will not change on a second try, and
        // 403 and 451 are refusals.
        let retryable: Vec<u16> = (100..=599).filter(|status| at(*status)).collect();
        assert_eq!(retryable, [408, 429, 500, 502, 503, 504]);
    }

    /// A person stopping a run is not a failure to report: logging it would leave a file, and use
    /// up one of the few kept, for a run that went wrong nowhere.
    #[test]
    fn a_stopped_request_is_not_a_failure_to_log() {
        let stopped = EgressError::Stopped {
            url: "https://example.com/".to_string(),
        };
        assert!(failure_fields("https://example.com/", &stopped, Duration::ZERO).is_none());
        let refused = EgressError::Status {
            url: "https://example.com/".to_string(),
            status: 503,
        };
        assert!(failure_fields("https://example.com/", &refused, Duration::ZERO).is_some());
    }

    /// A request that fails is written to the diagnostic log as the host, the kind of failure and
    /// the status, and nothing of the URL's path, query or credentials or of what the server sent.
    /// Writing the failure's text or the URL would put a token or a reply into a file a person
    /// attaches to a public issue.
    #[test]
    fn a_failed_request_is_logged_as_host_status_and_kind_only() {
        use std::io::{BufRead, BufReader, Write};

        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind loopback");
        let port = listener.local_addr().expect("addr").port();
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                if line == "\r\n" {
                    break;
                }
                line.clear();
            }
            let _ = stream.write_all(
                b"HTTP/1.1 503 Unavailable\r\nContent-Length: 10\r\nConnection: close\r\n\r\nSERVERBODY",
            );
        });

        let tmp = tempfile::tempdir().expect("a scratch directory");
        let dir = tmp.path().to_path_buf();
        bravebot_diag::configure(bravebot_diag::Level::Error, Some(dir.clone()));

        let mut routing = bravebot_core::policy::Routing::new();
        routing.insert_trusted("task", "fetch a page");
        let mut sink = bravebot_core::event::NullSink;
        let mut policy = bravebot_core::policy::Policy::begin(
            routing,
            bravebot_core::policy::ReleasePlan::new(),
            bravebot_core::capability::CapabilitySet::from_iter([
                bravebot_core::capability::Capability::WebFetch,
            ]),
            &mut sink,
        )
        .expect("policy begins");
        let outcome = Egress::new().fetch(
            &mut policy,
            Request::get(format!(
                "http://user:hunter2@127.0.0.1:{port}/private/path?token=abc"
            )),
            Label::untrusted_public(),
        );
        assert!(matches!(
            outcome,
            Err(EgressError::Status { status: 503, .. })
        ));

        let log: String = std::fs::read_dir(&dir)
            .expect("a failure makes the log")
            .flatten()
            .map(|e| std::fs::read_to_string(e.path()).unwrap_or_default())
            .collect();
        bravebot_diag::configure(bravebot_diag::Level::Error, None);

        assert!(log.contains("ERROR net.fetch"), "{log}");
        assert!(log.contains(&format!("host=127.0.0.1:{port}")), "{log}");
        assert!(
            log.contains("status=503") && log.contains("kind=status"),
            "{log}"
        );
        for leaked in ["hunter2", "private", "token", "SERVERBODY", "Unavailable"] {
            assert!(!log.contains(leaked), "{leaked} reached the log: {log}");
        }
    }

    #[test]
    fn only_http_schemes_are_permitted() {
        assert!(require_http_scheme("https://example.com").is_ok());
        assert!(require_http_scheme("http://example.com").is_ok());
        assert!(require_http_scheme("file:///etc/passwd").is_err());
        assert!(require_http_scheme("ftp://example.com").is_err());
        assert!(require_http_scheme("gopher://example.com").is_err());
    }

    /// Every hop re-sends the original request, so a chain that dropped TLS would put what the
    /// first hop carried on the wire in the clear: on this program's own connection the
    /// `authorization` header and the conversation, and on a `fetch_url` call a page a person
    /// approved after reading `https` on the prompt. Checking a hop's host without its transport
    /// leaves an endpoint able to turn its own traffic into plaintext, which hands third parties
    /// what only it had.
    #[test]
    fn a_redirect_may_not_take_an_https_chain_into_cleartext() {
        let refused = resolve("https://a.example/x", "http://a.example/y")
            .expect_err("a hop out of https is refused");
        assert!(
            matches!(&refused, EgressError::InsecureRedirect { url } if url == "https://a.example/x"),
            "refused, but naming somewhere else: {refused:?}"
        );

        // A chain with no TLS to lose continues, and one that gains it keeps it: what is refused
        // is leaving https, not a hop that changes scheme.
        assert_eq!(
            resolve("http://a.example/x", "http://b.example/y").unwrap(),
            "http://b.example/y"
        );
        assert_eq!(
            resolve("http://a.example/x", "https://a.example/y").unwrap(),
            "https://a.example/y"
        );

        // Each hop is held to the one before it rather than to where the chain started, so a
        // chain that picked TLS up part way through cannot put it down again.
        assert!(resolve("https://b.example/y", "http://c.example/z").is_err());
    }

    /// Past the first hop the URL a refusal was raised on is a string a server wrote into a
    /// `Location` header, and a failure's text is formatted into a sentence the planner reads. A
    /// refusal that named it would be handing the planner a server's own bytes with the driver's
    /// attribution on them, which is the channel quarantining a body otherwise closes.
    #[test]
    fn a_refused_downgrade_names_the_url_that_was_asked_for() {
        let refused = EgressError::InsecureRedirect {
            url: "https://a-server-chose-this.example/y".into(),
        }
        .into_a_failure_of("https://the-caller-asked-for-this.example/x");

        assert!(
            matches!(&refused, EgressError::InsecureRedirect { url }
                if url == "https://the-caller-asked-for-this.example/x"),
            "a hop's own URL left the crate that followed it: {refused:?}"
        );
    }

    #[test]
    fn absolute_redirects_are_used_as_given() {
        assert_eq!(
            resolve("https://a.example/x", "https://b.example/y").unwrap(),
            "https://b.example/y"
        );
    }

    #[test]
    fn path_absolute_redirects_keep_the_authority() {
        assert_eq!(
            resolve("https://a.example/x/y", "/z").unwrap(),
            "https://a.example/z"
        );
    }

    #[test]
    fn relative_redirects_resolve_against_the_parent_path() {
        assert_eq!(
            resolve("https://a.example/x/y", "z").unwrap(),
            "https://a.example/x/z"
        );
    }

    #[test]
    fn scheme_relative_redirects_keep_the_scheme() {
        assert_eq!(
            resolve("https://a.example/x", "//b.example/y").unwrap(),
            "https://b.example/y"
        );
    }

    #[test]
    fn redirect_status_codes_are_recognised() {
        assert!(is_redirect(301));
        assert!(is_redirect(302));
        assert!(is_redirect(307));
        assert!(!is_redirect(200));
        assert!(!is_redirect(404));
    }

    #[test]
    fn bodies_are_capped() {
        let oversized = vec![b'x'; MAX_RESPONSE_BYTES + 100];
        let (body, truncated) =
            read_capped(Box::new(std::io::Cursor::new(oversized))).expect("the read succeeds");
        assert_eq!(body.len(), MAX_RESPONSE_BYTES);
        assert!(truncated);
    }

    #[test]
    fn small_bodies_are_not_reported_as_truncated() {
        let (body, truncated) = read_capped(Box::new(std::io::Cursor::new(b"hello".to_vec())))
            .expect("the read succeeds");
        assert_eq!(body, b"hello");
        assert!(!truncated);
    }

    /// Reading a stream to the end, in the pieces the caller would see them in.
    ///
    /// Through a policy, because a chunk arrives labelled and the gate is the only way to the
    /// bytes inside one. Nothing about the gate is under test here; it is how the caller under
    /// test reads a chunk too.
    fn drain(mut stream: Streamed<'_>) -> (usize, bool) {
        let mut routing = bravebot_core::policy::Routing::new();
        routing.insert_trusted("task", "read a stream");
        let mut sink = bravebot_core::event::NullSink;
        let mut policy = bravebot_core::policy::Policy::begin(
            routing,
            bravebot_core::policy::ReleasePlan::new(),
            bravebot_core::capability::CapabilitySet::none(),
            &mut sink,
        )
        .expect("policy begins");

        let decoding = policy.decode_transport("test", Label::untrusted_public());
        let mut total = 0;
        while let Some(chunk) = stream.next_chunk().expect("the read succeeds") {
            total += decoding.decode(chunk).0.len();
        }
        (total, stream.truncated())
    }

    fn streamed(body: Vec<u8>) -> Streamed<'static> {
        Streamed {
            status: 200,
            content_type: None,
            requested: "https://example.com".into(),
            label: Label::untrusted_public(),
            reader: Box::new(std::io::Cursor::new(body)),
            read: 0,
            truncated: false,
            cap: MAX_RESPONSE_BYTES,
            chunk: STREAM_CHUNK_BYTES,
        }
    }

    /// A download is held to the figure its caller was given and not to the built-in cap, in both
    /// directions: a larger body than the built-in cap arrives whole, and a smaller figure cuts a
    /// body the built-in cap would have let through.
    #[test]
    fn a_stream_raised_or_lowered_is_cut_at_the_figure_it_was_given() {
        let (total, truncated) =
            drain(streamed(vec![b'x'; MAX_RESPONSE_BYTES + 10]).capped_at(MAX_RESPONSE_BYTES + 10));
        assert_eq!((total, truncated), (MAX_RESPONSE_BYTES + 10, false));

        let (total, truncated) = drain(streamed(vec![b'x'; 5000]).capped_at(3000));
        assert_eq!((total, truncated), (3000, true));
    }

    /// The flag has to mean the body was cut, not that the cap was reached: a caller that cannot
    /// tell a whole answer from a severed one has to treat every full-sized reply as suspect.
    #[test]
    fn a_streamed_body_that_ends_at_the_cap_is_not_truncated() {
        let (total, truncated) = drain(streamed(vec![b'x'; MAX_RESPONSE_BYTES]));
        assert_eq!(total, MAX_RESPONSE_BYTES);
        assert!(!truncated);
    }

    /// And one byte more is a cut, reported as such, with the extra byte kept back so the two
    /// paths hand a caller the same body for the same response.
    #[test]
    fn a_streamed_body_past_the_cap_is_cut_and_says_so() {
        let (total, truncated) = drain(streamed(vec![b'x'; MAX_RESPONSE_BYTES + 1]));
        assert_eq!(total, MAX_RESPONSE_BYTES);
        assert!(truncated);
    }

    /// The distinction that matters to a caller: a body that stopped early is not a short body,
    /// and handing back what arrived would leave nothing to tell them apart by.
    #[test]
    fn a_failed_read_is_not_a_short_body() {
        struct Interrupted;
        impl std::io::Read for Interrupted {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "gone"))
            }
        }

        assert!(read_capped(Box::new(Interrupted)).is_err());
    }

    /// What `names` resolve to, one answer per lookup in order, the last one repeating. Counts the
    /// lookups, since a second one is how an answer would differ from the address connected to.
    #[derive(Debug)]
    struct Answers {
        lookups: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        answers: Vec<Vec<std::net::IpAddr>>,
    }

    impl ureq::unversioned::resolver::Resolver for Answers {
        fn resolve(
            &self,
            uri: &ureq::http::Uri,
            _: &ureq::config::Config,
            _: ureq::unversioned::transport::NextTimeout,
        ) -> Result<ureq::unversioned::resolver::ResolvedSocketAddrs, ureq::Error> {
            let nth = self
                .lookups
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let answer = &self.answers[nth.min(self.answers.len() - 1)];
            let mut found = self.empty();
            for ip in answer {
                found.push(std::net::SocketAddr::new(*ip, uri.port_u16().unwrap_or(80)));
            }
            Ok(found)
        }
    }

    fn ip(text: &str) -> std::net::IpAddr {
        text.parse().expect("an address")
    }

    fn loopback_only(address: std::net::IpAddr) -> bool {
        address == ip("127.0.0.1")
    }

    /// An egress whose fetches resolve through `answers`, with `admits` standing in for the
    /// classification where a test needs an address it can actually connect to.
    fn egress_resolving(
        answers: Vec<Vec<&str>>,
        admits: fn(std::net::IpAddr) -> bool,
    ) -> (Egress, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
        let lookups = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let resolver = Answers {
            lookups: lookups.clone(),
            answers: answers
                .into_iter()
                .map(|answer| answer.into_iter().map(ip).collect())
                .collect(),
        };
        let egress = Egress::with_transport(
            Timeouts::default(),
            &Transport::stated(TrustRoots::Bundled, None, None),
        )
        .guarded_by(address::GuardedResolver::over(resolver, admits));
        (egress, lookups)
    }

    /// A listener that counts connections and answers each with `responses` in turn.
    fn serve_counting(
        responses: Vec<String>,
    ) -> (u16, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let connections = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = connections.clone();
        std::thread::spawn(move || {
            for response in responses {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                let mut line = String::new();
                while reader.read_line(&mut line).unwrap_or(0) > 0 {
                    if line == "\r\n" {
                        break;
                    }
                    line.clear();
                }
                let _ = stream.write_all(response.as_bytes());
            }
        });
        (port, connections)
    }

    fn page(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    fn redirect(location: &str) -> String {
        format!(
            "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
    }

    /// Runs `request` as a `fetch_url` call would: approved, then in flight.
    fn fetched(egress: &Egress, request: Request) -> Result<Response, EgressError> {
        let mut routing = bravebot_core::policy::Routing::new();
        routing.insert_trusted("task", "fetch a page");
        let mut sink = bravebot_core::event::NullSink;
        let Ok(mut policy) = bravebot_core::policy::Policy::begin(
            routing,
            bravebot_core::policy::ReleasePlan::new(),
            bravebot_core::capability::CapabilitySet::from_iter([
                bravebot_core::capability::Capability::WebFetch,
            ]),
            &mut sink,
        ) else {
            panic!("policy begins");
        };
        policy.endorse_fetch(&request.url);
        assert!(policy.before_fetch(&request.url).is_ok(), "approved");
        let outcome = egress.fetch(&mut policy, request, Label::untrusted_public());
        policy.fetch_finished();
        outcome
    }

    #[test]
    fn a_name_resolving_to_a_non_public_address_is_refused_before_any_request_is_sent() {
        for answer in [
            "10.1.2.3",
            "192.168.0.7",
            "172.20.0.1",
            "169.254.169.254",
            "fe80::1",
            "fd00:ec2::254",
            "100.100.100.200",
            "::ffff:169.254.169.254",
        ] {
            let (egress, lookups) = egress_resolving(vec![vec![answer]], address::is_public);
            let outcome = fetched(&egress, Request::get("http://docs.example.test:9/page"));
            assert!(
                matches!(&outcome, Err(EgressError::AddressRefused { url }) if url == "http://docs.example.test:9/page"),
                "{answer}: {outcome:?}"
            );
            assert_eq!(lookups.load(std::sync::atomic::Ordering::SeqCst), 1);
        }
    }

    /// Only the lookup is guarded. A listener on loopback that does answer shows the refusal came
    /// before the connection and not from there being nothing to connect to.
    #[test]
    fn a_refused_address_is_never_connected_to() {
        let (port, connections) = serve_counting(vec![page("secret")]);
        let (egress, _) = egress_resolving(vec![vec!["127.0.0.1"]], address::is_public);
        let outcome = fetched(
            &egress,
            Request::get(format!("http://rebind.example.test:{port}/")),
        );
        assert!(matches!(outcome, Err(EgressError::AddressRefused { .. })));
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(connections.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    /// An answer holding one address that is not public is the name pointing somewhere it should
    /// not, however many of the others are fine.
    #[test]
    fn one_non_public_address_among_public_ones_refuses_the_name() {
        let (port, connections) = serve_counting(vec![page("x")]);
        let (egress, _) = egress_resolving(vec![vec!["127.0.0.1", "10.0.0.1"]], loopback_only);
        let outcome = fetched(
            &egress,
            Request::get(format!("http://mixed.example.test:{port}/")),
        );
        assert!(matches!(outcome, Err(EgressError::AddressRefused { .. })));
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(connections.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    /// The same name on the next hop is looked up again and classified again, so an answer that
    /// changes between hops is caught on the hop it changed.
    #[test]
    fn a_redirect_hop_resolving_to_a_non_public_address_is_refused_and_names_the_url_asked_for() {
        // A path-absolute Location keeps the host, so the redirect is one the approval allows.
        let (port, connections) = serve_counting(vec![redirect("/second"), page("not reached")]);
        let (egress, lookups) = egress_resolving(
            vec![vec!["127.0.0.1"], vec!["169.254.169.254"]],
            |address| address == ip("127.0.0.1") || address::is_public(address),
        );
        let asked = format!("http://hop.example.test:{port}/first");
        let outcome = fetched(&egress, Request::get(&asked));

        match outcome {
            Err(EgressError::AddressRefused { url }) => assert_eq!(url, asked),
            other => panic!("expected the second hop refused: {other:?}"),
        }
        assert_eq!(lookups.load(std::sync::atomic::Ordering::SeqCst), 2);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(
            connections.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "only the first hop was sent"
        );
    }

    /// The connection goes to the address that was classified: one lookup per hop, whatever a
    /// later lookup would have said.
    #[test]
    fn the_connection_is_made_to_the_address_that_was_classified() {
        let (port, connections) = serve_counting(vec![page("served")]);
        let (egress, lookups) =
            egress_resolving(vec![vec!["127.0.0.1"], vec!["10.255.255.1"]], loopback_only);
        let response = fetched(
            &egress,
            Request::get(format!("http://pinned.example.test:{port}/")),
        )
        .expect("the classified address is connected to");

        assert_eq!(response.status, 200);
        assert_eq!(lookups.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(connections.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    /// A host written as an address is what the person approved, so nothing it resolves to is a
    /// surprise to refuse.
    #[test]
    fn an_approved_host_that_is_itself_an_address_is_fetched() {
        let (port, _) = serve_counting(vec![page("local")]);
        let egress = Egress::with_transport(
            Timeouts::default(),
            &Transport::stated(TrustRoots::Bundled, None, None),
        );
        let response = fetched(&egress, Request::get(format!("http://127.0.0.1:{port}/")))
            .expect("an address literal is not a name");
        assert_eq!(response.status, 200);
    }

    /// Everything but a fetch keeps the agent it had: the model endpoint a person runs on
    /// loopback is reached by name too.
    #[test]
    fn a_request_that_is_not_a_fetch_may_still_resolve_to_this_machine() {
        let (port, _) = serve_counting(vec![page("model")]);
        let egress = Egress::with_transport(
            Timeouts::default(),
            &Transport::stated(TrustRoots::Bundled, None, None),
        );
        let mut routing = bravebot_core::policy::Routing::new();
        routing.insert_trusted("task", "ask the model");
        let mut sink = bravebot_core::event::NullSink;
        let mut policy = bravebot_core::policy::Policy::begin(
            routing,
            bravebot_core::policy::ReleasePlan::new(),
            bravebot_core::capability::CapabilitySet::from_iter([
                bravebot_core::capability::Capability::WebFetch,
            ]),
            &mut sink,
        )
        .expect("policy begins");
        let response = egress
            .fetch(
                &mut policy,
                Request::get(format!("http://localhost:{port}/")),
                Label::untrusted_public(),
            )
            .expect("no fetch is in flight");
        assert_eq!(response.status, 200);
    }

    /// A proxy resolves the target, and ureq asks this resolver for the proxy's own address, so a
    /// guarded lookup there would refuse a proxy on this machine and check nothing about the
    /// target.
    #[test]
    fn a_proxied_fetch_is_not_sent_through_the_guarded_lookup() {
        let proxied = Egress::with_transport(
            Timeouts::default(),
            &Transport::stated(TrustRoots::Bundled, Some("http://127.0.0.1:3128"), None),
        );
        assert!(std::ptr::eq(
            proxied.agent_for(true, "http://docs.example.com/"),
            &proxied.agent
        ));

        let direct = Egress::with_transport(
            Timeouts::default(),
            &Transport::stated(TrustRoots::Bundled, None, None),
        );
        assert!(std::ptr::eq(
            direct.agent_for(true, "http://docs.example.com/"),
            &direct.guarded
        ));
        assert!(std::ptr::eq(
            direct.agent_for(false, "http://docs.example.com/"),
            &direct.agent
        ));
    }

    #[test]
    fn a_refused_address_is_a_failure_that_says_nothing_of_the_address_and_is_not_retried() {
        let error = EgressError::AddressRefused {
            url: "http://docs.example.com/".into(),
        };
        assert!(!error.is_transient());
        let text = error.to_string();
        assert!(text.contains("http://docs.example.com/"));
        let fields = failure_fields("http://docs.example.com/", &error, Duration::ZERO)
            .expect("a refusal is logged");
        assert!(fields.iter().any(
            |(name, value)| *name == "kind" && format!("{value:?}").contains("address_refused")
        ));
    }
}
