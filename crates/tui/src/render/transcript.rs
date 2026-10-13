//! The transcript: the lines each entry is drawn as, laid out against the area they fill.

use bravebot_i18n::t;
use bravebot_session::audit::TrailLine;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::logo;
use crate::markdown;
use crate::state::{Entry, Laid, Session, Speaker};
use crate::table;
use crate::theme;
use crate::wrap;

use super::{
    DETAIL_MARKER, LEAD, MAX_DIFF_LINES, TURN_MARKER, activity_lines, call_rows, delegate_lines,
    dim, printable, quarantined_lines, reported_lines, returned_lines, todo_lines,
};

/// Draw what the model said, styled rather than shown with its markdown markers.
///
/// Used for a finished turn in the transcript and for the reply still arriving beneath it, which
/// is what makes the handover invisible: the same words in the same shape in the same place, so
/// the moment a round ends nothing on the screen moves.
pub(super) fn assistant_lines(text: &str, width: u16) -> Vec<Line<'static>> {
    let source: Vec<&str> = text.lines().collect();
    let room = (width as usize).saturating_sub(LEAD);
    let lead = |at: usize| {
        if at == 0 {
            Span::styled(format!("{TURN_MARKER} "), Style::default().fg(theme::ok()))
        } else {
            Span::raw("  ")
        }
    };

    let mut lines = Vec::new();
    let mut at = 0;
    while at < source.len() {
        // A table is several source lines drawn as one block, so it is tried first and the lines
        // it consumed are skipped. Anything that is not one falls through to the styling every
        // other line gets.
        match table::table(&source[at..], room, Style::default()) {
            Some(laid) => {
                for (index, row) in laid.rows.into_iter().enumerate() {
                    let mut spans = vec![lead(at + index)];
                    spans.extend(row);
                    lines.push(Line::from(spans));
                }
                at += laid.consumed;
            }
            None => {
                let mut spans = vec![lead(at)];
                spans.extend(markdown::spans(source[at], Style::default()));
                lines.push(Line::from(spans));
                at += 1;
            }
        }
    }
    lines
}

/// Build the transcript as lines, so height is known before rendering.
///
/// `width` is the terminal's, and a table is laid out against what is left of it after the lead.
/// `height` is the transcript area's, which only the opening mark uses: it floats down from the
/// top edge by a share of whatever room there is.
#[cfg(test)]
pub(super) fn transcript_lines(session: &Session, width: u16, height: u16) -> Vec<Line<'static>> {
    with_prompts(session, width, height).0
}

