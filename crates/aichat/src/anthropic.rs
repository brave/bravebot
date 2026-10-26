//! Client for Anthropic's Messages API.
//!
//! The service an `anthropic` provider entry names. It answers the same questions as
//! [`crate::AichatClient`] and returns the same [`Completion`], so a caller chooses between them
//! once and the turn loop is unchanged. What differs is underneath: the Messages protocol rather
//! than chat completions, a key in `x-api-key` beside the version of the API the request is
//! written for, and a reply ceiling every request has to state.
//!
//! Every request goes through [`bravebot_net::Egress`], so the policy gate sees this traffic
//! exactly as it sees the other backends'. The reply is labelled untrusted-public and nothing here
//! reads it to decide anything: this module speaks the wire protocol and mints no witness.
//!
//! The retry helpers, the memory of what a model has refused, and the SSE decoder are the aichat
//! client's and are used here. The two loops that send a request, retry it and give up what a model
//! refuses are written for this protocol, following the aichat and Bedrock clients' in shape.
//!
//! A request refused on its contents is sent again one concession at a time, and the order is how
//! likely this API is to refuse each: the level, which models released before it refuse; the reply
//! ceiling, which models with a lower maximum refuse; the cache lifetime; the breakpoints, which
//! every model reads; and last the ask for arguments as they are written. What answered is
//! remembered, so a field that was never the cause is only recorded where the request that
//! succeeded went without it, and the fields the API is least likely to refuse are the last ones to
//! be blamed.

pub mod protocol;

use crate::call::{Concession, Driven, Pace, Plan, Spent, drive};
use crate::protocol::{ChatRequest, SseDecoder, Usage};
use crate::{
    ATTEMPTS, BACKOFF, ChatError, Completion, CutOff, OpenCall, Progress, Refusals, WAKE,
    learned_key, log_retry, pause_before_retry, read_in_background, refuses_the_body, remember,
    remembered, reply_bound, worth_another_attempt,
};
use bravebot_config::provider::Provider;
use bravebot_config::{CacheTtl, Config, Secret};
use bravebot_core::cancel::Cancel;
use bravebot_core::event::Sink;
use bravebot_core::label::Label;
use bravebot_core::policy::Policy;
use bravebot_core::value::Labelled;
use bravebot_net::{Egress, EgressError, Request};
use protocol::{
    BlockStart, Decoded, Delta, Failure, MessageResponse, MessagesRequest,
    STOP_REASON_CONTEXT_FULL, STOP_REASON_MAX_TOKENS, STOP_REASON_REFUSAL, StreamEvent, WireUsage,
};
use serde_json::Value;
use std::time::Duration;

/// The status the service answers with when it is overloaded.
///
/// Not among the statuses the egress layer counts as worth asking again, being one only this service
/// uses, and the commonest way it fails: the API documents it as the service being busy for everybody
/// and not as anything wrong with the request.
const OVERLOADED: u16 = 529;

/// Whether a failed attempt should be repeated, when `allowed` requests may be sent in all.
///
/// What the other clients ask, a service that is overloaded besides, and a failure it reports part
/// way through a reply that a second attempt could get past.
fn worth_asking_again(allowed: u32, attempt: u32, error: &ChatError) -> bool {
    worth_another_attempt(allowed, attempt, error)
        || (attempt < allowed
            && match error {
                ChatError::Egress(EgressError::Status { status, .. }) => *status == OVERLOADED,
                ChatError::Reported { failure } => failure.is_transient(),
                _ => false,
            })
}

