//! Naming a file with `@` from the window ([NAME-9](../../../docs/specs/naming-files.md#NAME-9)).
//!
//! The rules are `bravebot-mentions`', the ones the terminal uses. This module answers the two
//! questions the window asks of them, against the session's project: what to offer while a name is
//! typed, and which files a prompt names. Both answers go to a person, and neither reaches a model:
//! the files a turn reads are named again from the prompt by `turn.send` itself.

use crate::protocol::Failure;
use bravebot_agent::Workspace;
use bravebot_agent::workspace::WorkspaceError;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// What the window offers for `line`, with the cursor on row `cursor`.
///
/// `typed` is what follows the `@` of the last word, or null when the line is not being typed
/// towards a name, which is what closes the list. `completes` is whether Enter on that row
/// completes the name rather than sending the line (NAME-7).
///
/// Each entry carries `written`, the path as it is written in a line (each whitespace character and
/// each backslash behind a backslash, NAME-8), and the answer carries `word`, the last word of the
/// line, which is the part a completion replaces. Both come from `bravebot-mentions`, as they do in
/// the terminal, so the window splices them rather than working out where the word begins or how a
/// name is written (NAME-9).
///
/// The bots the line could consult come first, each marked `bot` with a `note` naming its model and
/// bound, as the terminal lists them (CONSULT-3).
///
/// `offerable` is asked for only once a name is being typed, because it opens the session's
/// workspace and the window asks about every line, most of which name nothing. It answers with the
/// reference directories and a row for each bot.
pub fn offer(
    project: &Path,
    line: &str,
    cursor: usize,
    offerable: impl FnOnce() -> Result<Offerable, Failure>,
) -> Result<Value, Failure> {
    let Some(typed) = bravebot_mentions::typed_reference(line) else {
        return Ok(json!({ "typed": null, "word": null, "entries": [], "completes": false }));
    };
    let Offerable { sources, bots } = offerable()?;
    let mut entries = bravebot_mentions::bots_matching(&typed, &bots);
    entries.extend(bravebot_mentions::matching(project, &typed, &sources));
    let completes = bravebot_mentions::enter_completes(project, &typed, &entries, cursor, &sources);
    let entries: Vec<Value> = entries
        .into_iter()
        .map(|entry| {
            json!({
                "path": entry.path,
                "written": bravebot_mentions::escape(&entry.path),
                "directory": entry.is_directory,
                "bot": entry.is_bot,
                "note": entry.note,
            })
        })
        .collect();
    let word = &line[bravebot_mentions::last_word_starts_at(line)..];
    Ok(json!({ "typed": typed, "word": word, "entries": entries, "completes": completes }))
}

/// What an `@` being typed could become besides a file in the project.
pub struct Offerable {
    pub sources: Vec<(String, PathBuf)>,
    pub bots: Vec<bravebot_mentions::Entry>,
}

/// A row for each bot an `@` could consult among `definitions` (CONSULT-3).
pub fn bots_offered(
    definitions: &bravebot_core::delegate::Definitions,
) -> Vec<bravebot_mentions::Entry> {
    bravebot_agent::agents::bots(definitions)
        .map(|definition| {
            bravebot_mentions::bot_entry(
                definition.name(),
                bravebot_agent::agents::bot_row(definition),
            )
        })
        .collect()
}

