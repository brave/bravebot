//! Client for Google's native `generateContent` API.
//!
//! The service a `google-vertex` entry naming no project reaches: Vertex AI express mode, which
//! takes an API key and nothing else. It answers the same questions as [`crate::AichatClient`] and
//! returns the same [`Completion`], so a caller chooses between them once and the turn loop is
//! unchanged. What differs is underneath: the `generateContent` protocol rather than chat
//! completions, a key in `x-goog-api-key`, the model named in the path rather than the body, and a
//! tool call that arrives whole with a signature to send back.
//!
//! Every request goes through [`bravebot_net::Egress`], so the policy gate sees this traffic
//! exactly as it sees the other backends'. The reply is labelled untrusted-public and nothing here
//! reads it to decide anything: this module speaks the wire protocol and mints no witness.
//!
//! The retry helpers, the memory of what a model has refused, and the SSE decoder are the aichat
//! client's and are used here. A request refused on its contents is sent again once without the
//! thinking budget, since which models read one is not listed anywhere, and what answered is
//! remembered.

pub mod protocol;

use crate::anthropic::what_was_written;
use crate::call::{EffortRefused, Pace, Spent};
use crate::protocol::{ChatRequest, SseDecoder, ToolCall, Usage};
use crate::{
    ATTEMPTS, BACKOFF, ChatError, Completion, CutOff, Progress, WAKE, learned_key, log_retry,
    pause_before_retry, read_in_background, reply_bound, worth_another_attempt,
};
use bravebot_config::Secret;
use bravebot_config::provider::Provider;
use bravebot_core::cancel::Cancel;
use bravebot_core::event::Sink;
use bravebot_core::label::Label;
use bravebot_core::policy::Policy;
use bravebot_core::value::Labelled;
use bravebot_net::{Egress, EgressError, Request};
use protocol::{Decoded, FINISH_MAX_TOKENS, Response, WireUsage};
use serde_json::Value;
use std::time::Duration;

/// How long a reply is allowed to run where nobody stated a ceiling: what the largest model this
/// API serves writes. It bounds how long a reply is waited for and is never sent.
const ASSUMED_CEILING: u64 = 65_536;

/// Whether a failed attempt should be repeated, when `allowed` requests may be sent in all.
///
/// What the other clients ask, and a failure the service reports part way through a reply that a
/// second attempt could get past.
fn worth_asking_again(allowed: u32, attempt: u32, error: &ChatError) -> bool {
    worth_another_attempt(allowed, attempt, error)
        || (attempt < allowed
            && matches!(error, ChatError::Reported { failure } if failure.is_transient()))
}

/// One client for one request, to the service a `google-vertex` entry naming no project reaches.
pub struct GenerateContentClient<'a> {
    spent: Spent,
    egress: &'a Egress,
    provider: &'a Provider,
    /// What the service calls the model, which is the name the request's path carries.
    ///
    /// Resolved by whoever chose the provider, because the name a session holds may be qualified
    /// by the provider's own id to say which service is meant, and that qualified form is one the
    /// service has never heard of.
    model: String,
    token: Option<Secret>,
    /// The headers the block's `options.headers` asked for, resolved by the caller (BACKEND-60).
    headers: Vec<(String, Secret)>,
    /// Whether requests still carry the thinking budget a turn's level asks for, and what the model
    /// was known to refuse before this request.
    refused: EffortRefused,
    /// The stop to watch for and how long to wait before the first retry, which is [`BACKOFF`]
    /// unless a test shortens it.
    pace: Pace,
}

impl<'a> GenerateContentClient<'a> {
    /// A client for `provider`, asking for `model` by the name that service knows it by, and
    /// carrying `token` as the key where the entry named a place for one to live.
    ///
    /// The token is taken rather than derived, as the aichat client's is: the caller has already
    /// had to find it to know whether the request can be sent at all.
    pub fn new(
        egress: &'a Egress,
        provider: &'a Provider,
        model: impl Into<String>,
        token: Option<Secret>,
    ) -> Self {
        Self {
            spent: Spent::default(),
            egress,
            provider,
            model: model.into(),
            token,
            headers: Vec::new(),
            refused: EffortRefused::default(),
            pace: Pace::new(BACKOFF),
        }
    }

