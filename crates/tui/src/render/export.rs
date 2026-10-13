//! The transcript outside a frame: as plain text for an editor, as markdown for a file, and laid
//! out at the last frame's size for a key that jumps.

use crate::state::{Laid, Session, Speaker};

use super::transcript::lay_out;

/// The transcript as plain text, laid out exactly as it is drawn.
///
/// The rows as they are on the screen, margins included, so untrusted content is marked in the
/// file the same way it is marked on the screen and control characters are already glyphs. What
/// goes to an editor is what the person was looking at, not a second rendering of it that agrees
/// about the words and about nothing else.
pub fn as_text(session: &Session) -> String {
    let (lines, _) = lay_out(session, session.laid.width, session.laid.height);
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<String>>()
        .join("\n")
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
    let recorded: std::collections::BTreeMap<_, _> = session
        .turn_history()
        .iter()
        .map(|turn| (turn.number, turn))
        .collect();
    for (index, entry) in session.transcript.iter().enumerate() {
        match entry.speaker {
            crate::state::Speaker::User => {
                out.push_str("## User\n\n");
                out.push_str(&entry.text);
                out.push_str("\n\n");
                if let Some(turn) = turns.get(&index) {
                    if let Some(recorded) = recorded.get(turn)
                        && let Some(outcome) = &recorded.outcome
                    {
                        let outcome = match outcome {
                            bravebot_session::sessions::StoredOutcome::Completed => "completed",
                            bravebot_session::sessions::StoredOutcome::Failed { .. } => "failed",
                            bravebot_session::sessions::StoredOutcome::Cancelled { .. } => {
                                "cancelled"
                            }
                        };
                        out.push_str(&format!("**Outcome:** {outcome}\n\n"));
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
                        out.push_str("**Tasks:**\n\n");
                        for task in tasks {
                            let mark = if task.status == bravebot_core::todo::Status::Done {
                                "x"
                            } else {
                                " "
                            };
                            out.push_str(&format!(
                                "- [{mark}] {} ({})\n",
                                task.content, task.status
                            ));
                        }
                        out.push('\n');
                    }
                }
            }
            crate::state::Speaker::Assistant => {
                if !entry.text.trim().is_empty() {
                    out.push_str("## Assistant\n\n");
                    out.push_str(&entry.text);
                    out.push_str("\n\n");
                }
            }
            crate::state::Speaker::Tool => {
                out.push_str(&format!("*Ran: `{}`*\n\n", entry.text.trim()));
            }
            crate::state::Speaker::Shell => {
                out.push_str("## Shell Command\n\n");
                out.push_str(&format!("`{}`\n\n", entry.text.trim()));
            }
            crate::state::Speaker::Output => {
                let fence = if entry.text.contains("```") {
                    "````"
                } else {
                    "```"
                };
                out.push_str("## Shell Output\n\n");
                out.push_str(&format!("{fence}\n{}\n{fence}\n\n", entry.text.trim()));
            }
            crate::state::Speaker::Delegate => {
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
            crate::state::Speaker::System => {}
            // Include outcomes so an unanswered prompt has an explanation in the export.
            crate::state::Speaker::Failure | Speaker::Stopped => {
                out.push_str(if entry.speaker == crate::state::Speaker::Stopped {
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

/// Lay the transcript out again at the size the last frame used.
///
/// What a key needs in order to jump: the rows a search matched, at the width they were matched
/// at. The size comes from the last frame because that is the frame the person is looking at when
/// they press the key.
pub fn as_last_drawn(session: &Session) -> Laid {
    lay_out(session, session.laid.width, session.laid.height).1
}
