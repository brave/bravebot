//! Wire types for OpenAI's Responses API, and the translation to and from the shapes the rest of
//! this agent speaks ([BACKEND-61](../../../../docs/specs/backends.md#BACKEND-61)).
//!
//! The agent's conversation is held in OpenAI-compatible chat types. [`crate::conversation`]
//! restates it as turns of blocks, and this module spells those blocks the way this API does:
//!
//! - The conversation is a flat list of `input` items. Words are a `message` item holding typed
//!   parts, and a call and its result are items of their own beside the messages and not parts
//!   inside them, so a turn is cut wherever one falls.
//! - The system prompt is `instructions`, and each tool is one flat object.
//! - A request is always streamed and never asks the service to keep anything: `stream` and
//!   `store` are fixed here.
//! - The reply is read from events. The items it is made of arrive in `output_item.done` events,
//!   and the `output` list of the final event is empty on the host that serves a ChatGPT account, so
//!   nothing is read from it.
//! - Usage counts the prompt with the cached part inside it, and the reasoning inside the reply.
//!
//! Nothing here inspects content to make a decision. Text is moved between shapes and handed on with
//! whatever label it arrived under.

use crate::anthropic::protocol::Failure;
use crate::conversation::{self, Spelling};
use crate::protocol::{Cached, Effort, Message, Tool, ToolCall, ToolCallFunction, Usage};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The media types this API takes as a picture. A document of any type is dropped.
const PICTURE_TYPES: [&str; 4] = ["image/png", "image/jpeg", "image/webp", "image/gif"];

/// The fields of a request that belong to the request and not to a model's `options`, whether or not
/// the turn set them: a person's configuration carries what a model is told, not which conversation
/// the service holds, whether it keeps it, how it is read back or what tools exist.
pub const OWNED: [&str; 8] = [
    "model",
    "instructions",
    "input",
    "tools",
    "tool_choice",
    "stream",
    "store",
    "previous_response_id",
];

/// The `reasoning.summary` value a request asks for where it asks for reasoning at all.
const SUMMARY: &str = "auto";

/// A request to `/responses`.
#[derive(Debug, Clone, Serialize)]
pub struct ResponsesRequest {
    /// The name the service knows the model by.
    pub model: String,
    /// The system prompt, hoisted out of the conversation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    pub input: Vec<Item>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolSpec>>,
    /// `auto` wherever a tool is offered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<&'static str>,
    /// Always set: a service in this protocol may refuse a reply asked for whole.
    pub stream: bool,
    /// Always `false`: nothing is asked to be kept at the service, so the conversation is the
    /// session's alone.
    pub store: bool,
    /// Absent unless a level was given, so a request nobody asked anything of sends the body it
    /// always sent and the model keeps its own defaults.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Reasoning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
}

/// One item of `input`.
///
/// Serialised only: the tag is a fixed string this crate chooses, never one it reads back.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Item {
    Message {
        role: &'static str,
        content: Vec<Part>,
    },
    FunctionCall {
        call_id: String,
        name: String,
        /// The arguments as the string that holds them.
        arguments: String,
    },
    FunctionCallOutput {
        call_id: String,
        output: String,
    },
}

/// One part of a message. A user's words are `input_text` and the model's own are `output_text`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Part {
    InputText { text: String },
    OutputText { text: String },
    InputImage { image_url: String },
}

/// A tool as this API takes it: one flat object, not nested under a `function` key.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolSpec {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub name: String,
    pub description: String,
    pub parameters: Value,
    /// Always `false`: strict mode wants every property required and no other allowed, which a tool
    /// with an optional argument is not, and the service's own default is not this program's to
    /// guess at.
    pub strict: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Reasoning {
    pub effort: &'static str,
    pub summary: &'static str,
}

/// A block of a turn before it is placed among the items.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Text(String),
    Picture(String),
    Call {
        id: String,
        name: String,
        arguments: String,
    },
    Result {
        id: String,
        text: String,
    },
}

/// How this API spells the blocks of a turn.
struct Spell;

impl Spelling for Spell {
    type Block = Block;

    fn text(&self, text: &str) -> Block {
        Block::Text(text.to_string())
    }

    /// A picture is a data URI, and a type this API does not take as one is dropped.
    fn attachment(&self, media_type: &str, data: &str, _position: usize) -> Option<Block> {
        PICTURE_TYPES
            .contains(&media_type)
            .then(|| Block::Picture(format!("data:{media_type};base64,{data}")))
    }

