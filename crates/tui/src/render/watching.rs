//! The watching view: the list of delegates, asides and command outputs, and the one opened
//! from it.

use bravebot_i18n::t;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};

use crate::state::{Delegate, Laid, Output, Session, Watched};
use crate::theme;

use super::transcript::{assistant_lines, draw_transcript};
use super::{QUARANTINE_BAR, TURN_MARKER, dim, marked_rows, one_line};

/// Draw the delegate view: the list of them, or the one somebody opened.
///
/// Two screens rather than one with a panel, because they answer different questions. The list
/// answers which delegate, and is read once; the delegate answers what it is doing, and is read
/// for as long as it runs.
pub(super) fn draw_watching(frame: &mut Frame, session: &Session) -> Laid {
    if session.listing_delegates() {
        return draw_delegate_list(frame, session);
    }
    if let Some(aside) = session.watched_aside() {
        return draw_aside(frame, session, aside);
    }
    if let Some(output) = session.watched_output() {
        return draw_output(frame, session, output);
    }

    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // which delegate, and what it was asked
            Constraint::Min(1),    // its own lines
            Constraint::Length(1), // footer
        ])
        .split(frame.area());

    draw_delegate_head(frame, areas[0], session);
    let laid = draw_transcript(frame, areas[1], session);
    draw_watching_footer(frame, areas[2], session);

    if let Some(selection) = &session.selection {
        crate::select::highlight(frame.buffer_mut(), selection);
    }

    laid
}

/// Draw a question asked beside the work, and the answer it came back with.
///
/// The same shape as a delegate's view and a command's: a header saying what this is, the thing
/// itself, and a footer with the way out. What differs is that both halves are drawn, because a
/// question with no answer under it is half of what a person came here to read, and the answer
/// alone stops making sense the moment there is more than one aside in the list.
///
/// The question is drawn the way a prompt is drawn in the transcript and the answer the way a
/// reply is, so somebody arriving here reads it as the exchange it is. That it is an exchange the
/// conversation never had is what the header says.
fn draw_aside(
    frame: &mut Frame,
    session: &Session,
    aside: &bravebot_session::sessions::Aside,
) -> Laid {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // what this is, and whether the record keeps it
            Constraint::Min(1),    // the question and the answer
            Constraint::Length(1), // footer
        ])
        .split(frame.area());

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(
                    format!("{TURN_MARKER} "),
                    Style::default().fg(theme::brand_primary()),
                ),
                Span::styled(
                    t!(watching_aside_head),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ]),
            // Said here as well as on the row, because the row is a line in a list and this is
            // the screen somebody reads the answer on. An answer the record will not keep is
            // worth knowing about while it is still on the screen to copy.
            Line::from(Span::styled(
                match aside.kept {
                    true => format!("  {}", t!(watching_aside_answer)),
                    false => format!("  {}", t!(watching_aside_not_kept)),
                },
                match aside.kept {
                    true => dim(),
                    false => Style::default().fg(theme::running()),
                },
            )),
        ]),
        areas[0],
    );

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        format!("  {}", t!(watching_aside_question)),
        dim(),
    )));
    for (index, text) in aside.question.lines().enumerate() {
        let prefix = if index == 0 { "> " } else { "  " };
        lines.push(Line::from(Span::styled(
            format!("{prefix}{text}"),
            Style::default().fg(theme::brand_primary()),
        )));
    }
    lines.push(Line::raw(""));
    match &aside.answer {
        Some(answer) => lines.extend(assistant_lines(answer, areas[1].width)),
        // A resumed aside whose answer the record could not hold. Said rather than drawn as an
        // empty screen, which reads as an answer that was never given.
        None => lines.push(Line::from(Span::styled(
            format!("  {}", t!(watching_aside_gone)),
            dim(),
        ))),
    }

    // Counted back from the end, the way the transcript is, so the same keys walk back through a
    // long answer. Counted in drawn rows, because the paragraph wraps: an answer that is one long
    // line would otherwise be clipped at the edge of the screen.
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    let total = paragraph.line_count(areas[1].width) as u16;
    let max_offset = total.saturating_sub(areas[1].height);
    let offset = max_offset.saturating_sub(session.scroll.min(max_offset));
    frame.render_widget(paragraph.scroll((offset, 0)), areas[1]);

    draw_watching_footer(frame, areas[2], session);
    if let Some(selection) = &session.selection {
        crate::select::highlight(frame.buffer_mut(), selection);
    }
    Laid::default()
}

