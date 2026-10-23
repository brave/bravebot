//! Client for OpenAI's Responses API ([BACKEND-61](../../../docs/specs/backends.md#BACKEND-61)).
//!
//! The service an entry naming `@ai-sdk/openai` reaches. It answers the same questions as
//! [`crate::AichatClient`] and returns the same [`Completion`], so a caller chooses between them
//! once and the turn loop is unchanged. What differs is underneath: the Responses protocol rather
//! than chat completions, a request that is always streamed and never asks the service to keep
//! anything, and a reply assembled from the items its stream completes.
//!
//! Every request goes through [`bravebot_net::Egress`], so the policy gate sees this traffic
//! exactly as it sees the other backends'. The reply is labelled untrusted-public and nothing here
//! reads it to decide anything: this module speaks the wire protocol and mints no witness.
//!
//! The retry helpers, the memory of what a model has refused, and the SSE decoder are the aichat
//! client's and are used here. A request refused on its contents is sent again once without the
//! level, since which models read one is not listed anywhere, and what answered is remembered.

pub mod protocol;

use crate::anthropic::protocol::Failure;
use crate::anthropic::what_was_written;
use crate::protocol::{ChatRequest, SseDecoder, ToolCall, Usage};
use crate::{
    ATTEMPTS, AccountAccess, BACKOFF, CHUNKS_AHEAD, ChatError, Completion, CutOff, OpenCall,
    Progress, Refusals, WAKE, learned_key, log_retry, pause_before_retry, refuses_the_body,
    remember, remembered, reply_bound, worth_another_attempt,
};
use bravebot_config::Secret;
use bravebot_config::provider::Provider;
use bravebot_core::cancel::Cancel;
use bravebot_core::event::Sink;
use bravebot_core::label::Label;
use bravebot_core::policy::Policy;
use bravebot_core::value::Labelled;
use bravebot_net::{Egress, EgressError, Request};
use protocol::{Decoded, Ending, Event, WireUsage};
use serde_json::Value;
use std::time::Duration;

/// Whether a failed attempt should be repeated, when `allowed` requests may be sent in all.
///
/// What the other clients ask, and a failure the service reports part way through a reply that a
/// second attempt could get past.
fn worth_asking_again(allowed: u32, attempt: u32, error: &ChatError) -> bool {
    worth_another_attempt(allowed, attempt, error)
        || (attempt < allowed
            && matches!(error, ChatError::Reported { failure } if failure.is_transient()))
}

/// The largest body sent to a ChatGPT account's serving host, which takes none larger uncompressed
/// ([CHATGPT-10](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-10)).
const ACCOUNT_BODY_LIMIT: usize = 1 << 20;

/// What a request to a ChatGPT account's serving host carries beyond a gateway's, and the token it
/// is sent with.
struct AccountRequests<'a> {
    access: &'a mut dyn AccountAccess,
    /// The identifier of the session, which is the value of each of the three session headers.
    session: String,
    /// The token the last attempt was sent with, which is what a refusal of it replaces.
    sent: Option<Secret>,
    /// Whether this call has already replaced a token the host refused. A second refusal ends it.
    replaced: bool,
}

/// One client for one request, to a service that takes the Responses protocol.
pub struct ResponsesClient<'a> {
    attempts: u32,
    completed_usage: Option<Usage>,
    retried_usage: Option<Usage>,
    egress: &'a Egress,
    provider: &'a Provider,
    /// What the service calls the model, which is the name the body carries.
    ///
    /// Resolved by whoever chose the provider, because the name a session holds may be qualified by
    /// the provider's own id to say which service is meant, and that qualified form is one the
    /// service has never heard of.
    model: String,
    token: Option<Secret>,
    /// Set where the service is a ChatGPT account, whose token is asked for before each request.
    account: Option<AccountRequests<'a>>,
    /// The headers the block's `options.headers` asked for, resolved by the caller (BACKEND-60).
    headers: Vec<(String, Secret)>,
    cancel: Option<Cancel>,
    /// Whether requests still carry the level a turn asks for.
    ///
    /// True until a model refuses it, which is the only way to find out that it reads none:
    /// nothing in a roster says which models do.
    effort: bool,
    /// What that model was known to refuse before this request, which a probe that settled nothing
    /// puts back.
    recalled: Refusals,
    /// How long to wait before the first retry. Doubled for each retry after that.
    backoff: Duration,
}

