//! Searching what past sessions say (SESSION-33).
//!
//! A person remembers a session by something said in it, not by its title, which is the first
//! thing they asked. What is searched is what a resumed transcript draws: the prompts typed, the
//! planner's replies and the line announcing each call it made. A tool result, a file body and
//! a quarantined reference are not drawn, so none is searched or shown (SESSION-2).

use crate::sessions::{self, Front};
use bravebot_agent::conversation::{Conversation, Said, Snapshot};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

/// The longest line kept, in characters. A reply can run to pages on one line.
const LONGEST_LINE: usize = 2000;

/// How wide a shown match is, in characters.
const SNIPPET_WIDTH: usize = 100;

/// How many characters of context a shown match keeps before the words it found.
const SNIPPET_LEAD: usize = 30;

const HOUR: u64 = 3600;

/// A `since:` word that is not a number followed by `h`, `d` or `w`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadFilter;

/// What was typed: the words to find, and how recent a session has to be.
///
/// `since:<n>h`, `since:<n>d` or `since:<n>w` is a fixed word the driver parses. Whatever else was
/// typed is the phrase, whitespace collapsed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    phrase: String,
    window: Option<u64>,
}

impl Query {
    /// Everything typed as the phrase, a `since:` included.
    pub fn plain(typed: &str) -> Self {
        Self {
            phrase: collapse(&typed.to_lowercase()),
            window: None,
        }
    }

    /// Split a `since:` word from the phrase.
    pub fn parse(typed: &str) -> Result<Self, BadFilter> {
        let mut window = None;
        let mut words = Vec::new();
        for word in typed.split_whitespace() {
            match word.strip_prefix("since:") {
                Some(span) => window = Some(window_of(span).ok_or(BadFilter)?),
                None => words.push(word),
            }
        }
        Ok(Self {
            phrase: words.join(" ").to_lowercase(),
            window,
        })
    }

    /// The phrase, lowercased.
    pub fn phrase(&self) -> &str {
        &self.phrase
    }

    /// Whether a `since:` word limits the sessions.
    pub fn has_window(&self) -> bool {
        self.window.is_some()
    }

    /// Whether there are words to find.
    pub fn has_phrase(&self) -> bool {
        !self.phrase.is_empty()
    }

    /// Whether a session last written at `updated` is recent enough, `now` being seconds since the
    /// epoch.
    pub fn admits(&self, updated: u64, now: u64) -> bool {
        self.window
            .is_none_or(|window| now.saturating_sub(updated) <= window)
    }

    /// [`Query::admits`], against the clock.
    pub fn admits_now(&self, updated: u64) -> bool {
        self.admits(updated, sessions::now())
    }
}

fn window_of(span: &str) -> Option<u64> {
    let unit = match span.chars().last()? {
        'h' => HOUR,
        'd' => 24 * HOUR,
        'w' => 7 * 24 * HOUR,
        _ => return None,
    };
    let digits = &span[..span.len() - 1];
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse::<u64>().ok()?.checked_mul(unit)
}

/// What a project's sessions say, a line at a time, by session id.
#[derive(Debug, Default)]
pub struct Corpus {
    lines: HashMap<String, Vec<String>>,
}

impl Corpus {
    /// Read every record in a project's session directory.
    pub fn read(project: &Path) -> Self {
        let mut lines = HashMap::new();
        let Some(directory) = sessions::project_directory(project) else {
            return Self { lines };
        };
        let Ok(entries) = std::fs::read_dir(directory) else {
            return Self { lines };
        };
        for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if !sessions::is_a_session_name(id) {
                continue;
            }
            let Ok(contents) = std::fs::read_to_string(&path) else {
                continue;
            };
            lines.insert(id.to_string(), lines_of(&contents));
        }
        Self { lines }
    }

    /// A corpus of lines already cleaned, for a caller that has them.
    pub fn of(entries: impl IntoIterator<Item = (String, Vec<String>)>) -> Self {
        Self {
            lines: entries.into_iter().collect(),
        }
    }

    /// The first line of session `id` holding the phrase, cut to the width of a row.
    pub fn found(&self, id: &str, query: &Query) -> Option<String> {
        if !query.has_phrase() {
            return None;
        }
        self.lines
            .get(id)?
            .iter()
            .find_map(|line| snippet(line, query.phrase()))
    }
}

/// The record fields a search reads.
#[derive(Deserialize)]
struct Searched {
    #[serde(default)]
    front: Option<String>,
    conversation: Snapshot,
}

