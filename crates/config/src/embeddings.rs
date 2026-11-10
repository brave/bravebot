//! Which service a bot's retrieval embeds with, and what the person chose for each one
//! (`docs/specs/retrieval.md`, RETRIEVE-33 and RETRIEVE-34).
//!
//! `retrieval.embeddings` in the person's own settings file:
//!
//! ```json
//! "retrieval": {
//!   "enabled": true,
//!   "embeddings": {
//!     "selected": "openrouter",
//!     "openai": { "model": "text-embedding-3-small" },
//!     "openrouter": { "model": "nvidia/nemotron-3.5-embed-1b:free", "dimensions": 2048 },
//!     "custom": { "url": "https://embed.example/v1/embeddings", "model": "m", "dimensions": 1024 }
//!   }
//! }
//! ```
//!
//! One service is selected at most. What was written for the others is kept, so switching back
//! finds it. The custom endpoint's key is never here: it is in [`crate::embeddings_key`].

use crate::settings::{object, word};
use serde_json::{Map, Value};

/// The key under `retrieval` this block is.
pub const BLOCK: &str = "embeddings";

/// A service retrieval can embed with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    /// OpenAI's own API, with the key the `openai` provider holds.
    OpenAi,
    /// Brave's embeddings endpoint.
    Brave,
    /// OpenRouter, with the key the `openrouter` provider holds.
    OpenRouter,
    /// An endpoint the person typed in, with the key stored for it.
    Custom,
}

impl Service {
    pub const ALL: [Self; 4] = [Self::OpenAi, Self::Brave, Self::OpenRouter, Self::Custom];

    /// The word the settings file names it by.
    pub fn word(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::Brave => "brave",
            Self::OpenRouter => "openrouter",
            Self::Custom => "custom",
        }
    }

    pub fn from_word(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|service| service.word() == word)
    }
}

/// What the person chose for one service. Every field is absence until written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Kept {
    pub model: Option<String>,
    /// The length of the vectors the model answers with, as a test call found it.
    pub dimensions: Option<usize>,
    /// Only the custom endpoint's is read.
    pub url: Option<String>,
    /// Put in front of a query, exactly as written: a prefix ends in a space more often than not.
    pub query_prefix: Option<String>,
    pub passage_prefix: Option<String>,
}

/// `retrieval.embeddings` as the settings in force state it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Choice {
    /// `None` where the block names no service, or one this build does not know.
    pub selected: Option<Service>,
    pub openai: Kept,
    pub brave: Kept,
    pub openrouter: Kept,
    pub custom: Kept,
}

impl Choice {
    /// The block under `retrieval` in `root`. A block that is absent or not an object is no choice.
    pub(crate) fn from_retrieval(retrieval: Option<&Map<String, Value>>) -> Self {
        let Some(block) = retrieval.and_then(|retrieval| object(retrieval, BLOCK)) else {
            return Self::default();
        };
        let kept = |service: Service| object(block, service.word()).map(read).unwrap_or_default();
        Self {
            selected: word(block, "selected").and_then(|word| Service::from_word(&word)),
            openai: kept(Service::OpenAi),
            brave: kept(Service::Brave),
            openrouter: kept(Service::OpenRouter),
            custom: kept(Service::Custom),
        }
    }

    pub fn kept(&self, service: Service) -> &Kept {
        match service {
            Service::OpenAi => &self.openai,
            Service::Brave => &self.brave,
            Service::OpenRouter => &self.openrouter,
            Service::Custom => &self.custom,
        }
    }
}

fn read(block: &Map<String, Value>) -> Kept {
    Kept {
        model: word(block, "model"),
        dimensions: block
            .get("dimensions")
            .and_then(Value::as_u64)
            .and_then(|count| usize::try_from(count).ok())
            .filter(|count| *count > 0),
        url: word(block, "url"),
        query_prefix: verbatim(block, "queryPrefix"),
        passage_prefix: verbatim(block, "passagePrefix"),
    }
}

/// A string as written, spaces and all; an empty one is a prefix of nothing, which is a choice.
fn verbatim(block: &Map<String, Value>, key: &str) -> Option<String> {
    block.get(key).and_then(Value::as_str).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choice(text: &str) -> Choice {
        let root: Map<String, Value> = serde_json::from_str(text).expect("json");
        Choice::from_retrieval(object(&root, "retrieval"))
    }

    #[test]
    fn nothing_written_is_no_choice() {
        assert_eq!(choice("{}"), Choice::default());
        assert_eq!(
            choice(r#"{"retrieval": {"enabled": true}}"#),
            Choice::default()
        );
        assert_eq!(
            choice(r#"{"retrieval": {"embeddings": "openai"}}"#),
            Choice::default()
        );
    }

    #[test]
    fn the_selection_and_every_services_values_are_read() {
        let read = choice(
            r#"{"retrieval": {"embeddings": {
                "selected": "custom",
                "openai": {"model": "text-embedding-3-large"},
                "openrouter": {"model": "cohere/embed-5-fast", "dimensions": 1024},
                "custom": {"url": " https://e.example/v1/embeddings ", "model": "m",
                           "dimensions": 384, "queryPrefix": "query: ", "passagePrefix": ""}
            }}}"#,
        );
        assert_eq!(read.selected, Some(Service::Custom));
        assert_eq!(read.openai.model.as_deref(), Some("text-embedding-3-large"));
        assert_eq!(read.openrouter.dimensions, Some(1024));
        assert_eq!(
            read.custom.url.as_deref(),
            Some("https://e.example/v1/embeddings")
        );
        assert_eq!(read.custom.query_prefix.as_deref(), Some("query: "));
        assert_eq!(read.custom.passage_prefix.as_deref(), Some(""));
        assert_eq!(read.brave, Kept::default());
    }

    #[test]
    fn an_unknown_service_or_a_malformed_field_is_absence() {
        let read = choice(
            r#"{"retrieval": {"embeddings": {
                "selected": "xai",
                "openai": {"model": 7, "dimensions": 0},
                "custom": {"dimensions": -3, "queryPrefix": 1}
            }}}"#,
        );
        assert_eq!(read.selected, None);
        assert_eq!(read.openai, Kept::default());
        assert_eq!(read.custom, Kept::default());
    }
}