impl<'a> ResponsesClient<'a> {
    /// A client for `provider`, asking for `model` by the name that service knows it by, and
    /// carrying `token` as the key where the entry named a place for one to live.
    pub fn new(
        egress: &'a Egress,
        provider: &'a Provider,
        model: impl Into<String>,
        token: Option<Secret>,
    ) -> Self {
        Self {
            attempts: 0,
            completed_usage: None,
            retried_usage: None,
            egress,
            provider,
            model: model.into(),
            token,
            account: None,
            headers: Vec::new(),
            cancel: None,
            effort: true,
            recalled: Refusals::default(),
            backoff: BACKOFF,
        }
    }

    /// A client for a ChatGPT account, whose `provider` is the one
    /// [`Provider::chatgpt_account`] builds and whose token `access` hands out one request at a
    /// time ([CHATGPT-10](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-10)).
    ///
    /// Each request carries the token and the three session headers with `session` as the value of
    /// each, refuses a redirect, and is not sent where its body is larger than the service takes.
    /// A token the host refuses is replaced once and the request sent again.
    pub fn for_account(
        egress: &'a Egress,
        provider: &'a Provider,
        model: impl Into<String>,
        access: &'a mut dyn AccountAccess,
        session: impl Into<String>,
    ) -> Self {
        let mut client = Self::new(egress, provider, model, None);
        client.account = Some(AccountRequests {
            access,
            session: session.into(),
            sent: None,
            replaced: false,
        });
        client
    }

    /// Requests handed to egress in the last call, including capability probes.
    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    /// Total reported usage of completed attempts in the last call, including retries.
    /// Incomplete attempts add nothing. Returns `None` if no completed usage was reported.
    pub fn completed_usage(&self) -> Option<Usage> {
        match (self.retried_usage, self.completed_usage) {
            (Some(mut earlier), Some(current)) => {
                earlier.add(current);
                Some(earlier)
            }
            (earlier, current) => earlier.or(current),
        }
    }

    /// The final attempt's measured prompt size, separate from the cost of earlier attempts.
    pub fn last_request_tokens(&self) -> Option<u64> {
        self.completed_usage.map(|usage| usage.prompt_tokens)
    }

    /// Stop reading a streamed reply as soon as this says to.
    pub fn with_cancel(mut self, cancel: Cancel) -> Self {
        self.cancel = Some(cancel);
        self
    }

    /// Also send the headers the gateway's block asked for, already resolved (BACKEND-60).
    ///
    /// Separate from [`Self::new`] because a value may name a variable and this crate reads none:
    /// the caller that resolved the token resolves these at the same moment, from the same
    /// environment.
    pub fn with_gateway_headers(mut self, headers: Vec<(String, Secret)>) -> Self {
        self.headers = headers;
        self
    }

    /// Wait this long before the first retry instead of the default one-second wait. Later
    /// retries still double it.
    ///
    /// Used by tests, which would otherwise spend real seconds on every retry they exercise.
    pub fn with_backoff(mut self, backoff: Duration) -> Self {
        self.backoff = backoff;
        self
    }

    /// How many requests are sent for one call before its failure is the caller's: the retry count
    /// the entry's block stated, plus the first request, and [`ATTEMPTS`] where it stated none.
    fn attempts_allowed(&self) -> u32 {
        self.provider.patience.attempts(ATTEMPTS)
    }

    /// What a refusal is remembered against: the service, and the name the model goes out under.
    fn refusal_key(&self) -> String {
        learned_key(&self.provider.completion_url(), &self.model)
    }

    /// Start a request from what this model has already been found to refuse.
    fn recall(&mut self) {
        let refusals = remembered(&self.refusal_key());
        self.effort = !refusals.effort;
        self.recalled = refusals;
    }

    /// Settle what a probe found, once the request it was part of has finished one way or the other.
    ///
    /// A probe that answered leaves what it gave up dropped, which is the model saying it does not
    /// read it. A probe that failed as well says the level was not what the service refused, so it
    /// goes back to what was known before the request: a request can be refused on its contents
    /// for reasons that have nothing to do with it.
    fn probe_settled(&mut self, probed: bool, failed: bool) {
        if !probed {
            return;
        }
        if failed {
            self.effort = !self.recalled.effort;
            return;
        }
        remember(
            &self.refusal_key(),
            Refusals {
                effort: !self.effort,
                ..Refusals::default()
            },
        );
    }