/// The lines of one record's JSON that a search covers, cleaned.
///
/// Nothing for a record written by anything but this program's own front ends: an imported
/// session holds another program's words (SESSION-32), and a record naming a front end this build
/// does not know could hold anything. Failing closed costs those sessions a place in the results.
pub fn lines_of(record: &str) -> Vec<String> {
    let Ok(searched) = serde_json::from_str::<Searched>(record) else {
        return Vec::new();
    };
    let ours = match searched.front.as_deref() {
        None => true,
        Some(word) => word == Front::Terminal.recorded() || word == Front::Desktop.recorded(),
    };
    if !ours {
        return Vec::new();
    }
    Conversation::restored(searched.conversation)
        .recounted()
        .into_iter()
        .filter_map(|said| match said {
            Said::User(text) | Said::Assistant(text) => Some(text),
            Said::Tool { line, .. } => Some(line),
            Said::Composed { .. } => None,
        })
        .flat_map(|text| {
            text.lines()
                .map(|line| clean(line, LONGEST_LINE))
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
        })
        .collect()
}

/// A line cut around the phrase if it holds it, to the width of a row.
fn snippet(line: &str, phrase: &str) -> Option<String> {
    let lowered = line.to_lowercase();
    let at = lowered.find(phrase)?;
    let before = lowered[..at].chars().count();
    let chars: Vec<char> = line.chars().collect();
    let from = before.saturating_sub(SNIPPET_LEAD).min(chars.len());
    let to = (from + SNIPPET_WIDTH).min(chars.len());
    let mut shown = String::new();
    if from > 0 {
        shown.push('…');
    }
    shown.extend(&chars[from..to]);
    if to < chars.len() {
        shown.push('…');
    }
    Some(shown)
}

