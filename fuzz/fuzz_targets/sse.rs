//! `bravebot_aichat::protocol::SseDecoder` and what its payloads feed: `ChatChunk` and the
//! accumulator that builds a reply from them.
//!
//! The decoder is built with a limit far below the sixteen mebibytes every caller gets, which no
//! input the fuzzer builds could reach, so the property that the decoder holds at most the limit
//! would never be tested. Half the inputs get a limit of a few dozen bytes, which reaches the
//! limit from short lines, and half get one from 128 bytes to a few kibibytes, which most generated
//! chunks fit inside, so that they are read and the accumulator sees them.

#![no_main]
#![forbid(unsafe_code)]

use arbitrary::Arbitrary;
use bravebot_aichat::protocol::{ChatChunk, STREAM_DONE, SseDecoder, StreamAccumulator, Usage};
use bravebot_fuzz::splits::pieces;
use libfuzzer_sys::fuzz_target;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};

#[derive(Arbitrary, Debug)]
struct Input {
    small: bool,
    limit: u8,
    stream: Stream,
    cuts: Vec<u8>,
}

/// How a line ends. A line that has no terminator is the last one, which `tail` holds.
#[derive(Arbitrary, Debug, Clone, Copy, PartialEq, Eq)]
enum Terminator {
    Lf,
    Crlf,
}

#[derive(Arbitrary, Debug)]
enum Stream {
    Raw(Vec<u8>),
    /// Lines with a known meaning, so that what the decoder should hand over is known too.
    Lines {
        lines: Vec<Line>,
        /// Bytes after the last newline, which are held and never handed over.
        tail: Vec<u8>,
    },
}

#[derive(Arbitrary, Debug)]
struct Line {
    kind: Kind,
    ending: Terminator,
}

#[derive(Arbitrary, Debug)]
enum Kind {
    Data(String),
    Chunk(ChunkSpec),
    Done,
    Comment(String),
    Event(String),
    Blank,
    Junk(Vec<u8>),
}

#[derive(Arbitrary, Debug)]
struct ChunkSpec {
    model: Option<String>,
    choices: Vec<ChoiceSpec>,
    usage: Option<(u16, u16)>,
    total_tokens: Option<u32>,
}

#[derive(Arbitrary, Debug)]
struct ChoiceSpec {
    content: Option<String>,
    reasoning: Option<String>,
    calls: Vec<CallSpec>,
    finish: Option<String>,
}

#[derive(Arbitrary, Debug)]
struct CallSpec {
    index: u8,
    id: Option<String>,
    name: Option<String>,
    arguments: Option<String>,
}

impl ChunkSpec {
    fn json(&self) -> String {
        let choices: Vec<Value> = self
            .choices
            .iter()
            .map(|choice| {
                let calls: Vec<Value> = choice
                    .calls
                    .iter()
                    .map(|call| {
                        json!({
                            "index": call.index % 3,
                            "id": call.id,
                            "function": { "name": call.name, "arguments": call.arguments },
                        })
                    })
                    .collect();
                let mut delta =
                    json!({ "content": choice.content, "reasoning_content": choice.reasoning });
                if !calls.is_empty() {
                    delta["tool_calls"] = Value::Array(calls);
                }
                json!({ "delta": delta, "finish_reason": choice.finish })
            })
            .collect();
        let usage = self
            .usage
            .map(|(prompt, completion)| json!({ "prompt_tokens": prompt, "completion_tokens": completion }));
        json!({
            "model": self.model,
            "choices": choices,
            "usage": usage,
            "total_tokens": self.total_tokens,
        })
        .to_string()
    }
}

/// A line with its newline and carriage returns removed, which would end it where it is not meant
/// to end.
fn one_line(text: &str) -> String {
    text.chars().filter(|c| !matches!(c, '\n' | '\r')).collect()
}