/// One client for one request, to the service an `anthropic` entry names.
pub struct AnthropicClient<'a> {
    spent: Spent,
    config: &'a Config,
    egress: &'a Egress,
    provider: &'a Provider,
    /// What the service calls the model, which is the name the request carries.
    ///
    /// Resolved by whoever chose the provider, because the name a session holds may be qualified
    /// by the provider's own id to say which service is meant, and that qualified form is one the
    /// service has never heard of.
    model: String,
    token: Option<Secret>,
    /// The headers the block's `options.headers` asked for, resolved by the caller (BACKEND-60).
    headers: Vec<(String, Secret)>,
    /// What this model has refused so far in the call in progress, loaded and settled by
    /// [`call::drive`].
    ///
    /// Nothing is refused until a model says so, which is the only way to find out: nothing in the
    /// listing says which models read a breakpoint, a lifetime on one, or a level. The listing says
    /// which models read a level, and a request is sent without one for a model it says reads none,
    /// so a level is only refused here for a roster that was not asked or is out of date. A model
    /// that refuses the assumed ceiling is sent the fallback.
    refusals: Refusals,
    /// The stop to watch for and how long to wait before the first retry, which is [`BACKOFF`]
    /// unless a test shortens it.
    pace: Pace,
}

impl Driven for AnthropicClient<'_> {
    type Error = ChatError;

    fn cancelled() -> ChatError {
        ChatError::Cancelled
    }

    fn spent_mut(&mut self) -> &mut Spent {
        &mut self.spent
    }

    fn pace(&self) -> &Pace {
        &self.pace
    }

    fn refusals(&self) -> &Refusals {
        &self.refusals
    }

    fn refusals_mut(&mut self) -> &mut Refusals {
        &mut self.refusals
    }

    fn recall(&self, key: &str) -> Refusals {
        remembered(key)
    }

    fn remember(&self, key: &str, found: Refusals) {
        remember(key, found);
    }

    /// Only a refusal of the body is answered by giving something up, and only where the request
    /// carried what would be given up: a request without a level was not refused for it, and
    /// giving it up there would record a refusal nobody saw. The ceiling only where it was the
    /// assumed one, a figure somebody stated being their statement about the model and quietly
    /// sending less cutting replies short that they said could run. The lifetime only while
    /// breakpoints are sent and a lifetime was chosen.
    fn refused(&self, request: &ChatRequest, concession: Concession, error: &ChatError) -> bool {
        refuses_the_body(error)
            && match concession {
                Concession::Effort => request.effort.is_some(),
                Concession::Ceiling => self.stated_ceiling().is_none(),
                Concession::CacheTtl => !self.refusals.caching && self.cache_ttl_asked().is_some(),
                Concession::Breakpoints => true,
                Concession::ArgumentsAsWritten => self.streams_arguments(request, true),
                Concession::ThinkingShown => false,
            }
    }

    fn asks_again(&self, attempt: u32, error: &ChatError) -> bool {
        worth_asking_again(self.attempts_allowed(), attempt, error)
    }

    fn pause(&self, failures: u32, error: &ChatError) -> Duration {
        pause_before_retry(self.pace.backoff, failures, error)
    }

    fn log_retry(&self, attempt: u32, pause: Duration) {
        log_retry(attempt, pause);
    }
}

