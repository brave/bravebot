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
    /// A line a person typed, sent with the excerpts of the sessions it named with `@session:`.
    /// Holds the line alone: the excerpts are another session's words, assistant text included,
    /// and are not typed.
    TypedWithSessions(String),
    /// A sentence this program wrote.
    Driver,
    /// The model's own earlier words, sent back to it.
    Planner,
    /// A file somebody named or vouched for, which the kernel judged trusted.
    TrustedFile(String),
    /// Something the kernel judged trusted and showed, by what it is.
    Trusted(&'static str),
    /// A setting, which only the person's own files may supply and the driver does not vouch for.
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
            Self::TypedWithSessions(_) => t!(request_label_typed_with_sessions).to_string(),
            Self::Driver => t!(request_label_driver).to_string(),
            Self::Planner => t!(request_label_planner).to_string(),
            Self::TrustedFile(path) => t!(request_label_trusted_file, path = path),
            Self::Trusted(what) => t!(request_trusted, what = what),
            Self::Setting => t!(request_label_setting).to_string(),
            Self::Reference(token) => token.clone(),
            Self::Released(from) => t!(request_label_released, from = from),
            Self::Vetted(token) => t!(request_label_vetted, token = token),
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
    /// The size in bytes of the tool definitions as sent, which no span holds.
    pub tools_bytes: u64,
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
            // A length of what is sent and nothing read out of it: the bytes are counted, never
            // looked at.
            tools_bytes: request
                .tools
                .as_ref()
                .and_then(|tools| serde_json::to_string(tools).ok())
                .map_or(0, |sent| sent.len() as u64),
        }
    }

    /// The instruction files the system prompt was built from, each with the bytes it contributed,
    /// in the order they were composed.
    ///
    /// Read back out of the spans rather than recorded beside them, so the list cannot name a file
    /// whose words are not in the request. Only the system prompt's spans: a file a person sent
    /// with a prompt is trusted and carries its path too, but it is a message rather than an
    /// instruction.
    ///
    /// A length of what was sent and nothing read out of it: the bytes are counted, never looked
    /// at, and the path is the one the composing code recorded.
    pub fn instruction_files(&self) -> Vec<(&str, u64)> {
        self.spans
            .iter()
            .filter(|span| span.role == "system")
            .filter_map(|span| match &span.provenance {
                Provenance::TrustedFile(path) => Some((path.as_str(), span.text.len() as u64)),
                _ => None,
            })
            .collect()
    }

    /// Share `measured`, the count the server gave for this request, out over the fixed sections,
    /// in [`Section::ALL`] order, leaving out each section whose share is zero.
    ///
    /// A section is chosen from the role and the provenance the driver recorded for a span, and
    /// its weight is the length in bytes of what was sent. No span's words are read, so no section
    /// is chosen by what a span says.
    pub fn sections(&self, measured: u64) -> Vec<(Section, u64)> {
        let mut bytes = [0u64; Section::ALL.len()];
        bytes[Section::Tools as usize] = self.tools_bytes;
        for span in &self.spans {
            bytes[Section::of(span) as usize] += span.text.len() as u64;
        }
        Section::ALL
            .into_iter()
            .zip(apportion(measured, &bytes))
            .filter(|(_, share)| *share > 0)
            .collect()
    }
}

/// A part of a request that `/context` gives a row of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    /// The system prompt's own paragraphs.
    System,
    /// The instruction files the system prompt was built from.
    Instructions,
    /// The skill list in the system prompt.
    Skills,
    /// The tool definitions the request offered.
    Tools,
    /// What a person typed.
    Typed,
    /// What the planner wrote earlier, sent back to it.
    Planner,
    /// Tool results.
    Results,
    /// Every other message.
    Other,
}

impl Section {
    /// Every section, in the order the rows are listed.
    pub const ALL: [Section; 8] = [
        Section::System,
        Section::Instructions,
        Section::Skills,
        Section::Tools,
        Section::Typed,
        Section::Planner,
        Section::Results,
        Section::Other,
    ];

    /// The section a span belongs to, by its role and the provenance the driver recorded.
    fn of(span: &Span) -> Section {
        match (span.role, &span.provenance) {
            ("system", Provenance::TrustedFile(_)) => Section::Instructions,
            ("system", Provenance::Trusted("skill list")) => Section::Skills,
            ("system", _) => Section::System,
            (
                "user",
                Provenance::Typed | Provenance::TypedWithFiles | Provenance::TypedWithSessions(_),
            ) => Section::Typed,
            ("assistant", _) => Section::Planner,
            ("tool", _) => Section::Results,
            _ => Section::Other,
        }
    }
}