    fn call(&self, id: &str, name: &str, input: Value) -> Block {
        Block::Call {
            id: id.to_string(),
            name: name.to_string(),
            arguments: input.to_string(),
        }
    }

    fn result(&self, id: &str, text: String) -> Block {
        Block::Result {
            id: id.to_string(),
            text,
        }
    }
}

/// Cut a turn's blocks into items: the words and pictures before a call or a result are one
/// message, the call or the result is an item of its own, and what follows starts another message.
fn items_of(role: &'static str, blocks: Vec<Block>, items: &mut Vec<Item>) {
    let mut parts: Vec<Part> = Vec::new();
    let flush = |parts: &mut Vec<Part>, items: &mut Vec<Item>| {
        if !parts.is_empty() {
            items.push(Item::Message {
                role,
                content: std::mem::take(parts),
            });
        }
    };
    for block in blocks {
        match block {
            Block::Text(text) if role == "assistant" => parts.push(Part::OutputText { text }),
            Block::Text(text) => parts.push(Part::InputText { text }),
            Block::Picture(image_url) => parts.push(Part::InputImage { image_url }),
            Block::Call {
                id,
                name,
                arguments,
            } => {
                flush(&mut parts, items);
                items.push(Item::FunctionCall {
                    call_id: id,
                    name,
                    arguments,
                });
            }
            Block::Result { id, text } => {
                flush(&mut parts, items);
                items.push(Item::FunctionCallOutput {
                    call_id: id,
                    output: text,
                });
            }
        }
    }
    flush(&mut parts, items);
}

/// The request body for a conversation, a model's name and the tools offered.
///
/// `effort` is the level a turn asked for and `ceiling` the output ceiling somebody stated, both
/// absent where nobody asked.
pub fn request_from(
    model: &str,
    messages: &[Message],
    tools: Option<&[Tool]>,
    effort: Option<Effort>,
    ceiling: Option<u64>,
) -> ResponsesRequest {
    let offered = tools.filter(|tools| !tools.is_empty());
    let walked = conversation::walk(&Spell, messages, offered.is_some());
    let mut input = Vec::new();
    for turn in walked.turns {
        items_of(turn.role, turn.blocks, &mut input);
    }
    ResponsesRequest {
        model: model.to_string(),
        instructions: walked.system,
        input,
        tools: offered.map(|tools| tools.iter().map(tool_from).collect()),
        tool_choice: offered.map(|_| "auto"),
        stream: true,
        store: false,
        reasoning: effort.map(|effort| Reasoning {
            effort: effort.as_str(),
            summary: SUMMARY,
        }),
        max_output_tokens: ceiling,
    }
}

fn tool_from(tool: &Tool) -> ToolSpec {
    ToolSpec {
        kind: "function",
        name: tool.function.name.clone(),
        description: tool.function.description.clone(),
        parameters: tool.function.parameters.clone(),
        strict: false,
    }
}

/// The figures a reply reports, each as the service states it.
///
/// Every figure is optional because only the final event carries them, and a service that leaves
/// one out has not said it was zero.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub struct WireUsage {
    #[serde(default)]
    pub input_tokens: Option<u64>,
    #[serde(default)]
    pub output_tokens: Option<u64>,
    #[serde(default)]
    pub input_tokens_details: Option<InputDetails>,
    #[serde(default)]
    pub output_tokens_details: Option<OutputDetails>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub struct InputDetails {
    #[serde(default)]
    pub cached_tokens: Option<u64>,
    #[serde(default)]
    pub cache_write_tokens: Option<u64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub struct OutputDetails {
    #[serde(default)]
    pub reasoning_tokens: Option<u64>,
}

impl WireUsage {
    /// Whether the reply's own count is stated, which is what a live figure is replaced by.
    pub fn counts_the_reply(&self) -> bool {
        self.output_tokens.is_some()
    }

    /// Whether the model reasoned, which a reply cut off before it wrote anything shows only by
    /// the count.
    pub fn reasoned(&self) -> bool {
        self.output_tokens_details
            .and_then(|details| details.reasoning_tokens)
            .is_some_and(|tokens| tokens > 0)
    }
}

impl From<WireUsage> for Usage {
    fn from(usage: WireUsage) -> Self {
        let details = usage.input_tokens_details.unwrap_or_default();
        Self {
            // The prompt as sent, whoever read it: this API counts the cached part inside it.
            prompt_tokens: usage.input_tokens.unwrap_or(0),
            // The reasoning is written and billed as output, and counted inside it.
            completion_tokens: usage.output_tokens.unwrap_or(0),
            cached: Cached {
                read_tokens: details.cached_tokens.unwrap_or(0),
                written_tokens: details.cache_write_tokens.unwrap_or(0),
            },
        }
    }
}

/// What a stream's final event says, which is how the reply ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    /// The reply is whole.
    Completed,
    /// The service stopped the reply, at the ceiling or for another reason.
    Incomplete,
}