impl<'a> AnthropicClient<'a> {
    /// A client for `provider`, asking for `model` by the name that service knows it by, and
    /// carrying `token` as the key where the entry named a place for one to live.
    ///
    /// The token is taken rather than derived, as the aichat client's is: the caller has already
    /// had to find it to know whether the request can be sent at all, and `None` is it saying the
    /// entry names nowhere for one to live.
    pub fn new(
        config: &'a Config,
        egress: &'a Egress,
        provider: &'a Provider,
        model: impl Into<String>,
        token: Option<Secret>,
    ) -> Self {
        Self {
            spent: Spent::default(),
            config,
            egress,
            provider,
            model: model.into(),
            token,
            headers: Vec::new(),
            refusals: Refusals::default(),
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

    /// The order a refusal of the body is answered in by [`AnthropicClient::complete`]: the level,
    /// which models released before it refuse; the reply ceiling, which models with a lower
    /// maximum refuse; the cache lifetime, ahead of the breakpoints that carry it so that a model
    /// reading a breakpoint and refusing a lifetime keeps its caching at its own default; and the
    /// breakpoints.
    const CONCESSIONS: [Concession; 4] = [
        Concession::Effort,
        Concession::Ceiling,
        Concession::CacheTtl,
        Concession::Breakpoints,
    ];

    /// The same for [`AnthropicClient::complete_streaming`], with the ask for arguments as they
    /// are written last: the thing a refusal is least likely to be about.
    const STREAMING_CONCESSIONS: [Concession; 5] = [
        Concession::Effort,
        Concession::Ceiling,
        Concession::CacheTtl,
        Concession::Breakpoints,
        Concession::ArgumentsAsWritten,
    ];

    /// The cache lifetime a request asks for: the one the settings chose, for a model that has not
    /// refused it.
    fn cache_ttl_asked(&self) -> Option<CacheTtl> {
        self.config
            .prompt_cache_ttl
            .filter(|_| !self.refusals.cache_ttl)
    }

    /// Whether this request asks for tool arguments as they are written.
    ///
    /// Streamed requests that offer a tool, and only those: a whole reply arrives at once however
    /// its arguments are sent, so there the ask buys nothing and gives up the service's own check
    /// that an argument parses.
    fn streams_arguments(&self, request: &ChatRequest, streaming: bool) -> bool {
        streaming
            && !self.refusals.arguments_as_written
            && request
                .tools
                .as_deref()
                .is_some_and(|tools| !tools.is_empty())
    }

    /// The ceiling somebody stated for this model, exported or in its block.
    fn stated_ceiling(&self) -> Option<u64> {
        self.provider.stated_output_limit(&self.model)
    }

    /// The ceiling a request to this model carries.
    fn ceiling_for(&self) -> u64 {
        self.stated_ceiling().unwrap_or(if self.refusals.ceiling {
            bravebot_config::bedrock::OUTPUT_LIMIT_FALLBACK
        } else {
            bravebot_config::bedrock::OUTPUT_LIMIT
        })
    }

    /// The body one request carries, before it is encoded.
    ///
    /// Separate from [`AnthropicClient::build`] so what a configuration puts in a request can be
    /// read without sending it: the ceiling, the level and the breakpoints are decided here.
    fn messages_for(&self, request: &ChatRequest, streaming: bool) -> MessagesRequest {
        let messages =
            protocol::request_from(&self.model, &request.messages, request.tools.as_deref())
                .with_ceiling(self.ceiling_for())
                .with_effort(request.effort.filter(|_| !self.refusals.effort))
                .with_cache_ttl(self.cache_ttl_asked());
        let messages = if self.streams_arguments(request, streaming) {
            messages.with_arguments_as_written()
        } else {
            messages
        };
        let messages = if streaming {
            messages.streamed()
        } else {
            messages
        };
        let messages = if request.conversation_is_sent_again {
            messages
        } else {
            messages.without_the_conversation_breakpoint()
        };
        if self.refusals.caching {
            messages.without_breakpoints()
        } else {
            messages
        }
    }

    /// The request for one attempt, addressed and authenticated.
    ///
    /// The options a model's block carries are merged at the top level of the body and never
    /// interpreted. They come from the settings file, which is the person's own configuration
    /// surface, so they are trusted exactly as far as a variable they exported would be. A key the
    /// request already set wins, because a configuration must not rewrite the model or the messages
    /// a turn built.
    fn build(&self, request: &ChatRequest, streaming: bool) -> Result<Request, ChatError> {
        let messages = self.messages_for(request, streaming);
        let bound = reply_bound(messages.max_tokens);
        let mut body = serde_json::to_value(&messages)
            .map_err(|error| ChatError::Encode(error.to_string()))?;
        if let (Some(object), Some(options)) = (
            body.as_object_mut(),
            self.provider
                .model(&self.model)
                .and_then(|model| model.options.as_ref()),
        ) {
            for (key, value) in options {
                object.entry(key.clone()).or_insert_with(|| value.clone());
            }
        }
        let body =
            serde_json::to_vec(&body).map_err(|error| ChatError::Encode(error.to_string()))?;

        let mut http = Request::post(self.provider.completion_url(), body)
            .header("content-type", "application/json");
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
        let key = self.refusal_key();
        let plan = Plan {
            key: &key,
            order: &Self::CONCESSIONS,
        };
        drive(
            self,
            request,
            plan,
            policy,
            |client, policy, _| client.complete_once(policy, request),
            |_, _| {},
        )
    }

    fn complete_once<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        request: &ChatRequest,
    ) -> Result<Completion, ChatError> {
        let http = self.build(request, false)?;
        self.spent.sent();
        let response = self.egress.fetch(policy, http, Label::untrusted_public())?;

        // The envelope is protocol, like the JSON envelope in the other backends: the bytes come
        // out to find the reply inside, and the reply is relabelled with exactly the label it
        // arrived under.
        let label = response.body.label();
        let (bytes, label) = policy
            .decode_transport("messages", label)
            .decode(response.body);

        // Decode the bill independently of the assistant's content.
        let envelope: Value =
            serde_json::from_slice(&bytes).map_err(|error| ChatError::Decode {
                detail: format!("{error} (received {} bytes)", bytes.len()),
            })?;
        self.spent
            .record(reported_usage(&envelope).map(Usage::from));

        let parsed: MessageResponse =
            serde_json::from_value(envelope).map_err(|error| ChatError::Decode {
                detail: format!("{error} (received {} bytes)", bytes.len()),
            })?;

        let stop = parsed.stop_reason.as_deref();
        if let Some(failure) = unfinished(stop) {
            return Err(ChatError::Reported { failure });
        }

        let (content, calls) = protocol::parts_of(&parsed.content);
        let usage = parsed.usage.map(Usage::from).unwrap_or_default();
        let model = parsed.model.unwrap_or_else(|| self.model.clone());

        if stop == Some(STOP_REASON_MAX_TOKENS) {
            // A call is open where the reply's last block is one: whatever came before it, the
            // model was writing that call when it ran out. A whole reply carries its arguments
            // parsed, so they are counted written back out.
            let open = match parsed.content.last() {
                Some(protocol::ReplyBlock::ToolUse { name, input, .. }) => {
                    Some((name.as_str(), input.to_string().len()))
                }
                _ => None,
            };
            let thought = parsed
                .content
                .iter()
                .any(protocol::ReplyBlock::is_reasoning);
            let cut_off = self.cut_off(request, open, thought);
            return what_was_written(content, label, model, usage, cut_off);
        }
        if content.is_empty() && calls.is_empty() {
            return Err(ChatError::NoContent);
        }

        Ok(Completion {
            content: Labelled::new(content, label),
            model,
            calls,
            context_tokens: usage.prompt_tokens,
            usage,
            cut_off: None,
        })
    }

    /// What a reply the ceiling stopped was doing, from its structure.
    ///
    /// `open` is the name of the call it was writing, where one was open, and how many bytes of
    /// its arguments had arrived. The name is looked up in the request's own list and the
    /// request's copy kept, so what travels on is a name this program offered and never one the
    /// reply spelt.
    fn cut_off(&self, request: &ChatRequest, open: Option<(&str, usize)>, thought: bool) -> CutOff {
        CutOff {
            ceiling: self.ceiling_for(),
            call: open.map(|(name, arguments)| OpenCall {
                tool: request.offered(name),
                arguments,
            }),
            thought,
        }
    }

    /// Send a request and read the reply as it arrives.
    ///
    /// Identical to [`AnthropicClient::complete`] in what it produces and the gates it passes.
    /// `progress` is called as events land so a caller can show the reply arriving.
    pub fn complete_streaming<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        request: &ChatRequest,
        mut progress: impl FnMut(Progress),
    ) -> Result<Completion, ChatError> {
        self.spent.begin_call();
        let key = self.refusal_key();
        let plan = Plan {
            key: &key,
            order: &Self::STREAMING_CONCESSIONS,
        };
        drive(
            self,
            request,
            plan,
            &mut progress,
            |client, progress, attempt| client.stream_once(policy, request, attempt, progress),
            |progress, attempt| {
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
            },
        )
    }

