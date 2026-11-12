//! Client for Ollama's own `/api/chat`.
//!
//! What an entry naming an Ollama SDK reaches. It answers the same questions as
//! [`crate::AichatClient`] and returns the same [`Completion`], so a caller chooses between them
//! once and the turn loop is unchanged. What differs is underneath: the request states the window
//! the server is to allocate, in `options.num_ctx`, which Ollama's OpenAI-compatible endpoint has no
//! field for. Without it the server allocates its own default and drops the start of any prompt
//! longer than that without saying so (BACKEND-72).
//!
//! Every request goes through [`bravebot_net::Egress`], so the policy gate sees this traffic
//! exactly as it sees the other backends'. The reply is labelled untrusted-public and nothing here
//! reads it to decide anything: this module speaks the wire protocol and mints no witness.

pub mod protocol;

use crate::anthropic::what_was_written;
use crate::call::{Concession, Driven, Pace, Plan, Spent, drive};
use crate::protocol::{ChatRequest, ToolCall, Usage};
use crate::{
    BACKOFF, ChatError, Completion, CutOff, Progress, Refusals, WAKE, learned_key, log_retry,
    pause_before_retry, read_in_background, remember, remembered, reply_bound,
    worth_another_attempt_or_report,
};
use bravebot_config::Secret;
use bravebot_config::provider::Provider;
use bravebot_core::cancel::Cancel;
use bravebot_core::event::Sink;
use bravebot_core::label::Label;
use bravebot_core::policy::Policy;
use bravebot_core::value::Labelled;
use bravebot_net::{Egress, EgressError, Request};
use protocol::{Decoded, LineDecoder, Response};
use serde_json::Value;
use std::time::Duration;

/// How long a reply is allowed to run where nobody stated a ceiling. It bounds how long a reply
/// asked for whole is waited for and is never sent.
const ASSUMED_CEILING: u64 = 32_768;

/// One client for one request, to an Ollama server.
pub struct OllamaChatClient<'a> {
    spent: Spent,
    egress: &'a Egress,
    provider: &'a Provider,
    /// What the server calls the model, without the provider's id in front.
    model: String,
    token: Option<Secret>,
    /// The headers the block's `options.headers` asked for, resolved by the caller (BACKEND-60).
    headers: Vec<(String, Secret)>,
    /// The window the server is asked to allocate, which is the one the session budgets for.
    window: Option<u64>,
    /// Never anything: nothing this client sends is given up on a refusal. Held because the driver
    /// keeps a model's refusals for every client.
    refusals: Refusals,
    pace: Pace,
}

impl Driven for OllamaChatClient<'_> {
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

    /// No field is given up: the request sends no thinking level, no cache marks and no ceiling
    /// nobody stated, so there is nothing a refusal could be about that leaving out would fix.
    fn refused(&self, _request: &ChatRequest, _concession: Concession, _error: &ChatError) -> bool {
        false
    }

    fn asks_again(&self, attempt: u32, error: &ChatError) -> bool {
        worth_another_attempt_or_report(self.provider.patience, attempt, error)
    }

    fn retry_window(&self) -> Option<Duration> {
        self.provider.patience.retry_window()
    }

    fn of_the_connection(error: &ChatError) -> bool {
        crate::failed_connection(error)
    }

    fn pause(&self, failures: u32, error: &ChatError) -> Duration {
        pause_before_retry(self.pace.backoff, failures, error)
    }

    fn log_retry(&self, attempt: u32, pause: Duration) {
        log_retry(attempt, pause);
    }
}

