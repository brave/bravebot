//! Wire types for Google's native `generateContent` API, and the translation to and from the
//! shapes the rest of this agent speaks.
//!
//! The agent's conversation is held in OpenAI-compatible types, because that is what the aichat
//! endpoint speaks. [`crate::conversation`] restates it as turns of blocks, and this module spells
//! those blocks the way this API does:
//!
//! - A turn is `contents` of `parts`, and the two roles are `user` and `model`. A part holds one of
//!   `text`, `inlineData`, `functionCall` or `functionResponse`, and a `thoughtSignature` rides
//!   beside the part it belongs to.
//! - The system prompt is `systemInstruction`, and the tools are one `functionDeclarations` list
//!   whose schemas go in `parametersJsonSchema`.
//! - The reply ceiling, the thinking budget and the schema a reply must match are fields of
//!   `generationConfig`, and none is required.
//! - A call carries no id the agent can use. One is made up when a reply arrives, and a result is
//!   matched back to its call by the call's name and its place in the conversation.
//! - Usage counts the prompt with the cached part inside it, and the reasoning apart from the reply.
//! - A streamed event is a whole response holding the next pieces, and the last holds a finish
//!   reason. A function call arrives whole, never in fragments.
//!
//! Nothing here inspects content to make a decision. Text is moved between shapes and handed on with
//! whatever label it arrived under.

use crate::anthropic::protocol::Failure;
use crate::conversation::{self, Spelling};
use crate::protocol::{Cached, Effort, Message, Role, Tool, ToolCall, ToolCallFunction, Usage};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// The finish reason of a reply that hit the ceiling rather than finishing.
pub const FINISH_MAX_TOKENS: &str = "MAX_TOKENS";

/// The media types this API takes inline.
const INLINE_TYPES: [&str; 6] = [
    "image/png",
    "image/jpeg",
    "image/webp",
    "image/heic",
    "image/heif",
    "application/pdf",
];

/// A request to `generateContent` or `streamGenerateContent`.
///
/// The model and whether to stream are the route's, so neither is a field here.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateContentRequest {
    pub contents: Vec<Content>,
    /// The system prompt, hoisted out of the conversation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_instruction: Option<SystemInstruction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolSet>>,
    /// Absent unless something in it was asked for, so a request nobody asked anything of sends
    /// the body it always sent and the model keeps its own defaults.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_config: Option<GenerationConfig>,
}

/// Serialised only: `role` is a fixed string this crate chooses, never one it reads back.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Content {
    pub role: &'static str,
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SystemInstruction {
    pub parts: Vec<Part>,
}

/// One part of a turn on the way out. Exactly one of the first four fields is set.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_data: Option<InlineData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_call: Option<FunctionCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_response: Option<FunctionResponse>,
    /// What the service attached to a call it made, sent back with the call unread. Gemini 3 models
    /// refuse the next request without it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
}

impl Part {
    fn text(text: &str) -> Self {
        Self {
            text: Some(text.to_string()),
            ..Self::default()
        }
    }
}

