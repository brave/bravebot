//! What a turn asks for when a bot's conversation is past its budget and retrieval is on: the
//! earlier excerpts nearest the prompt, in place of a summary
//! ([RETRIEVE-13](../../../docs/specs/retrieval.md#RETRIEVE-13)).
//!
//! The turn decides where the cut falls and where the excerpts go. Which excerpts, and how they
//! read, is the [`Retriever`]'s, which the session gives a turn that may retrieve.

use crate::conversation::Conversation;
use std::fmt;
use std::time::Duration;

/// Finds the excerpts a turn sends in front of the messages its cut keeps.
pub trait Retriever: Send + Sync + fmt::Debug {
    /// The excerpts of the messages before `cut`, a place in the record as the vectors number it,
    /// that are nearest `query`, costing at most `budget` tokens.
    ///
    /// # Errors
    ///
    /// Why the excerpts could not be found, for the turn to compact instead
    /// ([RETRIEVE-7](../../../docs/specs/retrieval.md#RETRIEVE-7)).
    fn retrieve(
        &self,
        conversation: &Conversation,
        query: &str,
        cut: u32,
        budget: usize,
    ) -> Result<Recalled, Unavailable>;
}

/// What a retrieval found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recalled {
    /// The excerpts as the planner reads them, as one message.
    pub text: String,
    /// The place in the record and the chunk of each excerpt, in the order they were said.
    pub excerpts: Vec<(u32, u16)>,
    /// The position in `excerpts` of each one sent as a context line, cut short.
    pub context: Vec<u16>,
    /// The model that embedded what they were found by.
    pub model: String,
    /// How many chunks they were chosen from.
    pub considered: usize,
    /// About how many tokens the excerpts come to.
    pub tokens: u64,
    /// How long the query took, embedding included.
    pub latency: Duration,
}

/// Why a turn could not retrieve.
///
/// Each is a fixed sentence and a fixed word, never anything a service replied, so what the person
/// is told and what the trail keeps carry no content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unavailable {
    /// There is no key, or the endpoint refused the one sent.
    Key,
    /// The endpoint could not be reached.
    Unreachable,
    /// The endpoint answered with an error or with something that is not vectors.
    Failed,
    /// The query was not answered within its time.
    OutOfTime,
    /// The vectors have not yet caught up with the messages the cut passes.
    NotReady,
    /// There is no vectors file, so no pass has begun building one.
    Missing,
    /// The vectors file has to be rebuilt before it is read.
    Stale,
    /// The conversation's context is not trusted, so nothing of it is embedded.
    Untrusted,
    /// The embeddings service the settings choose is not set up: none is chosen, or the one chosen
    /// lacks its provider, its key, its model or the length of its vectors.
    Unconfigured,
}

impl Unavailable {
    /// The reason as the person is told it, to follow "Earlier messages weren't recalled this
    /// turn: ".
    pub fn reason(self) -> &'static str {
        match self {
            Self::Key => "the embeddings service did not accept the key",
            Self::Unreachable => "the embeddings service could not be reached",
            Self::Failed => "the embeddings service returned an error",
            Self::OutOfTime => "the embeddings service did not answer in time",
            Self::NotReady => "the earlier messages are still being indexed",
            Self::Missing => "the earlier messages have not been indexed",
            Self::Stale => "the index of earlier messages is being rebuilt",
            Self::Untrusted => "this conversation holds content that is not trusted",
            Self::Unconfigured => "the embeddings service chosen in settings is not set up",
        }
    }

    /// The reason as the trail keeps it.
    pub fn word(self) -> &'static str {
        match self {
            Self::Key => "key",
            Self::Unreachable => "unreachable",
            Self::Failed => "failed",
            Self::OutOfTime => "out of time",
            Self::NotReady => "not ready",
            Self::Missing => "missing",
            Self::Stale => "stale",
            Self::Untrusted => "untrusted",
            Self::Unconfigured => "not set up",
        }
    }
}

/// What the person is told the first time a turn could not retrieve and summarised instead.
pub fn fallback_narration(why: Unavailable) -> String {
    format!(
        "Earlier messages weren't recalled this turn: {}. Summarised instead.",
        why.reason()
    )
}