impl<'a> OllamaChatClient<'a> {
    /// A client for `provider`, asking for `model` by the name the server knows it by, and carrying
    /// `token` as a bearer token where the entry named one, which a server behind a proxy may want.
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
            window: None,
            refusals: Refusals::default(),
            pace: Pace::new(BACKOFF),
        }
    }

    /// Ask the server to allocate `window` tokens for each request (BACKEND-72).
    pub fn with_window(mut self, window: u64) -> Self {
        self.window = Some(window);
        self
    }

    /// Requests handed to egress in the last call.
    pub fn attempts(&self) -> u32 {
        self.spent.attempts
    }

    /// The request fields the last call left out after the service refused them, which is none.
    pub fn conceded(&self) -> crate::call::Conceded {
        self.spent.conceded
    }

    /// Total reported usage of completed attempts in the last call, including retries.
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
    pub fn with_gateway_headers(mut self, headers: Vec<(String, Secret)>) -> Self {
        self.headers = headers;
        self
    }

    /// Wait this long before the first retry instead of the default one-second wait.
    pub fn with_backoff(mut self, backoff: Duration) -> Self {
        self.pace.backoff = backoff;
        self
    }

    fn refusal_key(&self) -> String {
        learned_key(&self.provider.completion_url(), &self.model)
    }

    /// The ceiling somebody stated for this model, exported or in its block.
    fn stated_ceiling(&self) -> Option<u64> {
        self.provider.stated_output_limit(&self.model)
    }

    /// The body one request carries, before it is encoded.
    fn contents_for(&self, request: &ChatRequest, streaming: bool) -> protocol::ChatRequest {
        protocol::request_from(
            &self.model,
            &request.messages,
            request.tools.as_deref(),
            streaming,
        )
        .with_window(self.window)
        .with_ceiling(self.stated_ceiling())
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
    /// level down for `options`, which the request also writes into. A key the request already set
    /// wins, so a block's `num_ctx` is overridden by the window the session budgets for: the two
    /// differing is the silent truncation this client exists to prevent.
    fn build(&self, request: &ChatRequest, streaming: bool) -> Result<Request, ChatError> {
        let mut body = serde_json::to_value(self.contents_for(request, streaming))
            .map_err(|error| ChatError::Encode(error.to_string()))?;
        if let (Some(object), Some(options)) = (
            body.as_object_mut(),
            self.provider
                .model(&self.model)
                .and_then(|model| model.request_options(request.variant.as_deref())),
        ) {
            for (key, value) in &options {
                match (object.get_mut(key), value) {
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

        let mut http = Request::post(self.provider.completion_url(), body)
            .with_client_certificate()
            .header("content-type", "application/json");
        if let Some(token) = self.token.as_ref().map(Secret::expose) {
            let (name, value) = self.provider.credential_header(token);
            http = http.header(name, value);
        }
        // After the credential, and never over it: a name this request already set is refused
        // where the block is read.
        for (name, value) in &self.headers {
            http = http.header(name.clone(), value.expose());
        }
        let bound = reply_bound(self.stated_ceiling().unwrap_or(ASSUMED_CEILING));
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
            order: &[],
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

        // The envelope is protocol: the bytes come out to find the reply inside, and the reply is
        // relabelled with exactly the label it arrived under.
        let label = response.body.label();
        let (bytes, label) = policy
            .decode_transport("ollama chat", label)
            .decode(response.body);

        let undecodable = |detail: String| ChatError::Decode {
            detail: format!("{detail} (received {} bytes)", bytes.len()),
        };
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|error| undecodable(error.to_string()))?;
        let parsed = match protocol::line(value) {
            Decoded::Line(parsed) => parsed,
            Decoded::Failed(failure) => return Err(ChatError::Reported { failure }),
            Decoded::Unreadable => {
                return Err(undecodable(
                    "the reply is not in the shape this reads".into(),
                ));
            }
        };
        let usage = parsed.usage();
        self.spent.record(usage);
        let usage = usage.unwrap_or_default();

        let (text, calls, thinking) = parsed.parts();
        let content = text.to_string();
        if parsed.cut_off() {
            let cut_off = self.cut_off(usage, !thinking.is_empty());
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

    /// What a reply the ceiling stopped was doing. A call arrives whole, so none is ever open.
    fn cut_off(&self, usage: Usage, thought: bool) -> CutOff {
        CutOff {
            ceiling: self.stated_ceiling().unwrap_or(usage.completion_tokens),
            call: None,
            thought,
        }
    }

    /// Send a request and read the reply as it arrives.
    ///
    /// Identical to [`OllamaChatClient::complete`] in what it produces and the gates it passes.
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
            order: &[],
        };
        drive(
            self,
            request,
            plan,
            &mut progress,
            |client, progress, attempt| client.stream_once(policy, request, attempt, progress),
            |progress, retry| progress(Progress::announcing(retry)),
        )
    }

    fn stream_once<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        request: &ChatRequest,
        attempt: u32,
        progress: &mut impl FnMut(Progress),
    ) -> Result<Completion, ChatError> {
        // A model on this machine may send nothing for minutes while it loads or writes a call, and
        // Ollama not even its headers. This wait can be stopped at any moment of it.
        let http = self
            .build(request, true)?
            .header("accept", "application/x-ndjson")
            .patient_on_this_machine();
        self.spent.sent();
        let stream = self.egress.fetch_streaming(
            policy,
            http,
            Label::untrusted_public(),
            self.pace.cancel.as_ref(),
        )?;
        let label = stream.label();

        let arriving = read_in_background(stream);

        let mut decoder = LineDecoder::default();
        let mut reply = Reply::default();
        // One envelope, arriving in lines, so it is authorised once rather than once a line.
        let decoding = policy.decode_transport("ollama chat stream", Label::untrusted_public());
        let mut began = false;

        loop {
            let piece = match arriving.recv_timeout(WAKE) {
                Ok(Ok(Some(piece))) => piece,
                Ok(Ok(None)) => break,
                // A read that fails once the reply has begun is the reply stopping.
                Ok(Err(EgressError::Transport { .. })) if began => {
                    return Err(ChatError::Incomplete);
                }
                Ok(Err(error)) => return Err(error.into()),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if self.pace.cancelled() {
                        return Err(ChatError::Cancelled);
                    }
                    continue;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            };
            began = true;

            if self.pace.cancelled() {
                return Err(ChatError::Cancelled);
            }

            let (bytes, _) = decoding.decode(piece);
            let written_before = reply.text.len();
            let thought_before = reply.thinking.len();
            for line in decoder.push(&bytes) {
                if let Some(failure) = reply.read(&line) {
                    return Err(ChatError::Reported { failure });
                }
            }
            if reply.ended {
                self.spent.record(reply.usage);
            }

            progress(Progress {
                written: Labelled::new(&reply.text[written_before..], label),
                output_tokens: reply.output_so_far(),
                calling: reply
                    .calls
                    .last()
                    .map(|call| Labelled::new(call.function.name.as_str(), label)),
                thinking: Labelled::new(&reply.thinking[thought_before..], label),
                counted_by_server: reply.usage.is_some(),
                prose: Progress::has_prose(&reply.text),
                ..Progress::quiet(attempt)
            });
        }
        // A server that ends its last line with the connection rather than a newline.
        if let Some(line) = decoder.finish() {
            if let Some(failure) = reply.read(&line) {
                return Err(ChatError::Reported { failure });
            }
            if reply.ended {
                self.spent.record(reply.usage);
            }
        }

        // A server that hangs up mid-reply leaves the same end of input as one that finished, so
        // without this a cut-off answer is returned as a whole one.
        if !reply.ended {
            return Err(ChatError::Incomplete);
        }
        if reply.unreadable {
            return Err(ChatError::Decode {
                detail: "the reply contained content in a shape this does not read".to_string(),
            });
        }

        let usage = reply.usage.unwrap_or(Usage {
            completion_tokens: reply.tally,
            ..Usage::default()
        });
        if reply.cut_off {
            let cut_off = self.cut_off(usage, !reply.thinking.is_empty());
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

/// A reply being assembled from the lines of one stream.
#[derive(Debug, Default)]
struct Reply {
    text: String,
    /// The model's reasoning, which goes to the display alone and never into the reply.
    thinking: String,
    calls: Vec<ToolCall>,
    /// What the last line said the request cost.
    usage: Option<Usage>,
    /// Lines that carried text, tallied for the live figure until the server gives its own.
    tally: u64,
    ended: bool,
    cut_off: bool,
    /// Whether a line could not be decoded. Kept rather than failed on the spot so the stream is
    /// still drained.
    unreadable: bool,
}

impl Reply {
    /// Take in one line, or say why the server gave up on the reply.
    fn read(&mut self, line: &[u8]) -> Option<crate::anthropic::protocol::Failure> {
        let Ok(value) = serde_json::from_slice::<Value>(line) else {
            self.unreadable = true;
            return None;
        };
        match protocol::line(value) {
            Decoded::Failed(failure) => Some(failure),
            Decoded::Unreadable => {
                self.unreadable = true;
                None
            }
            Decoded::Line(response) => {
                self.absorb(&response);
                None
            }
        }
    }

    fn absorb(&mut self, response: &Response) {
        let (text, calls, thinking) = response.parts();
        if !text.is_empty() {
            self.tally += 1;
        }
        self.text.push_str(text);
        self.thinking.push_str(thinking);
        self.calls.extend(calls);
        if response.done {
            self.ended = true;
            self.cut_off = response.cut_off();
            self.usage = response.usage();
        }
    }

    fn output_so_far(&self) -> u64 {
        match self.usage {
            Some(usage) => usage.completion_tokens,
            None => self.tally + self.calls.len() as u64,
        }
    }
}
