//! Wire types for the OpenAI-compatible chat completions API.
//!
//! Only the fields this client uses are modelled. Unknown response fields are ignored
//! rather than rejected, so a server-side addition does not break the client.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    /// The result of a call the assistant asked for.
    ///
    /// Distinct from a user message on purpose: a result that arrives as something the user
    /// said is a result that can be read as an instruction from them.
    Tool,
}

/// What a message carries.
///
/// A bare string for everything said in words, which is all but one of them, and a list of parts
/// when a message carries something that is not words. Attachments are the only thing that is not,
/// and an attachment always arrives beside the line the user typed, so the parts form is never a
/// picture on its own.
///
/// `Text` comes first in the untagged enum on purpose: a stored session records `content` as a
/// bare JSON string, so a record written before parts existed still reads back as one.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum Content {
    Text(String),
    Parts(Vec<Part>),
}

/// One piece of a message that carries more than words.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Part {
    Text { text: String },
    ImageUrl { image_url: ImageUrl },
}

/// Where an image part's bytes are.
///
/// A `data:` URI rather than a link, because the alternative is asking the endpoint to fetch a URL
/// the conversation chose, which is an effect with a routing field nobody endorsed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImageUrl {
    pub url: String,
}

impl Content {
    /// The words, when this message is words and nothing else.
    ///
    /// `None` rather than an empty string for a message carrying parts, so a caller asking "does
    /// this start with the resume marker" cannot get a yes from a message that has no text at all.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Content::Text(text) => Some(text),
            Content::Parts(_) => None,
        }
    }

    /// The words in this message, with anything that is not words left out.
    ///
    /// For a transcript, where an attachment is drawn from what the interface recorded rather than
    /// from the bytes on the wire.
    pub fn text(&self) -> String {
        match self {
            Content::Text(text) => text.clone(),
            Content::Parts(parts) => parts
                .iter()
                .filter_map(|part| match part {
                    Part::Text { text } => Some(text.as_str()),
                    Part::ImageUrl { .. } => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
}

impl From<String> for Content {
    fn from(text: String) -> Self {
        Content::Text(text)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Content,
    /// Calls this assistant turn asked for, in the API's own field.
    ///
    /// Tool calls belong here rather than written out in `content`. Described in prose they
    /// become an example of what an assistant turn looks like, and a model with such an example
    /// in front of it writes the next one as prose too, which is a call that never runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallRequest>>,
    /// Which call a [`Role::Tool`] message answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self::plain(Role::System, content)
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::plain(Role::User, content)
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self::plain(Role::Assistant, content)
    }

    /// An assistant turn that asked for calls, with the calls in the field the API reads.
    pub fn assistant_calling(content: impl Into<String>, calls: Vec<ToolCallRequest>) -> Self {
        Self {
            role: Role::Assistant,
            content: Content::Text(content.into()),
            tool_calls: Some(calls),
            tool_call_id: None,
        }
    }

    /// The result of one call, answering it by id.
    pub fn tool_result(call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: Content::Text(content.into()),
            tool_calls: None,
            tool_call_id: Some(call_id.into()),
        }
    }

    fn plain(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: Content::Text(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// A user turn carrying attachments beside what the user typed.
    ///
    /// The only constructor that produces parts. Everything else the driver sends is words.
    pub fn user_parts(parts: Vec<Part>) -> Self {
        Self {
            role: Role::User,
            content: Content::Parts(parts),
            tool_calls: None,
            tool_call_id: None,
        }
    }
}

/// A call being sent back to the model as part of the conversation.
///
/// Separate from [`ToolCall`], which is the shape one arrives in: what arrives may be missing
/// its arguments, and what is sent may not.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolCallRequest {
    pub id: String,
    /// Always "function", which is the only kind the API defines. Owned rather than borrowed so
    /// a stored conversation can be read back into one of these.
    #[serde(rename = "type", default = "function_kind")]
    pub kind: String,
    pub function: ToolCallRequestFunction,
}

fn function_kind() -> String {
    "function".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolCallRequestFunction {
    pub name: String,
    /// A JSON object, as a string, which is how the API carries it in both directions.
    pub arguments: String,
}

/// A tool the model may call, in OpenAI's function-tool shape.
#[derive(Debug, Clone, Serialize)]
pub struct Tool {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub function: ToolFunction,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolFunction {
    pub name: String,
    pub description: String,
    /// JSON Schema for the arguments.
    pub parameters: Value,
}

impl Tool {
    pub fn function(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: Value,
    ) -> Self {
        Self {
            kind: "function",
            function: ToolFunction {
                name: name.into(),
                description: description.into(),
                parameters,
            },
        }
    }
}

/// How hard the model is asked to think before it answers.
///
/// Five levels rather than a token count. A count is a number about a model's internals that only
/// somebody who already knows the model can set, and the same figure means something different on
/// the next one; a word ranks the same way whatever answers.
///
/// Not content, and nothing derived from content. The word comes from a person picking off a list
/// they read, on the same footing as the model name beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl Effort {
    /// Every level, cheapest first, which is the order a picker offers them in.
    pub const ALL: [Effort; 5] = [
        Effort::Low,
        Effort::Medium,
        Effort::High,
        Effort::Xhigh,
        Effort::Max,
    ];

    /// The word this level is sent and stored as.
    pub fn as_str(self) -> &'static str {
        match self {
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
            Effort::Xhigh => "xhigh",
            Effort::Max => "max",
        }
    }

    /// The level a word names, or `None` for a word that names none of them.
    ///
    /// Case-insensitive, because this reads a word somebody typed or a file they may have edited,
    /// and `High` is the same request as `high`. Anything else is no choice at all rather than a
    /// choice of something: an unrecognised word must not become a request field.
    pub fn named(word: &str) -> Option<Effort> {
        let word = word.trim();
        Effort::ALL
            .into_iter()
            .find(|level| level.as_str().eq_ignore_ascii_case(word))
    }
}

/// The name the effort field goes out under, for a caller that has to take it back out of a body a
/// service refused.
///
/// A `rename` cannot be written in terms of a constant, so this repeats the word the field
/// serializes as and a test holds the two together.
pub const EFFORT_FIELD: &str = "reasoning_effort";

#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// Asks for a usage report on the final chunk. Meaningless without `stream`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    /// How hard to think, in the name this protocol gives the field.
    ///
    /// Absent unless somebody asked for a level, so a build nobody has asked sends exactly what it
    /// sent before and every service keeps its own default.
    #[serde(rename = "reasoning_effort", skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    /// Whether a later request sends this conversation again, which is what makes a breakpoint on
    /// the end of it worth the write it costs.
    ///
    /// Not a field this protocol has: it decides what the body asks to be cached rather than
    /// travelling in one.
    #[serde(skip)]
    pub conversation_is_sent_again: bool,
}

/// Options that only apply to a streamed request.
#[derive(Debug, Clone, Serialize)]
pub struct StreamOptions {
    /// Without this a streamed response reports no usage at all, and the turn could not say what
    /// it cost.
    pub include_usage: bool,
}

impl ChatRequest {
    pub fn new(model: impl Into<String>, messages: Vec<Message>) -> Self {
        Self {
            model: model.into(),
            messages,
            stream: None,
            stream_options: None,
            tools: None,
            effort: None,
            conversation_is_sent_again: true,
        }
    }

    /// The conversation in this request is given up once it answers, so nothing asks for a cache of
    /// it.
    ///
    /// A breakpoint on the end of it would ask a service to store a prefix nothing sends again, and
    /// a cache write is charged above the tokens it covers. The prompt in front of it is marked as
    /// any other request's is, being the same bytes every time this request is made.
    pub fn giving_up_its_conversation(mut self) -> Self {
        self.conversation_is_sent_again = false;
        self
    }

    pub fn with_tools(mut self, tools: Vec<Tool>) -> Self {
        self.tools = if tools.is_empty() { None } else { Some(tools) };
        self
    }

    /// Ask for a particular amount of thinking, or leave the service to its own default.
    pub fn with_effort(mut self, effort: Option<Effort>) -> Self {
        self.effort = effort;
        self
    }

    /// Ask for the reply in chunks, with a usage report at the end.
    ///
    /// The two go together: streaming without `include_usage` would trade the cost figure for the
    /// live one, and both are wanted.
    pub fn streamed(mut self) -> Self {
        self.stream = Some(true);
        self.stream_options = Some(StreamOptions {
            include_usage: true,
        });
        self
    }

    /// This request's body with the prefixes worth caching marked, for a service that reads a
    /// breakpoint.
    ///
    /// A `Value` rather than a second request type. Every field is serialized from this struct and
    /// only the marked messages' content is written over, so a field added to a request cannot be
    /// forgotten here, which is the way a parallel wire struct goes wrong.
    pub fn marked_body(&self) -> Result<Value, serde_json::Error> {
        let mut body = serde_json::to_value(self)?;
        let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) else {
            return Ok(body);
        };
        for at in breakpoints(&self.messages, self.conversation_is_sent_again) {
            let content = serde_json::to_value(marked(&self.messages[at].content))?;
            if let Some(message) = messages.get_mut(at).and_then(Value::as_object_mut) {
                message.insert("content".to_string(), content);
            }
        }
        Ok(body)
    }
}

/// A breakpoint: everything up to and including the block carrying this may be answered out of
/// the service's cache.
///
/// `ephemeral` is the only lifetime the shape defines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CacheControl {
    Ephemeral,
}