/// Draw what one command printed, in full as far as it is kept.
///
/// The same shape as a delegate's view, because a person arriving here has already learned it:
/// a header saying what they are looking at, the lines, and a footer with the way out. What
/// differs is the header, which has to say which kind of view this is: two keys for two lists
/// would be worse than one list whose rows say what they are.
fn draw_output(frame: &mut Frame, session: &Session, output: &Output) -> Laid {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // which command, and whether the model read it
            Constraint::Min(1),    // what it printed
            Constraint::Length(1), // footer
        ])
        .split(frame.area());

    // A job that has printed nothing makes no claim either way; the line below says where it is.
    let (standing, colour) = match output.read() {
        Some(true) => (format!("  {}", t!(watching_output_read)), theme::ok()),
        Some(false) => (format!("  {}", t!(watching_output_kept)), theme::running()),
        None => (String::new(), mark_for(&output.outcome).1),
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(format!("{TURN_MARKER} "), Style::default().fg(colour)),
                Span::styled(
                    match &output.job {
                        Some(job) => t!(watching_output_job_head, name = job.label()),
                        None => t!(watching_output_head).to_string(),
                    },
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(standing, Style::default().fg(colour)),
            ]),
            // The command and how it ended, both the driver's own record of what ran rather than
            // anything read out of what it printed. How it ended is here as well as on the row
            // because a view opened on a long log shows its last lines, and the verdict of a build
            // is not always in them.
            Line::from(vec![
                Span::styled(format!("  {}", one_line(&output.command)), dim()),
                // A job's from its state and this end's clock, which is what tells one still
                // running apart from a run stopped at its deadline: both carry the same mark.
                Span::styled(
                    format!(
                        "  {}",
                        match &output.job {
                            Some(job) => job.standing(&output.outcome),
                            None => output.outcome.summary(),
                        }
                    ),
                    Style::default().fg(mark_for(&output.outcome).1),
                ),
            ]),
        ]),
        areas[0],
    );

    // Marked on every row where the planner was kept from it, the way every other block of
    // content it was kept from is marked, and marked structurally rather than by a line of text
    // the content could imitate: the bar is in the margin on the next row too.
    let width = areas[1].width as usize;
    let margin = Span::styled(
        format!("  {QUARANTINE_BAR} "),
        Style::default().fg(theme::running()),
    );
    let mut lines: Vec<Line> = Vec::new();
    for line in &output.lines {
        let spans = [Span::styled(line.clone(), dim())];
        if output.read_by_the_planner {
            lines.extend(marked_rows(&Span::raw("  "), &spans, width));
        } else {
            lines.extend(marked_rows(&margin, &spans, width));
        }
    }
    // Said rather than silently dropped, for the same reason a truncated diff says so.
    if output.total > output.lines.len() {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            format!(
                "  {}",
                t!(
                    watching_output_more,
                    count = output.total - output.lines.len()
                )
            ),
            dim(),
        )));
    }

    // Counted back from the end, the way the transcript is, so a view opened on a long log starts
    // at the last thing the command said and the same keys walk back through it.
    let total = lines.len() as u16;
    let max_offset = total.saturating_sub(areas[1].height);
    let offset = max_offset.saturating_sub(session.scroll.min(max_offset));
    frame.render_widget(Paragraph::new(lines).scroll((offset, 0)), areas[1]);

    draw_watching_footer(frame, areas[2], session);
    if let Some(selection) = &session.selection {
        crate::select::highlight(frame.buffer_mut(), selection);
    }
    Laid::default()
}

