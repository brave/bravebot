//! What a prompt that names a past session with `@session:<id>` carries of it (NAME-10).
//!
//! Read out of the stored record and nothing else. A record holds only what the planner was
//! allowed to hold (SESSION-2), so what is taken here adds no byte the planner could not have
//! had, and what the session wrote without keeping is said to be missing rather than filled in.

use bravebot_agent::Conversation;
use bravebot_agent::conversation::Said;
use bravebot_core::label::Confidentiality;
use bravebot_i18n::t;

use crate::sessions::{Front, Record};

/// How many characters one excerpt holds, the heading included.
///
/// A fixed bound so the person choosing a session can be told what it costs before sending, and so
/// that a long session cannot fill the request by being named.
pub const BOUND: usize = 8_000;

/// What every excerpt's heading begins with, after the blank line that sets it off.
///
/// What a turn stores of a prompt that named a session is the line and the excerpt in one message,
/// so quoting that session later would quote the earlier excerpt inside it, and each generation
/// would nest the last. The words before this are the ones the person typed.
const HEADING_START: &str = "\n\nExcerpt of an earlier session, \"";

const CUT: &str = "\n\nOlder turns of that session are left out.";

/// Why a session cannot be quoted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// No record of that name in this directory's store.
    NotFound,
    /// A manifest run holds no conversation to quote.
    Manifest,
    /// The words are another program's, or a front end this build does not know: nothing says
    /// they are what a planner here was shown (SESSION-32).
    NotOurs,
    /// The planner of that session had been shown private content, or the record does not say it
    /// had not, so its words are not carried into another turn.
    Private,
    /// Nothing was said in it that can be quoted.
    Empty,
}

/// The part of a session one turn is given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Excerpt {
    pub id: String,
    /// What is added to the person's message, heading included.
    pub text: String,
    /// How many characters that is, which is what the trail records of it.
    pub chars: usize,
    /// Whether older turns were left out to stay inside [`BOUND`].
    pub cut: bool,
}

impl Refused {
    /// The sentence a front end says for it, naming the session asked for.
    pub fn said(self, id: &str) -> String {
        match self {
            Self::NotFound => t!(session_mention_no_such, id = id),
            Self::Manifest => t!(session_mention_manifest, id = id),
            Self::NotOurs => t!(session_mention_not_ours, id = id),
            Self::Private => t!(session_mention_private, id = id),
            Self::Empty => t!(session_mention_empty, id = id),
        }
        .to_string()
    }
}

