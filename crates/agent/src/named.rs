//! Whether a file named with `@` can be sent, asked before the line is.
//!
//! A turn reads each file the person named, and a name the read refuses ends the turn after the
//! prompt was recorded. The terminal and the desktop bridge both ask this first, so the refusal
//! comes before anything is sent and the line is left to be corrected. The answer is the same
//! survey the read makes ([`Workspace::survey`]), and it is a fact about the path and the file's
//! kind: it is asked of the name the person typed, never of what the file holds.

use crate::workspace::{Workspace, WorkspaceError};
use bravebot_i18n::t;

/// Why a named file cannot be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unsendable {
    /// The name is under a reference directory whose path has no text spelling.
    PathNotText,
    /// The file is there and is not text.
    Binary,
    /// Not a file the turn can read: absent, a directory, or outside the workspace.
    NotAFile,
}

/// The line a person reads for a name that cannot be sent, naming it and saying how to send it as
/// text. The same words in the terminal and in the desktop window.
pub fn refusal(name: &str, why: Unsendable) -> String {
    match why {
        Unsendable::PathNotText => t!(named_path_not_text, name = name).to_string(),
        Unsendable::Binary => t!(named_not_text, name = name).to_string(),
        Unsendable::NotAFile => t!(named_not_a_file, name = name).to_string(),
    }
}

/// Survey each of `names`, the names a line carries with `@`, in the order written, and say for each
/// the name as `resolve` left it, or the sentence that refuses it.
///
/// `resolve` turns a name as written into the one the workspace is asked about, which is
/// `bravebot_mentions::resolved` for both front ends. It lives there and not here because this
/// crate does not depend on `bravebot-mentions`. A name `resolve` gives no answer for is refused
/// as [`Unsendable::PathNotText`], and every other name is surveyed by [`survey`]. Every name is
/// surveyed, so a caller that reports only the first refusal and one that reports them all ask
/// the same question; neither is decided from what a file holds.
pub fn surveyed<E>(
    workspace: &Workspace,
    names: impl IntoIterator<Item = String>,
    mut resolve: impl FnMut(&str) -> Result<String, E>,
) -> Vec<Result<String, String>> {
    names
        .into_iter()
        .map(|name| {
            let resolved = resolve(&name).map_err(|_| refusal(&name, Unsendable::PathNotText))?;
            survey(workspace, &resolved)
                .map(|()| resolved)
                .map_err(|why| refusal(&name, why))
        })
        .collect()
}

/// Survey `name`, a name as `bravebot_mentions::resolved` left it, the way a turn's read will.
///
/// A name that fails is not sent: a file the person thinks went and did not is worse than a send
/// that says why it stopped.
pub fn survey(workspace: &Workspace, name: &str) -> Result<(), Unsendable> {
    match workspace.survey(name) {
        Ok(_) => Ok(()),
        Err(WorkspaceError::Binary { .. }) => Err(Unsendable::Binary),
        Err(_) => Err(Unsendable::NotAFile),
    }
}