/// One content block on its way out, which unlike a [`Part`] can carry a breakpoint.
///
/// Separate from [`Part`] because a part is also what a stored session records, and a breakpoint
/// belongs to one request rather than to the conversation. Keeping the field out of the recorded
/// type is what stops a breakpoint reaching a session file, where it would be read back and sent
/// again in a request that never asked for one.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum MarkedPart<'a> {
    Text {
        text: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    ImageUrl {
        image_url: &'a ImageUrl,
    },
}

impl<'a> From<&'a Part> for MarkedPart<'a> {
    fn from(part: &'a Part) -> Self {
        match part {
            Part::Text { text } => MarkedPart::Text {
                text,
                cache_control: None,
            },
            Part::ImageUrl { image_url } => MarkedPart::ImageUrl { image_url },
        }
    }
}

/// Which messages carry a breakpoint: the system prompt, and the last thing the user said.
///
/// Those are the two prefixes that outlive a request. The system prompt and the tool schemas in
/// front of it are the same bytes every turn, and the conversation up to the last user turn is the
/// same bytes for every round of that turn, since what a round adds is a call and its result on
/// the end. So the rounds within one turn read the whole conversation back and pay only for what
/// they appended, and the next turn extends the cached prefix rather than starting one.
///
/// A result or a call is not marked. Marking one would mean sending a `tool` message's content as
/// a list of blocks, which not every service that reads this wire format accepts, and a service
/// that rejects it costs the system prompt's breakpoint too.
///
/// The conversation's own breakpoint goes on only where a later request sends that conversation
/// again. A request that gives its exchange up once it answers would be paying a cache write, which
/// costs more than the tokens it covers, for a prefix nothing can read back.
fn breakpoints(messages: &[Message], conversation_is_sent_again: bool) -> Vec<usize> {
    let system = messages
        .iter()
        .rposition(|message| matches!(message.role, Role::System));
    let last = messages
        .len()
        .checked_sub(1)
        .filter(|_| conversation_is_sent_again)
        .filter(|&at| matches!(messages[at].role, Role::User));
    system
        .into_iter()
        .chain(last)
        .filter(|&at| ends_in_words(&messages[at]))
        .collect()
}

/// Whether a message ends in words, which is what a breakpoint has to sit on.
///
/// A message ending in a picture is left unmarked rather than marked behind the picture: what a
/// service makes of a breakpoint on an image block is not a thing to guess at. An empty string is
/// no words at all.
fn ends_in_words(message: &Message) -> bool {
    match &message.content {
        Content::Text(text) => !text.is_empty(),
        Content::Parts(parts) => {
            matches!(parts.last(), Some(Part::Text { text }) if !text.is_empty())
        }
    }
}

/// The same content as blocks, with the breakpoint on the words it ends in.
fn marked(content: &Content) -> Vec<MarkedPart<'_>> {
    let mut parts = match content {
        Content::Text(text) => vec![MarkedPart::Text {
            text,
            cache_control: None,
        }],
        Content::Parts(parts) => parts.iter().map(MarkedPart::from).collect(),
    };
    if let Some(MarkedPart::Text { cache_control, .. }) = parts.last_mut() {
        *cache_control = Some(CacheControl::Ephemeral);
    }
    parts
}

/// A tool call the model asked for.
///
/// The arguments are model output, so they are untrusted. Nothing here may be treated as
/// routing without a human endorsement.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolCall {
    #[serde(default)]
    pub id: Option<String>,
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    /// A JSON object, delivered as a string by the API.
    #[serde(default)]
    pub arguments: Option<String>,
}

impl ToolCall {
    /// The same call, in the shape it is sent back in.
    ///
    /// `None` where the server gave the call no id. An id is what a result is matched to, so a
    /// call without one cannot be replayed as a call at all.
    pub fn as_request(&self) -> Option<ToolCallRequest> {
        Some(ToolCallRequest {
            id: self.id.clone()?,
            kind: function_kind(),
            function: ToolCallRequestFunction {
                name: self.function.name.clone(),
                arguments: self
                    .function
                    .arguments
                    .clone()
                    .unwrap_or_else(|| "{}".to_string()),
            },
        })
    }