    fn stream_once<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        request: &ChatRequest,
        attempt: u32,
        progress: &mut impl FnMut(Progress),
    ) -> Result<Completion, ChatError> {
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
        let decoding = policy.decode_transport("messages stream", Label::untrusted_public());
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
                // Keep reading a frame of a known kind that is malformed, so later completion and
                // usage frames can establish its cost. Anything that is not JSON at all and does not
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
                    Decoded::Event(StreamEvent::Failed { failure }) => {
                        return Err(ChatError::Reported { failure });
                    }
                    Decoded::Event(event) => reply.absorb(event),
                    Decoded::Unreadable => reply.unreadable = true,
                    Decoded::Ignored => {}
                }
                if reply.ended && reply.counted {
                    self.spent.record(Some(reply.usage()));
                }
            }

            progress(Progress {
                written: Labelled::new(&reply.text[written_before..], label),
                output_tokens: reply.output_so_far(),
                calling: reply.calling().map(|name| Labelled::new(name, label)),
                thinking: Labelled::new("", label),
                counted_by_server: reply.counted,
                attempt,
            });
        }

        // Checked before the reply is taken apart, because the question is about the stream and not
        // about what it carried. A service that hangs up mid-reply leaves the same end of input as
        // one that finished, so without this a cut-off answer is returned as a whole one and the
        // tool call the model was writing simply vanishes.
        if !reply.ended {
            return Err(ChatError::Incomplete);
        }

        if reply.unreadable {
            return Err(ChatError::Decode {
                detail: "the reply contained content in a shape this does not read".to_string(),
            });
        }

        let stop = reply.stop_reason.as_deref();
        if let Some(failure) = unfinished(stop) {
            return Err(ChatError::Reported { failure });
        }

        let usage = reply.usage();
        let model = reply.model.clone().unwrap_or_else(|| self.model.clone());

        if stop == Some(STOP_REASON_MAX_TOKENS) {
            let open = reply
                .open_call()
                .map(|call| (call.name.as_str(), call.arguments.len()));
            let cut_off = self.cut_off(request, open, reply.thought);
            return what_was_written(reply.text, label, model, usage, cut_off);
        }

        let calls = reply.calls();
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