impl Record {
    /// The prompts and the replies of this session, newest turn first, within [`BOUND`].
    ///
    /// Each turn reads in the order it happened, and the turns run from the last backwards so that
    /// the cut falls on the oldest. A turn that does not fit whole is cut at the bound.
    pub fn excerpt(&self) -> Result<Excerpt, Refused> {
        if self.manifest.is_some() {
            return Err(Refused::Manifest);
        }
        let ours = match self.front.as_deref() {
            None => true,
            Some(word) => word == Front::Terminal.recorded() || word == Front::Desktop.recorded(),
        };
        let copied = self
            .conversation
            .archive
            .iter()
            .chain(&self.conversation.messages)
            .any(|stored| {
                stored.composed == Some(bravebot_agent::conversation::Composed::Imported)
            });
        if !ours || copied {
            return Err(Refused::NotOurs);
        }
        let conversation = Conversation::restored(self.conversation.clone());
        if conversation.holds() != Confidentiality::Public {
            return Err(Refused::Private);
        }

        let mut turns: Vec<Vec<(&'static str, String)>> = Vec::new();
        let mut unkept = false;
        for said in conversation.recounted() {
            match said {
                Said::User(text) => {
                    let typed = text.split(HEADING_START).next().unwrap_or_default();
                    turns.push(vec![("You", typed.to_string())]);
                }
                Said::Assistant(text) => match turns.last_mut() {
                    Some(turn) => turn.push(("Assistant", text)),
                    None => turns.push(vec![("Assistant", text)]),
                },
                // What a call returned is not recounted, and a file the session read is not in
                // the record, so that the excerpt has no account of either is said below.
                Said::Tool { .. } | Said::Composed { .. } => unkept = true,
            }
        }
        if turns.is_empty() {
            return Err(Refused::Empty);
        }

        let heading = format!(
            "{}{}\" ({}), newest turn first.",
            HEADING_START.trim_start(),
            self.title,
            self.id
        );
        let footer = if unkept {
            "\n\nThe tool results and file contents of that session are not included."
        } else {
            ""
        };
        // What a cut adds is reserved whether or not one happens, so the bound holds either way.
        let mut room = BOUND
            .saturating_sub(heading.chars().count())
            .saturating_sub(footer.chars().count())
            .saturating_sub(CUT.chars().count());
        let mut body = String::new();
        let mut cut = false;
        for turn in turns.iter().rev() {
            let block: String = turn
                .iter()
                .map(|(who, text)| format!("{who}: {}", text.trim()))
                .collect::<Vec<_>>()
                .join("\n\n");
            let separator = 2;
            let needed = block.chars().count() + separator;
            if needed <= room {
                body.push_str("\n\n");
                body.push_str(&block);
                room -= needed;
                continue;
            }
            cut = true;
            // The newest turn is always shown, cut if it has to be: a reference that quoted
            // nothing would be a send that took no part of the session it named.
            if body.is_empty() && room > separator {
                body.push_str("\n\n");
                body.extend(block.chars().take(room - separator));
            }
            break;
        }
        let ending = if cut { CUT } else { "" };
        let text = format!("{heading}{body}{ending}{footer}");
        Ok(Excerpt {
            id: self.id.clone(),
            chars: text.chars().count(),
            text,
            cut,
        })
    }
}

/// The excerpt of one session of this project's store, by id.
pub fn of(project: &std::path::Path, id: &str) -> Result<Excerpt, Refused> {
    if !crate::sessions::is_a_session_name(id) {
        return Err(Refused::NotFound);
    }
    crate::sessions::load(project, id)
        .ok_or(Refused::NotFound)?
        .excerpt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_aichat::protocol::Message;

    fn a_record(messages: Vec<Message>) -> Record {
        let mut conversation = Conversation::new();
        for message in messages {
            conversation.push(message);
        }
        let mut record: Record = serde_json::from_value(serde_json::json!({
            "id": "abc-123",
            "directory": "/work",
            "title": "fix the build",
            "started": 1,
            "updated": 2,
            "conversation": conversation.snapshot(),
        }))
        .expect("a record");
        record.front = Some("terminal".to_string());
        record
    }

    fn said(turns: usize) -> Vec<Message> {
        (1..=turns)
            .flat_map(|n| {
                [
                    Message::user(format!("question {n}")),
                    Message::assistant(format!("answer {n}")),
                ]
            })
            .collect()
    }

    /// The prompts and the replies are what is carried, the newest turn first and each turn in the
    /// order it happened.
    #[test]
    fn an_excerpt_holds_the_prompts_and_replies_newest_turn_first() {
        let excerpt = a_record(said(2)).excerpt().expect("an excerpt");
        let at = |needle: &str| excerpt.text.find(needle).expect(needle);
        assert!(at("question 2") < at("answer 2"));
        assert!(at("answer 2") < at("question 1"));
        assert!(at("question 1") < at("answer 1"));
        assert!(excerpt.text.contains("\"fix the build\" (abc-123)"));
        assert!(!excerpt.cut);
        assert_eq!(excerpt.chars, excerpt.text.chars().count());
    }

    /// A prompt that named a session is stored with the excerpt after the line, and naming that
    /// session later quotes the line the person typed and not the excerpt inside it.
    #[test]
    fn an_excerpt_is_not_quoted_inside_the_next_one() {
        let first = a_record(said(1)).excerpt().expect("an excerpt");
        let stored = format!("carry on from @session:abc-123\n\n{}", first.text);
        let later = a_record(vec![Message::user(stored), Message::assistant("done")])
            .excerpt()
            .expect("an excerpt");
        assert!(
            later
                .text
                .contains("You: carry on from @session:abc-123\n\nAssistant: done")
        );
        assert_eq!(
            later.text.matches("Excerpt of an earlier session").count(),
            1
        );
        assert!(!later.text.contains("question 1"));
    }

    /// A long session is cut at the bound, the oldest turns first, and the cut is said.
    #[test]
    fn a_long_session_is_cut_at_the_bound_from_the_oldest_turn() {
        let mut messages = vec![
            Message::user("the very first question"),
            Message::assistant("a"),
        ];
        for n in 0..40 {
            messages.push(Message::user(format!("turn {n} {}", "x".repeat(400))));
            messages.push(Message::assistant("ok"));
        }
        let excerpt = a_record(messages).excerpt().expect("an excerpt");
        assert!(excerpt.cut);
        assert!(excerpt.chars <= BOUND, "{} characters", excerpt.chars);
        assert!(excerpt.text.contains("turn 39"));
        assert!(!excerpt.text.contains("the very first question"));
        assert!(
            excerpt
                .text
                .contains("Older turns of that session are left out.")
        );
    }

    /// A single turn longer than the bound is still quoted, cut, so naming a session always
    /// brings something of it.
    #[test]
    fn one_turn_longer_than_the_bound_is_cut_rather_than_dropped() {
        let excerpt = a_record(vec![Message::user("y".repeat(BOUND * 2))])
            .excerpt()
            .expect("an excerpt");
        assert!(excerpt.cut);
        assert!(excerpt.chars <= BOUND);
        assert!(excerpt.text.contains("You: yyy"));
    }

    /// What a session did with tools is not in the record as content, and the excerpt says so
    /// instead of reading as the whole of what happened.
    #[test]
    fn a_session_that_used_tools_says_their_results_are_not_included() {
        let call: bravebot_aichat::protocol::ToolCallRequest =
            serde_json::from_value(serde_json::json!({
                "id": "c1",
                "function": {"name": "read_file", "arguments": "{\"path\":\"a.txt\"}"},
            }))
            .expect("a call");
        let mut messages = said(1);
        messages.push(Message::assistant_calling("reading", vec![call]));
        let said_so = a_record(messages).excerpt().expect("an excerpt");
        assert!(said_so.text.contains("tool results and file contents"));
        let plain = a_record(said(1)).excerpt().expect("an excerpt");
        assert!(!plain.text.contains("tool results and file contents"));
    }

    /// What a planner here was not shown is not carried into a turn: another program's words, a
    /// manifest run, and a session that held private content.
    #[test]
    fn a_record_that_is_not_the_planners_to_repeat_is_refused() {
        let mut manifest = a_record(said(1));
        manifest.manifest = Some(
            serde_json::from_value(serde_json::json!({"shape": "s", "steps": []}))
                .expect("a manifest"),
        );
        assert_eq!(manifest.excerpt(), Err(Refused::Manifest));

        let mut foreign = a_record(said(1));
        foreign.front = Some("someone-elses-agent".to_string());
        assert_eq!(foreign.excerpt(), Err(Refused::NotOurs));

        let mut private = a_record(said(1));
        private.conversation.holds = "private".to_string();
        assert_eq!(private.excerpt(), Err(Refused::Private));

        assert_eq!(a_record(Vec::new()).excerpt(), Err(Refused::Empty));
    }

    /// An id is one path segment of a session's own name, so a mention cannot reach a file of
    /// the store that is not a record, or one outside it.
    #[test]
    fn an_id_that_is_not_a_session_name_is_not_looked_up() {
        let project = std::path::Path::new("/nonexistent");
        assert_eq!(of(project, "../../etc/passwd"), Err(Refused::NotFound));
        assert_eq!(of(project, ""), Err(Refused::NotFound));
    }
}