    /// Parse the arguments.
    ///
    /// Parsing changes representation, not trust: the values remain model-supplied and
    /// must still pass the gates before they can direct anything.
    pub fn arguments(&self) -> Result<Value, serde_json::Error> {
        match self.function.arguments.as_deref() {
            Some(raw) if !raw.trim().is_empty() => serde_json::from_str(raw),
            _ => Ok(serde_json::json!({})),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ChatResponse {
    /// The model actually used. Absent on some error shapes, so it is optional.
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub choices: Vec<Choice>,
    /// What the request cost. Absent on error shapes and from some servers.
    #[serde(default)]
    pub usage: Option<Usage>,
}

/// Tokens a request consumed, as the server counted them.
///
/// Reported rather than estimated: a local guess would drift from what the user is actually
/// billed for, and the point of showing it is to be honest about cost.
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
pub struct Usage {
    #[serde(default)]
    pub prompt_tokens: u64,
    #[serde(default)]
    pub completion_tokens: u64,
    /// Of the prompt, how much of it the service did not have to read again.
    #[serde(default, rename = "prompt_tokens_details", deserialize_with = "cached")]
    pub cached: Cached,
}

/// How much of a prompt a service answered out of its own cache.
///
/// A split of [`Usage::prompt_tokens`] rather than anything extra: both figures are already
/// inside it, because what was sent is what was sent whoever ended up reading it. Kept because
/// the total cannot say what a round cost. Reading a cached token is a fraction of the price of
/// reading a fresh one and writing one costs above it, so a session whose cache hit every round
/// and one whose cache missed every round report the same prompt and differ about tenfold in
/// money and in latency.
///
/// Both zero means a service that said nothing about a cache as much as it means a round that
/// missed, and nothing here distinguishes the two.
/// Serialised because a session record carries the figure the status panel is showing, so that a
/// rewind puts back what the turn before the rewound one read rather than what the rewound one did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cached {
    /// Served out of the cache instead of read.
    pub read_tokens: u64,
    /// Written into the cache on the way past, for the next request to read back.
    pub written_tokens: u64,
}

impl Cached {
    /// Add another request's figures, for a total over a turn or a session.
    pub fn add(&mut self, other: Cached) {
        self.read_tokens += other.read_tokens;
        self.written_tokens += other.written_tokens;
    }

    /// Whether either figure has anything in it.
    pub fn any(&self) -> bool {
        self.read_tokens > 0 || self.written_tokens > 0
    }
}

/// Read the cache figure out of the object this protocol nests it in.
///
/// The nesting stops here, so the rest of the agent reads one shape however a service stated it.
/// Nothing is read for what a request wrote: this protocol states no field for it, and a service
/// that charges for a cache write reports it in a shape of its own.
///
/// Both levels are optional because a server may state the field and put `null` in it, which
/// `serde(default)` does not cover: that only answers for a field nobody sent. Refusing such a
/// reply would fail the whole turn over a figure this reads for information.
fn cached<'de, D>(deserializer: D) -> Result<Cached, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    struct Details {
        #[serde(default)]
        cached_tokens: Option<u64>,
    }
    Ok(Cached {
        read_tokens: Option::<Details>::deserialize(deserializer)?
            .and_then(|details| details.cached_tokens)
            .unwrap_or_default(),
        written_tokens: 0,
    })
}

impl Usage {
    /// Everything this request cost, in and out.
    pub fn total(&self) -> u64 {
        self.prompt_tokens + self.completion_tokens
    }
}

#[derive(Debug, Deserialize)]
pub struct Choice {
    #[serde(default)]
    pub message: Option<ResponseMessage>,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ResponseMessage {
    #[serde(default)]
    pub content: Option<String>,
    /// Present when the model asked to call tools.
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCall>>,
}

impl ChatResponse {
    /// Tool calls from the first choice, if the model asked for any.
    pub fn tool_calls(&self) -> &[ToolCall] {
        self.choices
            .first()
            .and_then(|c| c.message.as_ref())
            .and_then(|m| m.tool_calls.as_deref())
            .unwrap_or(&[])
    }

    /// The first choice's text, if the response carried any.
    pub fn first_content(&self) -> Option<String> {
        self.choices.first()?.message.as_ref()?.content.clone()
    }

    /// What this response reported costing. Zero when the server said nothing.
    pub fn usage(&self) -> Usage {
        self.usage.unwrap_or_default()
    }
}

/// One chunk of a streamed completion.
///
/// Same envelope as [`ChatResponse`] but carrying `delta` instead of `message`, and with every
/// field optional: a chunk may be text, part of a tool call, a usage report, or nothing at all.
#[derive(Debug, Deserialize)]
pub struct ChatChunk {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub choices: Vec<ChunkChoice>,
    /// Present only on the final chunk, and only when usage was requested.
    #[serde(default)]
    pub usage: Option<Usage>,
    /// What the backend counted the request at, in its own final event.
    ///
    /// The deployed server does not answer `include_usage` with an OpenAI `usage` block. It sends
    /// a `brave-chat.contentReceipt` event instead, whose `total_tokens` is the input it counted
    /// after its own trimming. That is the only figure either side has for how large a request
    /// was, so it is read here rather than left on the floor: without it every prompt count in
    /// this crate is zero, which reads as a measurement and is not one.
    #[serde(default)]
    pub total_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct ChunkChoice {
    #[serde(default)]
    pub delta: Option<Delta>,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

/// What changed in this chunk.
#[derive(Debug, Deserialize)]
pub struct Delta {
    #[serde(default)]
    pub content: Option<String>,
    /// Tool calls arrive in fragments, each naming the index it belongs to.
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCallDelta>>,
}

/// A fragment of a tool call.
///
/// The API splits one call across many chunks: the first usually carries the id and name, and the
/// arguments arrive a few characters at a time. `index` says which call is being extended, since
/// a model may request several at once.
#[derive(Debug, Deserialize)]
pub struct ToolCallDelta {
    #[serde(default)]
    pub index: usize,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub function: Option<ToolCallFunctionDelta>,
}

#[derive(Debug, Deserialize)]
pub struct ToolCallFunctionDelta {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arguments: Option<String>,
}

/// A completion assembled from streamed chunks.
///
/// Accumulating is bookkeeping, not interpretation: text is concatenated in arrival order and
/// argument fragments are joined per index. Nothing here reads the content to decide anything, so
/// the result is exactly what a non-streamed response would have carried.
#[derive(Debug, Default)]
pub struct StreamAccumulator {
    content: String,
    model: Option<String>,
    usage: Option<Usage>,
    /// What the backend said the request came to, where it said so in its own shape.
    receipt: Option<u64>,
    /// Calls under construction, keyed by the index the server gave them.
    calls: Vec<(usize, PartialCall)>,
    /// Chunks carrying text, which is the only honest live measure of output before the server
    /// reports its own count.
    content_chunks: u64,
    /// Whether the server said the reply was over, rather than the bytes merely stopping.
    ended: bool,
}

#[derive(Debug, Default)]
struct PartialCall {
    id: Option<String>,
    name: String,
    arguments: String,
}

impl StreamAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fold one chunk in.
    pub fn push(&mut self, chunk: ChatChunk) {
        if self.model.is_none() {
            self.model = chunk.model;
        }
        // Usage arrives once, on the final chunk, and is authoritative over anything counted here.
        if chunk.usage.is_some() {
            self.usage = chunk.usage;
        }
        // The other shape the same fact arrives in. Kept apart from `usage` because it reports
        // the input only: treating it as a usage block would say the reply was zero tokens long
        // and throw away the live count that had been on the screen all the way through.
        if let Some(total) = chunk.total_tokens {
            self.receipt = Some(total);
        }

        for choice in chunk.choices {
            // One of the two ways this protocol says a reply is finished. Recorded before the
            // delta is looked at, because the frame carrying it often carries nothing else.
            if choice.finish_reason.is_some() {
                self.ended = true;
            }

            let Some(delta) = choice.delta else { continue };

            if let Some(text) = delta.content
                && !text.is_empty()
            {
                self.content.push_str(&text);
                self.content_chunks += 1;
            }

            for fragment in delta.tool_calls.unwrap_or_default() {
                let call = match self.calls.iter_mut().find(|(i, _)| *i == fragment.index) {
                    Some((_, call)) => call,
                    None => {
                        self.calls.push((fragment.index, PartialCall::default()));
                        &mut self.calls.last_mut().expect("just pushed").1
                    }
                };
                if let Some(id) = fragment.id {
                    call.id = Some(id);
                }
                if let Some(function) = fragment.function {
                    if let Some(name) = function.name {
                        call.name.push_str(&name);
                    }
                    if let Some(arguments) = function.arguments {
                        call.arguments.push_str(&arguments);
                    }
                }
            }
        }
    }

    /// The reply so far.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Note that the stream reached its end-of-reply marker.
    ///
    /// Separate from [`StreamAccumulator::push`] because the marker is not a chunk: it is a
    /// payload this protocol spells `[DONE]`, and the decoder is what recognises it.
    pub fn mark_ended(&mut self) {
        self.ended = true;
    }

    /// Whether the server said the reply was over.
    ///
    /// A server that finished and a server that hung up halfway both leave the caller at the end
    /// of the bytes, so the difference cannot come from the bytes running out. It has to come
    /// from something the server said, and in this protocol there are two such things: a finish
    /// reason on a choice, and the end-of-stream payload. Either will do, because a server may
    /// send one without the other, and the deployed backend sends only the second.
    ///
    /// Every streaming protocol has a marker of this kind, so a client for a different one
    /// answers the same question from whatever its own marker is.
    pub fn ended(&self) -> bool {
        self.ended
    }

    /// Output tokens as best they can be known right now.
    ///
    /// The server's own count once it has reported one, and until then the number of chunks that
    /// carried text. One chunk is one token by convention rather than by guarantee, so this is an
    /// estimate that gets replaced by the real figure, never one that persists beside it.
    pub fn output_tokens(&self) -> u64 {
        match self.usage {
            Some(usage) => usage.completion_tokens,
            None => self.content_chunks,
        }
    }

    /// Whether the server has reported usage, so the count is now authoritative.
    pub fn usage_is_reported(&self) -> bool {
        self.usage.is_some()
    }

    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    /// What the reply cost, in whichever shape the server reported it.
    ///
    /// Falls back to the receipt for the input and to the live estimate for the output, so a
    /// server that reports only one of the two does not zero the other.
    pub fn usage(&self) -> Usage {
        self.usage.unwrap_or(Usage {
            prompt_tokens: self.receipt.unwrap_or_default(),
            completion_tokens: self.content_chunks,
            ..Usage::default()
        })
    }

    /// The finished tool calls, in the order the server indexed them.
    ///
    /// A fragment that never received a name is dropped: it cannot name a tool, so there is
    /// nothing to dispatch.
    pub fn tool_calls(&self) -> Vec<ToolCall> {
        let mut calls: Vec<&(usize, PartialCall)> = self.calls.iter().collect();
        calls.sort_by_key(|(index, _)| *index);
        calls
            .into_iter()
            .filter(|(_, call)| !call.name.is_empty())
            .map(|(_, call)| ToolCall {
                id: call.id.clone(),
                function: ToolCallFunction {
                    name: call.name.clone(),
                    arguments: Some(call.arguments.clone()),
                },
            })
            .collect()
    }

    /// Everything the stream produced, in the shape a one-shot response would have had.
    /// What the reply cost, or the same estimate the count on the screen was showing.
    ///
    /// A server that answers `include_usage` with nothing left this at zero, so a session whose
    /// interface had been counting output all the way through recorded that it had cost nothing.
    /// Zero is not a better answer than an approximate one: it is a wrong answer that reads as a
    /// measurement, and the figure exists to tell somebody what a session cost them.
    ///
    /// The estimate is chunks of text rather than tokens. It is what
    /// [`StreamAccumulator::output_tokens`] has always shown live for the same reason, and
    /// [`StreamAccumulator::usage_is_reported`] is how a caller tells the two apart.
    pub fn finish(self) -> (String, Option<String>, Vec<ToolCall>, Usage) {
        let calls = self.tool_calls();
        let usage = self.usage.unwrap_or(Usage {
            prompt_tokens: self.receipt.unwrap_or_default(),
            completion_tokens: self.content_chunks,
            ..Usage::default()
        });
        (self.content, self.model, calls, usage)
    }
}

/// Pull `data:` payloads out of an SSE byte stream.
///
/// Framing only. This decides where one event ends and the next begins, which is transport
/// structure exactly like a JSON envelope, and never looks at what a payload says.
#[derive(Debug, Default)]
pub struct SseDecoder {
    buffer: Vec<u8>,
}

impl SseDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add bytes and take whatever complete `data:` payloads they finish.
    ///
    /// Events are separated by a blank line, and a payload split across reads stays buffered
    /// until it is whole: emitting a partial line would hand out truncated JSON.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.buffer.extend_from_slice(bytes);
        let mut payloads = Vec::new();

