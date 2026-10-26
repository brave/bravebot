//! The agent's conversation as turns of typed blocks, before a wire format spells them.
//!
//! The conversation is held in OpenAI-compatible types, because that is what the aichat endpoint
//! speaks. Two services here take a different shape, in which a turn is a list of blocks and a tool
//! call and its result are blocks inside user and assistant turns: AWS Bedrock's Converse API and
//! Anthropic's Messages API. They name every block differently and agree on what this module
//! decides, so it is decided once and each wire spells the blocks:
//!
//! - The system prompt is hoisted out of the message list and its turns are joined in order.
//! - A tool result is a block on a user turn, and consecutive turns that would now share a role are
//!   merged, because both services reject two turns of one role in a row.
//! - Arguments cross as a JSON object where the agent carries a string holding one.
//! - A request offering no tools sends each call and each result as text, because both services
//!   refuse a call or result block in a request that declares no tool.
//! - Nothing is sent that cannot be matched or read: a result with no call id is dropped, text with
//!   no word in it is no block, a call's arguments are an object whatever the model wrote, and an
//!   attachment is a block only where it is a `data:` URI the wire takes.
//!
//! Nothing here inspects content to make a decision. Text is moved between shapes and handed on with
//! whatever label it arrived under.

use crate::protocol::{Content, Message, Part, Role};
use serde_json::{Value, json};

/// How a wire names the blocks of a turn.
///
/// What a block looks like on the wire is the wire's business, so every constructor is the wire's.
/// Which blocks there are, in what order, and which turn each belongs to is this module's.
pub trait Spelling {
    /// One block of a turn, as the wire represents it.
    type Block;

    /// A block of words. Never called with text that is empty or only whitespace, which neither
    /// service takes.
    fn text(&self, text: &str) -> Self::Block;

    /// A block for an attachment, or `None` for a media type the wire does not take.
    ///
    /// `data` is the base64 the data URI carried, never empty. `position` is the attachment's place
    /// in the turn, counting from one, which is the only name a document can be given: a data URI
    /// carries no filename.
    fn attachment(&self, media_type: &str, data: &str, position: usize) -> Option<Self::Block>;

    /// A call the assistant made, with its arguments as the object they are.
    fn call(&self, id: &str, name: &str, input: Value) -> Self::Block;

    /// The result of a call, answering it by id.
    fn result(&self, id: &str, text: String) -> Self::Block;
}

/// One turn of the conversation as the wire states it.
#[derive(Debug, Clone, PartialEq)]
pub struct Turn<B> {
    /// `user` or `assistant`, the two words both services use.
    pub role: &'static str,
    pub blocks: Vec<B>,
}

/// A conversation walked into the shape both block-based wires share.
#[derive(Debug, Clone, PartialEq)]
pub struct Walked<B> {
    /// The system turns joined in order, where there was at least one.
    pub system: Option<String>,
    pub turns: Vec<Turn<B>>,
}

/// Restate `messages` as turns of blocks, spelled by `spelling`.
///
/// `offers_tools` is whether the request declares any tool: where it does not, calls and results are
/// written as text instead of as blocks.
pub fn walk<W: Spelling>(
    spelling: &W,
    messages: &[Message],
    offers_tools: bool,
) -> Walked<W::Block> {
    let calls_as_text = !offers_tools;

    let mut system: Vec<String> = Vec::new();
    let mut turns: Vec<Turn<W::Block>> = Vec::new();
    // Whether the turn being built ends in text a person or the agent wrote. A user message that
    // follows it joins the same turn, and a reader that lays the blocks of a turn end to end would
    // run the two together: a count at the start of a note read as the last digits of a number at
    // the end of the line before it.
    let mut ends_in_text = false;

    for message in messages {
        match message.role {
            // Hoisted rather than sent as a turn. Several may accumulate over a session, and they
            // are joined in order, which is the order they would have been read in.
            Role::System => system.push(message.content.text()),
            Role::User => {
                let blocks = user_blocks(spelling, message, ends_in_text);
                if !blocks.is_empty() {
                    ends_in_text = last_text_is_a_word(message);
                }
                push(&mut turns, "user", blocks);
            }
            Role::Tool => {
                let blocks = result_blocks(spelling, message, calls_as_text);
                ends_in_text &= blocks.is_empty();
                push(&mut turns, "user", blocks);
            }
            Role::Assistant => {
                let blocks = assistant_blocks(spelling, message, calls_as_text);
                ends_in_text &= blocks.is_empty();
                push(&mut turns, "assistant", blocks);
            }
        }
    }

    Walked {
        system: (!system.is_empty()).then(|| system.join("\n\n")),
        turns,
    }
}