impl Line {
    /// The bytes of the line without its newline, and the payload the decoder should find in it.
    fn render(&self) -> (Vec<u8>, Option<String>) {
        let (mut bytes, payload) = match &self.kind {
            Kind::Data(text) => {
                let text = one_line(text);
                let payload = Some(text.trim().to_string()).filter(|p| !p.is_empty());
                (format!("data: {text}").into_bytes(), payload)
            }
            Kind::Chunk(spec) => {
                let json = spec.json();
                (format!("data: {json}").into_bytes(), Some(json))
            }
            Kind::Done => (
                format!("data: {STREAM_DONE}").into_bytes(),
                Some(STREAM_DONE.to_string()),
            ),
            Kind::Comment(text) => (format!(": {}", one_line(text)).into_bytes(), None),
            Kind::Event(text) => (format!("event: {}", one_line(text)).into_bytes(), None),
            Kind::Blank => (Vec::new(), None),
            // Starts with a letter so that it is never a `data:` line, whatever follows.
            Kind::Junk(junk) => {
                let mut bytes = b"x".to_vec();
                bytes.extend(junk.iter().copied().filter(|b| *b != b'\n'));
                (bytes, None)
            }
        };
        if self.ending == Terminator::Crlf {
            bytes.push(b'\r');
        }
        (bytes, payload)
    }
}

/// Feed `stream` in the reads `cuts` makes of it, checking after each that no more than `limit`
/// bytes are held.
fn decode(stream: &[u8], cuts: &[u8], limit: usize) -> Vec<String> {
    let mut decoder = SseDecoder::with_limit(limit);
    let mut payloads = Vec::new();
    for read in pieces(stream, cuts) {
        payloads.extend(decoder.push(read));
        assert!(
            decoder.held() <= limit,
            "held {} bytes at a limit of {limit}",
            decoder.held()
        );
    }
    payloads
}

/// What a reply built from chunks should be, worked out from the chunks themselves.
#[derive(Default)]
struct Reply {
    content: String,
    thinking: String,
    /// Each call by the index the server gave it, and the indexes in the order they first came.
    calls: BTreeMap<usize, (String, String)>,
    order: Vec<usize>,
    model: Option<String>,
    usage: Option<Usage>,
    receipt: Option<u64>,
    content_chunks: u64,
    argument_chunks: u64,
    ended: bool,
}

impl Reply {
    fn read(&mut self, chunk: &ChatChunk) {
        if self.model.is_none() {
            self.model = chunk.model.clone();
        }
        if chunk.usage.is_some() {
            self.usage = chunk.usage;
        }
        if chunk.total_tokens.is_some() {
            self.receipt = chunk.total_tokens;
        }
        for choice in &chunk.choices {
            // A finish reason is one of the two things that say a reply is over.
            self.ended |= choice.finish_reason.is_some();
            let Some(delta) = &choice.delta else { continue };
            if let Some(text) = delta.content.as_deref().filter(|text| !text.is_empty()) {
                self.content.push_str(text);
                self.content_chunks += 1;
            }
            self.thinking.extend(delta.reasoning_content.as_deref());
            for fragment in delta.tool_calls.iter().flatten() {
                if !self.calls.contains_key(&fragment.index) {
                    self.order.push(fragment.index);
                }
                let call = self.calls.entry(fragment.index).or_default();
                let Some(function) = &fragment.function else {
                    continue;
                };
                call.0.extend(function.name.as_deref());
                if let Some(arguments) = function.arguments.as_deref().filter(|a| !a.is_empty()) {
                    call.1.push_str(arguments);
                    self.argument_chunks += 1;
                }
            }
        }
    }

    /// What the reply cost: the server's figure where it gave one, and where it did not, the input
    /// it reported in its receipt and the chunks of text as the output.
    fn usage(&self) -> Usage {
        self.usage.unwrap_or(Usage {
            prompt_tokens: self.receipt.unwrap_or_default(),
            completion_tokens: self.content_chunks,
            ..Usage::default()
        })
    }