/// Which delegate this is, and what it was asked to do.
///
/// Above its lines rather than in them, so it stays put while somebody reads back through a
/// delegate that has made hundreds of calls. The question a reader has at every row is which run
/// they are looking at, and a header that scrolled away would answer it only at the top.
fn draw_delegate_head(frame: &mut Frame, area: Rect, session: &Session) {
    let Some(delegate) = session.watched_delegate() else {
        return;
    };

    let head = if delegate.is_running() {
        Style::default().fg(theme::running())
    } else if delegate.failed {
        Style::default().fg(theme::fail())
    } else {
        Style::default().fg(theme::ok())
    };

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(format!("{TURN_MARKER} "), head),
                Span::styled(
                    t!(
                        watching_footer,
                        kind = delegate.kind.clone(),
                        number = delegate.id.to_string()
                    ),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ]),
            // What the planner asked for, released for a screen exactly as the target of any
            // other call is, and read by nothing.
            Line::from(Span::styled(
                format!("  {}", one_line(&delegate.task)),
                dim(),
            )),
        ]),
        area,
    );
}

/// The footer of one delegate's view: which one, how it is going, and the way out.
///
/// Nothing a model wrote is quoted here. The footer is the one row the interface speaks in its
/// own voice, and what the delegate was asked is above it where content belongs.
fn draw_watching_footer(frame: &mut Frame, area: Rect, session: &Session) {
    let Some(watching) = session.watching() else {
        return;
    };

    // Which kind of view this is, said in the footer as well as in the header, because the list
    // holds both kinds and the row somebody stepped onto may not be the kind they came from.
    let (name, standing, colour) = match session.watched() {
        Some(Watched::Delegate(delegate)) => {
            let (standing, colour) = if delegate.is_stopping() {
                (t!(watching_stopping), theme::running())
            } else if delegate.is_running() {
                (t!(watching_working), theme::running())
            } else if delegate.failed {
                (t!(watching_failed), theme::fail())
            } else {
                (t!(watching_answered), theme::ok())
            };
            (
                t!(
                    watching_footer,
                    kind = delegate.kind.clone(),
                    number = delegate.id.to_string()
                )
                .to_string(),
                standing.to_string(),
                colour,
            )
        }
        Some(Watched::Output(output)) => {
            let (standing, colour) = match output.read() {
                Some(true) => (t!(watching_output_read).to_string(), theme::ok()),
                Some(false) => (t!(watching_output_kept).to_string(), theme::running()),
                None => (
                    output
                        .job
                        .as_ref()
                        .map(|job| job.standing(&output.outcome))
                        .unwrap_or_default(),
                    mark_for(&output.outcome).1,
                ),
            };
            let name = match &output.job {
                Some(job) => t!(watching_footer_job, name = job.label()),
                None => t!(watching_list_command).to_string(),
            };
            (name, standing, colour)
        }
        // An aside is answered by the time it is a row, so what the standing says is whether the
        // record keeps it, which is the one thing about it a person cannot work out from the bytes.
        Some(Watched::Aside(aside)) => {
            let (standing, colour) = if aside.kept {
                (t!(watching_row_kept_answer), theme::ok())
            } else {
                (t!(watching_row_screen_only), theme::running())
            };
            (
                t!(watching_list_aside).to_string(),
                standing.to_string(),
                colour,
            )
        }
        None => return,
    };

    let mut spans = vec![
        Span::styled(
            format!("  {name}"),
            Style::default().fg(theme::brand_primary()),
        ),
        Span::styled(format!("  ·  {standing}"), Style::default().fg(colour)),
    ];

    // Where it sits among the others, and the keys for moving, only where there are others. One
    // row has no position to be in and nowhere to move to.
    let total = session.watchable().len();
    if total > 1 {
        spans.push(Span::styled(
            format!(
                "  ·  {}",
                t!(watching_position, at = watching.at + 1, total = total)
            ),
            dim(),
        ));
        spans.push(Span::styled(
            format!("  ·  {}", t!(watching_keys_back)),
            dim(),
        ));
    } else {
        spans.push(Span::styled(
            format!("  ·  {}", t!(watching_keys_one)),
            dim(),
        ));
    }
    if let Some(Watched::Delegate(delegate)) = session.watched()
        && delegate.is_running()
        && !delegate.is_stopping()
    {
        spans.push(Span::styled(
            format!("  ·  {}", t!(watching_keys_stop)),
            dim(),
        ));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Draw the list of delegates: a panel over the session, and the highlight on the one that would
/// open.
///
/// Every delegate the session has spawned, in the order they were spawned, whether it is working
/// or finished. A list that dropped the finished ones would lose exactly the runs somebody comes
/// looking for after the fact.
///
/// A panel rather than the screen, because the list is a question with a handful of answers and
/// the transcript behind it is what a person picking one is reading. The box is still not drawn
/// and no key reaches it: what stands behind the panel is the session, not something to type at.
fn draw_delegate_list(frame: &mut Frame, session: &Session) -> Laid {
    let laid = draw_transcript(frame, frame.area(), session);

    let watchable = session.watchable();
    // The session is the first row, so the panel is one taller than what it lists and the
    // highlight is counted over the rows rather than over the things.
    let rows = watchable.len() + 1;
    let at = session.list_highlight();
    let area = delegate_panel(frame.area(), rows);
    frame.render_widget(Clear, area);

    // The theme's own colours rather than the terminal's, the way every other panel here is
    // painted. `Clear` only empties the cells, so without this the panel is drawn on whatever the
    // terminal's default background is and a theme chosen for readability stops applying.
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::brand_primary()))
        .title(Span::styled(
            format!(" {} ", t!(watching_list_title)),
            Style::default()
                .fg(theme::brand_primary())
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme::background()).fg(theme::text()));
    let inside = block.inner(area);
    frame.render_widget(block, area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // the delegates
            Constraint::Length(1), // blank
            Constraint::Length(1), // keys
        ])
        .split(inside);

    // The window follows the highlight, so a session that has spawned more delegates than the
    // panel is tall still opens on the one the cursor is on.
    let visible = (layout[0].height as usize).max(1);
    let first = window_start(rows, at, visible);
    let width = layout[0].width as usize;
    let drawn: Vec<Line> = std::iter::once(session_row(at == 0, width))
        .chain(watchable.iter().enumerate().map(|(index, row)| {
            let highlighted = at == index + 1;
            match row {
                Watched::Aside(aside) => aside_row(aside, highlighted, width),
                Watched::Delegate(delegate) => delegate_row(delegate, highlighted, width),
                Watched::Output(output) => output_row(output, highlighted, width),
            }
        }))
        .skip(first)
        .take(visible)
        .collect();
    frame.render_widget(Paragraph::new(drawn), layout[0]);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {}", t!(watching_list_keys)),
            Style::default().fg(theme::muted()),
        ))),
        layout[2],
    );

    if let Some(selection) = &session.selection {
        crate::select::highlight(frame.buffer_mut(), selection);
    }

    laid
}