/// Add blocks to the conversation, merging into the previous turn when the role repeats.
///
/// Both services refuse two consecutive turns of the same role, and this conversion creates them:
/// two tool results in a row were two `Role::Tool` messages, and both become user turns.
fn push<B>(turns: &mut Vec<Turn<B>>, role: &'static str, blocks: Vec<B>) {
    if blocks.is_empty() {
        return;
    }
    match turns.last_mut() {
        Some(last) if last.role == role => last.blocks.extend(blocks),
        _ => turns.push(Turn { role, blocks }),
    }
}

/// Whether the last thing in a user message is text with a word in it, which is what a block
/// written after it would be read straight on from.
fn last_text_is_a_word(message: &Message) -> bool {
    match &message.content {
        Content::Text(text) => !text.trim().is_empty(),
        Content::Parts(parts) => parts
            .iter()
            .rev()
            .find_map(|part| match part {
                Part::Text { text } if !text.trim().is_empty() => Some(true),
                Part::Text { .. } => None,
                Part::ImageUrl { .. } => Some(false),
            })
            .unwrap_or(false),
    }
}

/// The blocks of a user message. `after_text` is whether the turn it joins already ends in text,
/// in which case the first text block starts on a new paragraph.
fn user_blocks<W: Spelling>(spelling: &W, message: &Message, after_text: bool) -> Vec<W::Block> {
    let mut separate = after_text;
    let mut paragraph = |text: &str| -> Vec<W::Block> {
        if text.trim().is_empty() {
            return Vec::new();
        }
        match std::mem::take(&mut separate) {
            true => text_block(spelling, &format!("\n\n{text}")),
            false => text_block(spelling, text),
        }
    };
    match &message.content {
        Content::Text(text) => paragraph(text),
        Content::Parts(parts) => {
            let mut blocks = Vec::new();
            let mut attachments = 0;
            for part in parts {
                match part {
                    // Through the same guard a whole turn's text goes through: a part carrying no
                    // text is an empty block, which neither service takes.
                    Part::Text { text } => blocks.extend(paragraph(text)),
                    // A data URI, which is the only form attachments take here:
                    // `data:<media>;base64,<data>`. Anything else is dropped rather than sent as a
                    // link, because asking the service to fetch a URL is an effect nobody endorsed.
                    Part::ImageUrl { image_url } => {
                        attachments += 1;
                        blocks.extend(attachment_block(spelling, &image_url.url, attachments));
                    }
                }
            }
            blocks
        }
    }
}

fn result_blocks<W: Spelling>(spelling: &W, message: &Message, as_text: bool) -> Vec<W::Block> {
    let Some(id) = message.tool_call_id.as_deref() else {
        // A result with no id cannot be matched to its call, and the service rejects one. Sent as
        // plain text it would read as something the user said, so it is dropped.
        return Vec::new();
    };
    if as_text {
        return vec![spelling.text(&format!(
            "[result of call {id}]\n{}",
            message.content.text()
        ))];
    }
    vec![spelling.result(id, message.content.text())]
}

fn assistant_blocks<W: Spelling>(
    spelling: &W,
    message: &Message,
    calls_as_text: bool,
) -> Vec<W::Block> {
    let mut blocks = text_block(spelling, &message.content.text());
    for call in message.tool_calls.iter().flatten() {
        if calls_as_text {
            blocks.push(spelling.text(&format!(
                "[call {}: {} {}]",
                call.id, call.function.name, call.function.arguments
            )));
            continue;
        }
        // Arguments cross as a string in the agent's protocol and as an object here. A string
        // that is not an object, whether it is not JSON or is `null` or a list, becomes an empty
        // object: both services take nothing else there, and the call is preserved so it can still
        // be answered, which keeps the conversation well-formed.
        let input = serde_json::from_str::<Value>(&call.function.arguments)
            .ok()
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({}));
        blocks.push(spelling.call(&call.id, &call.function.name, input));
    }
    blocks
}

