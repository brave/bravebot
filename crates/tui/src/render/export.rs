//! The transcript outside a frame: as plain text for an editor, as markdown for a file, and laid
//! out at the last frame's size for a key that jumps.

use crate::state::{Laid, Session, Speaker};

use super::transcript::{lay_out, plain_rows};

/// The transcript as plain text, laid out exactly as it is drawn.
///
/// The rows as they are on the screen, margins included, so untrusted content is marked in the
/// file the same way it is marked on the screen and control characters are already glyphs. What
/// goes to an editor is what the person was looking at, not a second rendering of it that agrees
/// about the words and about nothing else. The cells the interface draws for a click, the word that
/// copies a block and the blanks before it, are not text of the transcript and are left out, as they
/// are from a copy.
pub fn as_text(session: &Session) -> String {
    plain_rows(session, session.laid.width, session.laid.height).join("\n")
}

/// The transcript as a markdown document, for exporting to a file.
///
/// Recounts the conversation (user prompts, model replies, tool calls) with headings.
pub fn as_markdown(session: &Session, title: &str) -> String {
    let mut out = String::new();

    let display_title = if title.trim().is_empty() {
        "Untitled Session"
    } else {
        title.trim()
    };
    out.push_str(&format!("# {display_title}\n\n"));

    let plans = session.todos_by_turn();
    let turns = session.transcript_turns();
    let running = session.a_turn_is_running().then_some(session.turns);
    let recorded: std::collections::BTreeMap<_, _> = session
        .turn_history()
        .iter()
        .map(|turn| (turn.number, turn))
        .collect();
    for (index, entry) in session.transcript.iter().enumerate() {
        match entry.speaker {
            Speaker::User => {
                out.push_str("## User\n\n");
                out.push_str(&entry.text);
                out.push_str("\n\n");
                if let Some(turn) = turns.get(&index) {
                    if let Some(recorded) = recorded.get(turn)
                        && let Some(outcome) = &recorded.outcome
                    {
                        out.push_str(&format!("**Outcome:** {}\n\n", outcome_word(outcome)));
                    }
                    if running == Some(*turn) {
                        out.push_str("**Outcome:** still running when exported\n\n");
                    }
                    if let Some(tokens) = session.spend_by_turn().get(turn) {
                        out.push_str(&format!("**Usage:** {tokens} tokens\n\n"));
                    }
                    if let Some(timing) = session.timing_by_turn().get(turn) {
                        out.push_str(&format!(
                            "**Timing:** wall {} ms; inference {} ms; tools {} ms; stalled {} ms\n\n",
                            timing.wall_ms, timing.inference_ms, timing.tools_ms, timing.stalled_ms
                        ));
                    }
                    if let Some(tasks) = plans.get(turn) {
                        out.push_str(&task_list(tasks));
                    }
                }
            }
            Speaker::Assistant => {
                if !entry.text.trim().is_empty() {
                    out.push_str("## Assistant\n\n");
                    out.push_str(&entry.text);
                    out.push_str("\n\n");
                }
            }
            Speaker::Tool => {
                out.push_str(&format!("*Ran: `{}`*\n\n", entry.text.trim()));
            }
            Speaker::Shell => {
                out.push_str("## Shell Command\n\n");
                out.push_str(&format!("`{}`\n\n", entry.text.trim()));
            }
            Speaker::Output => {
                let fence = if entry.text.contains("```") {
                    "````"
                } else {
                    "```"
                };
                out.push_str("## Shell Output\n\n");
                out.push_str(&format!("{fence}\n{}\n{fence}\n\n", entry.text.trim()));
            }
            // The heading is the entry's first line and the quoted words follow it.
            Speaker::Message => {
                let (heading, words) = entry.text.split_once('\n').unwrap_or((&entry.text, ""));
                out.push_str(&format!("## {}\n\n{words}\n\n", heading.trim()));
            }
            Speaker::Delegate => {
                if let Some(delegate) = &entry.delegate {
                    out.push_str(&format!("## Delegate {}\n\n", delegate.id));
                    out.push_str(&format!("{}\n\n", delegate.task.trim()));
                    if let Some(note) = &delegate.note {
                        out.push_str(&format!("{}\n\n", note.trim()));
                    }
                }
            }
            // Notes are left out: they are this program talking about itself, and an export is a
            // record of the exchange.
            Speaker::System => {}
            // Include outcomes so an unanswered prompt has an explanation in the export.
            Speaker::Failure | Speaker::Stopped => {
                out.push_str(if entry.speaker == Speaker::Stopped {
                    "## Cancelled\n\n"
                } else {
                    "## Failed\n\n"
                });
                out.push_str(&format!("{}\n\n", entry.text.trim()));
            }
        }

        if let Some(shown) = &entry.shown {
            out.push_str(&format!(
                "> *Quarantined: {} ({})*\n\n",
                shown.origin, shown.label
            ));
        }
    }

    out
}