    /// Calls come in the order the server indexed them, and one that was never named is dropped.
    fn named_calls(&self) -> Vec<(String, String)> {
        self.calls
            .values()
            .filter(|(name, _)| !name.is_empty())
            .cloned()
            .collect()
    }
}

/// Fold every payload that is a `ChatChunk` into an accumulator, and hold what it reports to what
/// the chunks themselves say. `built` are the payloads known to be chunks.
fn accumulate(payloads: &[String], built: &HashSet<String>) {
    let mut accumulator = StreamAccumulator::new();
    let mut reply = Reply::default();
    for payload in payloads {
        if payload == STREAM_DONE {
            accumulator.mark_ended();
            reply.ended = true;
            continue;
        }
        let chunk = match serde_json::from_str::<ChatChunk>(payload) {
            Ok(chunk) => chunk,
            Err(error) => {
                assert!(
                    !built.contains(payload),
                    "a well-formed chunk was refused: {error}"
                );
                continue;
            }
        };
        reply.read(&chunk);
        accumulator.push(chunk);
    }

    assert_eq!(accumulator.content(), reply.content);
    assert_eq!(accumulator.ended(), reply.ended);
    assert_eq!(accumulator.model(), reply.model.as_deref());
    assert_eq!(accumulator.usage(), reply.usage());
    // The server's own count once it has given one, and until then the chunks that carried text or
    // a piece of an argument.
    assert_eq!(
        accumulator.output_tokens(),
        reply
            .usage
            .map_or(reply.content_chunks + reply.argument_chunks, |usage| usage
                .completion_tokens)
    );
    let calling = reply
        .order
        .last()
        .map(|index| reply.calls[index].0.as_str())
        .filter(|name| !name.is_empty());
    assert_eq!(accumulator.calling(), calling);

    let named = |calls: Vec<bravebot_aichat::protocol::ToolCall>| -> Vec<(String, String)> {
        calls
            .into_iter()
            .map(|call| {
                (
                    call.function.name,
                    call.function.arguments.unwrap_or_default(),
                )
            })
            .collect()
    };
    assert_eq!(named(accumulator.tool_calls()), reply.named_calls());

    // The thinking that arrived is handed over once.
    assert_eq!(accumulator.take_thinking(), reply.thinking);
    assert_eq!(accumulator.take_thinking(), "");

    let (content, model, calls, usage) = accumulator.finish();
    assert_eq!(content, reply.content);
    assert_eq!(model, reply.model);
    assert_eq!(named(calls), reply.named_calls());
    assert_eq!(usage, reply.usage());
}

fuzz_target!(|input: Input| {
    let limit = if input.small {
        1 + usize::from(input.limit % 48)
    } else {
        128 + usize::from(input.limit) * 16
    };
    let (stream, expected, built) = match input.stream {
        Stream::Raw(bytes) => (bytes, None, HashSet::new()),
        Stream::Lines { lines, tail } => {
            let mut stream = Vec::new();
            let mut expected = Vec::new();
            let mut built = HashSet::new();
            // A line over the limit ends the decoding, so what follows it is not read.
            let mut reading = true;
            for line in &lines {
                let (bytes, payload) = line.render();
                if reading && bytes.len() > limit {
                    reading = false;
                }
                if reading {
                    if let Kind::Chunk(_) = line.kind {
                        built.extend(payload.clone());
                    }
                    expected.extend(payload);
                }
                stream.extend(&bytes);
                stream.push(b'\n');
            }
            stream.extend(tail.iter().filter(|b| **b != b'\n'));
            (stream, Some(expected), built)
        }
    };

    let whole = decode(&stream, &[], limit);
    let split = decode(&stream, &input.cuts, limit);
    assert_eq!(whole, split, "the reads changed the payloads");
    for payload in &whole {
        assert!(
            !payload.is_empty() && payload.trim() == payload && !payload.contains('\n'),
            "{payload:?}"
        );
    }
    if let Some(expected) = expected {
        assert_eq!(whole, expected);
    }
    accumulate(&whole, &built);
});