/// A text block, or nothing at all for text with no word in it.
///
/// The services reject a text block that is empty or only whitespace, and an assistant turn that
/// only asked for tools has no text, or has the blank lines another model wrote before a call.
fn text_block<W: Spelling>(spelling: &W, text: &str) -> Vec<W::Block> {
    if text.trim().is_empty() {
        Vec::new()
    } else {
        vec![spelling.text(text)]
    }
}

/// An attachment block from a data URI, or nothing if it is not one the wire takes.
fn attachment_block<W: Spelling>(spelling: &W, url: &str, position: usize) -> Option<W::Block> {
    let rest = url.strip_prefix("data:")?;
    let (media_type, data) = rest.split_once(";base64,")?;
    if data.is_empty() {
        return None;
    }
    spelling.attachment(media_type, data, position)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ImageUrl, ToolCallRequest, ToolCallRequestFunction};

    /// Blocks written as the words a test reads them in, so what is asserted is which block there
    /// is and not how a wire would draw it.
    struct Plain;

    impl Spelling for Plain {
        type Block = String;

        fn text(&self, text: &str) -> String {
            format!("text:{text}")
        }

        fn attachment(&self, media_type: &str, data: &str, position: usize) -> Option<String> {
            (media_type == "image/png").then(|| format!("image#{position}:{data}"))
        }

        fn call(&self, id: &str, name: &str, input: Value) -> String {
            format!("call:{id}:{name}:{input}")
        }

        fn result(&self, id: &str, text: String) -> String {
            format!("result:{id}:{text}")
        }
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

    fn walked(messages: &[Message], offers_tools: bool) -> Walked<String> {
        walk(&Plain, messages, offers_tools)
    }

    fn picture(url: &str) -> Part {
        Part::ImageUrl {
            image_url: ImageUrl {
                url: url.to_string(),
            },
        }
    }

    /// Several system turns accumulate over a session, and they are read in the order they were
    /// given. A request with none has no system prompt, which is not an empty one.
    #[test]
    fn system_turns_are_hoisted_and_joined_in_order() {
        let walked = walked(
            &[
                Message::system("first"),
                Message::user("hello"),
                Message::system("second"),
            ],
            true,
        );
        assert_eq!(walked.system.as_deref(), Some("first\n\nsecond"));
        assert_eq!(walked.turns.len(), 1);
        assert_eq!(walked.turns[0].role, "user");

        assert_eq!(walk(&Plain, &[Message::user("hi")], true).system, None);
    }

    /// A note written after the line a person typed shares its turn on the wire, and starts on a
    /// paragraph of its own. Laid end to end with nothing between, "issues/1106" followed by "16
    /// skills are not listed" read as the issue number 110616.
    #[test]
    fn a_user_message_after_user_text_starts_a_new_paragraph() {
        let walked = walked(
            &[
                Message::user("see issues/1106"),
                Message::user("16 skills are not listed"),
            ],
            true,
        );
        assert_eq!(walked.turns.len(), 1);
        assert_eq!(
            walked.turns[0].blocks,
            ["text:see issues/1106", "text:\n\n16 skills are not listed"]
        );
    }

    /// Both services refuse two turns of one role in a row, and the conversion is what makes them:
    /// a result is a user turn, so the two results of two parallel calls are one turn of two
    /// blocks, and so is a result followed by the next thing the user typed.
    #[test]
    fn consecutive_turns_of_one_role_merge() {
        let walked = walked(
            &[
                Message::assistant_calling(
                    "",
                    vec![calling("a", "f", "{}"), calling("b", "g", "{}")],
                ),
                Message::tool_result("a", "one"),
                Message::tool_result("b", "two"),
                Message::user("next"),
            ],
            true,
        );
        assert_eq!(walked.turns.len(), 2);
        assert_eq!(walked.turns[0].role, "assistant");
        assert_eq!(walked.turns[1].role, "user");
        assert_eq!(
            walked.turns[1].blocks,
            ["result:a:one", "result:b:two", "text:next"]
        );
    }

    /// A result is matched to its call by id. One without an id matches nothing, and sent as a user
    /// turn it would read as something the person said.
    #[test]
    fn a_result_without_a_call_id_is_dropped_rather_than_sent_as_a_user_turn() {
        let mut orphan = Message::tool_result("x", "output");
        orphan.tool_call_id = None;
        let walked = walked(&[Message::user("go"), orphan], true);
        assert_eq!(walked.turns.len(), 1);
        assert_eq!(walked.turns[0].blocks, ["text:go"]);
    }

    /// The call is kept whatever the model wrote for its arguments, so the result that answers it
    /// still has a call to answer. A string that is not JSON becomes an empty object.
    #[test]
    fn tool_arguments_cross_from_a_string_to_an_object_and_a_bad_string_to_an_empty_one() {
        let walked = walked(
            &[Message::assistant_calling(
                "",
                vec![
                    calling("a", "f", r#"{"path":"x"}"#),
                    calling("b", "g", "not json"),
                ],
            )],
            true,
        );
        assert_eq!(
            walked.turns[0].blocks,
            [r#"call:a:f:{"path":"x"}"#, "call:b:g:{}"]
        );
    }

    /// A request that offers no tool is refused if it carries a call or a result block, so the
    /// history is told in words instead, with the id that links each result to its call.
    #[test]
    fn a_request_offering_no_tools_sends_its_calls_and_results_as_text() {
        let walked = walked(
            &[
                Message::assistant_calling("looking", vec![calling("a", "f", r#"{"k":1}"#)]),
                Message::tool_result("a", "found"),
            ],
            false,
        );
        assert_eq!(
            walked.turns[0].blocks,
            ["text:looking", r#"text:[call a: f {"k":1}]"#]
        );
        assert_eq!(walked.turns[1].blocks, ["text:[result of call a]\nfound"]);
    }

    /// Neither service takes an empty text block, and an assistant turn that only asked for tools has
    /// no text to send.
    #[test]
    fn empty_text_is_no_block_and_a_turn_of_nothing_is_no_turn() {
        let walked = walked(
            &[
                Message::user("hello"),
                Message::assistant(""),
                Message::user_parts(vec![Part::Text {
                    text: String::new(),
                }]),
            ],
            true,
        );
        assert_eq!(walked.turns.len(), 1);
        assert_eq!(walked.turns[0].blocks, ["text:hello"]);
    }

    /// Neither service takes a text block with no word in it. Another model's blank lines before a
    /// call, a command that printed one empty line, and a part holding only spaces are all text
    /// that is not empty and still refused, and one of them in the history is a request refused
    /// again on every later round.
    #[test]
    fn text_with_no_word_in_it_is_no_block() {
        let walked = walked(
            &[
                Message::user("hello"),
                Message::assistant("\n\n"),
                Message::user_parts(vec![Part::Text {
                    text: "   ".to_string(),
                }]),
                Message::user("again"),
            ],
            true,
        );
        assert_eq!(walked.turns.len(), 1);
        assert_eq!(walked.turns[0].blocks, ["text:hello", "text:\n\nagain"]);
    }

    /// Both services take nothing but an object as a call's arguments. A call recorded with `null`,
    /// a list, a number or a string is still a call the model made and a result still answers, so
    /// it is kept with an empty object, as one whose arguments were not JSON at all is.
    #[test]
    fn arguments_that_are_not_an_object_become_an_empty_object() {
        for arguments in ["null", "[]", "[1]", "3", r#""text""#, "true"] {
            let walked = walked(
                &[Message::assistant_calling(
                    "",
                    vec![calling("a", "f", arguments)],
                )],
                true,
            );
            assert_eq!(walked.turns[0].blocks, ["call:a:f:{}"], "{arguments}");
        }
    }

    /// Only a `data:` URI with something in it is an attachment, and the wire decides whether it
    /// takes the media type. A link is dropped because asking a service to fetch it is an effect
    /// nobody endorsed. The count is of attachments met, so a dropped one still takes a position.
    #[test]
    fn only_a_data_uri_with_bytes_is_offered_to_the_wire_and_every_one_takes_a_position() {
        let walked = walked(
            &[Message::user_parts(vec![
                Part::Text {
                    text: "look".to_string(),
                },
                picture("https://example.invalid/a.png"),
                picture("data:image/png;base64,"),
                picture("data:application/zip;base64,AAAA"),
                picture("data:image/png;base64,QUJD"),
            ])],
            true,
        );
        assert_eq!(walked.turns[0].blocks, ["text:look", "image#4:QUJD"]);
    }
}