    /// Requests handed to egress in the last call, including capability probes.
    pub fn attempts(&self) -> u32 {
        self.spent.attempts
    }

    /// Total reported usage of completed attempts in the last call, including retries.
    /// Incomplete attempts add nothing. Returns `None` if no completed usage was reported.
    pub fn completed_usage(&self) -> Option<Usage> {
        self.spent.total()
    }

    /// The final attempt's measured prompt size, separate from the cost of earlier attempts.
    pub fn last_request_tokens(&self) -> Option<u64> {
        self.spent.last_prompt_tokens()
    }

    /// Stop reading a streamed reply as soon as this says to.
    pub fn with_cancel(mut self, cancel: Cancel) -> Self {
        self.pace.cancel = Some(cancel);
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
        self.pace.backoff = backoff;
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
        let key = self.refusal_key();
        self.refused.recall(&key);
    }

    /// Settle what a probe found, once the request it was part of has finished one way or the other.
    fn probe_settled(&mut self, probed: bool, failed: bool) {
        let key = self.refusal_key();
        self.refused.probe_settled(&key, probed, failed);
    }

    /// Whether this failure is worth sending the same request again without its thinking budget.
    ///
    /// Only where the request carried one: a request without a level was not refused for it, and
    /// giving it up there would record a refusal nobody saw.
    fn worth_dropping_effort(&self, request: &ChatRequest, error: &ChatError) -> bool {
        self.refused.worth_dropping(request, error)
    }

    /// The ceiling somebody stated for this model, exported or in its block.
    fn stated_ceiling(&self) -> Option<u64> {
        self.provider.stated_output_limit(&self.model)
    }

    /// The body one request carries, before it is encoded.
    ///
    /// Separate from [`GenerateContentClient::build`] so what a configuration puts in a request can
    /// be read without sending it.
    fn contents_for(&self, request: &ChatRequest) -> protocol::GenerateContentRequest {
        protocol::request_from(&request.messages, request.tools.as_deref())
            .with_ceiling(self.stated_ceiling())
            .with_effort(request.effort.filter(|_| self.refused.effort))
            .with_schema(
                request
                    .response_format
                    .as_ref()
                    .map(crate::protocol::ResponseFormat::schema),
            )
    }

