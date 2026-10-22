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

use crate::protocol::{ChatRequest, SseDecoder, Usage};
use crate::{
    ATTEMPTS, BACKOFF, CHUNKS_AHEAD, ChatError, Completion, CutOff, OpenCall, Progress, Refusals,
    WAKE, learned_key, log_retry, pause_before_retry, refuses_the_body, remember, remembered,
    reply_bound, worth_another_attempt,
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
    attempts: u32,
    completed_usage: Option<Usage>,
    retried_usage: Option<Usage>,
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
    cancel: Option<Cancel>,
    /// Whether requests still carry cache breakpoints.
    ///
    /// True until a model refuses one, which is the only way to find out that it does not do
    /// prompt caching: nothing in the listing says which models read a breakpoint.
    breakpoints: bool,
    /// Whether requests still carry the cache lifetime the settings chose.
    ///
    /// True until a model refuses it, which is the only way to find out that it reads breakpoints
    /// and not a lifetime on them.
    cache_ttl: bool,
    /// Whether requests still carry the effort level a turn was given.
    ///
    /// True until a model refuses the field. The listing says which models read one, and a request
    /// is sent without it for a model it says reads none, so this is for a roster that was not
    /// asked or is out of date.
    effort: bool,
    /// Whether streamed requests that offer tools still ask for arguments as they are written.
    arguments_as_written: bool,
    /// Whether requests to a model nobody stated a ceiling for still carry the assumed one.
    ///
    /// True until a model refuses it, after which they carry the fallback.
    ceiling: bool,
    /// What that model was known to refuse before this request, which a probe that settled nothing
    /// puts back.
    recalled: Refusals,
    /// How long to wait before the first retry. Doubled for each retry after that.
    backoff: Duration,
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
            attempts: 0,
            completed_usage: None,
            retried_usage: None,
            config,
            egress,
            provider,
            model: model.into(),
            token,
            cancel: None,
            breakpoints: true,
            cache_ttl: true,
            effort: true,
            arguments_as_written: true,
            ceiling: true,
            recalled: Refusals::default(),
            backoff: BACKOFF,
        }
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
    ///
    /// Without this every turn re-learns it, at the cost of one refused request per concession per
    /// turn, and the interface is never told either.
    fn recall(&mut self) {
        let refusals = remembered(&self.refusal_key());
        self.breakpoints = !refusals.caching;
        self.cache_ttl = !refusals.cache_ttl;
        self.effort = !refusals.effort;
        self.arguments_as_written = !refusals.arguments_as_written;
        self.ceiling = !refusals.ceiling;
        self.recalled = refusals;
    }

    /// Settle what a probe found, once the request it was part of has finished one way or the other.
    ///
    /// A probe that answered leaves what it gave up dropped, which is the model saying it does not
    /// read it. A probe that failed as well says none of it was what the service refused, so each
    /// goes back to what was known before the request: a request can be refused on its contents for
    /// reasons that have nothing to do with them, and a session that gave the breakpoints up for
    /// one of those pays full price for a prefix the service would have read once, every round.
    fn probe_settled(&mut self, probed: bool, failed: bool) {
        if !probed {
            return;
        }
        if failed {
            self.breakpoints = !self.recalled.caching;
            self.cache_ttl = !self.recalled.cache_ttl;
            self.effort = !self.recalled.effort;
            self.arguments_as_written = !self.recalled.arguments_as_written;
            self.ceiling = !self.recalled.ceiling;
            return;
        }
        remember(
            &self.refusal_key(),
            Refusals {
                caching: !self.breakpoints,
                cache_ttl: !self.cache_ttl,
                effort: !self.effort,
                arguments_as_written: !self.arguments_as_written,
                ceiling: !self.ceiling,
            },
        );
    }

    /// The cache lifetime a request asks for: the one the settings chose, for a model that has not
    /// refused it.
    fn cache_ttl_asked(&self) -> Option<CacheTtl> {
        self.config.prompt_cache_ttl.filter(|_| self.cache_ttl)
    }

    /// Whether this failure is worth sending the same request again without the cache lifetime.
    ///
    /// Tried ahead of the breakpoints, which carry it: a model that reads a breakpoint and refuses a
    /// lifetime on it keeps its caching at its own default, where the other order would give up the
    /// caching for the sake of a lifetime. Only where a request carried one, and after the level and
    /// the ceiling, which this API refuses more often.
    fn worth_dropping_cache_ttl(&self, error: &ChatError) -> bool {
        self.breakpoints && self.cache_ttl_asked().is_some() && refuses_the_body(error)
    }

    /// Whether this failure is worth sending the same request again without its cache breakpoints.
    ///
    /// Only where there are breakpoints to drop, so a service that answers this status for its own
    /// reasons cannot make a retry loop out of it.
    fn worth_dropping_breakpoints(&self, error: &ChatError) -> bool {
        self.breakpoints && refuses_the_body(error)
    }

    /// Whether this failure is worth sending the same request again without its effort level.
    ///
    /// Only where the request carried one: a request without a level was not refused for it, and
    /// giving it up there would record a refusal nobody saw.
    fn worth_dropping_effort(&self, request: &ChatRequest, error: &ChatError) -> bool {
        self.effort && request.effort.is_some() && refuses_the_body(error)
    }

    /// Whether this failure is worth sending the same request again with the fallback ceiling.
    ///
    /// Only where the ceiling sent was the assumed one: a figure somebody stated is their statement
    /// about the model, and quietly sending less would cut replies short that they said could run.
    fn worth_lowering_ceiling(&self, error: &ChatError) -> bool {
        self.ceiling && self.stated_ceiling().is_none() && refuses_the_body(error)
    }

    /// Whether this failure is worth sending the same streamed request again without the ask for
    /// arguments as they are written. The last thing a refusal can be about, and only where the
    /// request carried it.
    fn worth_dropping_arguments_as_written(
        &self,
        request: &ChatRequest,
        error: &ChatError,
    ) -> bool {
        self.streams_arguments(request, true) && refuses_the_body(error)
    }

    /// Whether this request asks for tool arguments as they are written.
    ///
    /// Streamed requests that offer a tool, and only those: a whole reply arrives at once however
    /// its arguments are sent, so there the ask buys nothing and gives up the service's own check
    /// that an argument parses.
    fn streams_arguments(&self, request: &ChatRequest, streaming: bool) -> bool {
        streaming
            && self.arguments_as_written
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
        self.stated_ceiling().unwrap_or(if self.ceiling {
            bravebot_config::bedrock::OUTPUT_LIMIT
        } else {
            bravebot_config::bedrock::OUTPUT_LIMIT_FALLBACK
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
                .with_effort(request.effort.filter(|_| self.effort))
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
        if self.breakpoints {
            messages
        } else {
            messages.without_breakpoints()
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

    /// Send a request and wait for the whole reply.
    pub fn complete<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        request: &ChatRequest,
    ) -> Result<Completion, ChatError> {
        self.attempts = 0;
        self.completed_usage = None;
        self.retried_usage = None;
        self.recall();
        let mut attempt = 1;
        let mut probed = false;
        loop {
            match self.complete_once(policy, request) {
                // Sent again immediately rather than after a wait: nothing is busy, the request was
                // simply carrying something this model does not take.
                Err(error) if self.worth_dropping_effort(request, &error) => {
                    self.effort = false;
                    probed = true;
                }
                Err(error) if self.worth_lowering_ceiling(&error) => {
                    self.ceiling = false;
                    probed = true;
                }
                Err(error) if self.worth_dropping_cache_ttl(&error) => {
                    self.cache_ttl = false;
                    probed = true;
                }
                Err(error) if self.worth_dropping_breakpoints(&error) => {
                    self.breakpoints = false;
                    probed = true;
                }
                Err(error) if worth_asking_again(self.attempts_allowed(), attempt, &error) => {
                    let pause = pause_before_retry(self.backoff, attempt, &error);
                    log_retry(attempt + 1, pause);
                    if !self.wait(pause) {
                        return Err(ChatError::Cancelled);
                    }
                    attempt += 1;
                }
                result => {
                    self.probe_settled(probed, result.is_err());
                    return result.map(|mut completion| {
                        if let Some(usage) = self.retried_usage {
                            completion.usage.add(usage);
                        }
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
        self.retried_usage = self.completed_usage();
        self.completed_usage = None;
        if self.cancelled() {
            return Err(ChatError::Cancelled);
        }
        let http = self.build(request, false)?;
        self.attempts += 1;
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
        self.completed_usage = reported_usage(&envelope).map(Usage::from);

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
        self.attempts = 0;
        self.completed_usage = None;
        self.retried_usage = None;
        self.recall();
        let mut attempt = 1;
        let mut probed = false;
        loop {
            match self.stream_once(policy, request, attempt, &mut progress) {
                Err(error) if self.worth_dropping_effort(request, &error) => {
                    self.effort = false;
                    probed = true;
                }
                Err(error) if self.worth_lowering_ceiling(&error) => {
                    self.ceiling = false;
                    probed = true;
                }
                Err(error) if self.worth_dropping_cache_ttl(&error) => {
                    self.cache_ttl = false;
                    probed = true;
                }
                Err(error) if self.worth_dropping_breakpoints(&error) => {
                    self.breakpoints = false;
                    probed = true;
                }
                Err(error) if self.worth_dropping_arguments_as_written(request, &error) => {
                    self.arguments_as_written = false;
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
                    return result.map(|mut completion| {
                        if let Some(usage) = self.retried_usage {
                            completion.usage.add(usage);
                        }
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
        self.retried_usage = self.completed_usage();
        self.completed_usage = None;
        // Before the request is built, let alone sent. A stop that landed while the last attempt was
        // failing is still a stop.
        if self.cancelled() {
            return Err(ChatError::Cancelled);
        }

        let http = self
            .build(request, true)?
            .header("accept", "text/event-stream");
        self.attempts += 1;
        let stream = self.egress.fetch_streaming(
            policy,
            http,
            Label::untrusted_public(),
            self.cancel.as_ref(),
        )?;
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
                    self.completed_usage = Some(reply.usage());
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