    /// Whether this failure is worth sending the same request again without its level.
    ///
    /// Only where the request carried one: a request without a level was not refused for it, and
    /// giving it up there would record a refusal nobody saw.
    fn worth_dropping_effort(&self, request: &ChatRequest, error: &ChatError) -> bool {
        self.effort && request.effort.is_some() && refuses_the_body(error)
    }

    /// The ceiling somebody stated for this model, exported or in its block.
    fn stated_ceiling(&self) -> Option<u64> {
        self.provider.stated_output_limit(&self.model)
    }

    /// The body one request carries, before it is encoded.
    ///
    /// Separate from [`ResponsesClient::build`] so what a configuration puts in a request can be
    /// read without sending it.
    fn body_for(&self, request: &ChatRequest) -> protocol::ResponsesRequest {
        protocol::request_from(
            &self.model,
            &request.messages,
            request.tools.as_deref(),
            request.effort.filter(|_| self.effort),
            self.stated_ceiling(),
        )
    }

    /// The encoded body of one request.
    ///
    /// The options a model's block carries are merged into the body and never interpreted. They come
    /// from the settings file, which is the person's own configuration surface, so they are trusted
    /// exactly as far as a variable they exported would be. A key the request already set wins,
    /// because a configuration must not rewrite the conversation a turn built, and the fields in
    /// [`protocol::OWNED`] are the request's whether or not the turn set them.
    fn encode(&self, request: &ChatRequest) -> Result<Vec<u8>, ChatError> {
        let mut body = serde_json::to_value(self.body_for(request))
            .map_err(|error| ChatError::Encode(error.to_string()))?;
        if let (Some(object), Some(options)) = (
            body.as_object_mut(),
            self.provider
                .model(&self.model)
                .and_then(|model| model.options.as_ref()),
        ) {
            for (key, value) in options {
                if !protocol::OWNED.contains(&key.as_str()) {
                    object.entry(key.clone()).or_insert_with(|| value.clone());
                }
            }
        }
        serde_json::to_vec(&body).map_err(|error| ChatError::Encode(error.to_string()))
    }

    /// The request for one attempt, addressed and authenticated.
    fn build(&self, body: Vec<u8>, token: Option<&Secret>) -> Request {
        let mut http = Request::post(self.provider.completion_url(), body)
            .header("content-type", "application/json")
            .header("accept", "text/event-stream");
        for (name, value) in self.provider.fixed_headers() {
            http = http.header(*name, *value);
        }
        if let Some(token) = token.map(Secret::expose) {
            // Kept on the socket the request was addressed to, so a redirect to another host does
            // not carry the key along, which is the footing a bearer token has everywhere here.
            let (name, value) = self.provider.credential_header(token);
            http = http.socket_header(name, value);
        }
        if let Some(account) = &self.account {
            // Constants and a value generated here, so none can carry a line break or a length a
            // reply chose. Nothing else is added: a gateway's headers are not carried to the
            // account, and a redirect is a failure whatever host it names.
            for name in ["session-id", "thread-id", "x-client-request-id"] {
                http = http.header(name, account.session.as_str());
            }
            http = http.refusing_redirects().reading_a_refusal();
        } else {
            // After the credential, and never over it: a name this request already set is refused
            // where the block is read.
            for (name, value) in &self.headers {
                http = http.header(name.clone(), value.expose());
            }
        }
        // A stream's headers come at once and the model writes after them. Where nobody stated a
        // ceiling the reply is bounded as a gateway's is, since nothing says how long it may run.
        let mut http = match self.stated_ceiling() {
            Some(ceiling) => http.stream_within(reply_bound(ceiling)),
            None => http,
        };
        if let Some(gap) = self.provider.patience.idle_timeout {
            http = http.idle_within(gap);
        }
        http
    }

    /// Whether `error` is the serving host turning away the token the last attempt was sent with,
    /// where this call has not yet replaced one.
    fn refused_the_token(&self, error: &ChatError) -> bool {
        self.account
            .as_ref()
            .is_some_and(|account| !account.replaced && account.sent.is_some())
            && matches!(
                error,
                ChatError::Egress(EgressError::Status { status: 401, .. })
            )
    }