/// The failure a reply that stopped without finishing is, where its stop reason says it did.
///
/// A refusal and a conversation that filled the window are replies the service ended on purpose,
/// and sending either again unchanged says the same. What a person can do about each differs, which
/// is why they are two failures.
fn unfinished(stop: Option<&str>) -> Option<Failure> {
    match stop {
        Some(STOP_REASON_REFUSAL) => Some(Failure::Refusal),
        Some(STOP_REASON_CONTEXT_FULL) => Some(Failure::ContextFull),
        _ => None,
    }
}

/// What a reply the ceiling stopped is worth keeping.
///
/// The text, and never the calls. A reply cut off at the ceiling was cut off wherever the model
/// happened to be, so the last thing in it may be half a tool call: an argument stopped mid-string
/// is not an argument, and a call the model had not finished choosing is not one it asked for.
/// What it was doing goes with it, so the turn can tell the planner why its call was not made
/// (TURN-7).
///
/// Nothing written at all is returned as a failure. There is no reply to keep, so what is left is
/// the ceiling that stopped it and what the reply was doing.
pub(crate) fn what_was_written(
    content: String,
    label: Label,
    model: String,
    usage: Usage,
    cut_off: CutOff,
) -> Result<Completion, ChatError> {
    if content.is_empty() {
        return Err(ChatError::TooLong(cut_off));
    }
    Ok(Completion {
        content: Labelled::new(content, label),
        model,
        calls: Vec::new(),
        context_tokens: usage.prompt_tokens,
        usage,
        cut_off: Some(cut_off),
    })
}

/// Validate reported counts independently of assistant content.
fn reported_usage(envelope: &Value) -> Option<WireUsage> {
    let value = envelope.get("usage")?;
    value.get("input_tokens")?.as_u64()?;
    value.get("output_tokens")?.as_u64()?;
    serde_json::from_value(value.clone()).ok()
}

/// One tool call a reply has begun writing.
#[derive(Debug)]
struct OpenedCall {
    /// The block index its arguments arrive under.
    index: usize,
    id: String,
    name: String,
    /// The pieces of its arguments, in order, which are not JSON until the last has arrived.
    arguments: String,
}