    /// The request for one attempt, addressed and authenticated.
    ///
    /// The options a model's block carries are merged into the body and never interpreted, one
    /// level down for an object the turn also wrote into. They come from the settings file, which is
    /// the person's own configuration surface, so they are trusted exactly as far as a variable they
    /// exported would be. A key the request already set wins, because a configuration must not
    /// rewrite the conversation a turn built.
    fn build(&self, request: &ChatRequest, streaming: bool) -> Result<Request, ChatError> {
        // A model whose name could move the request has no URL, and nothing is sent: the key goes
        // only where the entry names.
        let url = self
            .provider
            .generate_content_url(&self.model, streaming)
            .ok_or_else(|| {
                ChatError::Encode(
                    "the model name holds a character that cannot go in a request path".to_string(),
                )
            })?;
        let mut body = serde_json::to_value(self.contents_for(request))
            .map_err(|error| ChatError::Encode(error.to_string()))?;
        if let (Some(object), Some(options)) = (
            body.as_object_mut(),
            self.provider
                .model(&self.model)
                .and_then(|model| model.options.as_ref()),
        ) {
            for (key, value) in options {
                match (object.get_mut(key), value) {
                    // The generation settings are one object the turn also writes into, so a
                    // setting the options add to it, such as a temperature, joins what the turn
                    // set and never replaces it.
                    (Some(Value::Object(built)), Value::Object(added)) => {
                        for (name, setting) in added {
                            built.entry(name.clone()).or_insert_with(|| setting.clone());
                        }
                    }
                    (Some(_), _) => {}
                    (None, _) => {
                        object.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        let body =
            serde_json::to_vec(&body).map_err(|error| ChatError::Encode(error.to_string()))?;

        let mut http = Request::post(url, body).header("content-type", "application/json");
        for (name, value) in self.provider.fixed_headers() {
            http = http.header(*name, *value);
        }
        if let Some(token) = self.token.as_ref().map(Secret::expose) {
            // Attached to the socket the request was addressed to and to no other, so a hop to
            // another host does not carry the key along. `authorization` gets that from the egress
            // layer, which does not know this header is a credential.
            let (name, value) = self.provider.credential_header(token);
            http = http.socket_header(name, value);
        }
        // After the credential, and never over it: a name this request already set is refused
        // where the block is read, because `Request::header` appends and a second copy of one of
        // these would be sent rather than replace the first.
        for (name, value) in &self.headers {
            http = http.header(name.clone(), value.expose());
        }
        let bound = reply_bound(self.stated_ceiling().unwrap_or(ASSUMED_CEILING));
        // A stream's headers come at once and the model writes after them; a reply asked for whole
        // is written before any of it is sent.
        let mut http = if streaming {
            http.stream_within(bound)
        } else {
            http.reply_within(bound)
        };
        if let Some(gap) = self.provider.patience.idle_timeout {
            http = http.idle_within(gap);
        }
        Ok(http)
    }

    /// Send a request and wait for the whole reply.
    pub fn complete<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        request: &ChatRequest,
    ) -> Result<Completion, ChatError> {
        self.spent.begin_call();
        self.recall();
        let mut attempt = 1;
        let mut probed = false;
        loop {
            match self.complete_once(policy, request) {
                // Sent again immediately rather than after a wait: nothing is busy, the request was
                // simply carrying something this model does not take.
                Err(error) if self.worth_dropping_effort(request, &error) => {
                    self.refused.effort = false;
                    probed = true;
                }
                Err(error) if worth_asking_again(self.attempts_allowed(), attempt, &error) => {
                    let pause = pause_before_retry(self.pace.backoff, attempt, &error);
                    log_retry(attempt + 1, pause);
                    if !self.pace.wait(pause) {
                        return Err(ChatError::Cancelled);
                    }
                    attempt += 1;
                }
                result => {
                    self.probe_settled(probed, result.is_err());
                    return result.map(|mut completion| {
                        self.spent.bill(&mut completion);
                        completion
                    });
                }
            }
        }
    }

    fn complete_once<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        request: &ChatRequest,
    ) -> Result<Completion, ChatError> {
        // `completed_usage` holds only the attempt that just ended and the next one replaces it, so
        // what the call has been billed so far is kept apart from it.
        self.spent.begin_attempt();
        if self.pace.cancelled() {
            return Err(ChatError::Cancelled);
        }
        let http = self.build(request, false)?;
        self.spent.sent();
        let response = self.egress.fetch(policy, http, Label::untrusted_public())?;

        // The envelope is protocol, like the JSON envelope in the other backends: the bytes come
        // out to find the reply inside, and the reply is relabelled with exactly the label it
        // arrived under.
        let label = response.body.label();
        let (bytes, label) = policy
            .decode_transport("generateContent", label)
            .decode(response.body);

        // Decode the bill independently of the assistant's content.
        let envelope: Value =
            serde_json::from_slice(&bytes).map_err(|error| ChatError::Decode {
                detail: format!("{error} (received {} bytes)", bytes.len()),
            })?;
        self.spent
            .record(reported_usage(&envelope).map(Usage::from));

        let parsed: Response =
            serde_json::from_value(envelope).map_err(|error| ChatError::Decode {
                detail: format!("{error} (received {} bytes)", bytes.len()),
            })?;

        if let Some(failure) = protocol::unfinished(&parsed) {
            return Err(ChatError::Reported { failure });
        }

        let (content, calls, mut thought) = protocol::parts_of(&parsed);
        thought |= parsed.usage_metadata.is_some_and(|usage| usage.reasoned());
        let usage = parsed.usage_metadata.map(Usage::from).unwrap_or_default();

        if protocol::finish_of(&parsed) == Some(FINISH_MAX_TOKENS) {
            let cut_off = self.cut_off(usage, thought);
            return what_was_written(content, label, self.model.clone(), usage, cut_off);
        }
        if content.is_empty() && calls.is_empty() {
            return Err(ChatError::NoContent);
        }

        Ok(Completion {
            content: Labelled::new(content, label),
            model: self.model.clone(),
            calls,
            context_tokens: usage.prompt_tokens,
            usage,
            cut_off: None,
        })
    }

    /// What a reply the ceiling stopped was doing, from its structure.
    ///
    /// A call arrives whole, so none is ever open. The ceiling is the one somebody stated, and
    /// otherwise what the reply had written when the model stopped, since no figure was sent and
    /// the model's own is not told.
    fn cut_off(&self, usage: Usage, thought: bool) -> CutOff {
        CutOff {
            ceiling: self.stated_ceiling().unwrap_or(usage.completion_tokens),
            call: None,
            thought,
        }
    }

    /// Send a request and read the reply as it arrives.
    ///
    /// Identical to [`GenerateContentClient::complete`] in what it produces and the gates it passes.
    /// `progress` is called as pieces land so a caller can show the reply arriving.
    pub fn complete_streaming<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        request: &ChatRequest,
        mut progress: impl FnMut(Progress),
    ) -> Result<Completion, ChatError> {
        self.spent.begin_call();
        self.recall();
        let mut attempt = 1;
        let mut probed = false;
        loop {
            match self.stream_once(policy, request, attempt, &mut progress) {
                Err(error) if self.worth_dropping_effort(request, &error) => {
                    self.refused.effort = false;
                    probed = true;
                }
                Err(error) if worth_asking_again(self.attempts_allowed(), attempt, &error) => {
                    attempt += 1;
                    let pause = pause_before_retry(self.pace.backoff, attempt - 1, &error);
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
                    if !self.pace.wait(pause) {
                        return Err(ChatError::Cancelled);
                    }
                }
                result => {
                    self.probe_settled(probed, result.is_err());
                    return result.map(|mut completion| {
                        self.spent.bill(&mut completion);
                        completion
                    });
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
        self.spent.begin_attempt();
        // Before the request is built, let alone sent. A stop that landed while the last attempt was
        // failing is still a stop.
        if self.pace.cancelled() {
            return Err(ChatError::Cancelled);
        }

        let http = self
            .build(request, true)?
            .header("accept", "text/event-stream");
        self.spent.sent();
        let stream = self.egress.fetch_streaming(
            policy,
            http,
            Label::untrusted_public(),
            self.pace.cancel.as_ref(),
        )?;
        let label = stream.label();

        let arriving = read_in_background(stream);

        let mut decoder = SseDecoder::new();
        let mut reply = Reply::default();
        // One envelope, arriving in frames, so it is authorised once rather than once a frame.
        let decoding = policy.decode_transport("generateContent stream", Label::untrusted_public());
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
                    if self.pace.cancelled() {
                        return Err(ChatError::Cancelled);
                    }
                    continue;
                }
                // The reader is gone without having said the body ended, which is the silence a dead
                // connection leaves, and is answered as one below.
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            };
            began = true;

            if self.pace.cancelled() {
                return Err(ChatError::Cancelled);
            }

            let (bytes, _) = decoding.decode(piece);
            let written_before = reply.text.len();

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
                match protocol::chunk(&value) {
                    // Reported rather than read past. A reply the service abandoned ends the same
                    // way a dead connection does, so without this the cause is replaced by "the
                    // reply was cut off" and a request nothing can fix is asked for twice more.
                    Decoded::Failed(failure) => return Err(ChatError::Reported { failure }),
                    Decoded::Chunk(response) => reply.absorb(&response),
                    Decoded::Unreadable => reply.unreadable = true,
                    Decoded::Ignored => {}
                }
                if reply.reported_in_full() {
                    self.spent.record(Some(reply.usage()));
                }
            }

            progress(Progress {
                written: Labelled::new(&reply.text[written_before..], label),
                output_tokens: reply.output_so_far(),
                calling: reply
                    .calls
                    .last()
                    .map(|call| Labelled::new(call.function.name.as_str(), label)),
                thinking: Labelled::new("", label),
                counted_by_server: reply.counted,
                attempt,
            });
        }

        // Checked before the reply is taken apart, because the question is about the stream and not
        // about what it carried. A service that hangs up mid-reply leaves the same end of input as
        // one that finished, so without this a cut-off answer is returned as a whole one.
        if !reply.ended {
            return Err(ChatError::Incomplete);
        }

        if reply.unreadable {
            return Err(ChatError::Decode {
                detail: "the reply contained content in a shape this does not read".to_string(),
            });
        }

        if let Some(failure) = reply.failure {
            return Err(ChatError::Reported { failure });
        }

        let usage = reply.usage();

        if reply.finish.as_deref() == Some(FINISH_MAX_TOKENS) {
            let thought = reply.thought || reply.reported.reasoned();
            let cut_off = self.cut_off(usage, thought);
            return what_was_written(reply.text, label, self.model.clone(), usage, cut_off);
        }

        if reply.text.is_empty() && reply.calls.is_empty() {
            return Err(ChatError::NoContent);
        }

        Ok(Completion {
            content: Labelled::new(reply.text, label),
            model: self.model.clone(),
            calls: reply.calls,
            context_tokens: usage.prompt_tokens,
            usage,
            cut_off: None,
        })
    }
}