/// An attachment's bytes, which this API takes inline as base64.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineData {
    pub mime_type: String,
    pub data: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FunctionCall {
    pub name: String,
    pub args: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FunctionResponse {
    pub name: String,
    /// An object, which is the only form this API takes here: the tool's words are the `output`.
    pub response: Value,
}

/// The tools a request offers, which this API takes as one list inside one entry.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSet {
    pub function_declarations: Vec<Declaration>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Declaration {
    pub name: String,
    pub description: String,
    pub parameters_json_schema: Value,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationConfig {
    /// The most the reply may write. Only where somebody stated one: the models differ in what they
    /// allow, and a figure above a model's own is refused.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_config: Option<ThinkingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_mime_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_json_schema: Option<Value>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingConfig {
    pub thinking_budget: u32,
}

/// How many tokens of thinking a level asks for.
///
/// Every figure is inside what each Gemini 2.5 model allows, 128 to 32,768 on Pro and 0 to 24,576 on
/// Flash, so a level is one the model takes rather than one it refuses. Gemini 3 models read the
/// same field as the older form of their own thinking level.
fn thinking_budget(effort: Effort) -> u32 {
    match effort {
        Effort::Low => 1_024,
        Effort::Medium => 4_096,
        Effort::High => 12_288,
        Effort::Xhigh => 20_480,
        Effort::Max => 24_576,
    }
}

impl GenerateContentRequest {
    /// Cap the reply at `ceiling` tokens, or leave the model to its own.
    pub fn with_ceiling(mut self, ceiling: Option<u64>) -> Self {
        if let Some(ceiling) = ceiling {
            self.config().max_output_tokens = Some(ceiling);
        }
        self
    }

    /// Ask for a particular amount of thinking, or leave the model to its own default.
    pub fn with_effort(mut self, effort: Option<Effort>) -> Self {
        if let Some(effort) = effort {
            self.config().thinking_config = Some(ThinkingConfig {
                thinking_budget: thinking_budget(effort),
            });
        }
        self
    }

    /// Ask for a reply that is one JSON value matching `schema`, or leave the reply free.
    pub fn with_schema(mut self, schema: Option<&Value>) -> Self {
        if let Some(schema) = schema {
            let config = self.config();
            config.response_mime_type = Some("application/json");
            config.response_json_schema = Some(schema.clone());
        }
        self
    }

    fn config(&mut self) -> &mut GenerationConfig {
        self.generation_config.get_or_insert_with(Default::default)
    }
}

/// A call the assistant made earlier, as the conversation recorded it.
struct Recorded {
    id: String,
    name: String,
    signature: Option<String>,
}

/// How this API names the parts of a turn, for [`conversation::walk`].
///
/// A walk gives a call and a result by id alone, and this API wants the function's name on a
/// result and a signature beside a call. Both are read off the conversation first, in order, and
/// the walk, which visits the calls in that same order, takes them back off as it reaches each.
/// A result is matched by the call it follows rather than by id alone, because the ids made up for
/// calls are not unique across a conversation that was saved and resumed.
struct Parts {
    to_visit: RefCell<std::collections::VecDeque<Recorded>>,
    answering: RefCell<HashMap<String, String>>,
}

impl Parts {
    fn new(messages: &[Message]) -> Self {
        let recorded = messages
            .iter()
            .filter(|message| message.role == Role::Assistant)
            .flat_map(|message| message.tool_calls.iter().flatten())
            .map(|call| Recorded {
                id: call.id.clone(),
                name: call.function.name.clone(),
                signature: call
                    .extra_content
                    .as_ref()
                    .and_then(signature_in)
                    .map(str::to_string),
            })
            .collect();
        Self {
            to_visit: RefCell::new(recorded),
            answering: RefCell::new(HashMap::new()),
        }
    }
}

/// The thought signature in a call's `extra_content`, where the call has one.
fn signature_in(extra_content: &Value) -> Option<&str> {
    extra_content
        .pointer("/google/thought_signature")
        .and_then(Value::as_str)
}

impl Spelling for Parts {
    type Block = Part;

    fn text(&self, text: &str) -> Part {
        Part::text(text)
    }

    fn attachment(&self, media_type: &str, data: &str, _position: usize) -> Option<Part> {
        INLINE_TYPES.contains(&media_type).then(|| Part {
            inline_data: Some(InlineData {
                mime_type: media_type.to_string(),
                data: data.to_string(),
            }),
            ..Part::default()
        })
    }

    fn call(&self, id: &str, name: &str, input: Value) -> Part {
        let signature = {
            let mut waiting = self.to_visit.borrow_mut();
            waiting
                .iter()
                .position(|call| call.id == id && call.name == name)
                .and_then(|at| waiting.remove(at))
                .and_then(|call| call.signature)
        };
        self.answering
            .borrow_mut()
            .insert(id.to_string(), name.to_string());
        Part {
            function_call: Some(FunctionCall {
                name: name.to_string(),
                args: input,
            }),
            thought_signature: signature,
            ..Part::default()
        }
    }

    fn result(&self, id: &str, text: String) -> Part {
        // A result whose call was not sent has no name to answer, which this API refuses. It is
        // said as words instead, as a request offering no tools says every result.
        let Some(name) = self.answering.borrow().get(id).cloned() else {
            return Part::text(&format!("[result of call {id}]\n{text}"));
        };
        Part {
            function_response: Some(FunctionResponse {
                name,
                response: json!({"output": text}),
            }),
            ..Part::default()
        }
    }
}

/// Build a request from the conversation the agent holds.
///
/// The turns are the same conversation, restated by [`conversation::walk`].
pub fn request_from(messages: &[Message], tools: Option<&[Tool]>) -> GenerateContentRequest {
    // An empty list is no list: a turn offering nothing sends no `tools` field at all.
    let tools = tools.filter(|tools| !tools.is_empty());
    let parts = Parts::new(messages);
    let walked = conversation::walk(&parts, messages, tools.is_some());

    let contents = walked
        .turns
        .into_iter()
        .map(|turn| Content {
            role: if turn.role == "assistant" {
                "model"
            } else {
                "user"
            },
            parts: turn.blocks,
        })
        .collect();

    GenerateContentRequest {
        contents,
        system_instruction: walked
            .system
            .filter(|system| !system.trim().is_empty())
            .map(|system| SystemInstruction {
                parts: vec![Part::text(&system)],
            }),
        tools: tools.map(|tools| {
            vec![ToolSet {
                function_declarations: tools
                    .iter()
                    .map(|tool| Declaration {
                        name: tool.function.name.clone(),
                        description: tool.function.description.clone(),
                        parameters_json_schema: tool.function.parameters.clone(),
                    })
                    .collect(),
            }]
        }),
        generation_config: None,
    }
}

/// One response, whole or as a piece of a stream.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    #[serde(default)]
    pub candidates: Vec<Candidate>,
    #[serde(default)]
    pub usage_metadata: Option<WireUsage>,
    /// Present, with a reason, where the prompt itself was refused and no candidate was written.
    #[serde(default)]
    pub prompt_feedback: Option<PromptFeedback>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    #[serde(default)]
    pub content: Option<ReplyContent>,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ReplyContent {
    #[serde(default)]
    pub parts: Vec<ReplyPart>,
}

/// One part of a reply. A part of a kind this does not model is read past, since a service adds
/// kinds, and a `functionCall` that cannot be read fails the whole response: it is a call the model
/// made, and passing over it loses the call.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplyPart {
    #[serde(default)]
    pub text: Option<String>,
    /// Whether `text` is a summary of the model's reasoning, which is no part of the reply.
    #[serde(default)]
    pub thought: bool,
    #[serde(default)]
    pub function_call: Option<ReplyCall>,
    #[serde(default)]
    pub thought_signature: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReplyCall {
    pub name: String,
    /// Absent where a call takes no arguments, and read as an empty object.
    #[serde(default)]
    pub args: Option<Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptFeedback {
    #[serde(default)]
    pub block_reason: Option<String>,
}

/// What a request cost, as this API states it.
///
/// Every figure is optional because a stream restates them: each piece may carry the running
/// count, and a figure a piece leaves out stands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireUsage {
    #[serde(default)]
    pub prompt_token_count: Option<u64>,
    #[serde(default)]
    pub candidates_token_count: Option<u64>,
    #[serde(default)]
    pub thoughts_token_count: Option<u64>,
    #[serde(default)]
    pub cached_content_token_count: Option<u64>,
}

impl WireUsage {
    /// This report with whatever `later` restates put over it.
    pub fn then(self, later: WireUsage) -> WireUsage {
        WireUsage {
            prompt_token_count: later.prompt_token_count.or(self.prompt_token_count),
            candidates_token_count: later.candidates_token_count.or(self.candidates_token_count),
            thoughts_token_count: later.thoughts_token_count.or(self.thoughts_token_count),
            cached_content_token_count: later
                .cached_content_token_count
                .or(self.cached_content_token_count),
        }
    }

    /// Whether the reply's own count is stated, which is what a live figure is replaced by.
    pub fn counts_the_reply(&self) -> bool {
        self.candidates_token_count.is_some()
    }

    /// Whether the model reasoned, which a reply that was cut off before it wrote anything shows
    /// only by the count: reasoning is not returned unless it is asked for.
    pub fn reasoned(&self) -> bool {
        self.thoughts_token_count.is_some_and(|tokens| tokens > 0)
    }
}

impl From<WireUsage> for Usage {
    fn from(usage: WireUsage) -> Self {
        Self {
            // The prompt as sent, whoever read it: this API counts the cached part inside it.
            prompt_tokens: usage.prompt_token_count.unwrap_or(0),
            // The reasoning is written and billed as output, and counted apart from the reply.
            completion_tokens: usage
                .candidates_token_count
                .unwrap_or(0)
                .saturating_add(usage.thoughts_token_count.unwrap_or(0)),
            cached: Cached {
                read_tokens: usage.cached_content_token_count.unwrap_or(0),
                written_tokens: 0,
            },
        }
    }
}

/// A call's id as the agent holds it.
///
/// This API gives a call none the agent can match a result by, so one is made up. Counted from the
/// start of the process, which makes it unique among the calls this process has seen.
pub fn made_up_call_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!("call_{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

/// A call as the agent holds it, with the signature the service attached kept in the member a
/// call carries for it ([`ToolCall::extra_content`]).
pub fn call_from(name: &str, arguments: &Value, signature: Option<&str>) -> ToolCall {
    ToolCall {
        id: Some(made_up_call_id()),
        function: ToolCallFunction {
            name: name.to_string(),
            arguments: Some(arguments.to_string()),
        },
        extra_content: signature
            .map(|signature| json!({"google": {"thought_signature": signature}})),
    }
}

/// The text and the calls in a whole reply, in the shapes the agent expects back, and whether any
/// reasoning arrived.
pub fn parts_of(response: &Response) -> (String, Vec<ToolCall>, bool) {
    let mut text = String::new();
    let mut calls = Vec::new();
    let mut thought = false;

    let parts = response
        .candidates
        .first()
        .and_then(|candidate| candidate.content.as_ref())
        .map(|content| content.parts.as_slice())
        .unwrap_or_default();
    for part in parts {
        if part.thought {
            thought = true;
        } else if let Some(piece) = &part.text {
            text.push_str(piece);
        }
        if let Some(call) = &part.function_call {
            let empty = json!({});
            calls.push(call_from(
                &call.name,
                call.args.as_ref().unwrap_or(&empty),
                part.thought_signature.as_deref(),
            ));
        }
    }
    (text, calls, thought)
}

/// The finish reason of a response's first candidate.
pub fn finish_of(response: &Response) -> Option<&str> {
    response
        .candidates
        .first()
        .and_then(|candidate| candidate.finish_reason.as_deref())
}

/// The failure a response that stopped without finishing is, where it says it did.
///
/// A prompt the service refused, or a reply it stopped for what it would have said, is one it
/// ended on purpose, and sending either again unchanged says the same. A call the model wrote
/// badly is another the model may write well the next time, so it is a fault worth asking again.
pub fn unfinished(response: &Response) -> Option<Failure> {
    if response
        .prompt_feedback
        .as_ref()
        .is_some_and(|feedback| feedback.block_reason.is_some())
    {
        return Some(Failure::Refusal);
    }
    match finish_of(response)? {
        "SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII" | "IMAGE_SAFETY"
        | "LANGUAGE" => Some(Failure::Refusal),
        "MALFORMED_FUNCTION_CALL" | "UNEXPECTED_TOOL_CALL" => Some(Failure::Api),
        _ => None,
    }
}

/// The failure a status word in a Google error stands for.
///
/// A closed set, as [`Failure`] is: the name that reaches a person is this crate's own and never
/// text the service wrote.
pub fn failure_named(status: &str) -> Failure {
    match status {
        "INVALID_ARGUMENT" | "FAILED_PRECONDITION" | "OUT_OF_RANGE" => Failure::InvalidRequest,
        "NOT_FOUND" => Failure::NotFound,
        "UNAUTHENTICATED" => Failure::Authentication,
        "PERMISSION_DENIED" => Failure::Permission,
        "RESOURCE_EXHAUSTED" => Failure::RateLimit,
        "UNAVAILABLE" => Failure::Overloaded,
        "DEADLINE_EXCEEDED" => Failure::Timeout,
        "INTERNAL" | "UNKNOWN" | "ABORTED" | "DATA_LOSS" => Failure::Api,
        _ => Failure::Unknown,
    }
}

/// What reading one streamed piece's JSON came to.
#[derive(Debug)]
pub enum Decoded {
    Chunk(Box<Response>),
    /// The service said, part way through the reply, that it was not going to finish it.
    Failed(Failure),
    /// A piece holding nothing this reads.
    Ignored,
    /// A piece in a shape this reads and whose body is not as it states. Kept apart from
    /// [`Decoded::Ignored`] so the stream is still drained and the reply is refused once it has
    /// ended, in the same place a reply that arrived whole is.
    Unreadable,
}

/// Read one streamed piece, given as the JSON its `data:` line carried.
pub fn chunk(value: &Value) -> Decoded {
    if let Some(error) = value.get("error") {
        return Decoded::Failed(
            error
                .get("status")
                .and_then(Value::as_str)
                .map_or(Failure::Unknown, failure_named),
        );
    }
    if value.get("candidates").is_none()
        && value.get("usageMetadata").is_none()
        && value.get("promptFeedback").is_none()
    {
        return Decoded::Ignored;
    }
    match serde_json::from_value(value.clone()) {
        Ok(response) => Decoded::Chunk(Box::new(response)),
        Err(_) => Decoded::Unreadable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ToolCallRequest, ToolCallRequestFunction};

    fn body_of(request: &GenerateContentRequest) -> Value {
        serde_json::to_value(request).expect("a request serialises")
    }

    fn offered() -> Vec<Tool> {
        vec![Tool::function(
            "read_file",
            "Read a file",
            json!({"type": "object", "properties": {"path": {"type": "string"}},
                   "additionalProperties": false}),
        )]
    }

    fn calling(id: &str, name: &str, arguments: &str, signature: Option<&str>) -> ToolCallRequest {
        ToolCallRequest {
            id: id.to_string(),
            kind: "function".to_string(),
            function: ToolCallRequestFunction {
                name: name.to_string(),
                arguments: arguments.to_string(),
            },
            extra_content: signature
                .map(|signature| json!({"google": {"thought_signature": signature}})),
        }
    }

    fn request(messages: &[Message]) -> GenerateContentRequest {
        let tools = offered();
        request_from(messages, Some(&tools))
    }

    /// The body this API states: contents of parts under the roles `user` and `model`, the system
    /// prompt beside them and not among them, and each tool schema in `parametersJsonSchema`.
    #[test]
    fn a_request_is_contents_of_parts_with_the_system_prompt_and_tools_beside_them() {
        let body = body_of(&request(&[
            Message::system("be brief"),
            Message::user("hello"),
            Message::assistant("hi"),
            Message::user("again"),
        ]));
        assert_eq!(
            body,
            json!({
                "contents": [
                    {"role": "user", "parts": [{"text": "hello"}]},
                    {"role": "model", "parts": [{"text": "hi"}]},
                    {"role": "user", "parts": [{"text": "again"}]},
                ],
                "systemInstruction": {"parts": [{"text": "be brief"}]},
                "tools": [{"functionDeclarations": [{
                    "name": "read_file",
                    "description": "Read a file",
                    "parametersJsonSchema": {
                        "type": "object",
                        "properties": {"path": {"type": "string"}},
                        "additionalProperties": false
                    }
                }]}],
            })
        );
    }

    /// A request nobody asked anything of carries no `generationConfig`, no `tools` where none are
    /// offered and no `systemInstruction` where there is no prompt.
    #[test]
    fn a_request_asking_nothing_extra_carries_no_generation_config() {
        let body = body_of(&request_from(&[Message::user("hello")], None));
        assert_eq!(
            body,
            json!({"contents": [{"role": "user", "parts": [{"text": "hello"}]}]})
        );
        let empty = body_of(&request_from(&[Message::user("hello")], Some(&[])));
        assert!(empty.get("tools").is_none(), "{empty}");
    }

    /// The ceiling, the thinking budget and the schema each go in `generationConfig` where asked
    /// for, and a level maps to a budget that grows with it.
    #[test]
    fn a_ceiling_a_level_and_a_schema_go_in_the_generation_config() {
        let schema = json!({"type": "object"});
        let body = body_of(
            &request_from(&[Message::user("go")], None)
                .with_ceiling(Some(500))
                .with_effort(Some(Effort::High))
                .with_schema(Some(&schema)),
        );
        assert_eq!(
            body["generationConfig"],
            json!({
                "maxOutputTokens": 500,
                "thinkingConfig": {"thinkingBudget": 12288},
                "responseMimeType": "application/json",
                "responseJsonSchema": {"type": "object"},
            })
        );
        let budgets: Vec<u32> = Effort::ALL.into_iter().map(thinking_budget).collect();
        assert!(
            budgets.windows(2).all(|pair| pair[0] < pair[1]),
            "{budgets:?}"
        );
    }

    /// A call is a `functionCall` part on a model turn and its result a `functionResponse` part
    /// on the user turn after it, named for the function and not for the made-up id. Two results
    /// in a row are one turn.
    #[test]
    fn a_call_and_its_result_are_matched_by_the_functions_name() {
        let body = body_of(&request(&[
            Message::user("go"),
            Message::assistant_calling(
                "",
                vec![
                    calling("call_7", "read_file", r#"{"path":"a"}"#, None),
                    calling("call_8", "read_file", r#"{"path":"b"}"#, None),
                ],
            ),
            Message::tool_result("call_7", "first"),
            Message::tool_result("call_8", "second"),
        ]));
        assert_eq!(
            body["contents"][1],
            json!({"role": "model", "parts": [
                {"functionCall": {"name": "read_file", "args": {"path": "a"}}},
                {"functionCall": {"name": "read_file", "args": {"path": "b"}}},
            ]})
        );
        assert_eq!(
            body["contents"][2],
            json!({"role": "user", "parts": [
                {"functionResponse": {"name": "read_file", "response": {"output": "first"}}},
                {"functionResponse": {"name": "read_file", "response": {"output": "second"}}},
            ]})
        );
    }

    /// The made-up ids restart with the process, so a resumed conversation can hold two calls
    /// under one id. A result answers the call it follows.
    #[test]
    fn a_result_answers_the_call_it_follows_when_an_id_was_used_twice() {
        let body = body_of(&request(&[
            Message::user("go"),
            Message::assistant_calling("", vec![calling("call_0", "read_file", "{}", None)]),
            Message::tool_result("call_0", "one"),
            Message::assistant_calling("", vec![calling("call_0", "list_dir", "{}", None)]),
            Message::tool_result("call_0", "two"),
        ]));
        assert_eq!(
            body["contents"][2]["parts"][0]["functionResponse"]["name"],
            "read_file"
        );
        assert_eq!(
            body["contents"][4]["parts"][0]["functionResponse"]["name"],
            "list_dir"
        );
    }

    /// A result whose call is not in the conversation names no function, which this API refuses,
    /// so it is said as words.
    #[test]
    fn a_result_with_no_call_in_the_conversation_is_sent_as_text() {
        let body = body_of(&request(&[
            Message::user("go"),
            Message::tool_result("call_9", "stray"),
        ]));
        assert_eq!(
            body["contents"][0]["parts"],
            json!([{"text": "go"}, {"text": "[result of call call_9]\nstray"}])
        );
    }

    /// A signature kept on a call goes back beside that call's part and no other part, and a call
    /// with none sends none.
    #[test]
    fn a_calls_thought_signature_is_sent_back_beside_its_part() {
        let body = body_of(&request(&[
            Message::user("go"),
            Message::assistant_calling(
                "",
                vec![
                    calling("call_1", "read_file", "{}", Some("opaque-signature")),
                    calling("call_2", "read_file", "{}", None),
                ],
            ),
            Message::tool_result("call_1", "x"),
            Message::tool_result("call_2", "y"),
        ]));
        let parts = &body["contents"][1]["parts"];
        assert_eq!(parts[0]["thoughtSignature"], "opaque-signature");
        assert!(parts[1].get("thoughtSignature").is_none(), "{parts}");
        assert!(
            body["contents"][2]["parts"][0]
                .get("thoughtSignature")
                .is_none()
        );
    }

    /// A request offering no tools says calls and results as words, because this API refuses a
    /// function part in a request that declares none.
    #[test]
    fn a_request_offering_no_tools_sends_calls_and_results_as_text() {
        let body = body_of(&request_from(
            &[
                Message::user("go"),
                Message::assistant_calling("", vec![calling("call_1", "read_file", "{}", None)]),
                Message::tool_result("call_1", "done"),
            ],
            None,
        ));
        let text = body.to_string();
        assert!(!text.contains("functionCall"), "{text}");
        assert!(!text.contains("functionResponse"), "{text}");
    }

    /// An attachment is inline data where the media type is one this API takes, and dropped where
    /// it is not.
    #[test]
    fn an_attachment_is_inline_data_for_a_media_type_this_api_takes() {
        use crate::protocol::{ImageUrl, Part as Attached};
        let picture = |url: &str| Attached::ImageUrl {
            image_url: ImageUrl {
                url: url.to_string(),
            },
        };
        let body = body_of(&request(&[Message::user_parts(vec![
            Attached::Text {
                text: "look".to_string(),
            },
            picture("data:image/png;base64,AAAA"),
            picture("data:video/mp4;base64,BBBB"),
        ])]));
        assert_eq!(
            body["contents"][0]["parts"],
            json!([
                {"text": "look"},
                {"inlineData": {"mimeType": "image/png", "data": "AAAA"}},
            ])
        );
    }

    fn response(json: &str) -> Response {
        serde_json::from_str(json).expect("a response")
    }

    /// A whole reply gives its text, its calls with their signatures kept where the agent keeps
    /// them, and the figures it states, with the reasoning counted as output and the cached part
    /// inside the prompt.
    #[test]
    fn a_whole_reply_is_read_into_text_calls_and_cost() {
        let reply = response(
            r#"{"candidates": [{"content": {"role": "model", "parts": [
                    {"text": "thinking about it", "thought": true},
                    {"text": "I will read it."},
                    {"functionCall": {"name": "read_file", "args": {"path": "a"}},
                     "thoughtSignature": "opaque-signature"}
                ]}, "finishReason": "STOP"}],
                "usageMetadata": {"promptTokenCount": 100, "candidatesTokenCount": 7,
                                  "thoughtsTokenCount": 30, "cachedContentTokenCount": 60}}"#,
        );
        let (text, calls, thought) = parts_of(&reply);
        assert_eq!(text, "I will read it.");
        assert!(thought);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "read_file");
        assert_eq!(
            calls[0].function.arguments.as_deref(),
            Some(r#"{"path":"a"}"#)
        );
        assert_eq!(
            calls[0].extra_content,
            Some(json!({"google": {"thought_signature": "opaque-signature"}}))
        );
        let usage = Usage::from(reply.usage_metadata.expect("usage"));
        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.completion_tokens, 37);
        assert_eq!(usage.cached.read_tokens, 60);
    }

    /// A call with no arguments is `{}`, which the turn loop can parse, and every call made up in
    /// a reply gets an id of its own.
    #[test]
    fn a_call_naming_no_arguments_has_an_empty_object_and_each_call_its_own_id() {
        let reply = response(
            r#"{"candidates": [{"content": {"parts": [
                {"functionCall": {"name": "list"}}, {"functionCall": {"name": "list"}}]}}]}"#,
        );
        let (_, calls, _) = parts_of(&reply);
        assert_eq!(calls[0].function.arguments.as_deref(), Some("{}"));
        assert_ne!(calls[0].id, calls[1].id);
        assert!(calls[0].id.is_some());
    }

    /// A refused prompt and a reply stopped for what it would have said are refusals, a call
    /// written badly is a fault worth asking again, and an ordinary stop is neither.
    #[test]
    fn a_reply_that_stopped_for_a_reason_is_a_failure_and_an_ordinary_stop_is_not() {
        for (json, expected) in [
            (
                r#"{"promptFeedback": {"blockReason": "SAFETY"}}"#,
                Some(Failure::Refusal),
            ),
            (
                r#"{"candidates": [{"finishReason": "SAFETY"}]}"#,
                Some(Failure::Refusal),
            ),
            (
                r#"{"candidates": [{"finishReason": "PROHIBITED_CONTENT"}]}"#,
                Some(Failure::Refusal),
            ),
            (
                r#"{"candidates": [{"finishReason": "MALFORMED_FUNCTION_CALL"}]}"#,
                Some(Failure::Api),
            ),
            (r#"{"candidates": [{"finishReason": "STOP"}]}"#, None),
            (r#"{"candidates": [{"finishReason": "MAX_TOKENS"}]}"#, None),
            (r#"{"candidates": [{}]}"#, None),
        ] {
            assert_eq!(unfinished(&response(json)), expected, "{json}");
        }
        assert!(Failure::Api.is_transient());
        assert!(!Failure::Refusal.is_transient());
    }

    /// Google's status words map to the closed set, and a word it has not met is unknown.
    #[test]
    fn a_google_status_word_maps_to_a_failure_in_the_closed_set() {
        for (status, expected) in [
            ("INVALID_ARGUMENT", Failure::InvalidRequest),
            ("NOT_FOUND", Failure::NotFound),
            ("UNAUTHENTICATED", Failure::Authentication),
            ("PERMISSION_DENIED", Failure::Permission),
            ("RESOURCE_EXHAUSTED", Failure::RateLimit),
            ("UNAVAILABLE", Failure::Overloaded),
            ("DEADLINE_EXCEEDED", Failure::Timeout),
            ("INTERNAL", Failure::Api),
            ("SOMETHING_NEW", Failure::Unknown),
        ] {
            assert_eq!(failure_named(status), expected, "{status}");
        }
    }

    /// A streamed piece is a response, an error is a failure, and a piece of a kind this does not
    /// read is passed over rather than refused.
    #[test]
    fn a_streamed_piece_is_a_response_an_error_or_nothing_this_reads() {
        let piece = chunk(&json!({"candidates": [{"content": {"parts": [{"text": "hi"}]}}]}));
        assert!(matches!(piece, Decoded::Chunk(_)), "{piece:?}");
        let error = chunk(
            &json!({"error": {"code": 429, "status": "RESOURCE_EXHAUSTED",
                                            "message": "slow down"}}),
        );
        assert!(
            matches!(error, Decoded::Failed(Failure::RateLimit)),
            "{error:?}"
        );
        assert!(matches!(chunk(&json!({"something": 1})), Decoded::Ignored));
        let malformed = chunk(&json!({"candidates": "not a list"}));
        assert!(matches!(malformed, Decoded::Unreadable), "{malformed:?}");
    }
}