/// Divide `total` in proportion to `weights`, in whole numbers that add up to exactly `total`.
///
/// Each part is rounded down and what is left over goes one apiece to the parts with the largest
/// remainders, so the shares of a measured count always sum to it. All zeros where the weights
/// are, since nothing says what the total is made of.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a weight is at most the sum of the weights, so its share is at most `total`, a u64"
)]
pub fn apportion(total: u64, weights: &[u64]) -> Vec<u64> {
    let sum: u128 = weights.iter().map(|weight| u128::from(*weight)).sum();
    if sum == 0 {
        return vec![0; weights.len()];
    }
    let mut shares: Vec<u64> = weights
        .iter()
        .map(|weight| (u128::from(total) * u128::from(*weight) / sum) as u64)
        .collect();
    let mut left = total - shares.iter().sum::<u64>();
    let mut by_remainder: Vec<usize> = (0..weights.len()).collect();
    by_remainder.sort_by_key(|index| {
        std::cmp::Reverse(u128::from(total) * u128::from(weights[*index]) % sum)
    });
    for index in by_remainder {
        if left == 0 {
            break;
        }
        shares[index] += 1;
        left -= 1;
    }
    shares
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

    /// The definitions are counted as they are sent, since no span holds them, and a request that
    /// offers none counts none.
    #[test]
    fn the_size_of_the_tool_definitions_is_what_was_sent() {
        let prompt = Prompt::default();
        let bare = request(vec![Message::system("")]);
        assert_eq!(RequestView::of(&bare, &prompt, &[]).tools_bytes, 0);

        let tool = bravebot_aichat::protocol::Tool::function(
            "read_file",
            "Read a file.",
            serde_json::json!({"type": "object"}),
        );
        let sent = serde_json::to_string(std::slice::from_ref(&tool))
            .unwrap()
            .len() as u64;
        let offered = request(vec![Message::system("")]).with_tools(vec![tool]);
        assert_eq!(RequestView::of(&offered, &prompt, &[]).tools_bytes, sent);
        assert!(sent > 0);
    }

    /// The list names each instruction file the system prompt carried, with the bytes it
    /// contributed, and nothing else: not a trusted file sent as a message, and not the driver's
    /// own paragraphs or the skill list that sit beside it in the prompt.
    #[test]
    fn the_instruction_files_are_the_trusted_files_the_system_prompt_was_built_from() {
        let mut prompt = Prompt::default();
        prompt.push(Provenance::Driver, "You are careful.\n");
        prompt.push(
            Provenance::TrustedFile("~/.bravebot/AGENTS.md".into()),
            "Be brief.",
        );
        prompt.push(Provenance::TrustedFile("AGENTS.md".into()), "Use tabs.");
        prompt.push(Provenance::Trusted("skill list"), "- design\n");
        let sent = request(vec![
            Message::system(prompt.text()),
            Message::user("read main.rs"),
        ]);

        let view = RequestView::of(&sent, &prompt, &[Provenance::TrustedFile("main.rs".into())]);

        assert_eq!(
            view.instruction_files(),
            vec![("~/.bravebot/AGENTS.md", 9), ("AGENTS.md", 9)]
        );
    }

    /// A prompt with no instruction file in it names none, so the view cannot report one that was
    /// refused or absent.
    #[test]
    fn a_prompt_with_no_instruction_file_names_none() {
        let mut prompt = Prompt::default();
        prompt.push(Provenance::Driver, "You are careful.\n");
        let sent = request(vec![Message::system(prompt.text())]);

        assert!(
            RequestView::of(&sent, &prompt, &[])
                .instruction_files()
                .is_empty()
        );
    }

    #[test]
    fn a_reference_is_labelled_by_its_token_alone() {
        assert_eq!(Provenance::Reference("ref:3".into()).label(), "ref:3");
        assert_eq!(
            Provenance::Trusted("tool result").label(),
            "tool result (trusted)"
        );
    }

    /// The `/context` rows must add up to the measured total, so each share is a whole number
    /// and the rounding remainders are handed out rather than dropped.
    #[test]
    fn a_count_is_apportioned_in_whole_numbers_that_add_up_to_it() {
        assert_eq!(apportion(10, &[1, 1, 1]), vec![4, 3, 3]);
        assert_eq!(apportion(777, &[0, 300, 0, 700]).iter().sum::<u64>(), 777);
        assert_eq!(apportion(100, &[0, 0]), vec![0, 0]);
        assert_eq!(apportion(0, &[5, 5]), vec![0, 0]);
        assert_eq!(
            apportion(u64::MAX, &[u64::MAX, u64::MAX])
                .iter()
                .sum::<u64>(),
            u64::MAX
        );
    }

    fn span(role: &'static str, provenance: Provenance, bytes: usize) -> Span {
        Span {
            role,
            provenance,
            text: "x".repeat(bytes),
        }
    }

    /// Each span goes to the section its role and recorded provenance name, the tool definitions
    /// have a section of their own, and the shares add up to the measured count.
    #[test]
    fn each_span_is_counted_in_the_section_its_role_and_provenance_name() {
        let view = RequestView {
            model: "model".into(),
            spans: vec![
                span("system", Provenance::Driver, 1),
                span("system", Provenance::TrustedFile("AGENTS.md".into()), 2),
                span("system", Provenance::Trusted("skill list"), 3),
                span("user", Provenance::Typed, 2),
                span("user", Provenance::TypedWithFiles, 1),
                span("user", Provenance::TypedWithSessions("hi".into()), 2),
                span("assistant", Provenance::Planner, 6),
                span("tool", Provenance::Trusted("tool result"), 7),
                span("user", Provenance::Reference("ref:1".into()), 8),
            ],
            tools: vec![],
            tools_bytes: 4,
        };

        assert_eq!(
            view.sections(36),
            vec![
                (Section::System, 1),
                (Section::Instructions, 2),
                (Section::Skills, 3),
                (Section::Tools, 4),
                (Section::Typed, 5),
                (Section::Planner, 6),
                (Section::Results, 7),
                (Section::Other, 8),
            ]
        );
        assert_eq!(
            view.sections(1_000)
                .iter()
                .map(|(_, share)| share)
                .sum::<u64>(),
            1_000
        );
    }

    /// A section with nothing in it has no row, and a request with nothing in it has none at all.
    #[test]
    fn a_section_with_nothing_in_it_is_left_out() {
        let view = RequestView {
            model: "model".into(),
            spans: vec![span("tool", Provenance::Trusted("tool result"), 3)],
            tools: vec![],
            tools_bytes: 1,
        };
        assert_eq!(
            view.sections(100),
            vec![(Section::Tools, 25), (Section::Results, 75)]
        );

        let empty = RequestView {
            model: "model".into(),
            spans: vec![],
            tools: vec![],
            tools_bytes: 0,
        };
        assert!(empty.sections(100).is_empty());
    }
}