/// Validate reported counts independently of assistant content.
fn reported_usage(envelope: &Value) -> Option<WireUsage> {
    let value = envelope.get("usageMetadata")?;
    value.get("promptTokenCount")?.as_u64()?;
    serde_json::from_value(value.clone()).ok()
}

/// A reply being assembled from the pieces of one stream.
#[derive(Debug, Default)]
struct Reply {
    text: String,
    /// Calls in the order they arrived, each whole.
    calls: Vec<ToolCall>,
    /// What the service has reported so far, each report replacing what it restates.
    reported: WireUsage,
    /// Whether the reply's count is the service's rather than a tally of what arrived.
    counted: bool,
    /// Pieces of text that arrived, tallied for the live figure until the service gives its own.
    tally: u64,
    /// Whether a piece said the reply was over: it gave a finish reason, or refused the prompt.
    ended: bool,
    finish: Option<String>,
    /// Why the service stopped the reply without finishing it, where it did.
    failure: Option<crate::anthropic::protocol::Failure>,
    /// Whether any reasoning arrived.
    thought: bool,
    /// Whether a known piece could not be decoded.
    ///
    /// Kept rather than failed on the spot so the stream is still drained: the reply is refused
    /// once it has ended, in the same place a reply that arrived whole is.
    unreadable: bool,
}