/// One event of a stream, as far as a reply is assembled from it.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A piece of the reply's words.
    Text(String),
    /// A piece of the reasoning summary.
    Thinking(String),
    /// An item that is a call has begun, with the name it was given.
    CallBegun {
        index: u64,
        id: String,
        name: String,
    },
    /// A piece of the arguments of the call at `index`, as the model wrote it.
    Arguments { index: u64, piece: String },
    /// A call is whole, with its arguments in full.
    CallDone {
        index: u64,
        id: String,
        name: String,
        arguments: String,
    },
    /// The reply is over, and the figures the service reported with it.
    Ended {
        ending: Ending,
        usage: Option<WireUsage>,
        model: Option<String>,
        /// Whether the service stopped the reply for its content, which is not a ceiling.
        withheld: bool,
    },
    /// The service says it could not write the reply.
    Failed(Failure),
    /// The service says what it says to an account that may not be used this way, in a code of
    /// [`REFUSAL_CODES`].
    Refused(Refusal),
}

/// What reading one payload of the stream came to.
#[derive(Debug, Clone, PartialEq)]
pub enum Decoded {
    Event(Event),
    /// A kind of event this does not know, or one that says nothing a reply is made of.
    Ignored,
    /// An event of a kind this does know, in a shape it cannot read.
    Unreadable,
}

/// Read one payload of the stream.
pub fn stream_event(value: &Value) -> Decoded {
    let Some(kind) = value.get("type").and_then(Value::as_str) else {
        return Decoded::Ignored;
    };
    match kind {
        "response.output_text.delta" => match value.get("delta").and_then(Value::as_str) {
            Some(delta) => Decoded::Event(Event::Text(delta.to_string())),
            None => Decoded::Unreadable,
        },
        "response.reasoning_summary_text.delta" => match value.get("delta").and_then(Value::as_str)
        {
            Some(delta) => Decoded::Event(Event::Thinking(delta.to_string())),
            None => Decoded::Unreadable,
        },
        "response.output_item.added" => {
            let Some(item) = value.get("item") else {
                return Decoded::Unreadable;
            };
            if item.get("type").and_then(Value::as_str) != Some("function_call") {
                return Decoded::Ignored;
            }
            match (
                value.get("output_index").and_then(Value::as_u64),
                item.get("call_id").and_then(Value::as_str),
                item.get("name").and_then(Value::as_str),
            ) {
                (Some(index), Some(id), Some(name)) => Decoded::Event(Event::CallBegun {
                    index,
                    id: id.to_string(),
                    name: name.to_string(),
                }),
                _ => Decoded::Unreadable,
            }
        }
        "response.function_call_arguments.delta" => match (
            value.get("output_index").and_then(Value::as_u64),
            value.get("delta").and_then(Value::as_str),
        ) {
            (Some(index), Some(piece)) => Decoded::Event(Event::Arguments {
                index,
                piece: piece.to_string(),
            }),
            _ => Decoded::Unreadable,
        },
        "response.output_item.done" => {
            let Some(item) = value.get("item") else {
                return Decoded::Unreadable;
            };
            if item.get("type").and_then(Value::as_str) != Some("function_call") {
                return Decoded::Ignored;
            }
            match (
                value.get("output_index").and_then(Value::as_u64),
                item.get("call_id").and_then(Value::as_str),
                item.get("name").and_then(Value::as_str),
                item.get("arguments").and_then(Value::as_str),
            ) {
                (Some(index), Some(id), Some(name), Some(arguments)) => {
                    Decoded::Event(Event::CallDone {
                        index,
                        id: id.to_string(),
                        name: name.to_string(),
                        arguments: arguments.to_string(),
                    })
                }
                _ => Decoded::Unreadable,
            }
        }
        "response.completed" => ended(value, Ending::Completed),
        "response.incomplete" => ended(value, Ending::Incomplete),
        "response.failed" => failed(value.pointer("/response/error/code")),
        // An error the service raised on the stream itself, outside any response.
        "error" => failed(value.get("code")),
        _ => Decoded::Ignored,
    }
}

