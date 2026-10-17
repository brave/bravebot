//! A message one session's planner wrote for another session's planner (MSG-1, MSG-2).
//!
//! What the policy layer lets leave this session, and the facts about it the trail records. The text
//! is the planner's own words and is never read by anything that decides where it goes.

use crate::label::Label;

/// The most characters a message may hold. A sender refuses a longer one (MSG-1) and a receiver
/// refuses one as well, because it cannot rely on the sender having done so (MSG-6).
pub const MOST_CHARACTERS: usize = 4_000;

/// How many messages one turn may put to one session (MSG-10).
pub const MOST_PER_TARGET_PER_TURN: usize = 10;

/// A message the policy layer let through: the name the planner gave the target, the words, and
/// the label the sender's own context gives them.
///
/// Only [`crate::policy::Policy::before_message`] makes one, so the label is never the planner's
/// choice and no text changes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionMessage {
    target: String,
    text: String,
    label: Label,
}

impl SessionMessage {
    pub(crate) fn new(target: String, text: String, label: Label) -> Self {
        Self {
            target,
            text,
            label,
        }
    }

    /// The session's name as the planner wrote it, to be compared with the roster.
    pub fn target(&self) -> &str {
        &self.target
    }

    /// The words to carry to the receiver.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The label the words are sent under: the integrity of the context that wrote them, and
    /// private where that context holds something private.
    pub fn label(&self) -> Label {
        self.label
    }
}

/// What a person was asked about a message, and what they answered (MSG-3, MSG-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answered {
    /// Nobody was asked: a standing answer given earlier this turn covers this target.
    Covered,
    /// The person said to send this one.
    Once,
    /// The person said to send this one and the later ones to this session until the turn ends.
    ForTheTurn,
    /// The person said not to send it, or nobody could be asked.
    Declined,
}

impl Answered {
    fn words(self) -> &'static str {
        match self {
            Self::Covered => "nobody was asked, an earlier answer this turn covered it",
            Self::Once => "the person said to send it",
            Self::ForTheTurn => {
                "the person said to send it and the later ones to this session this turn"
            }
            Self::Declined => "the person declined, or nobody could be asked",
        }
    }
}

/// The facts about one message that the trail keeps, and none of its words (MSG-11).
#[derive(Debug, Clone, Copy)]
pub struct Sent<'a> {
    /// The sender's id.
    pub from: &'a str,
    /// The receiver's id.
    pub to: &'a str,
    pub label: Label,
    /// How many characters the text held.
    pub length: usize,
    pub answered: Answered,
    /// What became of it at the receiver, in a word from a fixed set.
    pub outcome: &'static str,
}

impl Sent<'_> {
    pub(crate) fn detail(&self) -> String {
        format!(
            "a message of {} character(s) from session {} to session {}, labelled {}: {}; {}",
            self.length,
            self.from,
            self.to,
            self.label,
            self.answered.words(),
            self.outcome
        )
    }
}