impl Reply {
    /// Whether the reply is over and the service has said what the prompt cost. The service leaves
    /// out a count that is zero, so a reply that wrote nothing states no output figure, and the
    /// figures it did state are the whole of the bill.
    fn reported_in_full(&self) -> bool {
        self.ended && self.reported.prompt_token_count.is_some()
    }

    /// What the reply cost, as the service reported it where it has and as tallied where not.
    fn usage(&self) -> Usage {
        let mut usage = Usage::from(self.reported);
        if !self.counted && !self.reported_in_full() {
            usage.completion_tokens = self.tally;
        }
        usage
    }

    /// Output as best it can be known while the reply arrives: the service's figure once it has
    /// given one, and until then the pieces of text and the calls that have arrived.
    fn output_so_far(&self) -> u64 {
        if self.counted {
            Usage::from(self.reported).completion_tokens
        } else {
            self.tally + self.calls.len() as u64
        }
    }

    fn absorb(&mut self, response: &Response) {
        if let Some(usage) = response.usage_metadata {
            self.counted |= usage.counts_the_reply();
            self.reported = self.reported.then(usage);
        }
        let (text, calls, thought) = protocol::parts_of(response);
        if !text.is_empty() {
            self.tally += 1;
        }
        self.text.push_str(&text);
        self.calls.extend(calls);
        self.thought |= thought;
        if let Some(failure) = protocol::unfinished(response) {
            self.failure = Some(failure);
            self.ended = true;
        }
        if let Some(finish) = protocol::finish_of(response) {
            self.finish = Some(finish.to_string());
            self.ended = true;
        }
    }
}
