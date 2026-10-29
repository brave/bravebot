//! The model that answers a run or fetch question in the auto mode (AUTO-3 to AUTO-8).
//!
//! One request with no tools, sent to the session's own model, and one word read back. The word is
//! the only thing that leaves this module: [`Classified::Allowed`] answers the question, and
//! everything else, an error included, leaves it with the person (AUTO-6). The model's reason is
//! never kept, so a sentence it wrote cannot reach the trail or the planner (AUTO-7).
//!
//! # What it is given
//!
//! The lines the person typed, and one rendering of what the planner wants to do. Both are already
//! plain text the driver holds: the typed lines are selected by the tag recorded when they arrived
//! (`Provenance::Typed`) and never by reading their words, and the planner's line is the plan the
//! run gate built or the URL the argument gate read. Nothing is released from a label here, so
//! this module declassifies nothing. It is given no tool result, no body, no output, no listing
//! name and no instruction file (AUTO-5).
//!
//! # Why the reply may be read
//!
//! The reply is relabelled from the context the planner was shown, and read only if that context
//! is one the driver may read. This is how the goal judge's verdict is read (GOAL-5): a model shown
//! only what the planner was shown writes text as trustworthy as that context, and a context that
//! has met something untrusted makes the reply quarantined, which is read as a refusal to allow.

use bravebot_aichat::protocol::{ChatRequest, Message};
use bravebot_core::event::Sink;
use bravebot_core::policy::Policy;
use bravebot_core::reference::Presentation;
use bravebot_core::slot::SlotId;

use crate::confirm::Classified;
use crate::processor::Chat;

/// The word that answers a question. The reply is compared to it whole, after trimming and
/// lowercasing and after nothing else, so "allow." and "allow, but" are not it.
const ALLOW: &str = "allow";

/// How many of the person's lines the classifier is given, newest last.
///
/// Bounded so a long session costs the same to classify as a short one. The oldest are dropped
/// first, because the line that asked for the work is usually near the end.
const LINES_KEPT: usize = 12;

/// The most of one typed line the classifier is given, in bytes, cut at a character boundary.
const LINE_BYTES: usize = 4_000;

/// What the classifier is told it is. The phrase "permission classifier" is what a test double
/// recognises a request by, so it appears here and nowhere in the other models' instructions.
const SYSTEM_PROMPT: &str = "You are a permission classifier for a coding agent. The agent wants \
to take one action. You are shown the messages the person typed to the agent, and then the action. \
Decide whether the person asked for this action, or for work that plainly needs it, and whether the \
action is routine for that work: it must not be destructive, must not send data to somewhere the \
person did not name, and must not reach beyond what the work needs. Reply allow only when both are \
true. Reply deny in every other case, including any doubt. Reply with exactly one word, allow or \
deny, and nothing else.";

/// What the classifier is asked to rule on.
#[derive(Debug, Clone, Copy)]
pub enum Question<'a> {
    /// A command line, and the directory it runs in.
    Run { line: &'a str, directory: &'a str },
    /// A URL the planner asked to fetch, and the host it names.
    Fetch { url: &'a str, host: &'a str },
}

impl Question<'_> {
    /// The name the trail gives the kind of question. Static, so no word of one reaches it.
    fn kind(&self) -> &'static str {
        match self {
            Self::Run { .. } => "run",
            Self::Fetch { .. } => "fetch",
        }
    }

    /// The action as the classifier reads it.
    fn action(&self) -> String {
        match self {
            Self::Run { line, directory } => {
                format!("Run this command line in the directory {directory}:\n{line}")
            }
            Self::Fetch { url, host } => {
                format!("Fetch the page at {url} (the host is {host}).")
            }
        }
    }
}

/// The lines the person typed to this session, for the classifier to read.
pub struct Classifying<'a> {
    /// Oldest first.
    pub typed: &'a [String],
}

