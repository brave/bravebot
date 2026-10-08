//! The request a turn sent the planner, split into spans that each say where their words came from.
//!
//! Built from the [`ChatRequest`] that goes to the backend, never from the transcript or the
//! conversation: the view reads the request value itself, so it can show nothing the request did
//! not hold, and a quarantined body is not in a request to be shown. What a span claims about
//! itself comes from the label the driver recorded when it composed the message.
//!
//! The words are the request's and the provenance is the driver's. Nothing here decides anything:
//! no branch is taken on the text of a span.

use bravebot_aichat::protocol::{ChatRequest, Content, Message, Part, Role};
use bravebot_i18n::t;

/// Who is answerable for the words of one span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provenance {
    /// A keystroke: what a person typed to this session.
    Typed,
    /// A line a person typed, sent with files they dropped on it. The files' bytes are not typed.
    TypedWithFiles,
    /// A sentence this program wrote.
    Driver,
    /// The model's own earlier words, sent back to it.
    Planner,
    /// A file somebody named or vouched for, which the kernel judged trusted.
    TrustedFile(String),
    /// Something the kernel judged trusted and showed, by what it is.
    Trusted(&'static str),
    /// A setting, which a checkout may supply and the driver does not vouch for.
    Setting,
    /// A reference token standing for content the planner was not shown.
    Reference(String),
    /// Content held under a reference that was let through to the planner, by where it was held.
    Released(String),
    /// A picture or a PDF `vet_content` let through, by the reference it was held under.
    Vetted(String),
    /// A summary written in place of messages compaction took out of the request.
    Summary,
    /// A message nothing recorded the origin of, such as one restored from a saved session.
    Unrecorded,
}

impl Provenance {
    /// The words a person reads. A reference is its token and nothing else.
    pub fn label(&self) -> String {
        match self {
            Self::Typed => t!(request_label_typed).to_string(),
            Self::TypedWithFiles => t!(request_label_typed_with_files).to_string(),
            Self::Driver => t!(request_label_driver).to_string(),
            Self::Planner => t!(request_label_planner).to_string(),
            Self::TrustedFile(path) => t!(request_label_trusted_file, path = path).to_string(),
            Self::Trusted(what) => t!(request_trusted, what = what).to_string(),
            Self::Setting => t!(request_label_setting).to_string(),
            Self::Reference(token) => token.clone(),
            Self::Released(from) => t!(request_label_released, from = from).to_string(),
            Self::Vetted(token) => t!(request_label_vetted, token = token).to_string(),
            Self::Summary => t!(request_label_summary).to_string(),
            Self::Unrecorded => t!(request_label_unrecorded).to_string(),
        }
    }
}

/// A system prompt as the pieces it was put together from, in order.
///
/// The text sent is [`Prompt::text`], so the pieces cannot describe a prompt other than the one
/// that went.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Prompt {
    pieces: Vec<(Provenance, String)>,
}

impl Prompt {
    /// Add a piece. An empty one is left out, so no span in a view is blank.
    pub fn push(&mut self, provenance: Provenance, text: impl Into<String>) {
        let text = text.into();
        if !text.is_empty() {
            self.pieces.push((provenance, text));
        }
    }

    /// Add every piece of another prompt.
    pub fn extend(&mut self, other: Prompt) {
        self.pieces.extend(other.pieces);
    }

    /// The pieces, in order.
    pub fn into_pieces(self) -> Vec<(Provenance, String)> {
        self.pieces
    }

    /// The prompt as it is sent.
    pub fn text(&self) -> String {
        self.pieces.iter().map(|(_, text)| text.as_str()).collect()
    }
}

/// One stretch of the request, with the origin of its words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// The role the request gave it: `system`, `user`, `assistant` or `tool`.
    pub role: &'static str,
    pub provenance: Provenance,
    pub text: String,
}

/// The last request a turn built for the planner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestView {
    pub model: String,
    pub spans: Vec<Span>,
    /// The names of the tools the request offered.
    pub tools: Vec<String>,
}