/// A centred panel sized to the delegates, never larger than the terminal.
///
/// Sized to what it holds rather than to the screen: a session with two delegates gets a panel
/// two rows tall, and the transcript behind it stays readable.
fn delegate_panel(area: Rect, rows: usize) -> Rect {
    let available = area.width.saturating_sub(4);
    let width = available.min(76).max(available.min(32));
    // Borders, the blank above the key row, and the key row.
    let outside = area.height.saturating_sub(2).max(1);
    let height = (rows as u16)
        .saturating_add(4)
        .min(outside)
        .max(outside.min(5));

    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    }
}

/// Where a window of `visible` rows starts with the cursor inside it.
fn window_start(rows: usize, cursor: usize, visible: usize) -> usize {
    if rows <= visible {
        return 0;
    }
    cursor
        .saturating_sub(visible.saturating_sub(1))
        .min(rows - visible)
}

/// The first row of the list: the conversation the delegates were spawned from.
///
/// Here because the way back was the one destination the list did not offer. Somebody comparing
/// two delegates could reach either, and reaching the thing they were reading before meant
/// leaving the mode by a key that is not on any row.
///
/// Drawn in the same columns as a delegate's row, and with no count, because it is a place to go
/// rather than a run that has done any work.
fn session_row(highlighted: bool, width: usize) -> Line<'static> {
    let (mark_style, name_style, detail) = if highlighted {
        let on_bar = Style::default().fg(theme::on_primary());
        (on_bar, on_bar.add_modifier(Modifier::BOLD), on_bar)
    } else {
        (
            Style::default().fg(theme::brand_primary()),
            Style::default().fg(theme::text()),
            dim(),
        )
    };

    let name = t!(watching_list_session);
    let about = t!(watching_list_session_detail);
    let spent = 4 + NAME_COLUMN + 2;
    let room = width.saturating_sub(spent);
    let about: String = match about.chars().count() > room {
        true => about.chars().take(room).collect(),
        false => about.to_string(),
    };

    let line = Line::from(vec![
        Span::styled(format!("  {TURN_MARKER} "), mark_style),
        Span::styled(format!("{name:<NAME_COLUMN$}"), name_style),
        Span::styled(about, detail),
    ]);
    match highlighted {
        true => line.style(theme::picked_out()),
        false => line,
    }
}