/// Words with nothing in them a terminal would act on: control characters and the marks that
/// reorder or hide text become a space or nothing, runs of white space become one space, and the
/// result is cut to `longest` characters.
pub fn clean(text: &str, longest: usize) -> String {
    let kept: String = text
        .chars()
        .filter_map(|c| match c {
            '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2069}'
            | '\u{FEFF}' => None,
            c if c.is_control() => Some(' '),
            c => Some(c),
        })
        .take(longest)
        .collect();
    collapse(&kept)
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_agent::conversation::{Composed, Stored};
    use bravebot_aichat::protocol::{Message, ToolCallRequest, ToolCallRequestFunction};

    fn record(front: Option<&str>, messages: Vec<Stored>) -> String {
        let conversation = Conversation::new();
        let mut snapshot = conversation.snapshot();
        snapshot.messages = messages;
        let mut value = serde_json::json!({ "conversation": snapshot });
        if let Some(front) = front {
            value["front"] = front.into();
        }
        value.to_string()
    }

    fn said(text: &str) -> Stored {
        Stored::plain(Message::user(text))
    }

    fn answered(text: &str) -> Stored {
        Stored::plain(Message::assistant(text))
    }

    fn calling(tool: &str, arguments: &str) -> Stored {
        let mut message = Message::assistant("");
        message.tool_calls = Some(vec![ToolCallRequest {
            id: "call-1".into(),
            kind: "function".into(),
            function: ToolCallRequestFunction {
                name: tool.into(),
                arguments: arguments.into(),
            },
            extra_content: None,
        }]);
        Stored::plain(message)
    }

    fn found(lines: &[String], phrase: &str) -> bool {
        let query = Query::parse(phrase).expect("a query");
        Corpus::of([("s".to_string(), lines.to_vec())])
            .found("s", &query)
            .is_some()
    }

    #[test]
    fn a_typed_prompt_and_the_planners_reply_are_found() {
        let lines = lines_of(&record(
            Some("terminal"),
            vec![
                said("rotate the signing keys"),
                answered("Rotated the keys."),
            ],
        ));
        assert!(found(&lines, "signing keys"));
        assert!(found(&lines, "rotated"));
    }

    #[test]
    fn a_match_ignores_case_and_runs_of_white_space() {
        let lines = lines_of(&record(None, vec![said("Fix   the FLAKY\ttest")]));
        assert!(found(&lines, "flaky test"));
        assert!(found(&lines, "FIX the"));
    }

    #[test]
    fn the_path_a_call_named_is_found() {
        let lines = lines_of(&record(
            Some("desktop"),
            vec![calling("read_file", r#"{"path":"src/ledger/rounding.rs"}"#)],
        ));
        assert!(found(&lines, "ledger/rounding"));
    }

    #[test]
    fn a_tool_result_is_not_searched() {
        let result = Message::tool_result("call-1", "SECRET-FILE-BODY with a password");
        let lines = lines_of(&record(
            Some("terminal"),
            vec![
                said("read it"),
                calling("read_file", r#"{"path":"a.txt"}"#),
                Stored::plain(result),
            ],
        ));
        assert!(!found(&lines, "SECRET-FILE-BODY"));
        assert!(found(&lines, "read it"));
    }

    #[test]
    fn a_file_attached_to_a_prompt_is_not_searched() {
        let attached = Stored {
            message: Message::user("Contents of notes.md:\n\nthe tokenizer is broken"),
            composed: Some(Composed::Attached {
                path: "notes.md".into(),
            }),
            source: None,
        };
        let lines = lines_of(&record(Some("terminal"), vec![said("look"), attached]));
        assert!(!found(&lines, "tokenizer"));
    }

    #[test]
    fn an_imported_session_is_not_searched() {
        let imported = Stored {
            message: Message::user("deploy to the staging cluster"),
            composed: Some(Composed::Imported),
            source: None,
        };
        let plain = said("deploy to the staging cluster");
        assert!(lines_of(&record(Some("claude-code"), vec![imported])).is_empty());
        assert!(lines_of(&record(Some("claude-code"), vec![plain.clone()])).is_empty());
        assert!(lines_of(&record(Some("some-new-front"), vec![plain])).is_empty());
    }

    #[test]
    fn a_line_is_cleaned_of_what_a_terminal_would_act_on() {
        let lines = lines_of(&record(
            None,
            vec![said("clear\u{1b}[2J the \u{202E}screen\u{7} now")],
        ));
        let line = lines.first().expect("a line");
        assert!(
            line.chars().all(|c| !c.is_control() && c != '\u{202E}'),
            "{line:?}"
        );
        assert!(found(&lines, "the screen"));
    }

    #[test]
    fn a_shown_match_is_cut_around_the_words_and_no_wider_than_a_row() {
        let long = format!("{} needle {}", "a".repeat(400), "b".repeat(400));
        let lines = lines_of(&record(None, vec![said(&long)]));
        let query = Query::parse("needle").expect("a query");
        let shown = Corpus::of([("s".to_string(), lines)])
            .found("s", &query)
            .expect("found");
        assert!(shown.contains("needle"), "{shown}");
        assert!(shown.chars().count() <= SNIPPET_WIDTH + 2, "{shown}");
        assert!(shown.starts_with('…') && shown.ends_with('…'), "{shown}");
    }

    #[test]
    fn a_since_word_is_a_window_and_leaves_the_rest_as_the_phrase() {
        let query = Query::parse("fix since:2d flaky").expect("a query");
        assert_eq!(query.phrase(), "fix flaky");
        let now = 10 * 24 * HOUR;
        assert!(query.admits(now - 2 * 24 * HOUR, now));
        assert!(!query.admits(now - 2 * 24 * HOUR - 1, now));
        assert!(Query::parse("fix").expect("a query").admits(0, now));
        assert!(
            Query::parse("since:3h")
                .expect("a query")
                .admits(now - 3 * HOUR, now)
        );
        assert!(
            !Query::parse("since:1w")
                .expect("a query")
                .admits(now - 8 * 24 * HOUR, now)
        );
    }

    #[test]
    fn a_malformed_since_word_is_refused_rather_than_ignored() {
        for typed in [
            "since:",
            "since:d",
            "since:3",
            "since:3m",
            "since:-1d",
            "since:1.5d",
        ] {
            assert_eq!(Query::parse(typed), Err(BadFilter), "{typed}");
        }
    }

    #[test]
    fn a_plain_query_keeps_a_since_word_as_text() {
        let query = Query::plain("since:3m");
        assert_eq!(query.phrase(), "since:3m");
        assert!(query.admits(0, u64::MAX));
    }

    #[test]
    fn the_corpus_reads_a_projects_records_and_nothing_else_in_its_directory() {
        if !crate::test_profile::in_isolated_profile() {
            return;
        }
        let project = crate::test_profile::project("bravebot-search-corpus");
        let directory = sessions::project_directory(&project).expect("a directory");
        std::fs::create_dir_all(&directory).expect("create");
        let body = record(Some("terminal"), vec![said("rotate the signing keys")]);
        std::fs::write(directory.join("a1b2.json"), &body).expect("write");
        std::fs::write(directory.join("not a name.json"), &body).expect("write");
        std::fs::write(
            directory.join("a1b2.audit.jsonl"),
            "rotate the signing keys",
        )
        .expect("write");

        let corpus = Corpus::read(&project);
        let query = Query::parse("signing keys").expect("a query");
        assert!(corpus.found("a1b2", &query).is_some());
        assert!(corpus.found("not a name", &query).is_none());
        assert!(corpus.found("a1b2.audit", &query).is_none());
    }

    #[test]
    fn a_query_with_no_words_finds_no_line() {
        let query = Query::parse("since:1d").expect("a query");
        let corpus = Corpus::of([("s".to_string(), vec!["anything".to_string()])]);
        assert!(corpus.found("s", &query).is_none());
    }
}