/// A reply being assembled from the events of one stream.
#[derive(Debug, Default)]
struct Reply {
    model: Option<String>,
    text: String,
    /// Tool calls by block index, since their arguments arrive in pieces across events.
    calls: Vec<OpenedCall>,
    /// What the service has reported so far, each report replacing what it restates.
    reported: WireUsage,
    /// Whether the reply's count is the service's rather than a tally of what arrived.
    counted: bool,
    /// Pieces of text that arrived, tallied for the live figure until the service gives its own.
    tally: u64,
    /// Pieces of a tool argument, tallied for the live figure and never for what the reply is
    /// charged, which stays the service's figure or a tally of the words.
    argument_pieces: u64,
    ended: bool,
    stop_reason: Option<String>,
    /// The index of the last block anything arrived for, which is the one being written when the
    /// reply stopped.
    last_block: Option<usize>,
    /// Whether any reasoning arrived. Known from the kind of block it arrives in.
    thought: bool,
    /// Whether a known event could not be decoded.
    ///
    /// Kept rather than failed on the spot so the stream is still drained: the reply is refused
    /// once it has ended, in the same place a reply that arrived whole is.
    unreadable: bool,
}

impl Reply {
    /// The name of the last tool call the reply has begun.
    ///
    /// Known from the event that opens the call, which is the first thing a service holding the
    /// argument back sends and the last thing it sends until the argument is whole.
    fn calling(&self) -> Option<&str> {
        self.calls.last().map(|call| call.name.as_str())
    }

    /// The call still being written, which is the one whose block was the last to receive
    /// anything. A call with text after it was finished, and the text was being written instead.
    fn open_call(&self) -> Option<&OpenedCall> {
        let last = self.last_block?;
        self.calls.iter().find(|call| call.index == last)
    }

    /// What the reply cost, as the service reported it where it has and as tallied where not.
    fn usage(&self) -> Usage {
        let mut usage = Usage::from(self.reported);
        if !self.counted {
            usage.completion_tokens = self.tally;
        }
        usage
    }

    /// Output as best it can be known while the reply arrives: the service's figure once it has
    /// given one, and until then the pieces of text and of argument that have arrived.
    fn output_so_far(&self) -> u64 {
        if self.counted {
            self.reported.output_tokens.unwrap_or(0)
        } else {
            self.tally + self.argument_pieces
        }
    }

    fn absorb(&mut self, event: StreamEvent) {
        match event {
            StreamEvent::MessageStart { model, usage } => {
                self.model = model.or(self.model.take());
                // The prompt's figures and a stub of the reply's, which the end of the stream
                // replaces: not the service's count of the reply, so not `counted`.
                if let Some(usage) = usage {
                    self.reported = self.reported.then(usage);
                }
            }
            StreamEvent::BlockStart { index, start } => {
                self.last_block = Some(index);
                match start {
                    // The opening event names the call and nothing else; every byte of its
                    // arguments arrives in the deltas that follow.
                    BlockStart::ToolUse { id, name } => self.calls.push(OpenedCall {
                        index,
                        id,
                        name,
                        arguments: String::new(),
                    }),
                    BlockStart::Reasoning => self.thought = true,
                    BlockStart::Text | BlockStart::Other => {}
                }
            }
            StreamEvent::BlockDelta { index, delta } => {
                self.last_block = Some(index);
                match delta {
                    Delta::Text(text) => {
                        self.text.push_str(&text);
                        self.tally += 1;
                    }
                    Delta::Arguments(fragment) => {
                        if let Some(call) = self.calls.iter_mut().find(|call| call.index == index) {
                            if !fragment.is_empty() {
                                self.argument_pieces += 1;
                            }
                            call.arguments.push_str(&fragment);
                        }
                    }
                    Delta::Reasoning => self.thought = true,
                    Delta::Other => {}
                }
            }
            // The counts here are cumulative, so what a report states replaces the earlier figure.
            StreamEvent::MessageDelta { stop_reason, usage } => {
                if stop_reason.is_some() {
                    self.stop_reason = stop_reason;
                }
                if let Some(usage) = usage {
                    self.counted |= usage.output_tokens.is_some();
                    self.reported = self.reported.then(usage);
                }
            }
            StreamEvent::MessageStop => self.ended = true,
            // Handled where the event is read, which has to return rather than carry on.
            StreamEvent::Failed { .. } => {}
        }
    }