/// Wide enough for the longest kind and a two-digit number, so every task starts in one column
/// and the rows read as a table rather than as a ragged list.
const NAME_COLUMN: usize = 14;

/// One row for a question asked beside the work: what was asked, and whether the answer is kept.
///
/// The question is on the row for the reason a delegate's task is on its own: two asides are
/// otherwise identical, and which one somebody wants is the whole question the list is answering.
///
/// The mark is the one a delegate that answered carries, because that is what happened: the
/// question was asked and came back. What the word beside it says is whether the answer outlives
/// the session, which is the one thing about an aside a person cannot see by reading it.
fn aside_row(
    aside: &bravebot_session::sessions::Aside,
    highlighted: bool,
    width: usize,
) -> Line<'static> {
    let (standing, standing_colour) = if aside.kept {
        (t!(watching_row_kept_answer), theme::ok())
    } else {
        (t!(watching_row_screen_only), theme::running())
    };

    let name = t!(watching_list_aside);

    let spent = 4 + NAME_COLUMN + STANDING_COLUMN + 4;
    let room = width.saturating_sub(spent);
    let question = one_line(&aside.question);
    let question = if question.chars().count() > room {
        question
            .chars()
            .take(room.saturating_sub(1))
            .collect::<String>()
            + "…"
    } else {
        let padding = room.saturating_sub(question.chars().count());
        question + &" ".repeat(padding)
    };

    let (mark_style, name_style, detail, standing_style) = if highlighted {
        let on_bar = Style::default().fg(theme::on_primary());
        (on_bar, on_bar.add_modifier(Modifier::BOLD), on_bar, on_bar)
    } else {
        (
            Style::default().fg(theme::ok()),
            Style::default().fg(theme::text()),
            dim(),
            Style::default().fg(standing_colour),
        )
    };

    let line = Line::from(vec![
        Span::styled("  ✓ ".to_string(), mark_style),
        Span::styled(format!("{name:<NAME_COLUMN$}"), name_style),
        Span::styled(format!("{question}  "), detail),
        Span::styled(format!("{standing:<STANDING_COLUMN$}"), standing_style),
    ]);
    match highlighted {
        true => line.style(theme::picked_out()),
        false => line,
    }
}

/// One delegate on the list: how it is going, what it is, what it was asked, and how much it has
/// done.
///
/// The task is on the row because two delegates of the same kind are otherwise identical, and
/// which one somebody wants is the whole question the list is answering.
///
/// The row that would open is filled edge to edge rather than pointed at. A mark beside a short
/// name reads as decoration on the text; a bar reads as the row being chosen, which is what it is.
fn delegate_row(delegate: &Delegate, highlighted: bool, width: usize) -> Line<'static> {
    // The glyph carries the standing on the bar as well as off it, so the row under the cursor
    // does not have to spend a colour the highlight has already taken.
    let (mark, colour) = if delegate.is_running() {
        ("●", theme::running())
    } else if delegate.failed {
        ("✗", theme::fail())
    } else {
        ("✓", theme::ok())
    };

    let name = format!("{} {}", delegate.kind, delegate.id);
    let calls = t!(watching_calls, count = delegate.calls);

    // The task takes whatever the row has left, so a narrow terminal loses the end of a sentence
    // rather than the count saying how much the run has done. Padded as well as cut, so the
    // counts line up down the right edge and read as a column.
    let spent = 4 + NAME_COLUMN + calls.chars().count() + 4;
    let room = width.saturating_sub(spent);
    let task = one_line(&delegate.task);
    let task = if task.chars().count() > room {
        task.chars()
            .take(room.saturating_sub(1))
            .collect::<String>()
            + "…"
    } else {
        let padding = room.saturating_sub(task.chars().count());
        task + &" ".repeat(padding)
    };

    let (mark_style, name_style, detail) = if highlighted {
        let on_bar = Style::default().fg(theme::on_primary());
        (on_bar, on_bar.add_modifier(Modifier::BOLD), on_bar)
    } else {
        (
            Style::default().fg(colour),
            Style::default().fg(theme::text()),
            dim(),
        )
    };

    let line = Line::from(vec![
        Span::styled(format!("  {mark} "), mark_style),
        Span::styled(format!("{name:<NAME_COLUMN$}"), name_style),
        Span::styled(format!("{task}  "), detail),
        Span::styled(calls, detail),
    ]);
    match highlighted {
        true => line.style(theme::picked_out()),
        false => line,
    }
}