    /// Ask for a token other than the one the host refused. The next attempt asks for its token
    /// again and is sent the one this made.
    fn replace_the_token(&mut self) -> Result<(), ChatError> {
        let Some(account) = self.account.as_mut() else {
            return Ok(());
        };
        account.replaced = true;
        if let Some(refused) = account.sent.take() {
            account
                .access
                .replace_refused(&refused)
                .map_err(ChatError::Account)?;
        }
        Ok(())
    }

    /// A refusal of the token after it was replaced is the sign-in failing, which the person is
    /// told in the words of the sign-in. Any other failure is reported as it came.
    fn as_the_accounts(&self, error: ChatError) -> ChatError {
        match error {
            ChatError::Egress(EgressError::Status { status: 401, .. })
                if self.account.is_some() =>
            {
                ChatError::Account(crate::AccountFailure::Refused)
            }
            other => other,
        }
    }

    fn cancelled(&self) -> bool {
        self.cancel.as_ref().is_some_and(Cancel::is_cancelled)
    }

    /// Wait out a backoff, or give up on it when the caller says to stop.
    ///
    /// Slept in slices, because a stop landing in the middle of a seconds-long pause would otherwise
    /// wait the rest of it out with nothing to stop but a sleep.
    fn wait(&self, how_long: Duration) -> bool {
        const SLICE: Duration = Duration::from_millis(50);

        let until = std::time::Instant::now() + how_long;
        loop {
            if self.cancelled() {
                return false;
            }
            let left = until.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return true;
            }
            std::thread::sleep(left.min(SLICE));
        }
    }

    /// Send a request and wait for the reply.
    ///
    /// Read as events like [`ResponsesClient::complete_streaming`], which this is: the service may
    /// refuse a request asked for whole, and a reply read as events is read the same way whoever
    /// asked, so a caller that wants none of the progress gets the same reply.
    pub fn complete<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        request: &ChatRequest,
    ) -> Result<Completion, ChatError> {
        self.complete_streaming(policy, request, |_| {})
    }

    /// What a reply the service stopped was doing, from its structure.
    ///
    /// The ceiling is the one somebody stated, and otherwise what the reply had written when the
    /// service stopped, since no figure was sent and the model's own is not told. The call is the
    /// one still being written, named from the request's own list so that what travels on is a
    /// name this program offered and never one the reply spelt.
    fn cut_off(
        &self,
        request: &ChatRequest,
        usage: Usage,
        open: Option<(&str, usize)>,
        thought: bool,
    ) -> CutOff {
        CutOff {
            ceiling: self.stated_ceiling().unwrap_or(usage.completion_tokens),
            call: open.map(|(name, arguments)| OpenCall {
                tool: request.offered(name),
                arguments,
            }),
            thought,
        }
    }

    /// Send a request and read the reply as it arrives.
    ///
    /// `progress` is called as pieces land so a caller can show the reply arriving.
    pub fn complete_streaming<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        request: &ChatRequest,
        mut progress: impl FnMut(Progress),
    ) -> Result<Completion, ChatError> {
        self.attempts = 0;
        self.completed_usage = None;
        self.retried_usage = None;
        self.recall();
        if let Some(account) = self.account.as_mut() {
            account.sent = None;
            account.replaced = false;
        }
        let mut attempt = 1;
        let mut probed = false;
        loop {
            match self.stream_once(policy, request, attempt, &mut progress) {
                // Replaced once and sent again at once: the token may have been revoked early or
                // this clock may run behind, and a new one is what the host is asking for.
                Err(error) if self.refused_the_token(&error) => {
                    self.replace_the_token()?;
                }
                // Sent again immediately rather than after a wait: nothing is busy, the request was
                // simply carrying something this model does not take.
                Err(error) if self.worth_dropping_effort(request, &error) => {
                    self.effort = false;
                    probed = true;
                }
                Err(error) if worth_asking_again(self.attempts_allowed(), attempt, &error) => {
                    attempt += 1;
                    let pause = pause_before_retry(self.backoff, attempt - 1, &error);
                    log_retry(attempt, pause);
                    // Announced before the wait rather than after it, so the pause is explained
                    // while it is happening. Reply progress resets; completed costs remain charged.
                    progress(Progress {
                        written: Labelled::new("", Label::untrusted_public()),
                        output_tokens: 0,
                        calling: None,
                        thinking: Labelled::new("", Label::untrusted_public()),
                        counted_by_server: false,
                        attempt,
                    });
                    if !self.wait(pause) {
                        return Err(ChatError::Cancelled);
                    }
                }
                result => {
                    self.probe_settled(probed, result.is_err());
                    // No copy of the token outlives the call that was sent with it.
                    if let Some(account) = self.account.as_mut() {
                        account.sent = None;
                    }
                    return result
                        .map(|mut completion| {
                            if let Some(usage) = self.retried_usage {
                                completion.usage.add(usage);
                            }
                            completion
                        })
                        .map_err(|error| self.as_the_accounts(error));
                }
            }
        }
    }

    fn stream_once<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        request: &ChatRequest,
        attempt: u32,
        progress: &mut impl FnMut(Progress),
    ) -> Result<Completion, ChatError> {
        // `completed_usage` holds only the attempt that just ended and the next one replaces it, so
        // what the call has been billed so far is kept apart from it.
        self.retried_usage = self.completed_usage();
        self.completed_usage = None;
        // Before the request is built, let alone sent. A stop that landed while the last attempt was
        // failing is still a stop.
        if self.cancelled() {
            return Err(ChatError::Cancelled);
        }

        let body = self.encode(request)?;
        // Before a token is asked for, which can mean a renewal: a body that is not going to be sent
        // spends nothing.
        if self.account.is_some() && body.len() > ACCOUNT_BODY_LIMIT {
            return Err(ChatError::BodyTooLarge {
                limit: ACCOUNT_BODY_LIMIT,
            });
        }
        let token = match self.account.as_mut() {
            Some(account) => {
                let token = account.access.access_token().map_err(ChatError::Account)?;
                account.sent = Some(token.clone());
                Some(token)
            }
            None => self.token.clone(),
        };
        let http = self.build(body, token.as_ref());
        self.attempts += 1;
        let stream = match self.egress.fetch_streaming(
            policy,
            http,
            Label::untrusted_public(),
            self.cancel.as_ref(),
        ) {
            Ok(stream) => stream,
            Err(error) => return Err(crate::failure_of_refusal(policy, error)),
        };
        let label = stream.label();

        // Read on a thread this one can walk away from, for the same reason the other backends do:
        // a read blocks for as long as the service is quiet, and the longest quiet in a turn is the
        // one before the model's first word. Nothing on this thread holds a policy, a workspace or
        // a tool, so a request walked away from leaves only a socket to be dropped.
        let (chunks, arriving) = std::sync::mpsc::sync_channel(CHUNKS_AHEAD);
        std::thread::spawn(move || {
            let mut stream = stream;
            loop {
                match stream.next_chunk() {
                    Ok(Some(piece)) => {
                        if chunks.send(Ok(Some(piece))).is_err() {
                            return;
                        }
                    }
                    end => {
                        let _ = chunks.send(end);
                        return;
                    }
                }
            }
        });

        let mut decoder = SseDecoder::new();
        let mut reply = Reply::default();
        // One envelope, arriving in frames, so it is authorised once rather than once a frame.
        let decoding = policy.decode_transport("responses stream", Label::untrusted_public());
        let mut began = false;

        loop {
            let piece = match arriving.recv_timeout(WAKE) {
                Ok(Ok(Some(piece))) => piece,
                Ok(Ok(None)) => break,
                // A read that fails once the reply has begun is the reply stopping, whatever the
                // socket reported: the request plainly got through.
                Ok(Err(EgressError::Transport { .. })) if began => {
                    return Err(ChatError::Incomplete);
                }
                // A reply cut at its deadline among them: asked for again, it runs as long.
                Ok(Err(error)) => return Err(error.into()),
                // Nothing has arrived yet, which is the only chance to look at anything while a
                // reply is still being waited for.
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if self.cancelled() {
                        return Err(ChatError::Cancelled);
                    }
                    continue;
                }
                // The reader is gone without having said the body ended, which is the silence a dead
                // connection leaves, and is answered as one below.
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            };
            began = true;

            if self.cancelled() {
                return Err(ChatError::Cancelled);
            }

            let (bytes, _) = decoding.decode(piece);
            let written_before = reply.text.len();
            let thought_before = reply.thinking.len();

            for payload in decoder.push(&bytes) {
                // Keep reading a piece of a known kind that is malformed, so later completion and
                // usage pieces can establish its cost. Anything that is not JSON at all and does not
                // look like it is a keepalive.
                let Ok(value) = serde_json::from_str::<Value>(&payload) else {
                    if payload.trim_start().starts_with(['{', '[']) {
                        reply.unreadable = true;
                    }
                    continue;
                };
                match protocol::stream_event(&value) {
                    // Reported rather than read past. A reply the service abandoned ends the same
                    // way a dead connection does, so without this the cause is replaced by "the
                    // reply was cut off" and a request nothing can fix is asked for twice more.
                    Decoded::Event(Event::Failed(failure)) => {
                        return Err(ChatError::Reported { failure });
                    }
                    // The account's own words for a refusal, told as a refusal that came with a status
                    // is. Any other service has no such words, and a code it writes that is one of them is
                    // a failure it did not name.
                    Decoded::Event(Event::Refused(refusal)) => {
                        return Err(if self.account.is_some() {
                            ChatError::Account(crate::AccountFailure::Named(refusal))
                        } else {
                            ChatError::Reported {
                                failure: Failure::Unknown,
                            }
                        });
                    }
                    Decoded::Event(event) => reply.absorb(event),
                    Decoded::Unreadable => reply.unreadable = true,
                    Decoded::Ignored => {}
                }
                if reply.ending.is_some() && reply.reported.is_some() {
                    self.completed_usage = Some(reply.usage());
                }
            }

            progress(Progress {
                written: Labelled::new(&reply.text[written_before..], label),
                output_tokens: reply.output_so_far(),
                calling: reply
                    .calls
                    .last()
                    .map(|call| Labelled::new(call.name.as_str(), label)),
                thinking: Labelled::new(&reply.thinking[thought_before..], label),
                counted_by_server: reply.counted(),
                attempt,
            });
        }

        // Checked before the reply is taken apart, because the question is about the stream and not
        // about what it carried. A service that hangs up mid-reply leaves the same end of input as
        // one that finished, so without this a cut-off answer is returned as a whole one and the
        // tool call the model was writing simply vanishes.
        // Before the ending is looked for: a final event in a shape this cannot read leaves no
        // ending, and a protocol mismatch that repeats is not a connection that died.
        if reply.unreadable {
            return Err(ChatError::Decode {
                detail: "the reply contained content in a shape this does not read".to_string(),
            });
        }
        let Some(ending) = reply.ending else {
            return Err(ChatError::Incomplete);
        };

        let usage = reply.usage();
        let model = reply.model.clone().unwrap_or_else(|| self.model.clone());

        // A reply the service withheld for its content is a refusal, as it is where another service
        // says so, and is no figure's ceiling.
        if reply.withheld {
            return Err(ChatError::Reported {
                failure: Failure::Refusal,
            });
        }
        if ending == Ending::Incomplete {
            let thought = reply.thought || reply.reported.is_some_and(|usage| usage.reasoned());
            let open = reply
                .open_call()
                .map(|call| (call.name.as_str(), call.arguments.len()));
            let cut_off = self.cut_off(request, usage, open, thought);
            return what_was_written(reply.text, label, model, usage, cut_off);
        }

        // A whole reply holds no call that is still being written. One that does is a stream that
        // lost the end of an item, and is asked for again like any other that stopped short.
        if reply.open_call().is_some() {
            return Err(ChatError::Incomplete);
        }

        let calls = reply.finished_calls();
        if reply.text.is_empty() && calls.is_empty() {
            return Err(ChatError::NoContent);
        }

        Ok(Completion {
            content: Labelled::new(reply.text, label),
            model,
            calls,
            context_tokens: usage.prompt_tokens,
            usage,
            cut_off: None,
        })
    }
}

