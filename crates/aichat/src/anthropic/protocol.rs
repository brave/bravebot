//! Wire types for Anthropic's Messages API, and the translation to and from the shapes the rest of
//! this agent speaks.
//!
//! The agent's conversation is held in OpenAI-compatible types, because that is what the aichat
//! endpoint speaks. [`crate::conversation`] restates it as turns of blocks, which is what this API
//! and Bedrock's take, and this module spells those blocks the way this API does:
//!
//! - Every block names itself in a `type` field, and a cache breakpoint is a field on a block rather
//!   than a block of its own.
//! - The system prompt is a top-level list of text blocks.
//! - The model, the reply ceiling and whether to stream are fields of the body, and the ceiling is
//!   required.
//! - How hard to think goes in `output_config`, and asking for a tool's arguments as the model
//!   writes them is a field of the tool.
//! - Usage counts `input_tokens` net of the cache, with the cache's reads and writes beside it.
//! - A streamed event is named in its own `type` field.
//!
//! Nothing here inspects content to make a decision. Text is moved between shapes and handed on with
//! whatever label it arrived under.

use crate::conversation::{self, Spelling};
use crate::protocol::{
    CacheControl, Cached, Effort, Message, Tool, ToolCall, ToolCallFunction, Usage,
};
use bravebot_config::CacheTtl;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The stop reason meaning the reply hit the ceiling rather than finishing.
pub const STOP_REASON_MAX_TOKENS: &str = "max_tokens";

/// The stop reason of a reply the service declined to write.
pub const STOP_REASON_REFUSAL: &str = "refusal";

/// The stop reason of a reply that ran into the model's context window.
pub const STOP_REASON_CONTEXT_FULL: &str = "model_context_window_exceeded";

/// The media types this API takes as a picture.
const IMAGE_TYPES: [&str; 4] = ["image/png", "image/jpeg", "image/gif", "image/webp"];

/// The media types this API takes as a document.
///
/// This API sorts an attachment by what it is, so what it will not take as a picture it may still
/// take as a document, and a picture block naming one of these is refused by the service.
const DOCUMENT_TYPES: [&str; 1] = ["application/pdf"];

/// A request to the Messages API.
#[derive(Debug, Clone, Serialize)]
pub struct MessagesRequest {
    pub model: String,
    /// The most the reply may write, which this API requires of every request.
    pub max_tokens: u64,
    /// The system prompt, hoisted out of the message list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<Vec<Block>>,
    pub messages: Vec<Turn>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolSpec>>,
    /// Absent unless somebody asked for a level, so a request nobody asked sends the body it
    /// always sent and the model keeps its own default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<OutputConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

/// How hard to think, in the object this API keeps it in.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct OutputConfig {
    pub effort: Effort,
}

/// A tool the request offers.
#[derive(Debug, Clone, Serialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    /// Whether the service sends this tool's arguments as the model writes them.
    ///
    /// Without it the service holds an argument back until the model has finished writing it, so a
    /// long file is minutes of silence on a stream. With it the service stops checking that an
    /// argument parses before sending it on, so what arrives is whatever the model wrote, and one
    /// that does not parse is the turn loop's to answer as a failed call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eager_input_streaming: Option<bool>,
}

/// Serialised only: `role` is a fixed string this crate chooses, never one it reads back.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Turn {
    pub role: &'static str,
    pub content: Vec<Block>,
}

/// One content block on the way out.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    Text {
        text: String,
        /// A breakpoint: everything in front of this block, itself included, may be answered out of
        /// the service's cache.
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    Image {
        source: Source,
    },
    Document {
        source: Source,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        /// Absent where the tool wrote nothing or only whitespace: this API refuses a text block
        /// with no word in it, and a result with no content is one it takes.
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<Vec<Block>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
}

/// An attachment's bytes, which this API takes inline as base64.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Source {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub media_type: String,
    pub data: String,
}

/// How this API names the blocks of a turn, for [`conversation::walk`].
struct Messages;

impl Spelling for Messages {
    type Block = Block;

    fn text(&self, text: &str) -> Block {
        Block::Text {
            text: text.to_string(),
            cache_control: None,
        }
    }

    fn attachment(&self, media_type: &str, data: &str, _position: usize) -> Option<Block> {
        let source = Source {
            kind: "base64",
            media_type: media_type.to_string(),
            data: data.to_string(),
        };
        if IMAGE_TYPES.contains(&media_type) {
            Some(Block::Image { source })
        } else if DOCUMENT_TYPES.contains(&media_type) {
            Some(Block::Document { source })
        } else {
            None
        }
    }

    fn call(&self, id: &str, name: &str, input: Value) -> Block {
        Block::ToolUse {
            id: wire_id(id),
            name: name.to_string(),
            input,
        }
    }

    fn result(&self, id: &str, text: String) -> Block {
        Block::ToolResult {
            tool_use_id: wire_id(id),
            content: (!text.trim().is_empty()).then(|| {
                vec![Block::Text {
                    text,
                    cache_control: None,
                }]
            }),
            cache_control: None,
        }
    }
}

/// A call's id as this API takes it.
///
/// It takes letters, digits, `_` and `-` and nothing else, and a conversation carried over from
/// another service may hold ids with a dot or a colon in them, or none at all. Every other character
/// becomes `_`, and the same step is taken on a call and on the result that answers it, so the two
/// still name each other.
fn wire_id(id: &str) -> String {
    let id: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if id.is_empty() { "_".to_string() } else { id }
}

impl MessagesRequest {
    /// Cap the reply at `ceiling` tokens.
    ///
    /// A ceiling rather than a target: the model stops here having said whatever it had room for,
    /// so the figure costs nothing on a reply that was going to be short.
    pub fn with_ceiling(mut self, ceiling: u64) -> Self {
        self.max_tokens = ceiling;
        self
    }

    /// Ask for a particular amount of thinking, or leave the model to its own default.
    pub fn with_effort(mut self, effort: Option<Effort>) -> Self {
        self.output_config = effort.map(|effort| OutputConfig { effort });
        self
    }

