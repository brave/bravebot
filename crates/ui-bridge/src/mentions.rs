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
use std::path::Path;

/// What the window offers for `line`, with the cursor on row `cursor`.
///
/// `typed` is what follows the `@` of the last word, or null when the line is not being typed
/// towards a name, which is what closes the list. `completes` is whether Enter on that row
/// completes the name rather than sending the line (NAME-7).
pub fn offer(project: &Path, line: &str, cursor: usize) -> Value {
    let Some(typed) = bravebot_mentions::typed_reference(line) else {
        return json!({ "typed": null, "entries": [], "completes": false });
    };
    let entries = bravebot_mentions::matching(project, &typed);
    let completes = bravebot_mentions::enter_completes(project, &typed, &entries, cursor);
    let entries: Vec<Value> = entries
        .into_iter()
        .map(|entry| json!({ "path": entry.path, "directory": entry.is_directory }))
        .collect();
    json!({ "typed": typed, "entries": entries, "completes": completes })
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
        .map(|name| match workspace.survey(&name) {
            Ok(_) => Ok(name),
            Err(WorkspaceError::Binary { .. }) => Err(Failure::bad_request(format!(
                "@{name} is not a text file, so it cannot be sent. Remove the @ to send it as text."
            ))),
            Err(_) => Err(Failure::bad_request(format!(
                "@{name} is not a file in this project. Remove the @ to send it as text."
            ))),
        })
        .collect()
}