/// A call as it is being assembled from the stream.
#[derive(Debug)]
struct CallInProgress {
    /// The output item's place in the reply, which is how its pieces are matched.
    index: u64,
    id: String,
    name: String,
    /// The arguments as written so far, or in full once the item is done.
    arguments: String,
    done: bool,
}

/// A reply being assembled from the events of one stream.
#[derive(Debug, Default)]
struct Reply {
    text: String,
    /// The reasoning summary, for a screen and for nothing else.
    thinking: String,
    /// Calls in the order they began.
    calls: Vec<CallInProgress>,
    /// What the service reported with the final event.
    reported: Option<WireUsage>,
    /// Pieces of the reply's words that arrived, which is what a reply that states no figure is
    /// charged and the live figure until the service gives its own.
    tally: u64,
    /// Pieces of a call's arguments that arrived. Shown on a screen beside the words and never
    /// charged ([BACKEND-45](../../../docs/specs/backends.md#BACKEND-45)).
    argument_pieces: u64,
    /// How the stream ended, once it has.
    ending: Option<Ending>,
    model: Option<String>,
    /// Whether any reasoning arrived.
    thought: bool,
    /// Whether the service said it stopped the reply for its content.
    withheld: bool,
    /// Whether a known piece could not be decoded.
    ///
    /// Kept rather than failed on the spot so the stream is still drained: the reply is refused
    /// once it has ended, in the same place a reply that arrived whole is.
    unreadable: bool,
}

