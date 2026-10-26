//! The lines of a file a person clicked a path in the transcript to see (MOUSE-42).
//!
//! They are added to the end of the transcript as a call and a quarantined block under it, the way
//! the head of a file nobody vouched for is drawn under a read: behind the bar in the margin, with the
//! file's name and the label it carries in the heading, and with the line saying that no model has it.
//! What is in the block went from the file to the screen through the gates of
//! [`bravebot_agent::file_preview`], and none of it is kept anywhere a model or a later session reads.
//!
//! Only what is drawn is here. Whether a name is a file the session may read, and the lines of it,
//! are the agent's.

use crate::state::{Entry, Session};
use crate::text_links::PathRef;
use bravebot_agent::file_preview::{self, Refused};
use bravebot_agent::report::{Activity, Reach, Shown};
use bravebot_agent::workspace::Workspace;
use bravebot_core::permissions::Permissions;
use bravebot_core::trust::TrustStore;
use bravebot_i18n::t;
use bravebot_session::audit::Trail;

/// Show the lines of the file `target` names, or say why there are none, and hand back what the
/// gates recorded, for the session's audit.
///
/// A name the session cannot read is said without repeating it: it is text the assistant wrote, and
/// a note is the interface's own voice. The view goes to the end of the transcript, where the lines
/// are, since a click that showed nothing on the screen would read as one that did nothing.
pub fn show(
    session: &mut Session,
    workspace: &Workspace,
    trust: &TrustStore,
    permissions: &Permissions,
    target: &PathRef,
) -> Trail {
    let mut trail = Trail::new();
    let from = target.line.map_or(1, |line| line as usize);
    let shown = file_preview::preview(
        workspace,
        trust,
        permissions,
        &mut trail,
        &target.path,
        from,
    );
    match shown {
        Ok(found) => {
            let heading = Activity::running("Preview", found.named.clone())
                .naming_a_file(true)
                .done(t!(session_preview_lines, count = found.total).to_string());
            let mut entry = Entry::tool(heading);
            entry.shown = Some(Shown {
                origin: found.named,
                reach: Reach::NoModel,
                label: found.label,
                preview: found.lines,
                lines: found.total,
            });
            session.transcript.push(entry);
        }
        Err(refused) => session.note(refusal(&refused)),
    }
    session.scroll = 0;
    trail
}

/// What a refusal says, none of it the name that was asked for.
fn refusal(refused: &Refused) -> String {
    match refused {
        Refused::NotAFile => t!(session_path_not_a_file).to_string(),
        Refused::Denied => t!(session_preview_denied).to_string(),
        Refused::TooLarge(bytes) => t!(
            session_preview_too_large,
            bytes = bytes,
            limit = file_preview::MOST_BYTES
        )
        .to_string(),
        Refused::Policy(problem) => t!(session_preview_unavailable, problem = problem).to_string(),
    }
}