/// A final event: the model that answered and the usage, where the response states them.
fn ended(value: &Value, ending: Ending) -> Decoded {
    let Some(response) = value.get("response") else {
        return Decoded::Unreadable;
    };
    let usage = match response.get("usage") {
        None | Some(Value::Null) => None,
        Some(usage) => match serde_json::from_value::<WireUsage>(usage.clone()) {
            Ok(usage) => Some(usage),
            Err(_) => return Decoded::Unreadable,
        },
    };
    Decoded::Event(Event::Ended {
        ending,
        usage,
        model: response
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_string),
        withheld: response
            .pointer("/incomplete_details/reason")
            .and_then(Value::as_str)
            == Some("content_filter"),
    })
}

/// What a refusal of the ChatGPT account says about it
/// ([CHATGPT-8](../../../../docs/specs/chatgpt-sign-in.md#CHATGPT-8)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The account has reached its usage limit.
    UsageLimit,
    /// The account may not be used this way: it is not eligible, what the request uses or the route
    /// it takes is not allowed, or the connection is not valid or not authorized for the request.
    NotPermitted,
}

/// The sentence for what a refusal says, the one place it is worded
/// ([CHATGPT-8](../../../../docs/specs/chatgpt-sign-in.md#CHATGPT-8)). Neither names signing in again as
/// a remedy, and neither names a cause the six codes of the second do not say.
impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UsageLimit => {
                f.write_str("OpenAI says this ChatGPT account has reached its usage limit")
            }
            Self::NotPermitted => {
                f.write_str("OpenAI does not allow this ChatGPT account to be used this way")
            }
        }
    }
}

/// The codes of a refusal that says the account may not be used this way, each with what it says.
///
/// Read from another program's source and not observed against the service. A code that is not
/// listed, whatever it says, is not one of these: the two codes that service uses for a condition
/// that passes are left out on purpose.
pub(crate) const REFUSAL_CODES: &[(&str, Refusal)] = &[
    (
        "subscription_sharing_usage_limit_exceeded",
        Refusal::UsageLimit,
    ),
    (
        "subscription_sharing_user_not_eligible",
        Refusal::NotPermitted,
    ),
    (
        "subscription_sharing_unsupported_capability",
        Refusal::NotPermitted,
    ),
    (
        "subscription_sharing_route_not_supported",
        Refusal::NotPermitted,
    ),
    ("subscription_sharing_invalid_user", Refusal::NotPermitted),
    ("chatpass_v2_scope_not_authorized", Refusal::NotPermitted),
    (
        "chatpass_v2_invalid_authorization_context",
        Refusal::NotPermitted,
    ),
];

/// What `code` says, where it is one of [`REFUSAL_CODES`], compared as a whole.
pub(crate) fn refusal_named(code: &str) -> Option<Refusal> {
    REFUSAL_CODES
        .iter()
        .find(|(listed, _)| *listed == code)
        .map(|(_, refusal)| *refusal)
}

/// What the body of a refusal says: the `error.code` of an object, where it is a string and is one of
/// [`REFUSAL_CODES`].
///
/// The one comparison made on a refusal's body. Nothing else in the body is read, and nothing of it
/// is kept.
pub(crate) fn refusal_in(body: &[u8]) -> Option<Refusal> {
    serde_json::from_slice::<Value>(body)
        .ok()
        .as_ref()
        .and_then(|body| body.pointer("/error/code"))
        .and_then(Value::as_str)
        .and_then(refusal_named)
}

/// What a failure event says, from the `code` the service wrote in it: the account's refusal where it
/// is one of [`REFUSAL_CODES`], and otherwise the failure the code stands for.
fn failed(code: Option<&Value>) -> Decoded {
    let code = code.and_then(Value::as_str);
    match code.and_then(refusal_named) {
        Some(refusal) => Decoded::Event(Event::Refused(refusal)),
        None => Decoded::Event(Event::Failed(code.map_or(Failure::Unknown, failure_named))),
    }
}

/// The failure a code the service sent stands for.
///
/// Matched against the codes this API documents, and anything else is [`Failure::Unknown`]: what
/// the service wrote is never carried, because it is text a page could have written.
pub fn failure_named(code: &str) -> Failure {
    match code {
        "invalid_request_error" | "invalid_prompt" | "invalid_image" | "unsupported_value" => {
            Failure::InvalidRequest
        }
        "context_length_exceeded" => Failure::ContextFull,
        "insufficient_quota" => Failure::Billing,
        "rate_limit_exceeded" => Failure::RateLimit,
        "server_error" | "api_error" => Failure::Api,
        "overloaded" => Failure::Overloaded,
        "timeout" => Failure::Timeout,
        "content_filter" | "refusal" => Failure::Refusal,
        _ => Failure::Unknown,
    }
}

