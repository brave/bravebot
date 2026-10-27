//! A chat completion in the shape `api.openai.com` answered with on 2026-10-27: the usage objects of
//! four replies, the envelope of a whole reply and of each chunk of a stream, and the bodies of the
//! refusals it answers a level and an unknown field with. BACKEND-19, BACKEND-22 and BACKEND-31 say
//! what is known of each. Ids and the text of a reply are placeholders, and a refusal's message is
//! cut to its first forty characters.
//!
//! The keys of the envelope are those of the `gpt-4o-mini` replies. A `gpt-5.6-luna` reply held no
//! `logprobs` key in its choice and a `system_fingerprint` of `null`, and the keys of a choice in a
//! stream chunk were not recorded, so the envelope gives every model the `gpt-4o-mini` choice and
//! a placeholder fingerprint. The usage figures are those of the streamed requests; the same
//! prompts not streamed reported 1675 and 1672 prompt tokens.
#![allow(dead_code, reason = "each file that includes this one uses some of it")]

/// One usage object, with the figures this program reports for it.
pub struct UsageShape {
    /// The model that sent it, which is the model of the reply and of each chunk.
    pub model: &'static str,
    /// What the shape is, for a failure to name.
    pub what: &'static str,
    /// The `usage` object as it is sent.
    pub json: &'static str,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    /// `prompt_tokens_details.cached_tokens`.
    pub read_tokens: u64,
    /// `prompt_tokens_details.cache_write_tokens`, 0 where the key is not sent.
    pub written_tokens: u64,
}

/// What `gpt-4o-mini`, which states no write, and `gpt-5.6-luna`, one of eight ids that do,
/// reported in streamed requests for a prompt of about 1670 tokens sent for the first time and
/// again about nine seconds after the first reply ended. The cold request of the second reports a write of nearly the whole prompt with nothing
/// read, and the repeat reports the same number read and a write of 0 that is present in the
/// object. `gpt-6-luna` was sent the pair as well and the other six ids that state a write were
/// not.
pub const USAGE_SHAPES: [UsageShape; 4] = [
    UsageShape {
        model: "gpt-4o-mini-2024-07-18",
        what: "a model that states no write, on a cold prompt",
        json: r#"{"prompt_tokens":1671,"completion_tokens":1,"total_tokens":1672,
            "prompt_tokens_details":{"cached_tokens":0,"audio_tokens":0},
            "completion_tokens_details":{"reasoning_tokens":0,"audio_tokens":0,
                "accepted_prediction_tokens":0,"rejected_prediction_tokens":0}}"#,
        prompt_tokens: 1671,
        completion_tokens: 1,
        read_tokens: 0,
        written_tokens: 0,
    },
    UsageShape {
        model: "gpt-4o-mini-2024-07-18",
        what: "a model that states no write, on the repeat of that prompt",
        json: r#"{"prompt_tokens":1671,"completion_tokens":1,"total_tokens":1672,
            "prompt_tokens_details":{"cached_tokens":1536,"audio_tokens":0},
            "completion_tokens_details":{"reasoning_tokens":0,"audio_tokens":0,
                "accepted_prediction_tokens":0,"rejected_prediction_tokens":0}}"#,
        prompt_tokens: 1671,
        completion_tokens: 1,
        read_tokens: 1536,
        written_tokens: 0,
    },
    UsageShape {
        model: "gpt-5.6-luna",
        what: "a model that states a write, on a cold prompt",
        json: r#"{"prompt_tokens":1673,"completion_tokens":4,"total_tokens":1677,
            "prompt_tokens_details":{"cached_tokens":0,"audio_tokens":0,"cache_write_tokens":1670},
            "completion_tokens_details":{"reasoning_tokens":0,"audio_tokens":0,
                "accepted_prediction_tokens":0,"rejected_prediction_tokens":0}}"#,
        prompt_tokens: 1673,
        completion_tokens: 4,
        read_tokens: 0,
        written_tokens: 1670,
    },
    UsageShape {
        model: "gpt-5.6-luna",
        what: "a model that states a write, on the repeat of that prompt",
        json: r#"{"prompt_tokens":1673,"completion_tokens":4,"total_tokens":1677,
            "prompt_tokens_details":{"cached_tokens":1670,"audio_tokens":0,"cache_write_tokens":0},
            "completion_tokens_details":{"reasoning_tokens":0,"audio_tokens":0,
                "accepted_prediction_tokens":0,"rejected_prediction_tokens":0}}"#,
        prompt_tokens: 1673,
        completion_tokens: 4,
        read_tokens: 1670,
        written_tokens: 0,
    },
];

