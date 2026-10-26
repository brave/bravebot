//! One fact: a file of text with what it says about itself at its top (RECALL-5).

use crate::skills::{body_after_frontmatter, declarations};
use bravebot_core::label::Confidentiality;

/// Who asked for a fact to be kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The person asked for it to be remembered.
    Person,
    /// The planner judged it worth keeping.
    Agent,
}

/// Where a fact applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Every project.
    Everywhere,
    /// The project it was saved in.
    Project,
}

/// A fact as its file holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    pub key: String,
    pub source: Source,
    pub label: Confidentiality,
    /// `YYYY-MM-DD`.
    pub created: Option<String>,
    /// `YYYY-MM-DD`.
    pub verified: Option<String>,
    pub superseded_by: Option<String>,
    pub session: Option<String>,
    pub tags: Vec<String>,
    pub body: String,
}

/// The most segments a key may have.
const MAX_SEGMENTS: usize = 8;

/// Whether `key` is a slug path: 1 to 8 segments split by `/`, each `[a-z0-9][a-z0-9-]*`.
pub fn is_a_key(key: &str) -> bool {
    let segments: Vec<&str> = key.split('/').collect();
    segments.len() <= MAX_SEGMENTS
        && segments.iter().all(|segment| {
            let mut chars = segment.chars();
            chars
                .next()
                .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
                && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
}

impl Fact {
    /// Read a fact from its file's text.
    ///
    /// A file with no front matter, an unclosed block, or a field missing reads as an agent fact
    /// kept private with no dates, so nothing a person edits by hand can make a fact more
    /// trusted or less confidential than it was written.
    pub fn parse(key: &str, text: &str) -> Fact {
        let declared = declarations(text).unwrap_or_default();
        let field = |name: &str| declared.get(name).filter(|v| !v.is_empty()).cloned();
        Fact {
            key: key.to_string(),
            source: match field("source").as_deref() {
                Some("person") => Source::Person,
                _ => Source::Agent,
            },
            label: match field("label").as_deref() {
                Some("public") => Confidentiality::Public,
                _ => Confidentiality::Private,
            },
            created: field("created"),
            verified: field("verified"),
            superseded_by: field("superseded-by"),
            session: field("session"),
            tags: field("tags")
                .map(|tags| {
                    tags.split(',')
                        .map(|tag| tag.trim().to_string())
                        .filter(|tag| !tag.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            body: body_after_frontmatter(text).to_string(),
        }
    }

    /// The file's text, fields in the order RECALL-5 lists them and unset ones left out.
    pub fn render(&self) -> String {
        let mut out = String::from("---\n");
        out.push_str(match self.source {
            Source::Person => "source: person\n",
            Source::Agent => "source: agent\n",
        });
        out.push_str(match self.label {
            Confidentiality::Public => "label: public\n",
            Confidentiality::Private => "label: private\n",
        });
        let optional = [
            ("created", &self.created),
            ("verified", &self.verified),
            ("superseded-by", &self.superseded_by),
            ("session", &self.session),
        ];
        for (name, value) in optional {
            if let Some(value) = value {
                out.push_str(&format!("{name}: {value}\n"));
            }
        }
        if !self.tags.is_empty() {
            out.push_str(&format!("tags: {}\n", self.tags.join(", ")));
        }
        out.push_str("---\n");
        out.push_str(&self.body);
        out
    }

    /// Whether no other fact has replaced this one.
    pub fn is_live(&self) -> bool {
        self.superseded_by.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fact_round_trips_through_its_file() {
        let fact = Fact {
            key: "release/forge-poll".to_string(),
            source: Source::Person,
            label: Confidentiality::Public,
            created: Some("2026-10-01".to_string()),
            verified: Some("2026-10-20".to_string()),
            superseded_by: Some("release/forge-watch".to_string()),
            session: Some("abc123".to_string()),
            tags: vec!["forge".to_string(), "release".to_string()],
            body: "The forge is polled every minute.\n".to_string(),
        };
        assert_eq!(Fact::parse(&fact.key, &fact.render()), fact);
        assert!(!fact.is_live());
    }

    #[test]
    fn a_file_with_no_front_matter_is_an_agent_fact_kept_private() {
        let fact = Fact::parse("k", "just text");
        assert_eq!(fact.source, Source::Agent);
        assert_eq!(fact.label, Confidentiality::Private);
        assert_eq!(fact.body, "just text");
        assert_eq!((&fact.created, &fact.verified), (&None, &None));
        assert!(fact.is_live());
    }

    #[test]
    fn an_unclosed_front_matter_is_read_as_body() {
        let text = "---\nsource: person\nno close";
        let fact = Fact::parse("k", text);
        assert_eq!(fact.source, Source::Agent);
        assert_eq!(fact.label, Confidentiality::Private);
        assert_eq!(fact.body, text);
    }

    #[test]
    fn a_key_is_a_slug_path() {
        assert!(is_a_key("release/forge-poll"));
        assert!(is_a_key("a"));
        assert!(is_a_key(&["a"; 8].join("/")));
        for bad in ["../x", "A/b", "a//b", "", "-a", "a/"] {
            assert!(!is_a_key(bad), "{bad:?}");
        }
        assert!(!is_a_key(&["a"; 9].join("/")));
    }
}