    /// Ask for each tool argument as the model writes it, rather than once it is finished.
    pub fn with_arguments_as_written(mut self) -> Self {
        for tool in self.tools.iter_mut().flatten() {
            tool.eager_input_streaming = Some(true);
        }
        self
    }

    /// Ask for the reply as server-sent events.
    pub fn streamed(mut self) -> Self {
        self.stream = Some(true);
        self
    }

    /// The same request without its cache breakpoints.
    ///
    /// Prompt caching is not something every model this API serves offers, and one that does not
    /// refuses the whole request rather than reading past the breakpoints. They are also the one
    /// part of a request nobody asked for, so they are what a refusal is worth trying without.
    pub fn without_breakpoints(mut self) -> Self {
        for block in self.system.iter_mut().flatten() {
            set_breakpoint(block, None);
        }
        for turn in &mut self.messages {
            for block in &mut turn.content {
                set_breakpoint(block, None);
            }
        }
        self
    }

    /// The same request with every breakpoint asking the service to keep its prefix for `ttl`.
    ///
    /// Not every model reads a lifetime, and one that does not refuses the request, so this is
    /// something a refusal is worth trying without before the breakpoints are given up. `None`
    /// leaves each breakpoint as the service's default.
    pub fn with_cache_ttl(mut self, ttl: Option<CacheTtl>) -> Self {
        let Some(ttl) = ttl else {
            return self;
        };
        let point = Some(CacheControl::Ephemeral {
            ttl: Some(ttl.wire()),
        });
        for block in self.system.iter_mut().flatten() {
            if breakpoint(block).is_some() {
                set_breakpoint(block, point);
            }
        }
        for turn in &mut self.messages {
            for block in &mut turn.content {
                if breakpoint(block).is_some() {
                    set_breakpoint(block, point);
                }
            }
        }
        self
    }

    /// The same request without the breakpoint on the end of its conversation, the prompt's kept.
    ///
    /// For a request whose conversation is given up once it answers: the breakpoint that rolls to
    /// the end is worth a cache write because the next request sends everything in front of it
    /// again, and where there is no next request it is a premium paid for a prefix nothing reads
    /// back. The prompt is a different prefix, the same bytes every time such a request is made.
    pub fn without_the_conversation_breakpoint(mut self) -> Self {
        if let Some(last) = self.messages.last_mut() {
            for block in &mut last.content {
                set_breakpoint(block, None);
            }
        }
        self
    }
}

/// The breakpoint a block carries.
fn breakpoint(block: &Block) -> Option<CacheControl> {
    match block {
        Block::Text { cache_control, .. } | Block::ToolResult { cache_control, .. } => {
            *cache_control
        }
        Block::Image { .. } | Block::Document { .. } | Block::ToolUse { .. } => None,
    }
}

/// Put `point` on a block that can carry one, and nothing on one that cannot.
fn set_breakpoint(block: &mut Block, point: Option<CacheControl>) {
    match block {
        Block::Text { cache_control, .. } | Block::ToolResult { cache_control, .. } => {
            *cache_control = point;
        }
        Block::Image { .. } | Block::Document { .. } | Block::ToolUse { .. } => {}
    }
}

/// Build a Messages request from the conversation the agent holds.
///
/// The message list is the same conversation, restated by [`conversation::walk`]. The reply ceiling
/// starts at [`bravebot_config::bedrock::OUTPUT_LIMIT`], the figure assumed where nobody stated one,
/// and the caller sets the one that applies.
pub fn request_from(model: &str, messages: &[Message], tools: Option<&[Tool]>) -> MessagesRequest {
    // An empty list is no list: a turn offering nothing sends no `tools` field at all.
    let tools = tools.filter(|tools| !tools.is_empty());
    let walked = conversation::walk(&Messages, messages, tools.is_some());

    let mut turns: Vec<Turn> = walked
        .turns
        .into_iter()
        .map(|turn| Turn {
            role: turn.role,
            content: turn.blocks,
        })
        .collect();

    // Two breakpoints, which is what this conversation is shaped like: the prompt, which the tool
    // schemas in front of it travel with, and the last block of the conversation. A turn re-sends
    // its whole history each round, so without these the service reads all of it again every time.
    // The prefix runs tools, then system, then messages, so the breakpoint on the system prompt
    // covers the schemas too and the one on the last message covers everything.
    let point = Some(CacheControl::Ephemeral { ttl: None });
    let system = walked
        .system
        .filter(|system| !system.trim().is_empty())
        .map(|system| {
            vec![Block::Text {
                text: system,
                cache_control: point,
            }]
        });
    mark_the_end(&mut turns, point);

    MessagesRequest {
        model: model.to_string(),
        max_tokens: bravebot_config::bedrock::OUTPUT_LIMIT,
        system,
        messages: turns,
        tools: tools.map(|tools| tools.iter().map(tool_from).collect()),
        output_config: None,
        stream: None,
    }
}

/// Put a breakpoint at the end of the conversation.
///
/// Rolling rather than fixed: it moves to the end on every request, so each round writes the round
/// before it into the cache and reads back everything older. A conversation ending in an image or a
/// tool call is left with the one breakpoint on the system prompt, which costs a cache write and
/// nothing else.
fn mark_the_end(turns: &mut [Turn], point: Option<CacheControl>) {
    if let Some(last) = turns.last_mut().and_then(|turn| turn.content.last_mut())
        && matches!(last, Block::Text { .. } | Block::ToolResult { .. })
    {
        set_breakpoint(last, point);
    }
}

fn tool_from(tool: &Tool) -> ToolSpec {
    ToolSpec {
        name: tool.function.name.clone(),
        description: tool.function.description.clone(),
        input_schema: tool.function.parameters.clone(),
        eager_input_streaming: None,
    }
}

