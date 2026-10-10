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
/// `sources` is asked for only once a name is being typed, because it opens the session's
/// workspace and the window asks about every line, most of which name nothing.
pub fn offer(
    project: &Path,
    line: &str,
    cursor: usize,
    sources: impl FnOnce() -> Result<Vec<(String, PathBuf)>, Failure>,
) -> Result<Value, Failure> {
    let Some(typed) = bravebot_mentions::typed_reference(line) else {
        return Ok(json!({ "typed": null, "entries": [], "completes": false }));
    };
    let sources = sources()?;
    let entries = bravebot_mentions::matching(project, &typed, &sources);
    let completes = bravebot_mentions::enter_completes(project, &typed, &entries, cursor, &sources);
    let entries: Vec<Value> = entries
        .into_iter()
        .map(|entry| json!({ "path": entry.path, "directory": entry.is_directory }))
        .collect();
    Ok(json!({ "typed": typed, "entries": entries, "completes": completes }))
}

/// The files `prompt` names with `@`, in the order written, each surveyed by the read a turn
/// makes of a named file: confined to the workspace, a file that is there, and text.
///
/// A name that fails is a refused request naming it, so nothing is sent. Refused rather than left
/// out, because a file the person thinks went and did not would be worse than a send that says
/// why it stopped.
pub fn named(workspace: &Workspace, prompt: &str) -> Result<Vec<String>, Failure> {
    bravebot_mentions::referenced(prompt)
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

    /// The window asks about every line it holds, and opening the session's workspace for each one
    /// made a turn waiting on an approval unable to be stopped. Only a name being typed needs it.
    #[test]
    fn a_line_naming_nothing_does_not_open_the_workspace() {
        let asked = Cell::new(false);
        let answer = offer(Path::new("."), "no reference here", 0, || {
            asked.set(true);
            Ok(Vec::new())
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