/// The word an exported prompt carries for how its turn ended.
fn outcome_word(outcome: &bravebot_session::sessions::StoredOutcome) -> &'static str {
    match outcome {
        bravebot_session::sessions::StoredOutcome::Completed => "completed",
        bravebot_session::sessions::StoredOutcome::Failed { .. } => "failed",
        bravebot_session::sessions::StoredOutcome::Cancelled { .. } => "cancelled",
    }
}

/// A turn's task list: the heading, one checkbox line per task and a closing blank line.
fn task_list(tasks: &[bravebot_core::todo::Row]) -> String {
    let mut out = String::from("**Tasks:**\n\n");
    for task in tasks {
        let mark = if task.status == bravebot_core::todo::Status::Done {
            "x"
        } else {
            " "
        };
        out.push_str(&format!("- [{mark}] {} ({})\n", task.content, task.status));
    }
    out.push('\n');
    out
}

/// Lay the transcript out again at the size the last frame used.
///
/// What a key needs in order to jump: the rows a search matched, at the width they were matched
/// at. The size comes from the last frame because that is the frame the person is looking at when
/// they press the key.
pub fn as_last_drawn(session: &Session) -> Laid {
    as_drawn_at(session, session.laid.width, session.laid.height)
}

/// Lay the transcript out at a size the caller names, for a view about to be drawn at a size the
/// last frame did not have.
pub fn as_drawn_at(session: &Session, width: u16, height: u16) -> Laid {
    lay_out(session, width, height).1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Entry;

    /// MSG-11: an exported message between sessions is a heading with the quoted words under it,
    /// and one whose words were kept from the planner is a heading followed by the sentence saying
    /// so.
    #[test]
    fn a_message_between_sessions_is_exported_under_its_heading_with_the_words_quoted() {
        let mut session = Session::new("kernel-enforced");
        session.transcript.push(Entry::session_message(
            "message from session 11111111 (x) \u{b7} (T,pub)".to_string(),
            Some("first\nsecond"),
        ));
        session.transcript.push(Entry::session_message(
            "message to session 22222222 (y) \u{b7} (T,pub)".to_string(),
            None,
        ));
        let markdown = as_markdown(&session, "t");
        assert!(
            markdown.contains(
                "## message from session 11111111 (x) \u{b7} (T,pub)\n\n> first\n> second\n\n"
            ),
            "{markdown}"
        );
        assert!(
            markdown.contains(
                "## message to session 22222222 (y) \u{b7} (T,pub)\n\nits words were kept from the planner"
            ),
            "{markdown}"
        );
    }

    /// A finished task is ticked and every other status is left open, and the list is closed by a
    /// blank line so the next line of the export is not read as part of it.
    #[test]
    fn a_task_list_ticks_a_finished_task_and_ends_in_a_blank_line() {
        use bravebot_core::todo::{Row, Status};
        let row = |content: &str, status| Row {
            content: content.to_string(),
            marker: "",
            status,
        };
        let list = task_list(&[
            row("first", Status::Done),
            row("second", Status::Active),
            row("third", Status::Cancelled),
        ]);
        assert_eq!(
            list,
            "**Tasks:**\n\n- [x] first (completed)\n- [ ] second (in_progress)\n- [ ] third (cancelled)\n\n"
        );
    }
}