/// A complete reply.
#[derive(Debug, Deserialize)]
pub struct MessageResponse {
    /// The model that answered, which is the dated model behind an alias where the request named
    /// one.
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub content: Vec<ReplyBlock>,
    /// Why the model stopped. `max_tokens` here means the reply was cut off.
    #[serde(default)]
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub usage: Option<WireUsage>,
}

/// One content block of a reply.
///
/// A block of a kind this does not model is read past, since a service adds kinds. A `tool_use`
/// that cannot be read is not: it is a call the model made, and passing over it loses the call.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReplyBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        /// Absent where a call takes no arguments, and read as an empty object, which is what the
        /// same call arriving in pieces over a stream already becomes.
        #[serde(default = "empty_object")]
        input: Value,
    },
    Thinking {},
    RedactedThinking {},
    #[serde(other)]
    Other,
}

impl ReplyBlock {
    /// Whether this is the model's reasoning, which is no part of the reply.
    pub fn is_reasoning(&self) -> bool {
        matches!(self, Self::Thinking {} | Self::RedactedThinking {})
    }
}

/// The arguments of a call that named none.
fn empty_object() -> Value {
    json!({})
}

/// The text and the calls in a finished reply, in the shapes the agent expects back.
pub fn parts_of(blocks: &[ReplyBlock]) -> (String, Vec<ToolCall>) {
    let mut text = String::new();
    let mut calls = Vec::new();

    for block in blocks {
        match block {
            ReplyBlock::Text { text: piece } => text.push_str(piece),
            ReplyBlock::ToolUse { id, name, input } => calls.push(ToolCall {
                id: Some(id.clone()),
                function: ToolCallFunction {
                    name: name.clone(),
                    // Back to a string, which is how the rest of the agent carries arguments.
                    arguments: Some(input.to_string()),
                },
                extra_content: None,
            }),
            ReplyBlock::Thinking {} | ReplyBlock::RedactedThinking {} | ReplyBlock::Other => {}
        }
    }

    (text, calls)
}

/// What a request cost, as this API states it.
///
/// Every figure is optional because a stream states them in pieces: the prompt's at the start and
/// the reply's at the end, each later report replacing what it restates. A `null` is a figure not
/// stated, which is how this API says a cache was not involved.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub struct WireUsage {
    #[serde(default)]
    pub input_tokens: Option<u64>,
    #[serde(default)]
    pub output_tokens: Option<u64>,
    #[serde(default)]
    pub cache_creation_input_tokens: Option<u64>,
    #[serde(default)]
    pub cache_read_input_tokens: Option<u64>,
}

impl WireUsage {
    /// This report with whatever `later` restates put over it.
    ///
    /// The counts in a later report are cumulative, so a figure it states replaces the earlier one
    /// and a figure it leaves out stands.
    pub fn then(self, later: WireUsage) -> WireUsage {
        WireUsage {
            input_tokens: later.input_tokens.or(self.input_tokens),
            output_tokens: later.output_tokens.or(self.output_tokens),
            cache_creation_input_tokens: later
                .cache_creation_input_tokens
                .or(self.cache_creation_input_tokens),
            cache_read_input_tokens: later
                .cache_read_input_tokens
                .or(self.cache_read_input_tokens),
        }
    }
}

impl From<WireUsage> for Usage {
    fn from(usage: WireUsage) -> Self {
        let read = usage.cache_read_input_tokens.unwrap_or(0);
        let written = usage.cache_creation_input_tokens.unwrap_or(0);
        Self {
            // Everything the request carried, whoever read it. This API states `input_tokens`
            // net of the cache, so a round that hit it reports a fraction of the prompt it
            // actually sent, and a context gauge fed that figure would show a conversation
            // shrinking as it grew. What the three add up to is the prompt.
            //
            // Saturating, because the three are the service's figures and a sum that wrapped
            // would report a tiny prompt to the gauge that decides when a conversation is cut.
            prompt_tokens: usage
                .input_tokens
                .unwrap_or(0)
                .saturating_add(read)
                .saturating_add(written),
            completion_tokens: usage.output_tokens.unwrap_or(0),
            // Carried as well as added in. Summed alone they say what the round sent and nothing
            // about what it cost, and those differ by roughly ten times: a session where every
            // breakpoint missed reports the same figures as one where every one of them hit.
            cached: Cached {
                read_tokens: read,
                written_tokens: written,
            },
        }
    }
}

/// What the service said it was not going to finish a reply for.
///
/// A closed set, so the name of a failure that reaches a person or a diagnostic is this crate's own
/// and never text the service wrote: what the service sends is matched against the error types this
/// API documents, and anything else is [`Failure::Unknown`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    InvalidRequest,
    RequestTooLarge,
    NotFound,
    /// The service declined to write the reply.
    Refusal,
    /// The prompt and the reply together filled the model's window.
    ContextFull,
    Authentication,
    Permission,
    Billing,
    RateLimit,
    /// A fault of the service's own.
    Api,
    Overloaded,
    Timeout,
    /// A name this crate does not know.
    Unknown,
}

impl Failure {
    /// The failure an error type the service sent stands for.
    pub fn named(error_type: &str) -> Self {
        match error_type {
            "invalid_request_error" => Self::InvalidRequest,
            "request_too_large" => Self::RequestTooLarge,
            "not_found_error" => Self::NotFound,
            "authentication_error" => Self::Authentication,
            "permission_error" => Self::Permission,
            "billing_error" => Self::Billing,
            "rate_limit_error" => Self::RateLimit,
            "api_error" => Self::Api,
            "overloaded_error" => Self::Overloaded,
            "timeout_error" => Self::Timeout,
            _ => Self::Unknown,
        }
    }

