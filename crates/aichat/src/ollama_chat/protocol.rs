//! Wire types for Ollama's own `/api/chat`, and the translation to and from the shapes the rest of
//! this agent speaks.
//!
//! [`crate::conversation`] restates the conversation as turns of blocks, and this module flattens
//! them into the messages this API takes:
//!
//! - A message is `role` and `content`, with the roles `system`, `user`, `assistant` and `tool`.
//!   Images ride on the message as bare base64 in `images`, and nothing else can be attached.
//! - A call is `tool_calls: [{function: {name, arguments}}]` with the arguments an object, not a
//!   string. A result is a message of its own under the role `tool`, naming the function in
//!   `tool_name`, because a call carries no id to answer.
//! - The window the server allocates for the request and the reply ceiling are `options.num_ctx` and
//!   `options.num_predict`, and the schema a reply must match is `format`.
//! - A stream is one JSON object a line. The last says `done`, why in `done_reason`, and what the
//!   request cost in `prompt_eval_count` and `eval_count`. A call arrives whole.
//!
//! Nothing here inspects content to make a decision. Text is moved between shapes and handed on with
//! whatever label it arrived under.

use crate::anthropic::protocol::Failure;
use crate::conversation::{self, Spelling};
use crate::generate_content::protocol::made_up_call_id;
use crate::protocol::{Message, Tool, ToolCall, ToolCallFunction, Usage};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::cell::RefCell;
use std::collections::HashMap;

/// Why a reply stopped where the ceiling stopped it rather than the model.
pub const DONE_LENGTH: &str = "length";

/// The media types this API takes, which are the images its multimodal models read.
const IMAGE_TYPES: [&str; 3] = ["image/png", "image/jpeg", "image/webp"];

/// A request to `/api/chat`.
#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<WireMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    /// The JSON schema a reply must match, where one was asked for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<Value>,
    pub options: Map<String, Value>,
    pub stream: bool,
}

impl ChatRequest {
    /// Have the server allocate `window` tokens for this request, where it is known.
    ///
    /// Without it the server uses its own default, which is smaller than most conversations an
    /// agent holds, and it drops the start of a prompt that does not fit rather than refusing it.
    pub fn with_window(mut self, window: Option<u64>) -> Self {
        if let Some(window) = window {
            self.options.insert("num_ctx".to_string(), window.into());
        }
        self
    }

    /// Stop the reply at `ceiling` tokens, where somebody stated one.
    pub fn with_ceiling(mut self, ceiling: Option<u64>) -> Self {
        if let Some(ceiling) = ceiling {
            self.options
                .insert("num_predict".to_string(), ceiling.into());
        }
        self
    }

    /// Ask for a reply that is one JSON value matching `schema`, or leave the reply free.
    pub fn with_schema(mut self, schema: Option<&Value>) -> Self {
        self.format = schema.cloned();
        self
    }
}

/// One message as this API states it.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct WireMessage {
    pub role: &'static str,
    pub content: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<WireCall>,
    /// The function a `tool` message answers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireCall {
    pub function: WireFunction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireFunction {
    pub name: String,
    /// An object. Absent where a call takes no arguments, and read as an empty one.
    #[serde(default)]
    pub arguments: Option<Value>,
}

/// One block of a turn, before the turn is flattened into messages.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Text(String),
    Image(String),
    Call(WireCall),
    /// A result, with the name of the call it answers.
    Result {
        name: String,
        text: String,
    },
}

/// How this API names the parts of a turn, for [`conversation::walk`].
///
/// A result must name the function it answers, and the walk gives it by id alone, so the name is
/// taken from the call the walk visited under that id.
#[derive(Default)]
struct Blocks {
    answering: RefCell<HashMap<String, String>>,
}

impl Spelling for Blocks {
    type Block = Block;

    fn text(&self, text: &str) -> Block {
        Block::Text(text.to_string())
    }

    fn attachment(&self, media_type: &str, data: &str, _position: usize) -> Option<Block> {
        IMAGE_TYPES
            .contains(&media_type)
            .then(|| Block::Image(data.to_string()))
    }

    fn call(&self, id: &str, name: &str, input: Value) -> Block {
        self.answering
            .borrow_mut()
            .insert(id.to_string(), name.to_string());
        Block::Call(WireCall {
            function: WireFunction {
                name: name.to_string(),
                arguments: Some(input),
            },
        })
    }