impl RequestView {
    /// Read `request`, which is the value about to be sent.
    ///
    /// `prompt` is the pieces of its first message and `marks` the origin of each message after it.
    /// Where either does not match the request, the span is unrecorded rather than guessed at.
    pub fn of(request: &ChatRequest, prompt: &Prompt, marks: &[Provenance]) -> Self {
        let mut spans = Vec::new();
        let mut messages = request.messages.iter();

        if let Some(system) = messages.next() {
            let sent = words(system);
            if prompt.text() == sent && matches!(system.role, Role::System) {
                for (provenance, text) in &prompt.pieces {
                    spans.push(Span {
                        role: "system",
                        provenance: provenance.clone(),
                        text: text.clone(),
                    });
                }
            } else {
                spans.push(Span {
                    role: role_of(system.role),
                    provenance: Provenance::Unrecorded,
                    text: sent,
                });
            }
        }

        for (index, message) in messages.enumerate() {
            spans.push(Span {
                role: role_of(message.role),
                provenance: marks.get(index).cloned().unwrap_or(Provenance::Unrecorded),
                text: words(message),
            });
        }

        Self {
            model: request.model.clone(),
            spans,
            tools: request
                .tools
                .iter()
                .flatten()
                .map(|tool| tool.function.name.clone())
                .collect(),
        }
    }
}

fn role_of(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    }
}

/// What the message carries, as text: its words, the calls it asks for, and a mark where a picture
/// is.
fn words(message: &Message) -> String {
    let mut out = match &message.content {
        Content::Text(text) => text.clone(),
        Content::Parts(parts) => parts
            .iter()
            .map(|part| match part {
                Part::Text { text } => text.clone(),
                Part::ImageUrl { .. } => t!(request_bytes_mark).to_string(),
            })
            .collect::<Vec<_>>()
            .join("\n"),
    };
    for call in message.tool_calls.iter().flatten() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&format!(
            "[call {}: {}({})]",
            call.id, call.function.name, call.function.arguments
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(messages: Vec<Message>) -> ChatRequest {
        ChatRequest::new("model", messages)
    }

    #[test]
    fn the_system_prompt_is_shown_as_the_pieces_it_was_made_of() {
        let mut prompt = Prompt::default();
        prompt.push(Provenance::Driver, "You are careful.\n");
        prompt.push(Provenance::TrustedFile("AGENTS.md".into()), "Use tabs.\n");
        let sent = request(vec![Message::system(prompt.text())]);

        let view = RequestView::of(&sent, &prompt, &[]);

        assert_eq!(
            view.spans
                .iter()
                .map(|span| (span.provenance.label(), span.text.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("driver".to_string(), "You are careful.\n"),
                ("trusted file AGENTS.md".to_string(), "Use tabs.\n"),
            ]
        );
    }

    #[test]
    fn pieces_that_are_not_the_prompt_sent_are_not_trusted_to_describe_it() {
        let mut prompt = Prompt::default();
        prompt.push(Provenance::Driver, "what the pieces say");
        let sent = request(vec![Message::system("what was sent")]);

        let view = RequestView::of(&sent, &prompt, &[]);

        assert_eq!(view.spans.len(), 1);
        assert_eq!(view.spans[0].provenance, Provenance::Unrecorded);
        assert_eq!(view.spans[0].text, "what was sent");
    }

    #[test]
    fn a_message_without_a_mark_is_unrecorded_and_never_typed() {
        let prompt = Prompt::default();
        let sent = request(vec![
            Message::system(""),
            Message::user("one"),
            Message::user("two"),
        ]);

        let view = RequestView::of(&sent, &prompt, &[Provenance::Typed]);

        assert_eq!(view.spans[0].provenance, Provenance::Typed);
        assert_eq!(view.spans[1].provenance, Provenance::Unrecorded);
    }

    #[test]
    fn a_reference_is_labelled_by_its_token_alone() {
        assert_eq!(Provenance::Reference("ref:3".into()).label(), "ref:3");
        assert_eq!(
            Provenance::Trusted("tool result").label(),
            "tool result (trusted)"
        );
    }
}