/// A call as the agent holds it, with its arguments as the string the model wrote.
pub fn call_from(id: &str, name: &str, arguments: &str) -> ToolCall {
    ToolCall {
        id: Some(id.to_string()),
        function: ToolCallFunction {
            name: name.to_string(),
            // A call that takes nothing arrives with no arguments written, and is an empty object.
            arguments: Some(if arguments.trim().is_empty() {
                "{}".to_string()
            } else {
                arguments.to_string()
            }),
        },
        extra_content: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Content, Part as ChatPart, Role};
    use serde_json::json;

    fn say(role: Role, text: &str) -> Message {
        match role {
            Role::System => Message::system(text),
            Role::User => Message::user(text),
            Role::Assistant => Message::assistant(text),
            Role::Tool => unreachable!("a result names the call it answers"),
        }
    }

    fn event(value: Value) -> Decoded {
        stream_event(&value)
    }

    /// BACKEND-61: the body is the one this API states, with the two fields no request goes
    /// without. A body that streamed nothing, or asked the service to keep the conversation, is a
    /// request the host refuses or one that leaves the conversation where it cannot be deleted.
    #[test]
    fn a_request_is_the_body_this_api_states() {
        let messages = vec![
            say(Role::System, "Be terse."),
            say(Role::User, "hello"),
            say(Role::Assistant, "hi"),
            say(Role::User, "again"),
        ];
        let body = serde_json::to_value(request_from("gpt-example", &messages, None, None, None))
            .expect("encodes");
        assert_eq!(
            body,
            json!({
                "model": "gpt-example",
                "instructions": "Be terse.",
                "input": [
                    {"type": "message", "role": "user",
                     "content": [{"type": "input_text", "text": "hello"}]},
                    {"type": "message", "role": "assistant",
                     "content": [{"type": "output_text", "text": "hi"}]},
                    {"type": "message", "role": "user",
                     "content": [{"type": "input_text", "text": "again"}]},
                ],
                "stream": true,
                "store": false,
            })
        );
    }

    fn calling() -> Vec<Message> {
        let call = call_from("call_1", "add", r#"{"a":2,"b":3}"#)
            .as_request()
            .expect("a call with an id");
        vec![
            Message::user("add them"),
            Message::assistant_calling("one moment", vec![call]),
            Message::tool_result("call_1", "5"),
        ]
    }

    fn add_tool() -> Vec<Tool> {
        vec![Tool::function(
            "add",
            "Add two integers",
            json!({"type": "object", "properties": {"a": {"type": "integer"}}}),
        )]
    }

    /// BACKEND-61: a call and its result are items of their own, in the order they happened, and a
    /// tool is one flat object. A call nested in the assistant's message, or a tool under a
    /// `function` key, is a shape this API refuses.
    #[test]
    fn a_call_and_its_result_are_items_beside_the_messages() {
        let tools = add_tool();
        let body = serde_json::to_value(request_from("m", &calling(), Some(&tools), None, None))
            .expect("encodes");
        assert_eq!(
            body["input"],
            json!([
                {"type": "message", "role": "user",
                 "content": [{"type": "input_text", "text": "add them"}]},
                {"type": "message", "role": "assistant",
                 "content": [{"type": "output_text", "text": "one moment"}]},
                {"type": "function_call", "call_id": "call_1", "name": "add",
                 "arguments": "{\"a\":2,\"b\":3}"},
                {"type": "function_call_output", "call_id": "call_1", "output": "5"},
            ])
        );
        assert_eq!(
            body["tools"],
            json!([{"type": "function", "name": "add", "description": "Add two integers",
                    "parameters": {"type": "object", "properties": {"a": {"type": "integer"}}},
                    "strict": false}])
        );
        assert_eq!(body["tool_choice"], "auto");
    }

    /// BACKEND-61: a request offering no tool carries no tool fields and states its calls and
    /// results as text, which the shared walk does for every service that refuses a call block
    /// in a request that declares none.
    #[test]
    fn a_request_offering_no_tools_sends_no_tool_fields_and_its_calls_as_text() {
        let body =
            serde_json::to_value(request_from("m", &calling(), None, None, None)).expect("encodes");
        assert!(body.get("tools").is_none() && body.get("tool_choice").is_none());
        assert!(
            body["input"]
                .as_array()
                .expect("a list")
                .iter()
                .all(|item| item["type"] == "message"),
            "{body}"
        );
    }

    /// BACKEND-61: the level and its summary travel together, and only where a level was given.
    #[test]
    fn reasoning_is_the_level_and_a_summary_only_where_a_level_was_given() {
        let with = serde_json::to_value(request_from(
            "m",
            &[say(Role::User, "x")],
            None,
            Some(Effort::High),
            Some(4096),
        ))
        .expect("encodes");
        assert_eq!(
            with["reasoning"],
            json!({"effort": "high", "summary": "auto"})
        );
        assert_eq!(with["max_output_tokens"], 4096);

        let without =
            serde_json::to_value(request_from("m", &[say(Role::User, "x")], None, None, None))
                .expect("encodes");
        assert!(without.get("reasoning").is_none(), "{without}");
        assert!(without.get("max_output_tokens").is_none(), "{without}");
    }

    /// BACKEND-61: a picture is an `input_image` holding a data URI, and a document, which this API
    /// does not take as one, is dropped.
    #[test]
    fn a_picture_crosses_as_a_data_uri_and_a_document_is_dropped() {
        let message = Message {
            content: Content::Parts(vec![
                ChatPart::Text {
                    text: "look".to_string(),
                },
                ChatPart::ImageUrl {
                    image_url: crate::protocol::ImageUrl {
                        url: "data:image/png;base64,AAAA".to_string(),
                    },
                },
                ChatPart::ImageUrl {
                    image_url: crate::protocol::ImageUrl {
                        url: "data:application/pdf;base64,BBBB".to_string(),
                    },
                },
            ]),
            ..Message::user("")
        };
        let body =
            serde_json::to_value(request_from("m", &[message], None, None, None)).expect("encodes");
        assert_eq!(
            body["input"][0]["content"],
            json!([
                {"type": "input_text", "text": "look"},
                {"type": "input_image", "image_url": "data:image/png;base64,AAAA"},
            ])
        );
    }

    /// BACKEND-61: the events that carry a reply decode to what they carry, including the one that
    /// is whole only in its `done` form.
    #[test]
    fn the_events_that_carry_a_reply_decode() {
        assert_eq!(
            event(json!({"type": "response.output_text.delta", "delta": "ok"})),
            Decoded::Event(Event::Text("ok".to_string()))
        );
        assert_eq!(
            event(json!({"type": "response.reasoning_summary_text.delta", "delta": "hm"})),
            Decoded::Event(Event::Thinking("hm".to_string()))
        );
        assert_eq!(
            event(
                json!({"type": "response.output_item.added", "output_index": 1,
                         "item": {"type": "function_call", "call_id": "c", "name": "add",
                                  "arguments": ""}})
            ),
            Decoded::Event(Event::CallBegun {
                index: 1,
                id: "c".to_string(),
                name: "add".to_string()
            })
        );
        assert_eq!(
            event(
                json!({"type": "response.function_call_arguments.delta", "output_index": 1,
                         "delta": "{\"a\""})
            ),
            Decoded::Event(Event::Arguments {
                index: 1,
                piece: "{\"a\"".to_string()
            })
        );
        assert_eq!(
            event(
                json!({"type": "response.output_item.done", "output_index": 1,
                         "item": {"type": "function_call", "call_id": "c", "name": "add",
                                  "arguments": "{\"a\":2}"}})
            ),
            Decoded::Event(Event::CallDone {
                index: 1,
                id: "c".to_string(),
                name: "add".to_string(),
                arguments: "{\"a\":2}".to_string()
            })
        );
    }

    /// BACKEND-61: the three ways a stream ends, each with the figures it reported. The final
    /// event's own `output` list is not read, since it is empty on the host that serves a ChatGPT
    /// account and a reply built from it would have nothing in it.
    #[test]
    fn a_stream_ends_in_one_of_three_events_and_reports_usage() {
        let usage = json!({"input_tokens": 23, "output_tokens": 5,
                           "input_tokens_details": {"cached_tokens": 4, "cache_write_tokens": 2},
                           "output_tokens_details": {"reasoning_tokens": 3}, "total_tokens": 28});
        let wire = WireUsage {
            input_tokens: Some(23),
            output_tokens: Some(5),
            input_tokens_details: Some(InputDetails {
                cached_tokens: Some(4),
                cache_write_tokens: Some(2),
            }),
            output_tokens_details: Some(OutputDetails {
                reasoning_tokens: Some(3),
            }),
        };
        for (kind, ending) in [
            ("response.completed", Ending::Completed),
            ("response.incomplete", Ending::Incomplete),
        ] {
            assert_eq!(
                event(
                    json!({"type": kind, "response": {"model": "m", "usage": usage,
                                                       "output": []}})
                ),
                Decoded::Event(Event::Ended {
                    ending,
                    usage: Some(wire),
                    model: Some("m".to_string()),
                    withheld: false,
                }),
                "{kind}"
            );
        }
        let usage = Usage::from(wire);
        assert_eq!(usage.prompt_tokens, 23);
        assert_eq!(usage.completion_tokens, 5);
        assert_eq!(usage.cached.read_tokens, 4);
        assert_eq!(usage.cached.written_tokens, 2);
        assert!(wire.reasoned());

        assert_eq!(
            event(json!({"type": "response.completed", "response": {"output": []}})),
            Decoded::Event(Event::Ended {
                ending: Ending::Completed,
                usage: None,
                model: None,
                withheld: false,
            }),
            "a final event with no usage is a reply whose cost was not stated"
        );
    }

    /// BACKEND-61: a failure is one of a closed set and never named by what the service wrote, so a
    /// code this does not know is `Unknown` and its text reaches nobody.
    #[test]
    fn a_reported_failure_is_one_of_a_closed_set_and_never_named_by_the_service() {
        let failed = |code: &str| {
            event(json!({"type": "response.failed",
                         "response": {"error": {"code": code, "message": "words a page wrote"}}}))
        };
        assert_eq!(
            failed("rate_limit_exceeded"),
            Decoded::Event(Event::Failed(Failure::RateLimit))
        );
        assert_eq!(
            failed("server_error"),
            Decoded::Event(Event::Failed(Failure::Api))
        );
        for (code, failure) in [
            ("invalid_request_error", Failure::InvalidRequest),
            ("context_length_exceeded", Failure::ContextFull),
            ("insufficient_quota", Failure::Billing),
            ("api_error", Failure::Api),
            ("overloaded", Failure::Overloaded),
            ("timeout", Failure::Timeout),
            ("content_filter", Failure::Refusal),
        ] {
            assert_eq!(
                failed(code),
                Decoded::Event(Event::Failed(failure)),
                "{code}"
            );
        }
        assert_eq!(
            failed("a code nobody documented"),
            Decoded::Event(Event::Failed(Failure::Unknown))
        );
        assert_eq!(
            event(json!({"type": "error", "code": "insufficient_quota", "message": "x"})),
            Decoded::Event(Event::Failed(Failure::Billing))
        );
        assert_eq!(
            event(json!({"type": "response.failed", "response": {}})),
            Decoded::Event(Event::Failed(Failure::Unknown))
        );
    }

    const USAGE_LIMIT: &str = "subscription_sharing_usage_limit_exceeded";
    const NOT_ELIGIBLE: &str = "subscription_sharing_user_not_eligible";
    const INVALID_USER: &str = "subscription_sharing_invalid_user";

    /// CHATGPT-8: the codes that say something about the account are these seven, and the usage limit
    /// is told apart from the six that say it may not be used this way. The two codes the same
    /// service uses for a condition that passes are not among them.
    #[test]
    fn the_listed_codes_are_the_seven_the_clause_names_and_the_usage_limit_is_told_apart() {
        assert_eq!(
            REFUSAL_CODES,
            [
                (USAGE_LIMIT, Refusal::UsageLimit),
                (NOT_ELIGIBLE, Refusal::NotPermitted),
                (
                    "subscription_sharing_unsupported_capability",
                    Refusal::NotPermitted
                ),
                (
                    "subscription_sharing_route_not_supported",
                    Refusal::NotPermitted
                ),
                (INVALID_USER, Refusal::NotPermitted),
                ("chatpass_v2_scope_not_authorized", Refusal::NotPermitted),
                (
                    "chatpass_v2_invalid_authorization_context",
                    Refusal::NotPermitted
                ),
            ]
        );
    }

    /// CHATGPT-8: a refusal says something about the account only by its `error.code` being, as a
    /// whole, one of the listed codes, and what it says is what the code says. A code that merely
    /// resembles a listed one, a listed code written anywhere else in the body and a body that is not
    /// an object are not that.
    #[test]
    fn only_a_refusals_error_code_that_is_a_listed_code_says_something_of_the_account() {
        let coded = |code: &str| json!({"error": {"code": code}}).to_string();
        for (code, refusal) in REFUSAL_CODES {
            assert_eq!(refusal_in(coded(code).as_bytes()), Some(*refusal), "{code}");
        }
        let among_others = json!({"error": {"code": INVALID_USER, "message": "words", "type": "t"},
                                  "other": 1});
        assert_eq!(
            refusal_in(among_others.to_string().as_bytes()),
            Some(Refusal::NotPermitted)
        );

        for code in [
            "subscription_sharing_usage_unavailable",
            "subscription_sharing_user_unavailable",
            "subscription_sharing",
            "subscription_sharing_invalid_user_",
            " subscription_sharing_invalid_user",
            "Subscription_Sharing_Invalid_User",
            "rate_limit_exceeded",
            "",
        ] {
            assert_eq!(
                refusal_in(coded(code).as_bytes()),
                None,
                "{code:?} is not a listed code"
            );
        }

        for elsewhere in [
            json!({"code": INVALID_USER}),
            json!({"error": {"message": INVALID_USER}}),
            json!({"error": {"type": INVALID_USER}}),
            json!({"error": INVALID_USER}),
            json!({"detail": INVALID_USER}),
            json!({"error": {"code": [INVALID_USER]}}),
            json!({"error": {"code": 7}}),
            json!({"errors": [{"code": INVALID_USER}]}),
            json!({"response": {"error": {"code": INVALID_USER}}}),
            json!([{"error": {"code": INVALID_USER}}]),
            json!(INVALID_USER),
            json!(null),
        ] {
            assert_eq!(
                refusal_in(elsewhere.to_string().as_bytes()),
                None,
                "{elsewhere}"
            );
        }
        for body in [
            "",
            "not json",
            "{",
            "{\"error\":{\"code\":\"subscription_sharing_invalid_user",
        ] {
            assert_eq!(refusal_in(body.as_bytes()), None, "{body:?}");
        }
    }

    /// CHATGPT-8: a listed code reported on the stream is the account's refusal and no other failure,
    /// whichever event carries it, and the codes beside it that the same service uses for a condition
    /// that passes are failures it did not name.
    #[test]
    fn a_listed_code_reported_on_the_stream_is_a_refusal_and_no_other_failure() {
        for (code, refusal) in REFUSAL_CODES {
            assert_eq!(
                event(json!({"type": "response.failed",
                             "response": {"error": {"code": code, "message": "x"}}})),
                Decoded::Event(Event::Refused(*refusal)),
                "{code}"
            );
            assert_eq!(
                event(json!({"type": "error", "code": code, "message": "x"})),
                Decoded::Event(Event::Refused(*refusal)),
                "{code}"
            );
        }
        for code in [
            "subscription_sharing_usage_unavailable",
            "subscription_sharing_user_unavailable",
        ] {
            assert_eq!(
                event(json!({"type": "response.failed", "response": {"error": {"code": code}}})),
                Decoded::Event(Event::Failed(Failure::Unknown)),
                "{code}"
            );
        }
    }

    /// BACKEND-61: an event this does not know is ignored, and one it knows in a shape it cannot
    /// read is unreadable and not ignored, so a reply built from a malformed piece is refused
    /// rather than returned short.
    #[test]
    fn an_unknown_event_is_ignored_and_a_known_one_in_a_wrong_shape_is_not() {
        for ignored in [
            json!({"type": "response.created", "response": {}}),
            json!({"type": "response.metadata", "metadata": {}}),
            json!({"type": "response.output_item.added", "output_index": 0,
                   "item": {"type": "message", "role": "assistant", "content": []}}),
            json!({"type": "response.output_item.done", "output_index": 0,
                   "item": {"type": "reasoning", "summary": []}}),
            json!({"no": "type"}),
        ] {
            assert_eq!(event(ignored.clone()), Decoded::Ignored, "{ignored}");
        }
        for unreadable in [
            json!({"type": "response.output_text.delta", "delta": 7}),
            json!({"type": "response.output_item.done", "output_index": 0,
                   "item": {"type": "function_call", "name": "add"}}),
            json!({"type": "response.completed"}),
            json!({"type": "response.completed", "response": {"usage": {"input_tokens": "x"}}}),
        ] {
            assert_eq!(
                event(unreadable.clone()),
                Decoded::Unreadable,
                "{unreadable}"
            );
        }
    }

    /// BACKEND-61: a call that wrote no arguments is an empty object, which is what a tool taking
    /// none is called with.
    #[test]
    fn a_call_with_no_arguments_written_is_an_empty_object() {
        assert_eq!(
            call_from("c", "list", "").function.arguments.as_deref(),
            Some("{}")
        );
        assert_eq!(
            call_from("c", "list", r#"{"a":1}"#)
                .function
                .arguments
                .as_deref(),
            Some(r#"{"a":1}"#)
        );
    }
}