impl Reply {
    fn absorb(&mut self, event: Event) {
        match event {
            Event::Text(piece) => {
                self.tally += 1;
                self.text.push_str(&piece);
            }
            Event::Thinking(piece) => {
                self.thought = true;
                self.thinking.push_str(&piece);
            }
            Event::CallBegun { index, id, name } => {
                if self.call_at(index).is_none() {
                    self.calls.push(CallInProgress {
                        index,
                        id,
                        name,
                        arguments: String::new(),
                        done: false,
                    });
                }
            }
            Event::Arguments { index, piece } => {
                self.argument_pieces += 1;
                if let Some(call) = self.call_at(index) {
                    call.arguments.push_str(&piece);
                }
            }
            // The item in full, which stands in place of the pieces that led to it. A call whose
            // beginning was not seen is still a call.
            Event::CallDone {
                index,
                id,
                name,
                arguments,
            } => match self.call_at(index) {
                Some(call) => {
                    call.id = id;
                    call.name = name;
                    call.arguments = arguments;
                    call.done = true;
                }
                None => self.calls.push(CallInProgress {
                    index,
                    id,
                    name,
                    arguments,
                    done: true,
                }),
            },
            Event::Ended {
                ending,
                usage,
                model,
                withheld,
            } => {
                self.withheld = withheld;
                self.ending = Some(ending);
                self.reported = usage;
                self.model = model;
            }
            // A failure and a refusal end the stream where they arrive, in `stream_once`, before a
            // reply is built from anything after them.
            Event::Failed(_) | Event::Refused(_) => {}
        }
    }