    /// The calls this reply asked for, in the shape the agent expects.
    fn calls(&self) -> Vec<crate::protocol::ToolCall> {
        use crate::protocol::{ToolCall, ToolCallFunction};

        self.calls
            .iter()
            .map(|call| ToolCall {
                id: Some(call.id.clone()),
                function: ToolCallFunction {
                    name: call.name.clone(),
                    // An empty argument stream means a call with no arguments, which is `{}` rather
                    // than nothing: the turn loop parses this, and an empty string is not JSON.
                    arguments: Some(if call.arguments.is_empty() {
                        "{}".to_string()
                    } else {
                        call.arguments.clone()
                    }),
                },
                extra_content: None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Message;
    use bravebot_net::ReplyBound;

    fn config() -> Config {
        Config::from_lookup(|key| {
            match key {
                "SERVICES_KEY_AICHAT" => Some("test-signing-key"),
                "BRAVE_SERVICES_KEY_ID" => Some("test-key-id"),
                "BRAVE_AI_CHAT_ENDPOINT" => Some("https://brave.example.invalid"),
                _ => None,
            }
            .map(str::to_string)
        })
        .expect("configured")
    }

    fn provider(options: &str) -> Provider {
        let text = format!(
            r#"{{"provider": {{"anthropic": {{"env": ["A_VARIABLE"], "options": {options}}}}}}}"#
        );
        let serde_json::Value::Object(root) = serde_json::from_str(&text).expect("json") else {
            panic!("not an object");
        };
        Provider::all(&root).pop().expect("one provider")
    }

    /// How long a reply is given follows the ceiling its request carries, because the ceiling is the
    /// one figure that says how long the reply can run. A streamed reply is given it from the moment
    /// it begins and a whole one for the wait as well, since the model writes it before any of it is
    /// sent. The gap between two pieces is the egress layer's unless a block states one.
    #[test]
    fn a_reply_is_given_the_time_its_ceiling_takes_to_write() {
        let config = config();
        let egress = Egress::new();
        let provider = provider(r#"{"baseURL": "https://proxy.example.invalid/v1"}"#);
        let client = AnthropicClient::new(&config, &egress, &provider, "claude-test", None);
        let request = ChatRequest::new("anthropic/claude-test", vec![Message::user("hello")]);

        // 32,000 tokens at the slowest rate assumed, after the wait for the prompt to be read.
        let allowed = Duration::from_secs(120 + 32_000 / 25);
        assert!(allowed > bravebot_net::Timeouts::default().reply);
        assert_eq!(
            client.build(&request, true).expect("built").reply,
            Some(ReplyBound::Begun(allowed))
        );
        assert_eq!(
            client.build(&request, false).expect("built").reply,
            Some(ReplyBound::Whole(allowed))
        );
        assert_eq!(client.build(&request, true).expect("built").idle, None);

        // A ceiling somebody stated is the one the time follows, and a short one is never given
        // less than a reply of unstated length is.
        let stated = provider.clone().with_output_budget(Some(1_000));
        let short = AnthropicClient::new(&config, &egress, &stated, "claude-test", None);
        assert_eq!(
            short.build(&request, true).expect("built").reply,
            Some(ReplyBound::Begun(bravebot_net::Timeouts::default().reply))
        );
    }

    /// A gap a block states is the one allowed between two pieces of the reply.
    #[test]
    fn the_gap_a_block_states_is_the_one_allowed_between_pieces_of_a_reply() {
        let config = config();
        let egress = Egress::new();
        let provider =
            provider(r#"{"baseURL": "https://proxy.example.invalid/v1", "idleTimeoutMs": 4000}"#);
        let client = AnthropicClient::new(&config, &egress, &provider, "claude-test", None);
        let request = ChatRequest::new("anthropic/claude-test", vec![Message::user("hello")]);
        assert_eq!(
            client.build(&request, true).expect("built").idle,
            Some(Duration::from_millis(4000))
        );
    }
}