    fn result(&self, id: &str, text: String) -> Block {
        // A result whose call was not sent has no name to answer, and is said as words instead, as
        // a request offering no tools says every result.
        match self.answering.borrow().get(id) {
            Some(name) => Block::Result {
                name: name.clone(),
                text,
            },
            None => Block::Text(format!("[result of call {id}]\n{text}")),
        }
    }
}

/// The messages one turn of blocks is, in the order its blocks came.
///
/// Words and images join one message under the turn's role. A result is a message of its own, so a
/// turn holding results and then words is the results and then a message of the words, and an
/// assistant turn is one message holding its words and its calls.
fn flattened(role: &'static str, blocks: Vec<Block>, into: &mut Vec<WireMessage>) {
    let mut pending = WireMessage {
        role,
        ..WireMessage::default()
    };
    let flush = |pending: &mut WireMessage, into: &mut Vec<WireMessage>| {
        if !pending.content.is_empty()
            || !pending.images.is_empty()
            || !pending.tool_calls.is_empty()
        {
            into.push(std::mem::replace(
                pending,
                WireMessage {
                    role,
                    ..WireMessage::default()
                },
            ));
        }
    };
    for block in blocks {
        match block {
            Block::Text(text) => pending.content.push_str(&text),
            Block::Image(data) => pending.images.push(data),
            Block::Call(call) => pending.tool_calls.push(call),
            Block::Result { name, text } => {
                flush(&mut pending, into);
                into.push(WireMessage {
                    role: "tool",
                    content: text,
                    tool_name: Some(name),
                    ..WireMessage::default()
                });
            }
        }
    }
    flush(&mut pending, into);
}

/// Build a request from the conversation the agent holds.
pub fn request_from(
    model: &str,
    messages: &[Message],
    tools: Option<&[Tool]>,
    stream: bool,
) -> ChatRequest {
    // An empty list is no list: a turn offering nothing sends no `tools` field at all.
    let tools = tools.filter(|tools| !tools.is_empty());
    let walked = conversation::walk(&Blocks::default(), messages, tools.is_some());

    let mut wire = Vec::new();
    if let Some(system) = walked.system.filter(|system| !system.trim().is_empty()) {
        wire.push(WireMessage {
            role: "system",
            content: system,
            ..WireMessage::default()
        });
    }
    for turn in walked.turns {
        let role = if turn.role == "assistant" {
            "assistant"
        } else {
            "user"
        };
        flattened(role, turn.blocks, &mut wire);
    }

    ChatRequest {
        model: model.to_string(),
        messages: wire,
        tools: tools.map(<[Tool]>::to_vec),
        format: None,
        options: Map::new(),
        stream,
    }
}

/// One reply, whole or as one line of a stream.
#[derive(Debug, Default, Deserialize)]
pub struct Response {
    #[serde(default)]
    pub message: Option<ReplyMessage>,
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub done_reason: Option<String>,
    #[serde(default)]
    pub prompt_eval_count: Option<u64>,
    #[serde(default)]
    pub eval_count: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ReplyMessage {
    #[serde(default)]
    pub content: String,
    /// The model's reasoning, which is no part of the reply.
    #[serde(default)]
    pub thinking: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<WireCall>,
}

impl Response {
    /// What the request cost, where this line is the one that says so.
    pub fn usage(&self) -> Option<Usage> {
        Some(Usage {
            prompt_tokens: self.prompt_eval_count?,
            completion_tokens: self.eval_count.unwrap_or(0),
            ..Usage::default()
        })
    }

    /// Whether the ceiling stopped the reply.
    pub fn cut_off(&self) -> bool {
        self.done && self.done_reason.as_deref() == Some(DONE_LENGTH)
    }