/// The files `prompt` names with `@`, in the order written, less the words that consult one of
/// `bots`, each surveyed by the read a turn makes of a named file: confined to the workspace, a
/// file that is there, and text.
///
/// A name that fails is a refused request naming it, so nothing is sent. Refused rather than left
/// out, because a file the person thinks went and did not would be worse than a send that says
/// why it stopped.
pub fn named(workspace: &Workspace, prompt: &str, bots: &[&str]) -> Result<Vec<String>, Failure> {
    bravebot_mentions::files_named(prompt, bots)
        .into_iter()
        // A name under a reference's alias is the file in that directory (REFER-6).
        .map(|name| {
            bravebot_mentions::resolved(workspace.root(), &name, &workspace.reference_sources())
                .map_err(|_| {
                    Failure::bad_request(format!(
                        "@{name} is under a reference directory whose path is not text, so it \
                         cannot be sent. Remove the @ to send it as text."
                    ))
                })
        })
        .map(|name| {
            name.and_then(|name| match workspace.survey(&name) {
            Ok(_) => Ok(name),
            Err(WorkspaceError::Binary { .. }) => Err(Failure::bad_request(format!(
                "@{name} is not a text file, so it cannot be sent. Remove the @ to send it as text."
            ))),
            Err(_) => Err(Failure::bad_request(format!(
                "@{name} is not a file in this project. Remove the @ to send it as text."
            ))),
        })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ErrorCode;
    use std::cell::Cell;

    fn nothing() -> Offerable {
        Offerable {
            sources: Vec::new(),
            bots: Vec::new(),
        }
    }

    /// CONSULT-3: a bot is offered above a file of its name, marked as a bot with its model and
    /// bound, and is written as its bare name.
    #[test]
    fn a_bot_is_offered_first_and_marked_as_one() {
        let root = tempfile::Builder::new()
            .prefix("bravebot-offer-bot-")
            .tempdir()
            .expect("a scratch project");
        std::fs::write(root.path().join("reviewer.md"), "").expect("file");
        let answer = offer(root.path(), "ask @rev", 0, || {
            Ok(Offerable {
                sources: Vec::new(),
                bots: vec![bravebot_mentions::bot_entry(
                    "reviewer",
                    "bot · haiku · up to 60 rounds".to_string(),
                )],
            })
        })
        .expect("answers");
        assert_eq!(answer["entries"][0]["path"], "reviewer");
        assert_eq!(answer["entries"][0]["written"], "reviewer");
        assert_eq!(answer["entries"][0]["bot"], true);
        assert_eq!(
            answer["entries"][0]["note"],
            "bot · haiku · up to 60 rounds"
        );
        assert_eq!(answer["entries"][0]["directory"], false);
        assert_eq!(answer["entries"][1]["path"], "reviewer.md");
        assert_eq!(answer["entries"][1]["bot"], false);
    }

    /// CONSULT-2: a word that consults a bot is not a file the send has to find.
    #[test]
    fn a_word_naming_a_bot_is_not_refused_as_a_missing_file() {
        let root = tempfile::Builder::new()
            .prefix("bravebot-named-bot-")
            .tempdir()
            .expect("a scratch project");
        let workspace = Workspace::new(root.path()).expect("a workspace");
        assert_eq!(
            named(&workspace, "ask @reviewer", &["reviewer"]).expect("named"),
            Vec::<String>::new()
        );
        assert!(named(&workspace, "ask @reviewer", &[]).is_err());
    }

    /// The window splices what the bridge hands it, so a name holding a space has to arrive
    /// escaped, and the word being replaced has to be the whole last word, escaped space and all.
    #[test]
    fn an_entry_with_a_space_is_offered_escaped_over_the_whole_word() {
        let root = tempfile::Builder::new()
            .prefix("bravebot-offer-space-")
            .tempdir()
            .expect("a scratch project");
        std::fs::create_dir_all(root.path().join("My Documents")).expect("directory");
        let answer = offer(root.path(), "read @My\\ Doc", 0, || Ok(nothing())).expect("answers");
        assert_eq!(answer["word"], "@My\\ Doc");
        assert_eq!(answer["entries"][0]["path"], "My Documents/");
        assert_eq!(answer["entries"][0]["written"], "My\\ Documents/");
    }

    /// NAME-9, NAME-11: the window is offered what the terminal is, so a fragment that begins
    /// nothing in the project finds the file below it, with the path a completion writes, and Enter
    /// completes it rather than sending the fragment.
    #[test]
    fn a_fragment_is_offered_the_file_below_it_and_enter_completes_it() {
        let root = tempfile::Builder::new()
            .prefix("bravebot-offer-found-")
            .tempdir()
            .expect("a scratch project");
        std::fs::create_dir_all(root.path().join("crates/cli/src")).expect("directory");
        std::fs::write(root.path().join("crates/cli/src/main.rs"), "").expect("file");
        let answer = offer(root.path(), "read @mainrs", 0, || Ok(nothing())).expect("answers");
        assert_eq!(answer["entries"][0]["path"], "crates/cli/src/main.rs");
        assert_eq!(answer["entries"][0]["written"], "crates/cli/src/main.rs");
        assert_eq!(answer["entries"][0]["directory"], false);
        assert_eq!(answer["completes"], true);
    }

    /// The window asks about every line it holds, and opening the session's workspace for each one
    /// made a turn waiting on an approval unable to be stopped. Only a name being typed needs it.
    #[test]
    fn a_line_naming_nothing_does_not_open_the_workspace() {
        let asked = Cell::new(false);
        let answer = offer(Path::new("."), "no reference here", 0, || {
            asked.set(true);
            Ok(nothing())
        })
        .expect("answers");
        assert!(!asked.get(), "the workspace was opened for a plain line");
        assert_eq!(answer["typed"], Value::Null);
    }

    /// A workspace that cannot be opened is the refusal of the request being typed towards a name,
    /// not an empty list that looks like nothing matched.
    #[test]
    fn a_name_being_typed_reports_a_workspace_that_will_not_open() {
        let refused = offer(Path::new("."), "read @par", 0, || {
            Err(Failure::new(ErrorCode::Internal, "no workspace"))
        })
        .expect_err("refused");
        assert_eq!(refused.message, "no workspace");
    }
}