        // Lines are self-delimiting here, so events are taken a line at a time rather than by
        // scanning for a blank-line boundary: a `data:` line is complete on its own.
        while let Some(position) = self.buffer.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.buffer.drain(..=position).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_end_matches(['\r', '\n']);

            if let Some(payload) = line.strip_prefix("data:") {
                let payload = payload.trim();
                if !payload.is_empty() {
                    payloads.push(payload.to_string());
                }
            }
        }

        payloads
    }
}

/// The payload marking the end of an OpenAI-compatible stream.
pub const STREAM_DONE: &str = "[DONE]";

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_config::DEFAULT_MODEL;

    #[test]
    fn a_request_serialises_to_the_expected_shape() {
        let request = ChatRequest::new(DEFAULT_MODEL, vec![Message::user("hello")]);
        let json = serde_json::to_value(&request).unwrap();

        assert_eq!(json["model"], DEFAULT_MODEL);
        assert_eq!(json["messages"][0]["role"], "user");
        assert_eq!(json["messages"][0]["content"], "hello");
        // Omitted rather than sent as null, since the server has its own default.
        assert!(json.get("stream").is_none());
    }

    /// A build nobody has asked for a level must send what it always sent, or adding the field
    /// would change every request on every endpoint that has never seen it.
    #[test]
    fn a_request_nobody_asked_a_level_of_mentions_no_effort() {
        let request = ChatRequest::new(DEFAULT_MODEL, vec![Message::user("hello")]);
        let json = serde_json::to_value(&request).unwrap();
        assert!(json.get("reasoning_effort").is_none());
    }

    #[test]
    fn a_level_is_sent_in_the_name_this_protocol_gives_the_field() {
        let request = ChatRequest::new(DEFAULT_MODEL, vec![Message::user("hello")])
            .with_effort(Some(Effort::Xhigh));
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["reasoning_effort"], "xhigh");
    }

    /// A level a service refused is taken back out of the body by name, so the constant that names
    /// the field and the name the field goes out under have to be the same word. They are written
    /// twice because a `rename` takes a literal.
    #[test]
    fn the_constant_naming_the_effort_field_is_the_name_it_is_sent_under() {
        let request = ChatRequest::new(DEFAULT_MODEL, vec![Message::user("hello")])
            .with_effort(Some(Effort::Xhigh));
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json[EFFORT_FIELD], "xhigh");
    }

    #[test]
    fn every_level_has_a_word_that_reads_back_as_itself() {
        for level in Effort::ALL {
            assert_eq!(Effort::named(level.as_str()), Some(level));
        }
    }

    /// The word is read back from a file a person may have edited by hand, so the case they
    /// happened to type must not be the difference between a choice and none.
    #[test]
    fn a_level_is_named_whatever_case_it_was_written_in() {
        assert_eq!(Effort::named("  HIGH \n"), Some(Effort::High));
    }

    /// An unrecognised word is no choice at all rather than a choice of something: it must never
    /// become a request field.
    #[test]
    fn a_word_naming_no_level_is_not_a_choice() {
        for word in ["", "  ", "highest", "xxhigh", "3"] {
            assert_eq!(Effort::named(word), None, "{word:?} became a level");
        }
    }

    #[test]
    fn roles_serialise_lowercase() {
        let request = ChatRequest::new(
            DEFAULT_MODEL,
            vec![
                Message::system("be brief"),
                Message::user("hi"),
                Message::assistant("hello"),
            ],
        );
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["messages"][0]["role"], "system");
        assert_eq!(json["messages"][1]["role"], "user");
        assert_eq!(json["messages"][2]["role"], "assistant");
    }

    #[test]
    fn a_response_yields_its_first_choice() {
        let raw = r#"{
            "model": "some-model",
            "choices": [
                {"message": {"role": "assistant", "content": "the answer"}, "finish_reason": "stop"}
            ]
        }"#;
        let parsed: ChatResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.model.as_deref(), Some("some-model"));
        assert_eq!(parsed.first_content().as_deref(), Some("the answer"));
    }

    mod streaming {
        use super::*;

        /// The shape the deployed backend actually answers in. It never sends an OpenAI `usage`
        /// block, so without reading its own receipt every prompt count here is zero, and
        /// anything deciding from how large a request was decides from nothing.
        #[test]
        fn the_backends_own_receipt_is_read_as_the_prompt_count() {
            let mut accumulated = StreamAccumulator::new();
            accumulated.push(
                serde_json::from_str(
                    r#"{"model":"claude-4-6-sonnet","choices":[{"delta":{"content":"hello"}}]}"#,
                )
                .expect("a chunk"),
            );
            accumulated.push(
                serde_json::from_str(
                    r#"{"total_tokens":4530,"trimmed_tokens":0,"object":"brave-chat.contentReceipt"}"#,
                )
                .expect("a receipt"),
            );

            let (_, _, _, usage) = accumulated.finish();
            assert_eq!(usage.prompt_tokens, 4530);
        }

        /// The receipt reports the input and nothing else. Read as a usage block it would say the
        /// reply was zero tokens long, throwing away the live count the screen had been showing.
        #[test]
        fn a_receipt_does_not_zero_the_output_count() {
            let mut accumulated = StreamAccumulator::new();
            for piece in ["one", "two", "three"] {
                accumulated.push(
                    serde_json::from_str(&format!(
                        r#"{{"choices":[{{"delta":{{"content":"{piece}"}}}}]}}"#
                    ))
                    .expect("a chunk"),
                );
            }
            accumulated.push(
                serde_json::from_str(r#"{"total_tokens":4530,"trimmed_tokens":0}"#)
                    .expect("a receipt"),
            );

            assert_eq!(accumulated.output_tokens(), 3, "the live estimate was lost");
            assert!(
                !accumulated.usage_is_reported(),
                "a receipt reports no output, so the count it shows is still an estimate"
            );
            let (_, _, _, usage) = accumulated.finish();
            assert_eq!(usage.completion_tokens, 3);
        }

        /// A server that does send a usage block is still believed over the receipt, since that
        /// one reports both halves.
        #[test]
        fn an_openai_usage_block_still_wins() {
            let mut accumulated = StreamAccumulator::new();
            accumulated.push(serde_json::from_str(r#"{"total_tokens":4530}"#).expect("a receipt"));
            accumulated.push(
                serde_json::from_str(
                    r#"{"choices":[],"usage":{"prompt_tokens":11,"completion_tokens":22}}"#,
                )
                .expect("a usage chunk"),
            );

            let (_, _, _, usage) = accumulated.finish();
            assert_eq!(usage.prompt_tokens, 11);
            assert_eq!(usage.completion_tokens, 22);
        }

        /// A streamed request must ask for usage too, or the turn would trade the cost figure for
        /// the live one.
        #[test]
        fn a_streamed_request_also_asks_for_usage() {
            let request = ChatRequest::new(DEFAULT_MODEL, vec![Message::user("hi")]).streamed();
            let json = serde_json::to_value(&request).unwrap();
            assert_eq!(json["stream"], true);
            assert_eq!(json["stream_options"]["include_usage"], true);
        }

        /// And an unstreamed one sends neither, since the server has its own default.
        #[test]
        fn an_unstreamed_request_mentions_neither() {
            let request = ChatRequest::new(DEFAULT_MODEL, vec![Message::user("hi")]);
            let json = serde_json::to_value(&request).unwrap();
            assert!(json.get("stream").is_none());
            assert!(json.get("stream_options").is_none());
        }

        fn decode(decoder: &mut SseDecoder, raw: &str) -> Vec<String> {
            decoder.push(raw.as_bytes())
        }

        #[test]
        fn data_lines_are_extracted() {
            let mut decoder = SseDecoder::new();
            let events = decode(&mut decoder, "data: {\"a\":1}\n\ndata: {\"b\":2}\n\n");
            assert_eq!(events, vec!["{\"a\":1}", "{\"b\":2}"]);
        }

        /// A payload split across reads must not be handed over early: half a JSON object would
        /// fail to parse and the text in it would be lost.
        #[test]
        fn a_payload_split_across_reads_waits_until_it_is_whole() {
            let mut decoder = SseDecoder::new();
            assert!(decode(&mut decoder, "data: {\"par").is_empty());
            assert!(decode(&mut decoder, "tial\":true}").is_empty());
            assert_eq!(decode(&mut decoder, "\n\n"), vec!["{\"partial\":true}"]);
        }

        /// Comments and blank lines are framing, not content, and carry nothing to accumulate.
        #[test]
        fn comments_and_blank_lines_are_ignored() {
            let mut decoder = SseDecoder::new();
            let events = decode(&mut decoder, ": keepalive\n\n\ndata: {\"real\":1}\n\n");
            assert_eq!(events, vec!["{\"real\":1}"]);
        }

        #[test]
        fn carriage_returns_are_tolerated() {
            let mut decoder = SseDecoder::new();
            assert_eq!(
                decode(&mut decoder, "data: {\"a\":1}\r\n\r\n"),
                vec!["{\"a\":1}"]
            );
        }

        fn chunk(raw: &str) -> ChatChunk {
            serde_json::from_str(raw).expect("a chunk")
        }

        /// Text arrives in pieces and must come out as the reply that was written, in order.
        #[test]
        fn text_chunks_accumulate_in_order() {
            let mut acc = StreamAccumulator::new();
            for text in ["Hel", "lo", " there"] {
                acc.push(chunk(&format!(
                    r#"{{"choices":[{{"delta":{{"content":"{text}"}}}}]}}"#
                )));
            }
            assert_eq!(acc.content(), "Hello there");
        }

        /// Until the server reports, the count is the number of text chunks: something has to move
        /// while the user waits.
        /// A server that reports no usage used to end the turn at zero, so a session whose
        /// screen had been counting output all along recorded that it had cost nothing. Zero
        /// reads as a measurement, and it was not one.
        #[test]
        fn a_reply_with_no_usage_report_still_says_what_it_cost() {
            let mut acc = StreamAccumulator::new();
            for _ in 0..3 {
                acc.push(
                    serde_json::from_str(r#"{"choices":[{"delta":{"content":"word "}}]}"#)
                        .expect("a chunk"),
                );
            }

            assert!(!acc.usage_is_reported(), "the server reported nothing");
            let live = acc.output_tokens();
            let (_, _, _, usage) = acc.finish();
            assert_eq!(
                usage.completion_tokens, live,
                "the turn ended with a different figure from the one on the screen"
            );
            assert!(usage.completion_tokens > 0);
        }

        /// Several models sold through this protocol put their working in `reasoning_content`
        /// beside the text rather than inside it. It is not the answer, nothing in this tree asks
        /// for it, and a decoder that folded it into the reply would put a model's private
        /// working on a person's screen and back into the next request as words it had spoken.
        #[test]
        fn reasoning_in_its_own_field_is_no_part_of_the_reply() {
            let mut acc = StreamAccumulator::new();
            acc.push(chunk(
                r#"{"choices":[{"delta":{"reasoning_content":"the user wants a file"}}]}"#,
            ));
            acc.push(chunk(r#"{"choices":[{"delta":{"content":"Hello"}}]}"#));

            assert_eq!(acc.content(), "Hello");
            assert_eq!(
                acc.output_tokens(),
                1,
                "a chunk carrying only reasoning counted as output written"
            );
        }

        #[test]
        fn output_tokens_climb_as_text_arrives() {
            let mut acc = StreamAccumulator::new();
            assert_eq!(acc.output_tokens(), 0);
            for n in 1..=3 {
                acc.push(chunk(r#"{"choices":[{"delta":{"content":"x"}}]}"#));
                assert_eq!(acc.output_tokens(), n);
            }
            assert!(!acc.usage_is_reported());
        }

        /// And the server's figure replaces the estimate rather than sitting beside it, so there is
        /// only ever one number and it ends up correct.
        #[test]
        fn the_reported_usage_replaces_the_estimate() {
            let mut acc = StreamAccumulator::new();
            for _ in 0..3 {
                acc.push(chunk(r#"{"choices":[{"delta":{"content":"x"}}]}"#));
            }
            assert_eq!(acc.output_tokens(), 3);

            acc.push(chunk(
                r#"{"choices":[],"usage":{"prompt_tokens":100,"completion_tokens":51}}"#,
            ));
            assert_eq!(acc.output_tokens(), 51);
            assert!(acc.usage_is_reported());
            assert_eq!(acc.usage().prompt_tokens, 100);
        }

        /// An empty content field is not a token written, or a stream of empty deltas would inflate
        /// the count.
        #[test]
        fn empty_text_does_not_count() {
            let mut acc = StreamAccumulator::new();
            acc.push(chunk(r#"{"choices":[{"delta":{"content":""}}]}"#));
            acc.push(chunk(r#"{"choices":[{"delta":{}}]}"#));
            assert_eq!(acc.output_tokens(), 0);
        }

        /// Tool arguments arrive a few characters at a time and have to be reassembled exactly, or
        /// the call would not parse as JSON.
        #[test]
        fn tool_call_fragments_are_reassembled() {
            let mut acc = StreamAccumulator::new();
            acc.push(chunk(
                r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"read_file","arguments":"{\"pa"}}]}}]}"#,
            ));
            acc.push(chunk(
                r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"th\":\"a.rs\"}"}}]}}]}"#,
            ));

            let calls = acc.tool_calls();
            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0].function.name, "read_file");
            assert_eq!(calls[0].id.as_deref(), Some("call_1"));
            assert_eq!(calls[0].arguments().expect("parses")["path"], "a.rs");
        }

        /// Several calls in one round are kept apart by index and come back in that order.
        #[test]
        fn concurrent_tool_calls_are_kept_separate_and_ordered() {
            let mut acc = StreamAccumulator::new();
            // Deliberately out of order, as a server may interleave them.
            acc.push(chunk(
                r#"{"choices":[{"delta":{"tool_calls":[{"index":1,"function":{"name":"second","arguments":"{}"}}]}}]}"#,
            ));
            acc.push(chunk(
                r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"name":"first","arguments":"{}"}}]}}]}"#,
            ));

            let calls = acc.tool_calls();
            assert_eq!(calls.len(), 2);
            assert_eq!(calls[0].function.name, "first");
            assert_eq!(calls[1].function.name, "second");
        }

        /// A fragment that never named a tool cannot be dispatched, so it is dropped rather than
        /// producing a call with an empty name.
        #[test]
        fn a_nameless_tool_fragment_is_dropped() {
            let mut acc = StreamAccumulator::new();
            acc.push(chunk(
                r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{}"}}]}}]}"#,
            ));
            assert!(acc.tool_calls().is_empty());
        }

        /// The model is reported once, on an early chunk, and later chunks must not blank it.
        #[test]
        fn the_model_is_kept_from_the_first_chunk_that_names_it() {
            let mut acc = StreamAccumulator::new();
            acc.push(chunk(
                r#"{"model":"some-model","choices":[{"delta":{"content":"a"}}]}"#,
            ));
            acc.push(chunk(r#"{"choices":[{"delta":{"content":"b"}}]}"#));
            assert_eq!(acc.model(), Some("some-model"));
        }

        /// A streamed round must end up indistinguishable from a buffered one.
        #[test]
        fn a_finished_stream_matches_what_a_whole_response_would_have_carried() {
            let mut acc = StreamAccumulator::new();
            acc.push(chunk(
                r#"{"model":"m","choices":[{"delta":{"content":"the "}}]}"#,
            ));
            acc.push(chunk(r#"{"choices":[{"delta":{"content":"answer"}}]}"#));
            acc.push(chunk(
                r#"{"choices":[{"finish_reason":"stop"}],"usage":{"prompt_tokens":9,"completion_tokens":2}}"#,
            ));

            let (content, model, calls, usage) = acc.finish();
            assert_eq!(content, "the answer");
            assert_eq!(model.as_deref(), Some("m"));
            assert!(calls.is_empty());
            assert_eq!(usage.completion_tokens, 2);
            assert_eq!(usage.total(), 11);
        }
    }

    /// The server may substitute a model, so the reported one must be readable and is
    /// not assumed to match the request.
    #[test]
    fn the_reported_model_is_preserved() {
        let raw = r#"{"model":"substituted-model","choices":[{"message":{"content":"x"}}]}"#;
        let parsed: ChatResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.model.as_deref(), Some("substituted-model"));
    }

    /// Unknown fields must not break decoding, or a server-side addition would take the
    /// client down.
    #[test]
    fn unknown_fields_are_ignored() {
        let raw = r#"{
            "model": "m",
            "id": "chatcmpl-123",
            "usage": {"prompt_tokens": 1, "completion_tokens": 2},
            "some_future_field": {"nested": true},
            "choices": [{"message": {"content": "hi"}, "index": 0}]
        }"#;
        let parsed: ChatResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.first_content().as_deref(), Some("hi"));
    }

    /// The same field on a reply that was not streamed, and the same reason: what the model
    /// answered is the text, and its working reached this process in a place of its own.
    #[test]
    fn a_reply_that_reasoned_in_its_own_field_answers_with_the_text_alone() {
        let raw = r#"{"choices":[{"message":{"content":"Hello","reasoning_content":"the user wants a file"}}]}"#;
        let parsed: ChatResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.first_content().as_deref(), Some("Hello"));
    }

    #[test]
    fn an_empty_choices_array_has_no_content() {
        let parsed: ChatResponse = serde_json::from_str(r#"{"choices":[]}"#).unwrap();
        assert!(parsed.first_content().is_none());
    }

    #[test]
    fn a_choice_without_content_has_no_content() {
        let raw = r#"{"choices":[{"message":{"role":"assistant"}}]}"#;
        let parsed: ChatResponse = serde_json::from_str(raw).unwrap();
        assert!(parsed.first_content().is_none());
    }

    #[test]
    fn tools_serialise_in_the_openai_function_shape() {
        let request = ChatRequest::new(DEFAULT_MODEL, vec![Message::user("hi")]).with_tools(vec![
            Tool::function(
                "read_file",
                "read a file",
                serde_json::json!({"type": "object", "properties": {"path": {"type": "string"}}}),
            ),
        ]);
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["tools"][0]["type"], "function");
        assert_eq!(json["tools"][0]["function"]["name"], "read_file");
        assert_eq!(json["tools"][0]["function"]["parameters"]["type"], "object");
    }

    /// An empty tool list is omitted rather than sent, since some servers reject [].
    #[test]
    fn an_empty_tool_list_is_omitted() {
        let request = ChatRequest::new(DEFAULT_MODEL, vec![Message::user("hi")]).with_tools(vec![]);
        let json = serde_json::to_value(&request).unwrap();
        assert!(json.get("tools").is_none());
    }

    #[test]
    fn tool_calls_are_parsed_from_a_response() {
        let raw = r#"{"choices":[{"message":{"role":"assistant","tool_calls":[
            {"id":"call_1","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"main.rs\"}"}}
        ]}}]}"#;
        let parsed: ChatResponse = serde_json::from_str(raw).unwrap();
        let calls = parsed.tool_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "read_file");
        assert_eq!(calls[0].arguments().unwrap()["path"], "main.rs");
    }

    #[test]
    fn a_response_without_tool_calls_has_none() {
        let raw = r#"{"choices":[{"message":{"content":"just text"}}]}"#;
        let parsed: ChatResponse = serde_json::from_str(raw).unwrap();
        assert!(parsed.tool_calls().is_empty());
    }

    /// Some models send empty or absent arguments for a no-argument tool.
    #[test]
    fn empty_tool_arguments_parse_as_an_empty_object() {
        let raw = r#"{"choices":[{"message":{"tool_calls":[
            {"function":{"name":"list","arguments":""}}
        ]}}]}"#;
        let parsed: ChatResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(
            parsed.tool_calls()[0].arguments().unwrap(),
            serde_json::json!({})
        );
    }

    #[test]
    fn a_missing_model_is_tolerated() {
        let raw = r#"{"choices":[{"message":{"content":"x"}}]}"#;
        let parsed: ChatResponse = serde_json::from_str(raw).unwrap();
        assert!(parsed.model.is_none());
        assert_eq!(parsed.first_content().as_deref(), Some("x"));
    }
    #[test]
    fn usage_is_parsed_when_the_server_reports_it() {
        let raw = r#"{"model":"m","usage":{"prompt_tokens":120,"completion_tokens":34},
            "choices":[{"message":{"content":"hi"}}]}"#;
        let parsed: ChatResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.usage().prompt_tokens, 120);
        assert_eq!(parsed.usage().completion_tokens, 34);
        assert_eq!(parsed.usage().total(), 154);
    }

    /// Not every server reports usage, and a missing count must read as zero rather than
    /// failing the response: the indicator is cosmetic, the reply is not.
    #[test]
    fn a_response_without_usage_reports_zero() {
        let raw = r#"{"model":"m","choices":[{"message":{"content":"hi"}}]}"#;
        let parsed: ChatResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.usage(), Usage::default());
        assert_eq!(parsed.usage().total(), 0);
    }

    /// A partial usage object must not fail either.
    #[test]
    fn a_partial_usage_object_parses() {
        let raw = r#"{"model":"m","usage":{"prompt_tokens":9},
            "choices":[{"message":{"content":"hi"}}]}"#;
        let parsed: ChatResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.usage().total(), 9);
    }

    /// A gateway that caches states how much of the prompt it did not read again. Reading it costs
    /// a field, and without it a session behind such a gateway cannot say whether caching is on.
    #[test]
    fn the_cache_figure_is_read_out_of_the_details_object() {
        let raw = r#"{"model":"m","usage":{"prompt_tokens":1200,"completion_tokens":34,
            "prompt_tokens_details":{"cached_tokens":1100}},
            "choices":[{"message":{"content":"hi"}}]}"#;
        let parsed: ChatResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.usage().cached.read_tokens, 1100);
        // Inside the prompt rather than beside it, so nothing about the total moved.
        assert_eq!(parsed.usage().prompt_tokens, 1200);
    }

    /// The field is the newer half of this protocol and plenty of servers state neither it nor the
    /// object holding it, so both absences have to read as a server that said nothing.
    #[test]
    fn a_usage_object_without_cache_figures_still_parses() {
        let raw = r#"{"model":"m","usage":{"prompt_tokens":9,"prompt_tokens_details":{}},
            "choices":[{"message":{"content":"hi"}}]}"#;
        let parsed: ChatResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.usage().cached, Cached::default());
        assert!(!parsed.usage().cached.any());
    }

    /// A server may state the field and put `null` in it, at either level. Reading the figure is
    /// worth a field and no more than that, so a null has to read as the silence it is rather than
    /// failing the reply it arrived on and with it the turn.
    #[test]
    fn a_null_cache_figure_reads_as_silence_rather_than_failing_the_reply() {
        for usage in [
            r#"{"prompt_tokens":9,"completion_tokens":1,"prompt_tokens_details":null}"#,
            r#"{"prompt_tokens":9,"completion_tokens":1,"prompt_tokens_details":{"cached_tokens":null}}"#,
        ] {
            let raw = format!(
                r#"{{"model":"m","usage":{usage},"choices":[{{"message":{{"content":"hi"}}}}]}}"#
            );
            let parsed: ChatResponse =
                serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{usage} was refused: {e}"));
            assert_eq!(parsed.usage().cached, Cached::default());
            // The reply is still read for everything else it stated.
            assert_eq!(parsed.usage().prompt_tokens, 9);
        }
    }

    /// Sessions on disk record `content` as a bare string. Reading one back has to keep working,
    /// or parts would have quietly orphaned every conversation anyone had already had.
    #[test]
    fn a_message_recorded_before_parts_existed_still_reads_back() {
        let raw = r#"{"role":"user","content":"make a game"}"#;
        let parsed: Message = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.content, Content::Text("make a game".into()));
    }

    /// And a message of words still goes out as a bare string, so nothing about the request the
    /// server sees changes for the conversations that carry no attachment.
    #[test]
    fn words_are_still_sent_as_a_bare_string() {
        let json = serde_json::to_string(&Message::user("hello")).unwrap();
        assert!(json.contains(r#""content":"hello""#), "{json}");
    }

    #[test]
    fn a_message_with_an_attachment_round_trips() {
        let message = Message::user_parts(vec![
            Part::Text {
                text: "what is this".into(),
            },
            Part::ImageUrl {
                image_url: ImageUrl {
                    url: "data:image/png;base64,AAAA".into(),
                },
            },
        ]);

        let json = serde_json::to_string(&message).unwrap();
        assert!(json.contains(r#""type":"image_url""#), "{json}");

        let back: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(back.content, message.content);
    }

    /// The transcript reads the words out of a message that carries an attachment. A data URI is
    /// not something anyone wants scrolling past.
    #[test]
    fn the_words_of_a_message_leave_out_its_attachments() {
        let message = Message::user_parts(vec![
            Part::Text {
                text: "what is this".into(),
            },
            Part::ImageUrl {
                image_url: ImageUrl {
                    url: "data:image/png;base64,AAAA".into(),
                },
            },
        ]);
        assert_eq!(message.content.text(), "what is this");
    }

    /// `as_text` answers `None` for a message carrying parts rather than an empty string, so a
    /// caller asking whether it starts with a marker cannot get a yes out of a message that has
    /// no text at all.
    #[test]
    fn a_message_carrying_parts_has_no_text_to_match_a_marker_against() {
        let message = Message::user_parts(vec![Part::ImageUrl {
            image_url: ImageUrl {
                url: "data:image/png;base64,AAAA".into(),
            },
        }]);
        assert_eq!(message.content.as_text(), None);
    }

    mod breakpoints {
        use super::*;
        use serde_json::json;

        fn picture() -> Part {
            Part::ImageUrl {
                image_url: ImageUrl {
                    url: "data:image/png;base64,AAAA".into(),
                },
            }
        }

        /// The two prefixes the next request will send again: the system prompt, which the tool
        /// schemas sit in front of, and the conversation up to the last thing the user said.
        #[test]
        fn the_system_prompt_and_the_last_thing_the_user_said_are_marked() {
            let request = ChatRequest::new(
                DEFAULT_MODEL,
                vec![
                    Message::system("be brief"),
                    Message::user("hi"),
                    Message::assistant("hello"),
                    Message::user("make a game"),
                ],
            );

            let messages = &request.marked_body().unwrap()["messages"];

            assert_eq!(
                messages[0]["content"],
                json!([{"type": "text", "text": "be brief", "cache_control": {"type": "ephemeral"}}])
            );
            assert_eq!(
                messages[3]["content"],
                json!([{"type": "text", "text": "make a game", "cache_control": {"type": "ephemeral"}}])
            );
            // Everything between them is the request it always was, down to the bare string.
            assert_eq!(messages[1]["content"], json!("hi"));
            assert_eq!(messages[2]["content"], json!("hello"));
        }

        /// A request that gives its conversation up once it answers has no next request to read the
        /// conversation's prefix back, and a cache write is charged above the tokens it covers. The
        /// prompt is the other prefix: the same bytes every time such a request is made, so it is
        /// marked as any other request's is.
        #[test]
        fn a_request_giving_up_its_conversation_marks_the_prompt_alone() {
            let request = ChatRequest::new(
                DEFAULT_MODEL,
                vec![
                    Message::system("summarise what you are shown"),
                    Message::user("hi"),
                    Message::assistant("hello"),
                    Message::user("summarise everything above"),
                ],
            )
            .giving_up_its_conversation();

            let messages = &request.marked_body().unwrap()["messages"];

            assert_eq!(
                messages[0]["content"][0]["cache_control"],
                json!({"type": "ephemeral"})
            );
            assert_eq!(messages[3]["content"], json!("summarise everything above"));
        }

        /// A round in the middle of a turn ends in a result rather than in what the person said.
        /// Marking one would mean sending a `tool` message as a list of blocks, which not every
        /// service reading this wire format accepts, and a service that rejects the shape costs
        /// the system prompt's breakpoint too. The prefix through the last user turn is cached
        /// from the first round of the turn regardless, which is the part worth having.
        #[test]
        fn a_result_the_assistant_asked_for_is_not_marked() {
            let request = ChatRequest::new(
                DEFAULT_MODEL,
                vec![
                    Message::system("be brief"),
                    Message::user("read it"),
                    Message::assistant_calling("", vec![]),
                    Message::tool_result("call-1", "the file"),
                ],
            );

            let messages = &request.marked_body().unwrap()["messages"];

            assert_eq!(
                messages[0]["content"][0]["cache_control"],
                json!({"type": "ephemeral"})
            );
            assert_eq!(messages[3]["content"], json!("the file"));
        }

        /// A breakpoint has to sit on a block, and what a service makes of one on a picture is not
        /// a thing to guess at. The words the person typed arrive in front of the attachment, so
        /// the block on the end is the picture.
        #[test]
        fn a_turn_ending_in_a_picture_is_left_unmarked() {
            let request = ChatRequest::new(
                DEFAULT_MODEL,
                vec![Message::user_parts(vec![
                    Part::Text {
                        text: "what is this".into(),
                    },
                    picture(),
                ])],
            );

            let messages = &request.marked_body().unwrap()["messages"];

            assert_eq!(
                messages[0]["content"],
                json!([
                    {"type": "text", "text": "what is this"},
                    {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}}
                ])
            );
        }

        /// And where the words are the last block, the mark goes on them and the picture beside
        /// them is sent exactly as it was.
        #[test]
        fn the_words_after_a_picture_carry_the_mark() {
            let request = ChatRequest::new(
                DEFAULT_MODEL,
                vec![Message::user_parts(vec![
                    picture(),
                    Part::Text {
                        text: "what is this".into(),
                    },
                ])],
            );

            let messages = &request.marked_body().unwrap()["messages"];

            assert_eq!(
                messages[0]["content"][1]["cache_control"],
                json!({"type": "ephemeral"})
            );
            assert_eq!(messages[0]["content"][0]["type"], json!("image_url"));
        }

        /// The marked body is this request and not a second one built beside it. A field added to a
        /// request must reach the service whether or not the prefix was marked, and the way that
        /// goes wrong is a parallel wire struct nobody remembered to extend.
        #[test]
        fn a_marked_body_changes_nothing_but_the_messages() {
            let request = ChatRequest::new(DEFAULT_MODEL, vec![Message::user("hi")])
                .with_tools(vec![Tool::function("read", "reads", json!({}))])
                .with_effort(Some(Effort::Xhigh))
                .streamed();

            let plain = serde_json::to_value(&request).unwrap();
            let marked = request.marked_body().unwrap();

            let plain = plain.as_object().unwrap();
            let marked = marked.as_object().unwrap();
            for (field, value) in plain {
                if field == "messages" {
                    continue;
                }
                assert_eq!(marked.get(field), Some(value), "{field} did not survive");
            }
            assert_eq!(
                marked.len(),
                plain.len(),
                "a field appeared or went missing"
            );
        }

        /// A marked message's blocks are the only thing built a second time, so they are where
        /// something can go missing: a block carries what the part it came from carried, and the
        /// mark is all that is new. Asserted against the serialization of the same content rather
        /// than against a literal, so a field a part gains and a marked block does not fails here
        /// instead of quietly stopping arriving.
        #[test]
        fn a_marked_block_is_the_block_it_came_from_and_the_mark() {
            let parts = vec![
                Part::Text {
                    text: "look".into(),
                },
                picture(),
                Part::Text {
                    text: "at this".into(),
                },
            ];
            let request = ChatRequest::new(DEFAULT_MODEL, vec![Message::user_parts(parts.clone())]);

            let mut expected = serde_json::to_value(&parts).unwrap();
            expected.as_array_mut().unwrap().last_mut().unwrap()["cache_control"] =
                json!({"type": "ephemeral"});

            assert_eq!(
                request.marked_body().unwrap()["messages"][0]["content"],
                expected
            );
        }

        /// An empty message carries no words to mark. Marking one would send a block a service is
        /// entitled to reject, in place of the empty string it accepts today.
        #[test]
        fn an_empty_turn_is_left_alone() {
            let request = ChatRequest::new(DEFAULT_MODEL, vec![Message::user("")]);
            let messages = &request.marked_body().unwrap()["messages"];
            assert_eq!(messages[0]["content"], json!(""));
        }
    }
}