    /// The text, the calls and the reasoning this reply or line carries.
    pub fn parts(&self) -> (&str, Vec<ToolCall>, &str) {
        let Some(message) = &self.message else {
            return ("", Vec::new(), "");
        };
        let calls = message.tool_calls.iter().map(call_from).collect();
        (
            &message.content,
            calls,
            message.thinking.as_deref().unwrap_or(""),
        )
    }
}

/// A call as the agent holds it, with an id made up for it, since this API gives none.
fn call_from(call: &WireCall) -> ToolCall {
    ToolCall {
        id: Some(made_up_call_id()),
        function: ToolCallFunction {
            name: call.function.name.clone(),
            arguments: Some(
                call.function
                    .arguments
                    .clone()
                    .unwrap_or_else(|| json!({}))
                    .to_string(),
            ),
        },
        extra_content: None,
    }
}

/// What reading one line of a stream, or a whole reply, came to.
#[derive(Debug)]
pub enum Decoded {
    Line(Box<Response>),
    /// The server said it was not going to finish the reply. Its words are not kept: they are the
    /// server's, and what reaches a person is this crate's own name for the failure.
    Failed(Failure),
    /// A line in a shape this does not read.
    Unreadable,
}

/// Read one line, given as the JSON it carried.
pub fn line(value: Value) -> Decoded {
    if value.get("error").is_some() {
        return Decoded::Failed(Failure::Api);
    }
    match serde_json::from_value(value) {
        Ok(response) => Decoded::Line(Box::new(response)),
        Err(_) => Decoded::Unreadable,
    }
}

/// Splits a byte stream into the lines it is made of, keeping a line that has not ended yet until
/// the rest of it arrives.
#[derive(Debug, Default)]
pub struct LineDecoder {
    buffer: Vec<u8>,
}

impl LineDecoder {
    /// The lines `bytes` completes, without their ends and skipping blank ones.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        self.buffer.extend_from_slice(bytes);
        let mut lines = Vec::new();
        while let Some(end) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.buffer.drain(..=end).collect();
            let line = line.trim_ascii();
            if !line.is_empty() {
                lines.push(line.to_vec());
            }
        }
        lines
    }

    /// What is left once the stream has ended: a last line its sender did not end.
    pub fn finish(&mut self) -> Option<Vec<u8>> {
        let rest = std::mem::take(&mut self.buffer);
        let rest = rest.trim_ascii();
        (!rest.is_empty()).then(|| rest.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ToolCallRequest, ToolCallRequestFunction};

    fn body_of(request: &ChatRequest) -> Value {
        serde_json::to_value(request).expect("a request serialises")
    }

    fn offered() -> Vec<Tool> {
        vec![Tool::function(
            "read_file",
            "Read a file",
            json!({"type": "object", "properties": {"path": {"type": "string"}}}),
        )]
    }

    fn calling(id: &str, name: &str, arguments: &str) -> ToolCallRequest {
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

    /// The body this API states: the system prompt as the first message, the turns under their own
    /// roles, the tools in the function shape, and the window and ceiling in `options`.
    #[test]
    fn a_request_is_messages_with_the_window_and_ceiling_in_options() {
        let tools = offered();
        let request = request_from(
            "qwen3:8b",
            &[
                Message::system("be brief"),
                Message::user("hello"),
                Message::assistant("hi"),
                Message::user("again"),
            ],
            Some(&tools),
            true,
        )
        .with_window(Some(32_768))
        .with_ceiling(Some(4_096));
        assert_eq!(
            body_of(&request),
            json!({
                "model": "qwen3:8b",
                "messages": [
                    {"role": "system", "content": "be brief"},
                    {"role": "user", "content": "hello"},
                    {"role": "assistant", "content": "hi"},
                    {"role": "user", "content": "again"},
                ],
                "tools": [{"type": "function", "function": {
                    "name": "read_file",
                    "description": "Read a file",
                    "parameters": {"type": "object", "properties": {"path": {"type": "string"}}}
                }}],
                "options": {"num_ctx": 32_768, "num_predict": 4_096},
                "stream": true,
            })
        );
    }

    /// A request nobody stated a window, a ceiling, tools or a schema for sends none of them, so
    /// the server keeps its own defaults.
    #[test]
    fn a_request_nobody_asked_anything_of_carries_only_the_conversation() {
        let body = body_of(&request_from(
            "m",
            &[Message::user("hello")],
            Some(&[]),
            false,
        ));
        assert_eq!(
            body,
            json!({
                "model": "m",
                "messages": [{"role": "user", "content": "hello"}],
                "options": {},
                "stream": false,
            })
        );
    }

    #[test]
    fn a_schema_goes_in_format() {
        let schema = json!({"type": "object"});
        let body = body_of(
            &request_from("m", &[Message::user("x")], None, false).with_schema(Some(&schema)),
        );
        assert_eq!(body["format"], schema);
    }

    /// A call goes back with its arguments as the object they are, and its result as a `tool`
    /// message naming the function, since a call carries no id here to answer by.
    #[test]
    fn a_call_and_its_result_are_an_assistant_message_and_a_tool_message_naming_it() {
        let tools = offered();
        let messages = [
            Message::user("read x"),
            Message::assistant_calling("", vec![calling("call_1", "read_file", r#"{"path":"x"}"#)]),
            Message::tool_result("call_1", "contents of x"),
            Message::user("thanks"),
        ];
        let body = body_of(&request_from("m", &messages, Some(&tools), true));
        assert_eq!(
            body["messages"],
            json!([
                {"role": "user", "content": "read x"},
                {"role": "assistant", "content": "",
                 "tool_calls": [{"function": {"name": "read_file", "arguments": {"path": "x"}}}]},
                {"role": "tool", "content": "contents of x", "tool_name": "read_file"},
                {"role": "user", "content": "thanks"},
            ])
        );
    }

    /// An image rides on its message as bare base64, and an attachment of a type this API does not
    /// take is dropped.
    #[test]
    fn an_image_rides_on_its_message_as_bare_base64() {
        use crate::protocol::{ImageUrl, Part as Attached};
        let picture = |url: &str| Attached::ImageUrl {
            image_url: ImageUrl {
                url: url.to_string(),
            },
        };
        let body = body_of(&request_from(
            "m",
            &[Message::user_parts(vec![
                Attached::Text {
                    text: "look".to_string(),
                },
                picture("data:image/png;base64,AAAA"),
                picture("data:video/mp4;base64,BBBB"),
            ])],
            None,
            false,
        ));
        assert_eq!(
            body["messages"],
            json!([{"role": "user", "content": "look", "images": ["AAAA"]}])
        );
    }

    /// A result whose call is not in the conversation has no function to name, so it is said as
    /// words rather than sent as a `tool` message nothing answers.
    #[test]
    fn a_result_whose_call_was_not_sent_is_said_as_words() {
        let tools = offered();
        let messages = [
            Message::user("go on"),
            Message::tool_result("call_9", "late output"),
        ];
        let body = body_of(&request_from("m", &messages, Some(&tools), true));
        let messages = body["messages"].as_array().expect("messages");
        assert!(
            messages.iter().all(|message| message["role"] != "tool"),
            "{body}"
        );
        let said = messages
            .iter()
            .filter_map(|message| message["content"].as_str())
            .collect::<String>();
        assert!(said.contains("late output"), "{body}");
    }

    /// The tail of one line arrives with the next read, and a line is read only once it has ended.
    #[test]
    fn a_line_split_across_reads_is_read_once_whole() {
        let mut decoder = LineDecoder::default();
        assert!(decoder.push(br#"{"message":{"content":"he"#).is_empty());
        assert_eq!(
            decoder.push(b"llo\"},\"done\":false}\n\n{\"done\":true}"),
            vec![br#"{"message":{"content":"hello"},"done":false}"#.to_vec()]
        );
        assert_eq!(decoder.finish(), Some(br#"{"done":true}"#.to_vec()));
        assert_eq!(decoder.finish(), None);
    }

    #[test]
    fn the_last_line_states_the_cost_and_whether_the_ceiling_stopped_it() {
        let Decoded::Line(last) = line(json!({
            "message": {"role": "assistant", "content": ""},
            "done": true, "done_reason": "length",
            "prompt_eval_count": 1200, "eval_count": 64
        })) else {
            panic!("not read");
        };
        assert!(last.cut_off());
        let usage = last.usage().expect("a cost");
        assert_eq!((usage.prompt_tokens, usage.completion_tokens), (1200, 64));
    }

    /// A call arrives whole, with an object for arguments and no id, and goes to the agent with one
    /// made up and the arguments as the string the rest of the agent reads.
    #[test]
    fn a_call_in_a_reply_is_given_an_id_and_its_arguments_as_a_string() {
        let Decoded::Line(reply) = line(json!({
            "message": {"role": "assistant", "content": "",
                        "thinking": "weighing it up",
                        "tool_calls": [{"function": {"name": "read_file", "arguments": {"path": "x"}}}]},
            "done": false
        })) else {
            panic!("not read");
        };
        let (text, calls, thinking) = reply.parts();
        assert_eq!((text, thinking), ("", "weighing it up"));
        assert_eq!(calls[0].function.name, "read_file");
        assert_eq!(
            calls[0].function.arguments.as_deref(),
            Some(r#"{"path":"x"}"#)
        );
        assert!(calls[0].id.is_some());
    }

    #[test]
    fn an_error_line_is_a_failure_and_not_a_reply() {
        assert!(matches!(
            line(json!({"error": "model runner has unexpectedly stopped"})),
            Decoded::Failed(Failure::Api)
        ));
    }
}