/// How wide the word saying whether the planner read a command's output is drawn, so the counts
/// beside it still line up down the right edge.
const STANDING_COLUMN: usize = 8;

/// One row for what a command printed: how the run ended, what ran, whether the planner read it,
/// and how much it printed.
///
/// The glyph says how it ended, in the same three marks a delegate's row uses, because a row that
/// gives only the line count leaves a person unable to tell the build that passed from the one
/// that failed. Whether the planner read the output is beside the count, in a word:
/// it is the thing the whole design turns on, and a second glyph competing with the first says it
/// to nobody who has not been told what the glyphs mean.
fn output_row(output: &Output, highlighted: bool, width: usize) -> Line<'static> {
    let (mark, colour) = mark_for(&output.outcome);
    let (standing, standing_colour) = match output.read() {
        Some(true) => (t!(watching_row_read).to_string(), theme::ok()),
        Some(false) => (t!(watching_row_kept).to_string(), theme::running()),
        None => (String::new(), theme::muted()),
    };

    // A job says so in the column a delegate's kind goes in, and leads its line with the name the
    // planner reads it by, which is what tells two runs of the same line apart.
    let (name, command) = match &output.job {
        Some(job) => (
            t!(watching_list_job).to_string(),
            format!("{}  {}", job.label(), one_line(&output.command)),
        ),
        None => (
            t!(watching_list_command).to_string(),
            one_line(&output.command),
        ),
    };
    let count = t!(watching_lines, count = output.total);

    let spent = 4 + NAME_COLUMN + STANDING_COLUMN + count.chars().count() + 6;
    let room = width.saturating_sub(spent);
    let command = if command.chars().count() > room {
        command
            .chars()
            .take(room.saturating_sub(1))
            .collect::<String>()
            + "…"
    } else {
        let padding = room.saturating_sub(command.chars().count());
        command + &" ".repeat(padding)
    };

    let (mark_style, name_style, detail, standing_style) = if highlighted {
        let on_bar = Style::default().fg(theme::on_primary());
        (on_bar, on_bar.add_modifier(Modifier::BOLD), on_bar, on_bar)
    } else {
        (
            Style::default().fg(colour),
            Style::default().fg(theme::text()),
            dim(),
            Style::default().fg(standing_colour),
        )
    };

    let line = Line::from(vec![
        Span::styled(format!("  {mark} "), mark_style),
        Span::styled(format!("{name:<NAME_COLUMN$}"), name_style),
        Span::styled(format!("{command}  "), detail),
        Span::styled(format!("{standing:<STANDING_COLUMN$}  "), standing_style),
        Span::styled(count, detail),
    ]);
    match highlighted {
        true => line.style(theme::picked_out()),
        false => line,
    }
}

/// The mark and the colour for how a command ended.
///
/// A run stopped at the wall-clock limit, or by the person, keeps the mark and the colour of one
/// still working, because that is what it was doing when it was stopped: a server told to serve a
/// page prints as it goes and never exits, and drawing that as a failure would say something about
/// the program that is not true.
fn mark_for(outcome: &bravebot_agent::report::Outcome) -> (&'static str, ratatui::style::Color) {
    use bravebot_agent::report::Outcome;
    match outcome {
        Outcome::Succeeded => ("✓", theme::ok()),
        Outcome::Failed { .. } => ("✗", theme::fail()),
        Outcome::Stopped(_) | Outcome::StoppedByTheUser(_) | Outcome::Running { .. } => {
            ("●", theme::running())
        }
    }
}