    /// The word this crate says it with.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request_error",
            Self::RequestTooLarge => "request_too_large",
            Self::NotFound => "not_found_error",
            Self::Refusal => STOP_REASON_REFUSAL,
            Self::ContextFull => STOP_REASON_CONTEXT_FULL,
            Self::Authentication => "authentication_error",
            Self::Permission => "permission_error",
            Self::Billing => "billing_error",
            Self::RateLimit => "rate_limit_error",
            Self::Api => "api_error",
            Self::Overloaded => "overloaded_error",
            Self::Timeout => "timeout_error",
            Self::Unknown => "unknown_error",
        }
    }

    /// Whether asking again could get past it: an overloaded service, a fault of its own, a rate
    /// limit, a timeout. Everything else it names is a property of the request, which a second
    /// identical one has too.
    pub fn is_transient(self) -> bool {
        matches!(
            self,
            Self::RateLimit | Self::Api | Self::Overloaded | Self::Timeout
        )
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One event of a stream, as far as a reply is assembled from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamEvent {
    /// The first event: the model that is answering and what the prompt cost.
    MessageStart {
        model: Option<String>,
        usage: Option<WireUsage>,
    },
    BlockStart {
        index: usize,
        start: BlockStart,
    },
    BlockDelta {
        index: usize,
        delta: Delta,
    },
    /// Why the model stopped, and the count so far.
    MessageDelta {
        stop_reason: Option<String>,
        usage: Option<WireUsage>,
    },
    MessageStop,
    /// The service said, part way through the reply, that it was not going to finish it.
    Failed {
        failure: Failure,
    },
}

/// What a block opening says it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockStart {
    Text,
    /// The opening event names the call and nothing else; every byte of its arguments arrives in
    /// the deltas that follow.
    ToolUse {
        id: String,
        name: String,
    },
    Reasoning,
    Other,
}

/// A piece of a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delta {
    Text(String),
    /// A fragment of a call's arguments, which are not JSON until the last one has arrived.
    Arguments(String),
    Reasoning,
    Other,
}

/// What reading one event's JSON came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decoded {
    Event(StreamEvent),
    /// A kind this crate does not model, which a service may add: ignored rather than refused.
    Ignored,
    /// An event of a kind this reads whose body is not in the shape it states. Kept apart from
    /// [`Decoded::Ignored`] so the stream is still drained and the reply is refused once it has
    /// ended, in the same place a reply that arrived whole is.
    Unreadable,
}