/// The words of every reply here.
pub const WORD: &str = "ok";

/// The same JSON on one line, which is how a payload of an event stream is written.
fn on_one_line(json: &str) -> String {
    serde_json::from_str::<serde_json::Value>(json)
        .expect("the shape is JSON")
        .to_string()
}

/// A reply that was not streamed, carrying `usage`.
pub fn whole_reply(model: &str, usage: &str) -> String {
    on_one_line(&format!(
        r#"{{"id":"chatcmpl-placeholder","object":"chat.completion","created":1793118000,
        "model":"{model}",
        "choices":[{{"index":0,"message":{{"role":"assistant","content":"{WORD}","refusal":null,
            "annotations":[]}},"logprobs":null,"finish_reason":"stop"}}],
        "usage":{usage},"service_tier":"default","system_fingerprint":"fp_placeholder"}}"#
    ))
}

/// The payloads of a stream asked for with `stream_options.include_usage`, in the order they
/// arrive, before `[DONE]`: a chunk that carries the role, one that carries the words, one that
/// carries the finish reason and an empty delta, and one with no choices at all that carries
/// `usage`. Every chunk carries the keys of the first, and `usage` is `null` on the first three.
pub fn stream_chunks(model: &str, usage: &str) -> Vec<String> {
    let chunk = |choices: &str, usage: &str| {
        on_one_line(&format!(
            r#"{{"id":"chatcmpl-placeholder","object":"chat.completion.chunk","created":1793118000,
            "model":"{model}","service_tier":"default","system_fingerprint":"fp_placeholder",
            "choices":{choices},"usage":{usage},"obfuscation":"placeholder"}}"#
        ))
    };
    vec![
        chunk(
            r#"[{"index":0,"delta":{"role":"assistant","content":"","refusal":null},
                "logprobs":null,"finish_reason":null}]"#,
            "null",
        ),
        chunk(
            &format!(
                r#"[{{"index":0,"delta":{{"content":"{WORD}"}},"logprobs":null,"finish_reason":null}}]"#
            ),
            "null",
        ),
        chunk(
            r#"[{"index":0,"delta":{},"logprobs":null,"finish_reason":"stop"}]"#,
            "null",
        ),
        chunk("[]", usage),
    ]
}

/// The refusal of a level by a model that does not reason: 400, and neither a code nor a param.
pub const LEVEL_TO_A_MODEL_THAT_DOES_NOT_REASON: &str = r#"{"error":{
    "message":"Unrecognized request argument supplied: ",
    "type":"invalid_request_error","param":null,"code":null}}"#;

/// The refusal by a reasoning model of a level it does not take (`xhigh` on `gpt-5-mini`): 400,
/// the code `unsupported_value` and the param `reasoning_effort`.
pub const A_LEVEL_THE_MODEL_DOES_NOT_TAKE: &str = r#"{"error":{
    "message":"Unsupported value: 'reasoning_effort' do",
    "type":"invalid_request_error","param":"reasoning_effort","code":"unsupported_value"}}"#;

/// The refusal of a field the service does not know, which is the same status as the refusal of a
/// level and names the field in its param.
pub const A_FIELD_THE_SERVICE_DOES_NOT_KNOW: &str = r#"{"error":{
    "message":"Unknown parameter: 'probe_unknown_field'",
    "type":"invalid_request_error","param":"probe_unknown_field","code":"unknown_parameter"}}"#;