    fn call_at(&mut self, index: u64) -> Option<&mut CallInProgress> {
        self.calls.iter_mut().find(|call| call.index == index)
    }

    /// Whether the figure is the service's rather than a tally of what arrived.
    fn counted(&self) -> bool {
        self.reported.is_some_and(|usage| usage.counts_the_reply())
    }

    /// What the reply cost, as the service reported it where it has and as tallied where not.
    fn usage(&self) -> Usage {
        let mut usage = self.reported.map(Usage::from).unwrap_or_default();
        if !self.counted() {
            usage.completion_tokens = self.tally;
        }
        usage
    }

    /// Output as best it can be known while the reply arrives: the service's figure once it has
    /// given one, and until then the pieces of text and of argument that have arrived.
    fn output_so_far(&self) -> u64 {
        if self.counted() {
            self.usage().completion_tokens
        } else {
            self.tally + self.argument_pieces
        }
    }

    /// The call the reply was still writing when it stopped, where one was open.
    fn open_call(&self) -> Option<&CallInProgress> {
        self.calls.iter().rev().find(|call| !call.done)
    }

    /// The calls the reply completed, as the agent holds them.
    ///
    /// Only those whose item is done: a call still being written is not one the model asked for.
    fn finished_calls(&self) -> Vec<ToolCall> {
        self.calls
            .iter()
            .filter(|call| call.done)
            .map(|call| protocol::call_from(&call.id, &call.name, &call.arguments))
            .collect()
    }
}
