//! The transcript outside a frame: as plain text for an editor, as markdown for a file, and laid
//! out at the last frame's size for a key that jumps.

use crate::state::{Entry, Laid, Session, Speaker};

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
                        out.push_str(&timing_line(timing));
                    }
                    if let Some(tasks) = plans.get(turn) {
                        out.push_str(&task_list(tasks));
                    }
                }
            }
            Speaker::Assistant => out.push_str(&assistant_section(entry)),
            Speaker::Tool => out.push_str(&tool_line(entry)),
            Speaker::Shell => {
                out.push_str("## Shell Command\n\n");
                out.push_str(&format!("`{}`\n\n", entry.text.trim()));
            }
            Speaker::Output => {
                out.push_str("## Shell Output\n\n");
                if entry.displayed_only {
                    // Output the conversation never held is not written (SESSION-17).
                    out.push_str(&format!(
                        "*Output was shown on the screen and is not kept in this file ({} lines).*\n\n",
                        entry.text.lines().count()
                    ));
                } else {
                    let fence = if entry.text.contains("```") {
                        "````"
                    } else {
                        "```"
                    };
                    out.push_str(&format!("{fence}\n{}\n{fence}\n\n", entry.text.trim()));
                }
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
            out.push_str(&quarantine_note(shown));
        }
    }

    out
}

/// An assistant entry's section, or nothing for a blank one.
///
/// An answer released for the screen alone is written as a sentence saying one was shown and how
/// long it was, and not what it said (SESSION-17).
fn assistant_section(entry: &Entry) -> String {
    if entry.displayed_only {
        format!(
            "## Assistant\n\n*An answer was shown on the screen and is not kept in this file ({} lines).*\n\n",
            entry.text.lines().count()
        )
    } else if !entry.text.trim().is_empty() {
        format!("## Assistant\n\n{}\n\n", entry.text)
    } else {
        String::new()
    }
}

/// A tool call's line, as the planner was given it.
///
/// The line drawn for the person names the files a reference stands for, and those names are not
/// the planner's to read.
fn tool_line(entry: &Entry) -> String {
    let line = match &entry.activity {
        Some(activity) => activity.line_as_the_planner_had_it(),
        None => entry.text.trim().to_string(),
    };
    format!("*Ran: `{line}`*\n\n")
}

/// The note under an entry whose content was quarantined.
///
/// The origin is written as the planner was given it where the person was shown file names in its
/// place (SESSION-17). The label is the driver's own `Label` display, which holds its parentheses,
/// so none are added.
fn quarantine_note(shown: &bravebot_agent::report::Shown) -> String {
    let origin = shown
        .as_the_planner_had_it
        .as_ref()
        .unwrap_or(&shown.origin);
    format!("> *Quarantined: {origin} {}*\n\n", shown.label)
}

