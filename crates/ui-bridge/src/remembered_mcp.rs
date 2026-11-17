//! A project's standing MCP answers on the Permissions channel: `mcpRemembered` in
//! `permissions.list`, and `permissions.revoke` with `kind: "mcp"`, which forgets them as
//! `bravebot mcp forget` does in the terminal.
//!
//! Both read the records when asked rather than when the session opened: they belong to every
//! session begun in the project, and another may have added to them since.
//!
//! A session holds no copy of these answers that would outlive a forget. A tool answer is read from
//! the record at each call (`Session::stands` in the agent), so the next call in this session asks
//! again. The answer about servers is read only as the session starts its servers: a server already
//! started keeps running for the rest of it, as one does in a terminal session when `bravebot mcp
//! forget` runs beside it, and the next session started in the project asks.

use crate::protocol::{ErrorCode, Failure};
use bravebot_agent::confirm::{shown_path, shown_text};
use bravebot_agent::mcp::{self, Remembered, Unforgotten, UnreadableRecord};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// The project the records name for a session in `project`: the absolute path it resolves to, as
/// the agent records answer 2 under it.
fn project_of(project: &Path) -> PathBuf {
    mcp::project_named(project, None)
}

/// `mcpRemembered`: what the state directory remembers for the session's project, or `null` where
/// there is no state directory and so nothing remembered.
///
/// `{ project, shownProject, everyServer, tools: [{ tool, shownTool }], forgettable }`, or
/// `{ project, shownProject, unreadable: { path, shownPath, reason }, forgettable: false }` for a
/// record that is there and cannot be read, which is neither listed as empty nor forgotten from.
/// `forgettable` is false in an incognito session, which writes nothing.
pub(crate) fn listed(project: &Path) -> Value {
    let Some(directory) = bravebot_agent::home::directory() else {
        return Value::Null;
    };
    let project = project_of(project);
    let incognito = bravebot_core::incognito::engaged();
    let mut listed = match mcp::remembered(&directory, &project) {
        Ok(remembered) => {
            let mut drawn = drawn(&remembered);
            drawn["forgettable"] = json!(!incognito);
            drawn
        }
        Err(record) => json!({ "unreadable": unreadable(&record), "forgettable": false }),
    };
    listed["project"] = json!(project.display().to_string());
    listed["shownProject"] = json!(shown_path(&project));
    listed
}

/// `permissions.revoke` with `kind: "mcp"`: forget what is remembered for the session's project,
/// and nothing recorded for any other, and return what was forgotten as `{ everyServer, tools }`.
pub(crate) fn forget(project: &Path) -> Result<Value, Failure> {
    // Before the state directory is looked for, so the refusal says why whatever else is missing.
    if bravebot_core::incognito::engaged() {
        return Err(incognito());
    }
    let directory = bravebot_agent::home::directory().ok_or_else(|| {
        Failure::new(
            ErrorCode::NoHome,
            "No MCP answers are remembered: there is no state directory.",
        )
    })?;
    let project = project_of(project);
    match mcp::forget(&directory, &project) {
        Ok(forgotten) => Ok(drawn(&forgotten)),
        Err(Unforgotten::Incognito) => Err(incognito()),
        Err(Unforgotten::Unreadable(record)) => Err(Failure::new(
            ErrorCode::StateUnusable,
            format!(
                "Nothing was forgotten: {} cannot be read ({}).",
                shown_path(&record.path),
                mcp::unreadable_record(&record.why)
            ),
        )),
        Err(Unforgotten::NotWritten {
            path,
            error,
            forgotten,
        }) => Err(not_written(&path, &error, &forgotten)),
    }
}

/// A record that could not be written, saying what was forgotten before it: the projects file is
/// written first, so a tools file that then fails leaves the answer about servers forgotten.
fn not_written(path: &Path, error: &std::io::Error, forgotten: &Remembered) -> Failure {
    let not_written = format!("{} could not be written: {error}", shown_path(path));
    Failure::new(
        ErrorCode::StateUnusable,
        if forgotten.every_server {
            format!(
                "The answer for every server was forgotten, and the tools are still remembered: {not_written}"
            )
        } else {
            format!("Nothing was forgotten: {not_written}")
        },
    )
}

fn incognito() -> Failure {
    Failure::bad_request(
        "An incognito session writes nothing, so it cannot forget remembered MCP answers.",
    )
}

fn drawn(remembered: &Remembered) -> Value {
    json!({
        "everyServer": remembered.every_server,
        "tools": remembered.tools.iter().map(|tool| json!({
            "tool": tool,
            "shownTool": shown_text(tool),
        })).collect::<Vec<_>>(),
    })
}

fn unreadable(record: &UnreadableRecord) -> Value {
    json!({
        "path": record.path.display().to_string(),
        "shownPath": shown_path(&record.path),
        "reason": mcp::unreadable_record(&record.why),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tools_file_not_written_says_the_server_answer_was_already_forgotten() {
        let error = std::io::Error::other("disk full");
        let path = Path::new("/state/mcp-tools");
        let half = not_written(
            path,
            &error,
            &Remembered {
                every_server: true,
                tools: Vec::new(),
            },
        );
        assert!(
            half.message
                .starts_with("The answer for every server was forgotten, and the tools are still remembered: "),
            "{}",
            half.message
        );
        assert!(half.message.ends_with("could not be written: disk full"), "{}", half.message);

        let none = not_written(path, &error, &Remembered::default());
        assert!(none.message.starts_with("Nothing was forgotten: "), "{}", none.message);
    }
}