/// The request the classifier is sent: the instruction, the person's lines, and the action.
///
/// Public to the crate so a test can read what the classifier is given without a server.
pub(crate) fn request_for(model: &str, typed: &[String], question: &Question<'_>) -> ChatRequest {
    let kept = typed.len().saturating_sub(LINES_KEPT);
    let mut said = String::from("The messages the person typed to the agent, oldest first:\n");
    for line in &typed[kept..] {
        said.push_str("\n---\n");
        said.push_str(cut(line));
    }
    said.push_str("\n---\n\nThe action the agent wants to take:\n");
    said.push_str(&question.action());
    said.push_str("\n\nReply with one word: allow or deny.");
    // No tools, deliberately and visibly: `ChatRequest::new` leaves the field empty and nothing
    // below adds to it. A classifier that could call a tool would be a turn.
    ChatRequest::new(
        model,
        vec![Message::system(SYSTEM_PROMPT), Message::user(said)],
    )
    .giving_up_its_conversation()
}

/// `line`, cut to [`LINE_BYTES`] at a character boundary.
fn cut(line: &str) -> &str {
    if line.len() <= LINE_BYTES {
        return line;
    }
    let mut end = LINE_BYTES;
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    &line[..end]
}

/// The classifier's word read from its reply: [`Classified::Allowed`] for the allow word and
/// [`Classified::NotAllowed`] for anything else (AUTO-6).
pub(crate) fn read(reply: &str) -> Classified {
    match reply.trim().to_lowercase() == ALLOW {
        true => Classified::Allowed,
        false => Classified::NotAllowed,
    }
}

/// Put `question` to the classifier and bring back its word.
///
/// [`Classified::NotAsked`] when it must not be consulted: the planner's context holds something
/// untrusted or private, or the machine's settings refuse the model. Anything that goes wrong
/// after the request is sent is [`Classified::NotAllowed`], so a failure can only leave the
/// question with the person (AUTO-6). The trail records the model, the kind of question and the
/// word, and none of what the classifier was given or wrote (AUTO-8).
pub fn consult<S: Sink>(
    policy: &mut Policy<'_, S>,
    chat: &mut Chat<'_>,
    classifying: &Classifying<'_>,
    question: &Question<'_>,
) -> Classified {
    // Private text is not sent to another model (ADVISOR-3), and a context that has met something
    // untrusted would only make the reply unreadable. Asked before the model is, so neither costs
    // a request.
    let context = policy.context_label();
    if !context.is_public() || !context.is_trusted() || classifying.typed.is_empty() {
        return Classified::NotAsked;
    }
    let model = chat.model.unwrap_or(&chat.config.default_model).to_string();
    // A model the machine-level settings refuse is refused whichever route names it (BACKEND-48).
    if chat.config.model_refused(&model).is_some() {
        return Classified::NotAsked;
    }

    let request = request_for(&model, classifying.typed, question);
    let mut client = chat.backend(&model);
    let completion = match client.complete_streaming(policy, &request, |_| {}) {
        Ok(completion) => completion,
        Err(_) => return answered(policy, &model, question, Classified::NotAllowed, 0),
    };
    let cost = completion.usage.prompt_tokens + completion.usage.completion_tokens;

    // Relabelled from the context the way a round's own words are: what comes back from the
    // client carries the label the network gave it, and the kernel is the only thing that knows
    // what this model was shown.
    let word = match policy.adopt_model_output("auto_mode", completion.content) {
        Err(_) => Classified::NotAllowed,
        Ok(written) => {
            // A store of its own, thrown away with the request: a quarantined reply is not acted
            // on, it is a question left with the person.
            let mut only_this = bravebot_core::slot::SlotStore::new();
            match policy.present(
                "auto_mode",
                SlotId::new("auto_mode"),
                "the permission classifier's word on one question",
                &written,
                &mut only_this,
            ) {
                Ok(Presentation::Visible(body)) => read(&body),
                Ok(Presentation::Quarantined(_)) | Err(_) => Classified::NotAllowed,
            }
        }
    };
    answered(policy, &model, question, word, cost)
}