/// The request a turn sent the planner, one header per span saying whose words follow.
///
/// Every span's text goes through [`printable`] like any other content, and the lines it comes to
/// are the request's own: nothing is added between a header and the words under it. A header is
/// the row `{` and `}` reach, so a long request is crossed span by span.
fn request_lines(
    view: &bravebot_agent::request_view::RequestView,
) -> (Vec<Line<'static>>, Vec<usize>) {
    let mut lines = vec![
        Line::from(Span::styled(
            printable(&t!(request_title, model = view.model.clone())),
            Style::default()
                .fg(theme::brand_primary())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            printable(&if view.tools.is_empty() {
                t!(request_no_tools).to_string()
            } else {
                t!(request_tools, names = view.tools.join(", "))
            }),
            dim(),
        )),
        Line::from(Span::styled(
            printable(&{
                let loaded = view.instruction_files();
                if loaded.is_empty() {
                    t!(request_no_instruction_files).to_string()
                } else {
                    t!(
                        request_instruction_files,
                        files = loaded
                            .iter()
                            .map(|(path, bytes)| t!(
                                request_instruction_file,
                                path = *path,
                                bytes = *bytes
                            )
                            .to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            }),
            dim(),
        )),
    ];
    let mut headers = Vec::new();
    for span in &view.spans {
        lines.push(Line::raw(""));
        headers.push(lines.len());
        lines.push(Line::from(Span::styled(
            printable(&format!("-- {} · {}", span.role, span.provenance.label())),
            Style::default()
                .fg(theme::brand_primary())
                .add_modifier(Modifier::BOLD),
        )));
        for text in span.text.lines() {
            lines.push(Line::from(printable(text)));
        }
    }
    (lines, headers)
}

/// Whether the screen opens on the mark: nothing has been said yet, and no delegate is on view.
pub(crate) fn opening_screen(session: &Session) -> bool {
    !session.watching_a_delegate()
        && session
            .viewed()
            .iter()
            .all(|entry| entry.speaker == Speaker::System)
}

/// The lines of a transcript, the line each prompt begins at, and the first line, last line and
/// index of each call with more to show than it draws.
type Transcript = (Vec<Line<'static>>, Vec<usize>, Vec<(usize, usize, usize)>);

/// Whether a call has lines past its glimpse, or changes past the twelve it draws, that the
/// expand key would reveal.
fn has_more_to_show(entry: &Entry) -> bool {
    entry
        .returned
        .as_ref()
        .is_some_and(|returned| returned.whole.len() > returned.lines.len())
        || entry
            .activity
            .as_ref()
            .is_some_and(|activity| activity.changes.len() > MAX_DIFF_LINES)
}

/// The transcript, and the index of the line each prompt the person typed begins at.
///
/// Two answers from one pass, because working the second out afterwards would mean deciding which
/// drawn lines were prompts by looking at them, and the thing that knows is the pass that drew
/// them.
fn with_prompts(session: &Session, width: u16, height: u16) -> Transcript {
    if let Some(view) = session
        .scroller()
        .and_then(|scroller| scroller.request.as_deref())
    {
        let (lines, prompts) = request_lines(view);
        return (lines, prompts, Vec::new());
    }
    let mut prompts: Vec<usize> = Vec::new();
    let mut calls: Vec<(usize, usize, usize)> = Vec::new();
    let mut lines: Vec<Line> = Vec::new();

    // Nothing has been said yet, so the screen opens on the mark, laid out against the height
    // rather than stacked into the top corner. A note is not a conversation: the transcript
    // already holds what starting up reported, and that belongs under the mark rather than in
    // place of it, which is why this is not a test for an empty transcript. It was one, and the
    // mark was never drawn at all, since the trust answer is noted before the first frame.
    // Never over a delegate: the mark invites a prompt, and a delegate takes none. A delegate
    // that has not yet made a call would otherwise satisfy this vacuously and be drawn as an
    // empty session.
    let viewed = session.viewed();
    let opening = opening_screen(session);
    if opening {
        lines.extend(logo::lines(
            &session.confinement,
            session.sandbox_mode(),
            session.sockets_with_a_path_reachable,
            &session.tier,
            width,
            height,
        ));
        lines.push(Line::raw(""));
    }

    for (index, entry) in viewed.iter().enumerate() {
        let began = lines.len();
        match entry.speaker {
            // The user's own words, echoed the way they were typed.
            Speaker::User => {
                prompts.push(lines.len());
                for (index, text) in entry.text.lines().enumerate() {
                    let prefix = if index == 0 { "> " } else { "  " };
                    lines.push(Line::from(Span::styled(
                        format!("{prefix}{text}"),
                        Style::default().fg(theme::brand_primary()),
                    )));
                }
            }
            // The model writes markdown whether or not it is asked to, so the reply is styled
            // rather than shown with its markers.
            //
            // Under the definition's name where a person addressed one, which the driver matched
            // and the reply had no say in (ADDRESS-12).
            Speaker::Assistant => {
                if let Some(name) = &entry.answered_as {
                    lines.push(Line::from(Span::styled(
                        format!("{:LEAD$}{}", "", t!(agent_answered, name = name.as_str())),
                        Style::default().add_modifier(Modifier::BOLD),
                    )));
                }
                lines.extend(assistant_lines(&entry.text, width))
            }
            // The session in its own voice, indented to the column the rest of the transcript's
            // text starts in. Not behind the detail marker: that says the line belongs to the
            // entry above it, and the notes starting up leaves are drawn before there is one.
            Speaker::System => {
                for text in entry.text.lines() {
                    lines.push(Line::from(Span::styled(
                        format!("{:LEAD$}{text}", ""),
                        Style::default().fg(theme::note()),
                    )));
                }
            }
            // In the same column as a note and in the colour of a failure, wrapped rather than
            // clipped: a reason is the one line in the transcript somebody reads to the end.
            Speaker::Failure | Speaker::Stopped => {
                for text in entry.text.lines() {
                    for row in
                        wrap::wrap(text, (width as usize).saturating_sub(LEAD).max(8), 0).rows
                    {
                        lines.push(Line::from(Span::styled(
                            format!("{:LEAD$}{row}", ""),
                            Style::default().fg(theme::fail()),
                        )));
                    }
                }
            }
            // Echoed with the marker the user typed it behind, so the scrollback reads back the way
            // the session happened.
            Speaker::Shell => {
                for (index, text) in entry.text.lines().enumerate() {
                    let prefix = if index == 0 { "! " } else { "  " };
                    lines.push(Line::from(Span::styled(
                        format!("{prefix}{}", printable(text)),
                        Style::default().fg(theme::accent()),
                    )));
                }
            }
            // Plainly, and indented to sit under the command. No marker down the margin and no
            // markdown: this is a terminal's output, and the user is reading it as one.
            //
            // Not a quarantine block either, which is the point of the whole feature: the user
            // typed the command, so the kernel labelled what it printed trusted, and dressing it as
            // untrusted would say the opposite of what is true.
            Speaker::Output => {
                for text in entry.text.lines() {
                    lines.push(Line::from(Span::raw(format!("  {}", printable(text)))));
                }
            }
            // A delegate the turn started, with its own work under it rather than mixed into
            // the turn's. Several run at once, so interleaved neither sequence could be read.
            Speaker::Delegate => {
                if let Some(delegate) = &entry.delegate {
                    lines.extend(delegate_lines(delegate, width as usize));
                }
            }
            // What the turn did, kept in the scrollback next to what it said about it.
            Speaker::Tool => match &entry.activity {
                Some(activity) => lines.extend(activity_lines(
                    activity,
                    entry.landing,
                    entry.expanded && session.scrolling(),
                    width as usize,
                )),
                // A call read back out of a stored session, which records that it happened and
                // not what came of it. Drawn without the coloured marker a live call earns,
                // since green would claim an outcome the record does not have.
                None => {
                    lines.extend(call_rows(
                        Span::styled(format!("{TURN_MARKER} "), dim()),
                        entry.text.clone(),
                        dim(),
                        &entry.why,
                    ));
                }
            },
        }

        if let Some(returned) = &entry.returned {
            lines.extend(returned_lines(
                returned,
                entry.expanded && session.scrolling(),
                width as usize,
            ));
        }
        if has_more_to_show(entry) {
            calls.push((began, lines.len(), index));
        }

        // What the model was not allowed to read, for the person who is.
        if let Some(shown) = &entry.shown {
            lines.extend(quarantined_lines(shown, width as usize));
        }

        // The plan the turn worked to, kept next to what it produced.
        lines.extend(todo_lines(&entry.todos));

        if session.show_trail && !entry.trail.is_empty() {
            for recorded in &entry.trail {
                lines.push(trail_line(recorded));
            }
        }

        // Tool calls come in runs and read as one block, so they are not spaced apart. A turn
        // that read six files would otherwise take twelve lines of blank. Notes run together the
        // same way, and only for as long as the run lasts: starting up leaves three at once,
        // which a blank between each would present as three separate turns, while the last of
        // them still has to be held apart from whatever is said next.
        let runs_on = match entry.speaker {
            Speaker::Tool => true,
            // The sequence being drawn, which in a delegate's view is that delegate's own. Read
            // off the turn's, a run of notes inside a delegate would be held apart or run
            // together according to what the turn happened to be saying at the same moment.
            Speaker::System => viewed
                .get(index + 1)
                .is_some_and(|next| next.speaker == Speaker::System),
            _ => false,
        };
        if !runs_on {
            lines.push(Line::raw(""));
        }
    }

    // The reply being written right now, under everything that has already happened. Drawn from
    // the session rather than from the transcript because it is not an entry yet: when the round
    // ends the same words arrive as one, in this same place and this same shape, and the tail is
    // dropped in the same breath. Nothing on the screen moves at the handover.
    //
    // The turn's, and never drawn over a delegate: what a delegate writes is dropped rather than
    // kept, so the only half-written sentence there is belongs to the planner, and it would read
    // in a delegate's view as that delegate writing it.
    if !session.reply_so_far().is_empty() && !session.watching_a_delegate() {
        lines.extend(assistant_lines(session.reply_so_far(), width));
        lines.push(Line::raw(""));
    }

    // What a command typed during the turn answered, under the turn and drawn as a note is. Kept
    // off the transcript until the turn ends (CMD-8), and the session's rather than a delegate's.
    if !session.said_while_working().is_empty() && !session.watching_a_delegate() {
        for entry in session.said_while_working() {
            for text in entry.text.lines() {
                lines.push(Line::from(Span::styled(
                    format!("{:LEAD$}{text}", ""),
                    Style::default().fg(theme::note()),
                )));
            }
        }
        lines.push(Line::raw(""));
    }

    // One blank above it and no more. Every entry already leaves a trailing blank behind it, so
    // an invitation that carried its own would sit two rows below whatever startup reported.
    if opening {
        if lines.last().is_none_or(|line| line.width() != 0) {
            lines.push(Line::raw(""));
        }
        lines.push(logo::invitation());
    }

    // What the turn was told, closing the delegate's own view. A view that stopped at the last
    // call leaves a reader looking at a command, unable to tell an answer from a failure.
    if session.watching_a_delegate()
        && let Some(delegate) = session.watched_delegate()
    {
        match &delegate.note {
            Some(note) => {
                lines.push(Line::raw(""));
                lines.push(Line::from(Span::styled(
                    format!("{TURN_MARKER} {note}"),
                    Style::default().fg(if delegate.failed {
                        theme::fail()
                    } else {
                        theme::ok()
                    }),
                )));
                // What it actually answered, under the sentence saying how it ended. A view that
                // closed on the round count leaves somebody who read every call it made still
                // not knowing what it concluded from them.
                if let Some(reported) = &delegate.reported {
                    lines.extend(reported_lines(reported, "  ", width as usize));
                }
            }
            // Approved and started, with nothing to show for it yet. An empty screen would read
            // as a mode that failed to open.
            None if delegate.lines.is_empty() => {
                lines.push(Line::from(Span::styled(
                    format!("  {}", t!(watching_nothing_yet)),
                    dim(),
                )));
            }
            None => {}
        }
    }

    (lines, prompts, calls)
}

/// Drawn over every character a search matched.
///
/// Reversed rather than coloured, so it reads as a highlight against whatever the row was already
/// wearing: a match in a quarantined preview is dim grey, and a colour of its own would be one
/// more thing on the screen the content had chosen.
fn marked() -> Style {
    Style::default().add_modifier(Modifier::REVERSED)
}

/// Highlight every occurrence of `needle`, and say which line each one was found on.
///
/// One entry per occurrence rather than per line, repeating the line where it held several, so
/// that what comes back is the matches and not the rows: a line holding two of them is counted
/// twice and walked twice, which is what the footer says and what `n` steps through.
///
/// The spans are split where a match begins and ends and the pieces restyled. Nothing on the
/// screen moves and nothing leaves the block it was drawn in: a match inside a quarantined
/// preview is highlighted between that block's margin and the end of its row, exactly where the
/// characters already were.
fn highlight(lines: &mut [Line<'static>], needle: &str) -> Vec<usize> {
    let mut held = Vec::new();
    if needle.is_empty() {
        return held;
    }

    for (index, line) in lines.iter_mut().enumerate() {
        let text: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        let found = crate::state::matched(&text, needle);
        if found.is_empty() {
            continue;
        }
        held.extend(std::iter::repeat_n(index, found.len()));

        let mut rebuilt: Vec<Span<'static>> = Vec::new();
        let mut at = 0;
        for span in line.spans.drain(..) {
            let characters: Vec<char> = span.content.chars().collect();
            let mut cut = 0;
            while cut < characters.len() {
                let here = at + cut;
                // Either this character is inside a match, and the piece runs to the end of that
                // match, or it is not, and the piece runs to the start of the next one. Both
                // bounds are past `cut`, so the walk always advances.
                let (until, style) =
                    match found.iter().find(|(from, to)| here >= *from && here < *to) {
                        Some((_, to)) => (to - at, span.style.patch(marked())),
                        None => (
                            found
                                .iter()
                                .map(|(from, _)| *from)
                                .find(|from| *from > here)
                                .map_or(characters.len(), |from| from - at),
                            span.style,
                        ),
                    };
                let until = until.min(characters.len());
                let piece: String = characters[cut..until].iter().collect();
                rebuilt.push(Span::styled(piece, style));
                cut = until;
            }
            at += characters.len();
        }
        line.spans = rebuilt;
    }
    held
}

/// How many rows one line takes at this width, once it has been wrapped.
///
/// Through the same wrapping that will draw it, rather than through a second implementation that
/// agrees with the first until one day it does not: a count that drifts puts a jump a row or two
/// off, which is the kind of wrong nobody can see and everybody blames on something else.
///
/// This runs over every line of the transcript on every frame the scroller is open, so it does
/// not copy the text to count it. A line that fits is one row and is answered from its width
/// alone, which is nearly all of them.
pub(crate) fn rows_of(line: &Line<'_>, width: u16) -> u16 {
    let width = width.max(1);
    if line.width() <= width as usize {
        return 1;
    }
    let borrowed: Vec<Span<'_>> = line
        .spans
        .iter()
        .map(|span| Span::styled(span.content.as_ref(), span.style))
        .collect();
    Paragraph::new(Line::from(borrowed))
        .wrap(Wrap { trim: false })
        .line_count(width) as u16
}

/// The transcript laid out at a width: the lines to draw, and where the rows worth reaching are.
///
/// Where each line begins is only worked out while the scroller is open. It costs a wrap of every
/// line, and at rest nothing asks the question: the wheel moves by rows and never has to know
/// which row is which.
pub(super) fn lay_out(session: &Session, width: u16, height: u16) -> (Vec<Line<'static>>, Laid) {
    let (mut lines, prompts, calls) = with_prompts(session, width, height);
    let held = highlight(&mut lines, session.needle());

    let mut laid = Laid {
        width,
        height,
        ..Laid::default()
    };
    if !session.scrolling() {
        return (lines, laid);
    }

    let mut prompts = prompts.into_iter().peekable();
    let mut held = held.into_iter().peekable();
    let mut at = 0u16;
    let mut starts: Vec<u16> = Vec::with_capacity(lines.len() + 1);
    for (index, line) in lines.iter().enumerate() {
        starts.push(at);
        if prompts.peek() == Some(&index) {
            laid.prompts.push(at);
            prompts.next();
        }
        while held.peek() == Some(&index) {
            laid.matches.push(at);
            held.next();
        }
        at = at.saturating_add(rows_of(line, width));
    }
    starts.push(at);
    laid.expandable = calls
        .into_iter()
        .map(|(first, last, index)| (starts[first], starts[last].saturating_sub(1), index))
        .collect();
    laid.rows = at;
    (lines, laid)
}

pub(super) fn draw_transcript(frame: &mut Frame, area: Rect, session: &Session) -> Laid {
    let (lines, mut laid) = lay_out(session, area.width, area.height);
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });

    // Scroll counts up from the bottom, so new output stays in view by default. The count has to
    // be of *drawn* rows rather than of lines: the paragraph wraps, so one line of a reply can
    // occupy three rows, and counting the lines put the bottom of the transcript exactly that
    // many rows below the screen. The end of a wrapped reply was therefore never shown, and
    // appeared only once the next message pushed it up.
    let total = paragraph.line_count(area.width) as u16;
    let max_offset = total.saturating_sub(area.height);

    // Any view scrolled away from the tail is drawn from the row it is holding, counted from the
    // top, and not from the offset the last frame left behind. The end of the transcript moves
    // with every token a turn writes, so a frame drawn by counting back from it puts the view
    // wherever the rows that arrived since the last frame have pushed it: the anchor is correct
    // and the arithmetic reaching it is a frame out of date. Read from the top, nothing a turn
    // appends below can move what is above it.
    let offset = if session.scrolling() || session.scroll > 0 {
        session.top_row().min(max_offset)
    } else {
        max_offset
    };

    frame.render_widget(paragraph.scroll((offset, 0)), area);

    // The paragraph's own count rather than the sum of the rows measured line by line, so what a
    // key is answered against is the number the view was actually drawn with.
    laid.rows = total;
    laid
}

/// Render one line of the audit trail.
///
/// Refusals are coloured differently from passes: a blocked gate is the most important
/// thing on the screen when it happens. The wording is settled in [`bravebot_session::audit`], so a line
/// that happened in this session and one read back off disk are drawn the same way.
fn trail_line(recorded: &TrailLine) -> Line<'static> {
    let style = if recorded.blocked {
        Style::default()
            .fg(theme::fail())
            .add_modifier(Modifier::BOLD)
    } else {
        dim()
    };
    Line::from(Span::styled(
        format!("  {DETAIL_MARKER} {}", recorded.text),
        style,
    ))
}