/// Read one stream event, given as the JSON its `data:` line carried.
pub fn stream_event(value: &Value) -> Decoded {
    let Some(kind) = value.get("type").and_then(Value::as_str) else {
        return Decoded::Ignored;
    };
    match kind {
        "message_start" => {
            let message = value.get("message");
            Decoded::Event(StreamEvent::MessageStart {
                model: message
                    .and_then(|message| message.get("model"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                usage: message
                    .and_then(|message| message.get("usage"))
                    .and_then(usage_of),
            })
        }
        "content_block_start" => {
            let (Some(index), Some(block)) = (index_of(value), value.get("content_block")) else {
                return Decoded::Unreadable;
            };
            let start = match block.get("type").and_then(Value::as_str) {
                Some("text") => BlockStart::Text,
                Some("tool_use") => {
                    let (Some(id), Some(name)) = (
                        block.get("id").and_then(Value::as_str),
                        block.get("name").and_then(Value::as_str),
                    ) else {
                        return Decoded::Unreadable;
                    };
                    BlockStart::ToolUse {
                        id: id.to_string(),
                        name: name.to_string(),
                    }
                }
                Some("thinking" | "redacted_thinking") => BlockStart::Reasoning,
                _ => BlockStart::Other,
            };
            Decoded::Event(StreamEvent::BlockStart { index, start })
        }
        "content_block_delta" => {
            let (Some(index), Some(delta)) = (index_of(value), value.get("delta")) else {
                return Decoded::Unreadable;
            };
            let piece = |field: &str| delta.get(field).and_then(Value::as_str).map(str::to_string);
            let delta = match delta.get("type").and_then(Value::as_str) {
                Some("text_delta") => match piece("text") {
                    Some(text) => Delta::Text(text),
                    None => return Decoded::Unreadable,
                },
                Some("input_json_delta") => match piece("partial_json") {
                    Some(fragment) => Delta::Arguments(fragment),
                    None => return Decoded::Unreadable,
                },
                Some("thinking_delta" | "signature_delta") => Delta::Reasoning,
                _ => Delta::Other,
            };
            Decoded::Event(StreamEvent::BlockDelta { index, delta })
        }
        "message_delta" => Decoded::Event(StreamEvent::MessageDelta {
            stop_reason: value
                .get("delta")
                .and_then(|delta| delta.get("stop_reason"))
                .and_then(Value::as_str)
                .map(str::to_string),
            usage: value.get("usage").and_then(usage_of),
        }),
        "message_stop" => Decoded::Event(StreamEvent::MessageStop),
        "error" => Decoded::Event(StreamEvent::Failed {
            failure: value
                .get("error")
                .and_then(|error| error.get("type"))
                .and_then(Value::as_str)
                .map_or(Failure::Unknown, Failure::named),
        }),
        _ => Decoded::Ignored,
    }
}

fn index_of(value: &Value) -> Option<usize> {
    value
        .get("index")
        .and_then(Value::as_u64)
        .and_then(|index| usize::try_from(index).ok())
}

fn usage_of(value: &Value) -> Option<WireUsage> {
    serde_json::from_value(value.clone()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Part, ToolCallRequest, ToolCallRequestFunction};

    fn body_of(request: &MessagesRequest) -> Value {
        serde_json::to_value(request).expect("a request serialises")
    }

    fn offered() -> Vec<Tool> {
        vec![Tool::function(
            "read_file",
            "Read a file",
            json!({"type": "object", "properties": {"path": {"type": "string"}}}),
        )]
    }

    fn request(messages: &[Message]) -> MessagesRequest {
        let tools = offered();
        request_from("claude-test", messages, Some(&tools))
    }

    fn call(id: &str, name: &str, arguments: &str) -> ToolCallRequest {
        ToolCallRequest {
            id: id.to_string(),
            kind: "function".to_string(),
            function: ToolCallRequestFunction {
                name: name.to_string(),
                arguments: arguments.to_string(),
            },
            extra_content: None,
        }
    }

    /// The body this API states: the model, a required ceiling, and a system prompt that is a list
    /// of blocks beside the messages rather than one of them.
    #[test]
    fn a_request_is_the_body_this_api_states() {
        let body = body_of(&request(&[
            Message::system("be brief"),
            Message::user("hello"),
        ]));

        assert_eq!(body["model"], "claude-test");
        assert_eq!(body["max_tokens"], 32_000);
        assert_eq!(body["system"][0]["type"], "text");
        assert_eq!(body["system"][0]["text"], "be brief");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"][0]["type"], "text");
        assert_eq!(body["messages"][0]["content"][0]["text"], "hello");
        assert_eq!(body["tools"][0]["name"], "read_file");
        assert_eq!(body["tools"][0]["input_schema"]["type"], "object");
        // Neither is sent unless somebody asked, so a model keeps its own default.
        assert!(body.get("output_config").is_none());
        assert!(body.get("stream").is_none());
        assert!(body["tools"][0].get("eager_input_streaming").is_none());
    }

    /// The note the agent writes after the first prompt of a session begins with a count. A reader
    /// that lays the blocks of a turn end to end would read a prompt ending in a number and a note
    /// beginning with one as a single number unless the note's block starts on a new paragraph.
    #[test]
    fn a_note_after_the_line_a_person_typed_is_a_block_on_a_new_paragraph() {
        let body = body_of(&request(&[
            Message::system("be brief"),
            Message::user("see issues/1106"),
            Message::user("16 skills, of your own or the project's, are not listed above: a, b."),
        ]));
        let blocks = body["messages"][0]["content"].as_array().expect("blocks");
        assert_eq!(body["messages"].as_array().expect("messages").len(), 1);
        assert_eq!(blocks[0]["text"], "see issues/1106");
        let note = blocks[1]["text"].as_str().expect("a text block");
        assert!(note.starts_with("\n\n16 skills"), "{note:?}");
    }

    /// The level is a word in `output_config`, the object this API keeps it in, and not the field
    /// the OpenAI-shaped wire names it by.
    #[test]
    fn a_level_is_sent_inside_the_object_this_api_states() {
        for level in Effort::ALL {
            let body = body_of(&request(&[Message::user("hi")]).with_effort(Some(level)));
            assert_eq!(body["output_config"]["effort"], level.as_str());
            assert!(body.get("reasoning_effort").is_none());
        }
        let body = body_of(&request(&[Message::user("hi")]).with_effort(None));
        assert!(body.get("output_config").is_none());
    }

    /// Arguments as written is a field of each tool, and a request that did not ask carries none.
    #[test]
    fn asking_for_arguments_as_written_marks_every_tool_and_nothing_else() {
        let tools = vec![
            Tool::function("a", "first", json!({"type": "object"})),
            Tool::function("b", "second", json!({"type": "object"})),
        ];
        let asked = request_from("m", &[Message::user("hi")], Some(&tools))
            .with_arguments_as_written()
            .streamed();
        let body = body_of(&asked);
        assert_eq!(body["tools"][0]["eager_input_streaming"], true);
        assert_eq!(body["tools"][1]["eager_input_streaming"], true);
        assert_eq!(body["stream"], true);
        // The legacy beta is not how this API is asked, and no header or field of it is sent.
        assert!(body.get("anthropic_beta").is_none());
    }

    /// No tools offered means no field, since this API refuses `tools: []` as a request to use none
    /// and refuses a call block where no tool is declared.
    #[test]
    fn a_request_offering_no_tools_sends_no_tools_and_its_calls_as_text() {
        let messages = [
            Message::user("go"),
            Message::assistant_calling("", vec![call("a", "read_file", r#"{"path":"x"}"#)]),
            Message::tool_result("a", "contents"),
        ];
        for tools in [None, Some(Vec::new())] {
            let body = body_of(&request_from("m", &messages, tools.as_deref()));
            assert!(body.get("tools").is_none());
            let sent = body["messages"].to_string();
            assert!(!sent.contains("tool_use"), "{sent}");
            assert!(!sent.contains("tool_result"), "{sent}");
            assert!(sent.contains("[call a: read_file"), "{sent}");
        }
    }

    /// A call and its result are blocks on turns, matched by id, and the arguments are an object.
    #[test]
    fn a_call_and_its_result_are_matched_blocks_and_the_arguments_an_object() {
        let body = body_of(&request(&[
            Message::user("go"),
            Message::assistant_calling("", vec![call("toolu_1", "read_file", r#"{"path":"x"}"#)]),
            Message::tool_result("toolu_1", "contents"),
        ]));

        let call = &body["messages"][1]["content"][0];
        assert_eq!(body["messages"][1]["role"], "assistant");
        assert_eq!(call["type"], "tool_use");
        assert_eq!(call["id"], "toolu_1");
        assert_eq!(call["input"], json!({"path": "x"}));

        let result = &body["messages"][2]["content"][0];
        assert_eq!(body["messages"][2]["role"], "user");
        assert_eq!(result["type"], "tool_result");
        assert_eq!(result["tool_use_id"], "toolu_1");
        assert_eq!(result["content"][0]["type"], "text");
        assert_eq!(result["content"][0]["text"], "contents");
    }

    /// This API refuses an empty text block, and a tool that wrote nothing is a result it takes
    /// with no content.
    #[test]
    fn a_result_that_wrote_nothing_carries_no_content() {
        let body = body_of(&request(&[
            Message::user("go"),
            Message::assistant_calling("", vec![call("a", "read_file", "{}")]),
            Message::tool_result("a", ""),
        ]));
        let result = &body["messages"][2]["content"][0];
        assert_eq!(result["type"], "tool_result");
        assert!(result.get("content").is_none(), "{result}");
    }

    /// A picture and a document are different blocks, each with its media type and bytes inline.
    /// A type this API takes as neither is dropped rather than refused by the service.
    #[test]
    fn an_attachment_crosses_as_inline_data_under_the_block_its_type_names() {
        let body = body_of(&request(&[Message::user_parts(vec![
            Part::Text {
                text: "look".to_string(),
            },
            Part::ImageUrl {
                image_url: crate::protocol::ImageUrl {
                    url: "data:image/png;base64,QUJD".to_string(),
                },
            },
            Part::ImageUrl {
                image_url: crate::protocol::ImageUrl {
                    url: "data:application/pdf;base64,REVG".to_string(),
                },
            },
            Part::ImageUrl {
                image_url: crate::protocol::ImageUrl {
                    url: "data:image/tiff;base64,QUJD".to_string(),
                },
            },
        ])]));
        let content = body["messages"][0]["content"].as_array().expect("blocks");
        assert_eq!(content.len(), 3, "{content:?}");
        assert_eq!(content[1]["type"], "image");
        assert_eq!(content[1]["source"]["type"], "base64");
        assert_eq!(content[1]["source"]["media_type"], "image/png");
        assert_eq!(content[1]["source"]["data"], "QUJD");
        assert_eq!(content[2]["type"], "document");
        assert_eq!(content[2]["source"]["media_type"], "application/pdf");
    }

    /// The prompt is a breakpoint, and the last block of the conversation is another.
    #[test]
    fn the_system_prompt_and_the_end_of_the_conversation_carry_a_breakpoint() {
        let body = body_of(&request(&[
            Message::system("be brief"),
            Message::user("one"),
            Message::assistant("two"),
            Message::user("three"),
        ]));
        assert_eq!(
            body["system"][0]["cache_control"],
            json!({"type": "ephemeral"})
        );
        let messages = body["messages"].as_array().expect("turns");
        for turn in &messages[..messages.len() - 1] {
            assert!(turn["content"][0].get("cache_control").is_none(), "{turn}");
        }
        assert_eq!(
            messages[2]["content"][0]["cache_control"],
            json!({"type": "ephemeral"})
        );
    }

    /// A round that ends in a result is marked on the result, which this API takes a breakpoint on.
    #[test]
    fn a_conversation_ending_in_a_tool_result_is_marked_on_the_result() {
        let body = body_of(&request(&[
            Message::user("go"),
            Message::assistant_calling("", vec![call("a", "read_file", "{}")]),
            Message::tool_result("a", "contents"),
        ]));
        let result = &body["messages"][2]["content"][0];
        assert_eq!(result["cache_control"], json!({"type": "ephemeral"}));
    }

    /// A conversation ending in a picture or a call is left with the prompt's breakpoint alone,
    /// which costs a cache write and nothing else.
    #[test]
    fn a_conversation_ending_in_a_call_is_marked_nowhere_but_the_prompt() {
        let body = body_of(&request(&[
            Message::system("be brief"),
            Message::user("go"),
            Message::assistant_calling("", vec![call("a", "read_file", "{}")]),
        ]));
        let sent = body["messages"].to_string();
        assert!(!sent.contains("cache_control"), "{sent}");
        assert!(body["system"][0].get("cache_control").is_some());
    }

    /// A request whose conversation no later request sends keeps the prompt's breakpoint and loses
    /// the one on the end of it.
    #[test]
    fn a_request_giving_up_its_conversation_keeps_the_prompts_breakpoint_alone() {
        let body = body_of(
            &request(&[Message::system("be brief"), Message::user("one")])
                .without_the_conversation_breakpoint(),
        );
        assert!(body["system"][0].get("cache_control").is_some());
        assert!(!body["messages"].to_string().contains("cache_control"));
    }

    /// Without breakpoints a request keeps everything else it was asked to carry.
    #[test]
    fn a_request_without_breakpoints_keeps_everything_that_was_asked_for() {
        let tools = offered();
        let with = request_from(
            "claude-test",
            &[Message::system("be brief"), Message::user("one")],
            Some(&tools),
        )
        .with_effort(Some(Effort::High))
        .with_arguments_as_written()
        .streamed();
        let without = with.clone().without_breakpoints();

        let mut expected = body_of(&with);
        expected["system"][0]
            .as_object_mut()
            .expect("a block")
            .remove("cache_control");
        expected["messages"][0]["content"][0]
            .as_object_mut()
            .expect("a block")
            .remove("cache_control");
        assert_eq!(body_of(&without), expected);
        assert!(!body_of(&without).to_string().contains("cache_control"));
    }

    /// A lifetime goes on every breakpoint the request carries and adds none, and a request
    /// nobody chose one for states none.
    #[test]
    fn a_chosen_lifetime_is_stated_on_every_breakpoint_and_none_is_added() {
        let base = request(&[Message::system("be brief"), Message::user("one")]);

        let chosen = body_of(&base.clone().with_cache_ttl(Some(CacheTtl::OneHour)));
        let hour = json!({"type": "ephemeral", "ttl": "1h"});
        assert_eq!(chosen["system"][0]["cache_control"], hour);
        assert_eq!(chosen["messages"][0]["content"][0]["cache_control"], hour);

        let none = body_of(&base.clone().with_cache_ttl(None));
        assert_eq!(
            none["system"][0]["cache_control"],
            json!({"type": "ephemeral"})
        );

        // Not added where a breakpoint is not: a request that gave them up stays without them.
        let bare = body_of(
            &base
                .without_breakpoints()
                .with_cache_ttl(Some(CacheTtl::OneHour)),
        );
        assert!(!bare.to_string().contains("cache_control"));
    }

    /// A reply is what the agent works from, so each kind of block lands where the rest of the agent
    /// reads it: words joined in order, a call with its id, name and arguments as the string the agent
    /// carries, and reasoning in neither.
    #[test]
    fn a_reply_is_read_back_into_text_and_calls() {
        let reply: MessageResponse = serde_json::from_value(json!({
            "id": "msg_1",
            "type": "message",
            "role": "assistant",
            "model": "claude-test-20260101",
            "content": [
                {"type": "thinking", "thinking": "hmm", "signature": "sig"},
                {"type": "text", "text": "I will "},
                {"type": "text", "text": "read it."},
                {"type": "tool_use", "id": "toolu_1", "name": "read_file", "input": {"path": "x"}}
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 10, "output_tokens": 5}
        }))
        .expect("a reply");

        assert_eq!(reply.model.as_deref(), Some("claude-test-20260101"));
        assert!(reply.content[0].is_reasoning());
        let (text, calls) = parts_of(&reply.content);
        assert_eq!(text, "I will read it.");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id.as_deref(), Some("toolu_1"));
        assert_eq!(calls[0].function.name, "read_file");
        assert_eq!(
            calls[0].function.arguments.as_deref(),
            Some(r#"{"path":"x"}"#)
        );
    }

    /// A call that takes no arguments may arrive without the field, and is still the call the model
    /// made. Dropping the block would lose it.
    #[test]
    fn a_reply_calling_a_tool_with_no_arguments_keeps_the_call() {
        let reply: MessageResponse = serde_json::from_value(json!({
            "content": [{"type": "tool_use", "id": "toolu_1", "name": "ls"}]
        }))
        .expect("a reply");
        let (_, calls) = parts_of(&reply.content);
        assert_eq!(calls[0].function.arguments.as_deref(), Some("{}"));
    }

    /// A service adds block kinds, and one this does not model leaves the rest readable.
    #[test]
    fn a_block_kind_this_does_not_model_leaves_the_rest_of_the_reply_readable() {
        let reply: MessageResponse = serde_json::from_value(json!({
            "content": [
                {"type": "server_tool_use", "id": "srvtoolu_1", "name": "web_search"},
                {"type": "text", "text": "answer"}
            ]
        }))
        .expect("a reply");
        assert_eq!(parts_of(&reply.content).0, "answer");
    }

    /// The cache's reads and writes are counted in the prompt, because this API states the prompt
    /// net of them, and are carried separately too.
    #[test]
    fn cached_tokens_are_counted_as_the_prompt_they_were_and_carried_apart() {
        let usage: WireUsage = serde_json::from_value(json!({
            "input_tokens": 12,
            "output_tokens": 7,
            "cache_creation_input_tokens": 300,
            "cache_read_input_tokens": 4000
        }))
        .expect("usage");
        let usage = Usage::from(usage);
        assert_eq!(usage.prompt_tokens, 12 + 300 + 4000);
        assert_eq!(usage.completion_tokens, 7);
        assert_eq!(usage.cached.read_tokens, 4000);
        assert_eq!(usage.cached.written_tokens, 300);
    }

    /// The three figures are the service's, so a sum that wrapped would report a tiny prompt to the gauge
    /// that decides when a conversation is cut, and one that panicked would end the session.
    #[test]
    fn a_prompt_too_large_to_add_saturates_and_never_wraps() {
        let usage = Usage::from(WireUsage {
            input_tokens: Some(u64::MAX),
            cache_read_input_tokens: Some(5),
            cache_creation_input_tokens: Some(7),
            ..WireUsage::default()
        });
        assert_eq!(usage.prompt_tokens, u64::MAX);
    }

    /// A system prompt of nothing but whitespace is no prompt, since this API refuses a text block
    /// with no word in it and a request that carried one would never be answered.
    #[test]
    fn a_system_prompt_of_only_whitespace_is_not_sent() {
        let body = body_of(&request(&[Message::system(" \n "), Message::user("hello")]));
        assert!(body.get("system").is_none(), "{body}");
        assert_eq!(body["messages"][0]["content"][0]["text"], "hello");
    }

    /// This API takes letters, digits, `_` and `-` in a call's id. Ids another service wrote can hold a
    /// dot or a colon, or be empty, and one in the history is a request refused on every later round.
    /// The call and the result that answers it are changed the same way, so they still name each other.
    #[test]
    fn a_call_id_this_api_would_refuse_is_changed_on_the_call_and_its_result_alike() {
        for (written, sent) in [
            ("toolu_01A-b", "toolu_01A-b"),
            ("functions.read_file:0", "functions_read_file_0"),
            ("call 7/é", "call_7__"),
            ("", "_"),
        ] {
            let body = body_of(&request(&[
                Message::user("go"),
                Message::assistant_calling("", vec![call(written, "read_file", "{}")]),
                Message::tool_result(written, "contents"),
            ]));
            assert_eq!(body["messages"][1]["content"][0]["id"], sent, "{written:?}");
            assert_eq!(
                body["messages"][2]["content"][0]["tool_use_id"], sent,
                "{written:?}"
            );
        }
    }

    /// A tool that printed one empty line wrote nothing this API takes as a text block, so the result
    /// that answers the call carries no content, as it does where the tool wrote nothing at all.
    #[test]
    fn a_result_of_only_whitespace_carries_no_content() {
        let body = body_of(&request(&[
            Message::user("go"),
            Message::assistant_calling("", vec![call("a", "read_file", "{}")]),
            Message::tool_result("a", "\n"),
        ]));
        let result = &body["messages"][2]["content"][0];
        assert_eq!(result["type"], "tool_result");
        assert!(result.get("content").is_none(), "{result}");
    }

    /// A `null` is a figure not stated, which is how this API says no cache was involved, and reads
    /// as silence rather than failing the reply.
    #[test]
    fn a_null_cache_figure_reads_as_silence() {
        let usage: WireUsage = serde_json::from_value(json!({
            "input_tokens": 12,
            "output_tokens": 7,
            "cache_creation_input_tokens": null
        }))
        .expect("usage");
        let usage = Usage::from(usage);
        assert_eq!(usage.prompt_tokens, 12);
        assert!(!usage.cached.any());
    }

    /// The prompt is stated at the start of a stream and the reply's count at the end, and a later
    /// report replaces only what it restates.
    #[test]
    fn a_later_report_replaces_what_it_restates_and_keeps_the_rest() {
        let start = WireUsage {
            input_tokens: Some(100),
            output_tokens: Some(1),
            cache_read_input_tokens: Some(90),
            ..WireUsage::default()
        };
        let end = WireUsage {
            output_tokens: Some(40),
            ..WireUsage::default()
        };
        let both = start.then(end);
        assert_eq!(both.input_tokens, Some(100));
        assert_eq!(both.output_tokens, Some(40));
        assert_eq!(both.cache_read_input_tokens, Some(90));
    }

    fn decoded(payload: Value) -> Decoded {
        stream_event(&payload)
    }

    /// Each event a reply is assembled from decodes to what it states: the model and prompt cost at
    /// the start, a block opening, the pieces of its text and arguments, the stop reason with the
    /// count at the end, and the close.
    #[test]
    fn the_events_that_carry_a_reply_decode() {
        assert_eq!(
            decoded(json!({"type": "message_start", "message": {
                "model": "claude-test-20260101",
                "usage": {"input_tokens": 25, "output_tokens": 1}
            }})),
            Decoded::Event(StreamEvent::MessageStart {
                model: Some("claude-test-20260101".to_string()),
                usage: Some(WireUsage {
                    input_tokens: Some(25),
                    output_tokens: Some(1),
                    ..WireUsage::default()
                }),
            })
        );
        assert_eq!(
            decoded(
                json!({"type": "content_block_start", "index": 1, "content_block": {
                    "type": "tool_use", "id": "toolu_1", "name": "read_file", "input": {}
                }})
            ),
            Decoded::Event(StreamEvent::BlockStart {
                index: 1,
                start: BlockStart::ToolUse {
                    id: "toolu_1".to_string(),
                    name: "read_file".to_string()
                },
            })
        );
        assert_eq!(
            decoded(json!({"type": "content_block_delta", "index": 0,
                "delta": {"type": "text_delta", "text": "Hel"}})),
            Decoded::Event(StreamEvent::BlockDelta {
                index: 0,
                delta: Delta::Text("Hel".to_string())
            })
        );
        assert_eq!(
            decoded(json!({"type": "content_block_delta", "index": 1,
                "delta": {"type": "input_json_delta", "partial_json": "{\"pa"}})),
            Decoded::Event(StreamEvent::BlockDelta {
                index: 1,
                delta: Delta::Arguments("{\"pa".to_string())
            })
        );
        assert_eq!(
            decoded(
                json!({"type": "message_delta", "delta": {"stop_reason": "end_turn"},
                "usage": {"output_tokens": 15}})
            ),
            Decoded::Event(StreamEvent::MessageDelta {
                stop_reason: Some("end_turn".to_string()),
                usage: Some(WireUsage {
                    output_tokens: Some(15),
                    ..WireUsage::default()
                }),
            })
        );
        assert_eq!(
            decoded(json!({"type": "message_stop"})),
            Decoded::Event(StreamEvent::MessageStop)
        );
    }

    /// Reasoning is no part of the reply, and a block opening as one is only marked as it.
    #[test]
    fn reasoning_arrives_as_reasoning_and_adds_nothing_to_the_reply() {
        assert_eq!(
            decoded(json!({"type": "content_block_start", "index": 0,
                "content_block": {"type": "thinking", "thinking": "", "signature": ""}})),
            Decoded::Event(StreamEvent::BlockStart {
                index: 0,
                start: BlockStart::Reasoning
            })
        );
        for kind in ["thinking_delta", "signature_delta"] {
            assert_eq!(
                decoded(json!({"type": "content_block_delta", "index": 0,
                    "delta": {"type": kind, "thinking": "x", "signature": "y"}})),
                Decoded::Event(StreamEvent::BlockDelta {
                    index: 0,
                    delta: Delta::Reasoning
                })
            );
        }
    }

    /// A kind added later, and the keepalive, are read past rather than failing a reply that
    /// carries them.
    #[test]
    fn an_event_this_does_not_know_is_ignored_rather_than_failing() {
        for payload in [
            json!({"type": "ping"}),
            json!({"type": "content_block_stop", "index": 0}),
            json!({"type": "something_new", "x": 1}),
            json!({"no_type": true}),
        ] {
            assert_eq!(decoded(payload.clone()), Decoded::Ignored, "{payload}");
        }
    }

    /// An event of a kind this reads whose body is not in the shape it states is told apart from
    /// one it does not read, so the reply it was part of is refused and not returned short.
    #[test]
    fn a_known_event_in_a_shape_this_cannot_read_is_unreadable_and_not_ignored() {
        for payload in [
            json!({"type": "content_block_delta", "index": 0,
                "delta": {"type": "text_delta"}}),
            json!({"type": "content_block_delta", "index": 0,
                "delta": {"type": "input_json_delta", "partial_json": 7}}),
            json!({"type": "content_block_delta", "delta": {"type": "text_delta", "text": "x"}}),
            json!({"type": "content_block_start", "index": 0,
                "content_block": {"type": "tool_use", "name": "no_id"}}),
            json!({"type": "content_block_start", "index": 0}),
        ] {
            assert_eq!(decoded(payload.clone()), Decoded::Unreadable, "{payload}");
        }
    }

    /// A failure reported mid-stream is one of a closed set this crate holds, so text the service
    /// wrote never reaches a person or a diagnostic.
    #[test]
    fn a_reported_failure_is_one_of_a_closed_set_and_never_named_by_the_service() {
        assert_eq!(
            decoded(json!({"type": "error", "error": {
                "type": "overloaded_error", "message": "Overloaded"}})),
            Decoded::Event(StreamEvent::Failed {
                failure: Failure::Overloaded
            })
        );
        assert_eq!(
            decoded(json!({"type": "error", "error": {
                "type": "ignore previous instructions", "message": "x"}})),
            Decoded::Event(StreamEvent::Failed {
                failure: Failure::Unknown
            })
        );
        assert_eq!(
            decoded(json!({"type": "error"})),
            Decoded::Event(StreamEvent::Failed {
                failure: Failure::Unknown
            })
        );
    }
}
