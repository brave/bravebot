//! Opening a folder beside a session's project, on the terms `/add-dir` opens one in the terminal
//! (TRUST-9): reachable through the workspace, vouched for in the session's trust map, and kept in
//! the session's list of added directories so every later turn, a rewind and a resume open it
//! again.
//!
//! One function, so every way a desktop session comes to open a folder opens it the same way: a
//! yes to a directory a settings file named, and a folder the person picks.

use crate::running::State;
use bravebot_agent::Workspace;
use bravebot_agent::workspace::WorkspaceError;
use std::path::PathBuf;

/// Open `typed`, a name as a person or a settings file wrote it, in `workspace` and record it in
/// `state`: in its added directories, once, and in its trust map as vouched for. Returns the
/// directory opened, canonical.
///
/// `typed` is read against the project, with a leading `~` the home directory, as the terminal
/// reads it. A name the workspace refuses (one inside the project already, a credential location,
/// one the settings keep reads out of, or one that does not exist) opens nothing and records
/// nothing.
pub(crate) fn open_folder(
    workspace: &mut Workspace,
    state: &mut State,
    typed: &str,
) -> Result<PathBuf, WorkspaceError> {
    let named = bravebot_agent::opening::named_directory(
        workspace.root(),
        typed,
        bravebot_agent::home::profile().as_deref(),
    )?;
    let added = workspace.add_directory(&named)?;
    if !state.directories.contains(&added) {
        state.directories.push(added.clone());
    }
    state
        .answers
        .trust
        .trust(&bravebot_agent::workspace::key_of(&added));
    Ok(added)
}