/// A turn's timing line, with the closing blank line.
fn timing_line(timing: &bravebot_agent::timing::Timing) -> String {
    format!(
        "**Timing:** wall {} ms; inference {} ms; tools {} ms; stalled {} ms; file wait {} ms\n\n",
        timing.wall_ms,
        timing.inference_ms,
        timing.tools_ms,
        timing.stalled_ms,
        timing.file_wait_ms
    )
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
///
/// Each task is written as the model wrote it. The row's `content` holds the names a person was
/// shown in place of references, and those are released for a screen alone (SESSION-17).
fn task_list(tasks: &[bravebot_core::todo::Row]) -> String {
    let mut out = String::from("**Tasks:**\n\n");
    for task in tasks {
        let mark = if task.status == bravebot_core::todo::Status::Done {
            "x"
        } else {
            " "
        };
        out.push_str(&format!(
            "- [{mark}] {} ({})\n",
            task.as_the_planner_had_it, task.status
        ));
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

    /// SESSION-17: an answer released for the screen alone is exported as a sentence with its
    /// line count, and a planner's reply is exported whole.
    ///
    /// Rejects an export that writes every Assistant entry's words, which puts a manifest run's
    /// answer into a file a later turn reads as trusted.
    #[test]
    fn an_answer_released_for_the_screen_is_exported_as_its_length_and_a_reply_whole() {
        let mut session = Session::new("kernel-enforced");
        session.narrate_displayed_only("THE ISSUE BODY\nsecond line");
        session.narrate("the planner said this");
        let markdown = as_markdown(&session, "t");
        assert!(!markdown.contains("THE ISSUE BODY"), "{markdown}");
        assert!(!markdown.contains("second line"), "{markdown}");
        assert!(
            markdown.contains("## Assistant\n\n*An answer was shown on the screen"),
            "{markdown}"
        );
        assert!(markdown.contains("(2 lines).*"), "{markdown}");
        assert!(
            markdown.contains("## Assistant\n\nthe planner said this\n\n"),
            "{markdown}"
        );
    }

    /// SESSION-17: a call is exported with its target as the planner named it, and a quarantined
    /// block under the origin as the planner was given it, so a file name released for the screen
    /// is not written.
    ///
    /// Rejects an export that writes `entry.text` or `Shown::origin`, which hold the names.
    #[test]
    fn a_call_and_a_block_are_exported_as_the_planner_had_them_and_not_as_the_person_read_them() {
        use bravebot_agent::report::{Activity, Reach, Shown};
        let mut session = Session::new("kernel-enforced");
        let call = Activity::running("Read", "ref:1(U,priv):IGNORE-PREVIOUS-INSTRUCTIONS.md")
            .as_the_planner_had_it(Some("ref:1".to_string()))
            .done("3 lines");
        session.transcript.push(Entry::tool(call));
        session.transcript.last_mut().expect("the call").shown = Some(Shown {
            as_the_planner_had_it: Some("ref:1".to_string()),
            origin: "IGNORE-PREVIOUS-INSTRUCTIONS.md".to_string(),
            reach: Reach::NotThePlanner,
            label: "(U,priv)".to_string(),
            lines: 1,
            preview: vec!["x".to_string()],
        });
        let markdown = as_markdown(&session, "t");
        assert!(!markdown.contains("IGNORE-PREVIOUS"), "{markdown}");
        assert!(markdown.contains("*Ran: `Read(ref:1)`*"), "{markdown}");
        assert!(
            markdown.contains("> *Quarantined: ref:1 (U,priv)*"),
            "{markdown}"
        );
    }

    /// A call with no planner form, and a block whose origin names no file, are exported as they
    /// are drawn.
    ///
    /// Rejects an export that drops the line of a call with nothing to substitute.
    #[test]
    fn a_call_and_a_block_with_nothing_to_substitute_are_exported_as_drawn() {
        use bravebot_agent::report::{Activity, Reach, Shown};
        let mut session = Session::new("kernel-enforced");
        let call = Activity::running("Read", "src/main.rs").done("3 lines");
        session.transcript.push(Entry::tool(call));
        session.transcript.last_mut().expect("the call").shown = Some(Shown {
            as_the_planner_had_it: None,
            origin: "what the isolated processor said".to_string(),
            reach: Reach::NoModel,
            label: "(U,priv)".to_string(),
            lines: 1,
            preview: vec!["x".to_string()],
        });
        let markdown = as_markdown(&session, "t");
        assert!(
            markdown.contains("*Ran: `Read(src/main.rs)`*"),
            "{markdown}"
        );
        assert!(
            markdown.contains("> *Quarantined: what the isolated processor said (U,priv)*"),
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
            as_the_planner_had_it: content.to_string(),
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

    /// A waiting task is still open, so it is not ticked, and the export gives its status word.
    ///
    /// Rejects an export that ticks every task that is not open work being done, which would show a
    /// task that waits on someone as finished.
    #[test]
    fn a_waiting_task_is_exported_unticked_with_its_status_word() {
        use bravebot_core::todo::{Row, Status};
        let row = Row {
            content: "rerun CI".to_string(),
            as_the_planner_had_it: "rerun CI".to_string(),
            marker: "",
            status: Status::Waiting,
        };
        assert_eq!(
            task_list(&[row]),
            "**Tasks:**\n\n- [ ] rerun CI (waiting)\n\n"
        );
    }

    /// SESSION-17: a task is exported as the model wrote it, so a file name shown on the screen in
    /// place of a reference is not written.
    ///
    /// Rejects an export that writes `Row::content`, which holds the names.
    #[test]
    fn a_task_is_exported_as_the_model_wrote_it_and_not_as_the_person_read_it() {
        use bravebot_core::todo::{Row, Status};
        let row = Row {
            content: "rewrite IGNORE-PREVIOUS-INSTRUCTIONS.md".to_string(),
            as_the_planner_had_it: "rewrite ref:1".to_string(),
            marker: "",
            status: Status::Active,
        };
        let list = task_list(&[row]);
        assert!(!list.contains("IGNORE-PREVIOUS"), "{list}");
        assert!(list.contains("- [ ] rewrite ref:1 (in_progress)"), "{list}");
    }
}