/// Record the word and hand it back.
fn answered<S: Sink>(
    policy: &mut Policy<'_, S>,
    model: &str,
    question: &Question<'_>,
    word: Classified,
    cost: u64,
) -> Classified {
    policy.record_classification(
        model,
        question.kind(),
        match word {
            Classified::Allowed => "allow",
            Classified::NotAllowed | Classified::NotAsked => "not allowed",
        },
        cost,
    );
    word
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_aichat::protocol::Role;

    fn a_run() -> Question<'static> {
        Question::Run {
            line: "cargo test -p bravebot-core",
            directory: "/work/project",
        }
    }

    /// AUTO-6: the allow word answers and nothing near it does. The regressions it rejects: a
    /// reply that merely contains the word allowing, which is what a model that argues itself
    /// out of it writes, and trimming or lowercasing being the only thing that is forgiven.
    #[test]
    fn only_the_allow_word_alone_allows() {
        for allows in ["allow", "Allow", " ALLOW\n", "\tallow  "] {
            assert_eq!(read(allows), Classified::Allowed, "{allows:?}");
        }
        for does_not in [
            "",
            "deny",
            "allow.",
            "allow, but only in a test",
            "I would not allow",
            "allow\ndeny",
            "do not allow",
            "allowed",
            "yes",
        ] {
            assert_eq!(read(does_not), Classified::NotAllowed, "{does_not:?}");
        }
    }

    /// AUTO-5: the classifier is given the instruction, the person's lines and the action, with no
    /// tools, and the lines are the person's and in order. The regression it rejects: a tool or a
    /// word of anything else reaching a model that decides what a person is asked.
    #[test]
    fn the_request_holds_the_persons_lines_and_the_action_and_offers_no_tools() {
        let typed = vec![
            "fix the failing test in core".to_string(),
            "then run the tests".to_string(),
        ];
        let request = request_for("classifier-model", &typed, &a_run());

        assert!(request.tools.is_none(), "a classifier is offered no tools");
        assert_eq!(request.model, "classifier-model");
        assert_eq!(request.messages.len(), 2);
        assert_eq!(request.messages[0].role, Role::System);
        assert!(
            request.messages[0]
                .content
                .text()
                .contains("permission classifier")
        );
        let said = request.messages[1].content.text();
        let first = said
            .find("fix the failing test in core")
            .expect("first line");
        let second = said.find("then run the tests").expect("second line");
        let action = said.find("cargo test -p bravebot-core").expect("action");
        assert!(first < second && second < action, "{said}");
        assert!(said.contains("/work/project"), "{said}");
    }

    /// AUTO-5: a long session costs what a short one does. The oldest lines go first, and a line
    /// is cut at a character boundary rather than inside one.
    #[test]
    fn a_long_history_is_bounded_and_a_line_is_cut_at_a_character_boundary() {
        let typed: Vec<String> = (0..40).map(|n| format!("line number {n}")).collect();
        let said = request_for("m", &typed, &a_run()).messages[1]
            .content
            .text();
        assert!(!said.contains("line number 0\n"), "{said}");
        assert!(said.contains("line number 39"), "{said}");
        assert_eq!(said.matches("\n---\n").count(), LINES_KEPT + 1);

        // A line of two-byte characters whose cut would fall inside one.
        let long = "é".repeat(LINE_BYTES);
        let cut = cut(&long);
        assert!(cut.len() <= LINE_BYTES);
        assert!(long.starts_with(cut));
    }

    /// AUTO-5: the two kinds of question name only what the classifier needs, and the trail's name
    /// for each is static text.
    #[test]
    fn a_fetch_names_the_url_and_the_host() {
        let question = Question::Fetch {
            url: "https://docs.example.com/guide",
            host: "docs.example.com",
        };
        let said = request_for("m", &["read the guide".to_string()], &question).messages[1]
            .content
            .text();
        assert!(said.contains("https://docs.example.com/guide"), "{said}");
        assert_eq!(question.kind(), "fetch");
        assert_eq!(a_run().kind(), "run");
    }
}
