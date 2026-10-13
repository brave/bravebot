//! The input box and the status row above it.

use bravebot_i18n::t;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::state::{Session, Status};
use crate::theme;
use crate::wrap;

use super::{dim, head_of, placeholder, printable, todo_lines};

/// Columns available for input text inside the box.
///
/// Two for the borders, two for the `> ` prompt, and one for the caret, so a wrap computed against
/// this matches what the terminal will actually show. The caret takes a column only past the end of
/// the text, where it has no character to sit on and is drawn as a highlighted space, but the row
/// it ends up on is not known before wrapping, so the column is kept on every row.
pub(super) fn input_text_width(total: u16) -> usize {
    (total as usize).saturating_sub(5).max(1)
}

/// Rows the indicator above the box needs, and the task list under it.
///
/// Zero when nothing is running. Bounded so a long list cannot take the transcript and the box
/// with it: the point of showing what is happening is lost if the box it is happening above has
/// been squeezed off the screen.
pub(super) fn status_height(session: &Session, width: u16, height: u16) -> u16 {
    // A row of transcript, three of box, and the hint line are what has to survive this.
    let ceiling = (height as usize).saturating_sub(5).max(1);

    match session.status {
        // The indicator, and the task list beneath it if the turn kept one.
        Status::Working => (1 + session.todos.len()).min(ceiling) as u16,
        // A command spends no tokens and keeps no task list, so one line says everything.
        Status::Running => 1,
        // Keep the failure reason visible even when its transcript entry is off screen.
        Status::Idle if session.finished.is_some() => {
            (1 + failure_rows(session, width).len()).min(ceiling) as u16
        }
        Status::Idle | Status::Quitting => 0,
    }
}

/// Limit the reason's height so the turn status and input remain visible.
const REASON_ROWS: usize = 3;

/// Why the turn that just ended failed, wrapped for the status area, or nothing.
fn failure_rows(session: &Session, width: u16) -> Vec<String> {
    let Some(reason) = session.failure_said() else {
        return Vec::new();
    };
    // Indented to the width the row above it starts at, so the reason reads as belonging to it.
    let room = (width as usize).saturating_sub(4).max(8);
    let mut rows = wrap::wrap(reason, room, 0).rows;
    rows.truncate(REASON_ROWS);
    rows
}

/// Rows the input box needs, borders included.
///
/// Grows with the text up to [`wrap::MAX_ROWS`], and never takes so much of a short terminal that
/// the transcript disappears. The box is measured the same way whatever the session is doing: a
/// turn in flight does not take it away, because a person typing their next prompt has to be able
/// to see what they are typing.
pub(super) fn input_height(session: &Session, width: u16, height: u16) -> u16 {
    // Leave at least one line of transcript and the hint line, whatever is in the box.
    let ceiling = (height as usize).saturating_sub(2).max(3);

    let rows = wrap::wrap(session.input(), input_text_width(width), session.caret())
        .rows
        .len()
        .min(wrap::MAX_ROWS);

    (rows + 2).min(ceiling) as u16
}

/// A row with `at..through` of it drawn as a block, which is where the caret is.
///
/// A block rather than a glyph inserted between two characters, because inserting one moves
/// everything after it by a column: the text shifted left and right under the caret as it moved,
/// which is far more distracting than the caret itself. Nothing moves now, since the caret occupies
/// cells that were already there.
///
/// An empty range is the caret past the end of the line, where there is no cell to occupy, so a
/// highlighted space is added. That is the one place the caret still takes a column, and
/// [`input_text_width`] reserves it.
///
/// `words` are the stretches of the row that name a command or a skill, drawn in the colour of
/// the prompt around the caret.
fn caret_spans(
    row: &str,
    at: usize,
    through: usize,
    colour: Color,
    words: &[(usize, usize)],
) -> Vec<Span<'static>> {
    // Reversed rather than a chosen pair of colours, so the cells invert whatever the terminal's
    // own foreground and background happen to be and stay legible on either kind of theme.
    let block = Style::default().fg(colour).add_modifier(Modifier::REVERSED);

    let mut spans = text_spans(row, 0, at, words);
    if through > at {
        spans.push(Span::styled(row[at..through].to_string(), block));
        spans.extend(text_spans(row, through, row.len(), words));
    } else {
        spans.push(Span::styled(" ", block));
    }
    spans
}

/// `row[from..to]` as plain text, with the stretches in `words` drawn as a recognised word.
///
/// `words` are in row offsets, in order and not overlapping. A stretch is cut at `from` and `to`,
/// so the caret block can take the middle of a word and leave the rest of it coloured.
fn text_spans(row: &str, from: usize, to: usize, words: &[(usize, usize)]) -> Vec<Span<'static>> {
    let named = Style::default().fg(theme::brand_primary());
    let mut spans = Vec::new();
    let mut at = from;
    for &(start, end) in words {
        let start = start.clamp(at, to);
        let end = end.clamp(start, to);
        if start > at {
            spans.push(Span::raw(row[at..start].to_string()));
        }
        if end > start {
            spans.push(Span::styled(row[start..end].to_string(), named));
        }
        at = end;
    }
    if at < to {
        spans.push(Span::raw(row[at..to].to_string()));
    }
    spans
}

/// Where the line names a command or a skill, as byte ranges of the line.
///
/// A command only where the line is that command, since `/undo the last change` is a prompt and
/// is drawn as one. A skill anywhere, since it may be named mid-sentence, but not inside a command
/// line, whose argument is taken verbatim. The same reading the completion list makes, so a word
/// is drawn as recognised exactly where taking it from the list would leave it recognised.
///
/// The line is the person's own typing and the names are the program's and the trusted skills', so
/// the only thing decided from them is a colour.
fn named_words(session: &Session) -> Vec<(usize, usize)> {
    if session.shell {
        return Vec::new();
    }
    let line = session.input();
    if let Some(name) = crate::app::command_typed(line) {
        let start = line.len() - line.trim_start().len();
        return vec![(start, start + name.len())];
    }
    let held = session.held_skills();
    let mut words = Vec::new();
    let mut offset = 0;
    for piece in line.split_inclusive(char::is_whitespace) {
        let word = piece.trim_end_matches(char::is_whitespace);
        if crate::skills::named(held, word).is_some() {
            words.push((offset, offset + word.len()));
        }
        offset += piece.len();
    }
    words
}

/// What the line is ready to be given, drawn after the caret, or `None` where nothing is.
///
/// Only where the line is a command or a skill that takes something, a space has been typed after
/// it, and nothing else has: the first character of the argument replaces the hint. At the end of
/// the line, since that is where the argument will be typed.
fn argument_hint(session: &Session) -> Option<String> {
    let line = session.input();
    if session.shell || session.caret() != line.len() {
        return None;
    }
    let word = line.trim_end_matches(' ');
    if word.len() == line.len() || word.contains(char::is_whitespace) {
        return None;
    }
    let hint = match crate::app::commands()
        .iter()
        .find(|command| command.name == word)
    {
        Some(command) => command.typed_hint(),
        None => crate::skills::named(session.held_skills(), word)?
            .argument_hint
            .clone()?,
    };
    (!hint.is_empty()).then(|| printable(&hint))
}

/// What opens a row of the box: the prompt character, or the indent a continuation lines up under.
///
/// One place, because the invitation is drawn behind the same opening as the line it stands in
/// for, and two copies of it are how the two came to sit in different columns.
fn lead_for(index: usize, shell: bool, private: bool) -> &'static str {
    // Only the first row carries the prompt; continuations are indented to line up beneath it.
    if index != 0 {
        "  "
    } else if shell && private {
        // As wide as the others, which the wrap and the caret are measured against, so the second
        // marker takes the place of the space.
        "!!"
    } else if shell {
        "! "
    } else {
        "> "
    }
}

/// The invitation, with the caret sitting on its first character.
///
/// The caret keeps its place rather than being drawn past the words, because the box is empty and
/// that is where the first character will land. Dimmed so it reads as an absence of text rather
/// than as text somebody has to clear.
fn placeholder_spans(colour: Color, words: &str) -> Vec<Span<'static>> {
    let block = Style::default().fg(colour).add_modifier(Modifier::REVERSED);
    let mut characters = words.chars();
    let first: String = characters.by_ref().take(1).collect();
    vec![
        Span::styled(first, block),
        Span::styled(characters.as_str().to_string(), dim()),
    ]
}

/// How much of the row starting at `start` a span of the line covers, in the row's own offsets.
///
/// `None` when the two do not meet, which is every row a covered marker does not reach.
fn covered(row: &str, start: usize, span: (usize, usize)) -> Option<(usize, usize)> {
    let (from, to) = span;
    let at = from.saturating_sub(start).min(row.len());
    let through = to.saturating_sub(start).min(row.len());
    (at < through).then_some((at, through))
}

/// Draw what is running, above the box.
///
/// Above rather than inside, because the box is still the user's: a turn takes as long as it takes
/// and the next prompt is usually thought of during it, so the line being typed has to stay on
/// screen. It was drawn over, and someone typing through a slow turn watched their words go
/// nowhere, which is indistinguishable from an interface that has stopped responding.
pub(super) fn draw_status(frame: &mut Frame, area: Rect, session: &Session) {
    let working = session.status == Status::Working;
    let running = session.status == Status::Running;

    let indicator = if running {
        // Elapsed time and nothing else, because a command spends no tokens and reports no phase.
        // The clock is what a waiting user wants, and Escape is the other half of the answer.
        Line::from(vec![
            Span::styled(
                format!("  {} ", session.spinner()),
                Style::default().fg(theme::accent()),
            ),
            Span::styled("running… ", Style::default().fg(theme::accent())),
            Span::styled(format!("({})  esc to stop", session.elapsed_words()), dim()),
        ])
    } else if working {
        match session.indicator() {
            Some(indicator) => {
                Line::from(vec![
                    Span::styled(
                        format!("  {} ", indicator.glyph),
                        Style::default().fg(theme::accent()),
                    ),
                    Span::styled(
                        format!("{}… ", indicator.verb),
                        Style::default().fg(theme::ok()),
                    ),
                    // Dim: the counters answer a question without competing for attention.
                    Span::styled(indicator.detail(), dim()),
                ])
            }
            None => Line::from(Span::styled("  waiting for the model…", dim())),
        }
    } else if let Some(finished) = session.finished {
        // The end of a turn, said rather than left to the spinner's absence. A turn whose last
        // words were "now let me look at the dispatch code" is over, and nothing on the screen
        // used to say so: the indicator was simply gone, and a user reads that as a session that
        // has stopped responding rather than one waiting for them.
        let (glyph, colour, word) = match finished.ending {
            bravebot_agent::Ending::Failed(_) => {
                ("✗", theme::fail(), t!(turn_failed, turn = finished.turn))
            }
            bravebot_agent::Ending::Stopped { .. } => {
                ("■", theme::fail(), t!(turn_cancelled, turn = finished.turn))
            }
            bravebot_agent::Ending::Done => ("✓", theme::ok(), t!(turn_done, turn = finished.turn)),
        };
        let mut spans = vec![
            Span::styled(format!("  {glyph} "), Style::default().fg(colour)),
            Span::styled(format!("{word} "), Style::default().fg(colour)),
        ];
        if matches!(finished.ending, bravebot_agent::Ending::Done) {
            spans.push(Span::styled(
                format!(
                    "({}, {})",
                    crate::indicator::format_tokens(finished.tokens),
                    crate::indicator::format_elapsed(finished.took)
                ),
                dim(),
            ));
        }
        Line::from(spans)
    } else {
        return;
    };

    // The list sits under the indicator, so what is being worked on and what remains are read
    // together. Trimmed to what this area was given rather than overflowing into the box below.
    let room = (area.height as usize).saturating_sub(1);
    let mut lines = vec![indicator];
    if working {
        lines.extend(todo_lines(&session.todos).into_iter().take(room));
    }
    // Fit the reason below the status; the transcript and export retain the full text.
    lines.extend(
        failure_rows(session, area.width)
            .into_iter()
            .take(room)
            .map(|row| Line::from(Span::styled(format!("    {row}"), dim()))),
    );

    frame.render_widget(Paragraph::new(lines), area);
}

pub(super) fn draw_input(frame: &mut Frame, area: Rect, session: &Session) {
    let wrapped = wrap::wrap(
        session.input(),
        input_text_width(area.width),
        session.caret(),
    );
    let visible = (area.height as usize).saturating_sub(2).max(1);
    let (first, rows) = wrapped.window(visible);
    let marker = session.marker_at_caret();
    let selection = session.vi_selection();
    let words = named_words(session);
    let hint = argument_hint(session);

    // Shell mode is coloured throughout rather than only in the marker, because the whole line
    // means something different: it goes to a shell instead of the model, and that is worth more
    // than one character of distinction at the moment somebody presses Enter.
    let colour = if session.shell {
        theme::accent()
    } else {
        theme::brand_primary()
    };

    // Wrapping is computed above rather than left to `Paragraph`, because the cursor has to be
    // placed after the last character and only an explicit wrap knows where that is.
    let lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .map(|(offset, row)| {
            let index = first + offset;
            let mut spans = vec![Span::styled(
                lead_for(index, session.shell, session.shell_private()),
                Style::default().fg(colour),
            )];
            // A selection is drawn before the caret is, and instead of it: the whole marked stretch is
            // reversed, so the caret within it would be a block inside a block and say nothing. Every
            // row it crosses is covered, since a selection over a paragraph is what `V` is for.
            //
            // Drawn at all because a selection nobody can see is the failure this mode has: the next
            // key acts on a stretch, and a person who cannot see which one is guessing.
            let named: Vec<(usize, usize)> = words
                .iter()
                .filter_map(|&word| covered(row, wrapped.starts[index], word))
                .collect();
            if let Some((at, through)) =
                selection.and_then(|span| covered(row, wrapped.starts[index], span))
            {
                spans.extend(caret_spans(row, at, through, colour, &named));
                return Line::from(spans);
            }
            // A marker is one thing to the caret, so the caret covers the whole of it rather than
            // the one character it happens to start with. The span is located in the line, not in
            // the row, because the wrap is free to put a long marker across two of them.
            match marker.and_then(|span| covered(row, wrapped.starts[index], span)) {
                Some((at, through)) => {
                    spans.extend(caret_spans(row, at, through, colour, &named));
                }
                None if index == wrapped.cursor_row => {
                    let at = wrapped.cursor_index;
                    let on = row[at..].chars().next().map_or(at, |c| at + c.len_utf8());
                    spans.extend(caret_spans(row, at, on, colour, &named));
                    // Past the caret, and cut to the row rather than wrapped, since the box was
                    // sized for the line and a hint that wrapped would take a row nobody reserved.
                    let room =
                        input_text_width(area.width).saturating_sub(wrap::display_width(row));
                    if let (Some(hint), true) = (&hint, room >= 2) {
                        spans.push(Span::styled(head_of(hint, room), dim()));
                    }
                }
                None => spans.extend(text_spans(row, 0, row.len(), &named)),
            }
            Line::from(spans)
        })
        .collect();

    // An empty box says what it is for. Shell mode says something different with its own colour
    // and its own prompt character, so it is left to say it: what a person needs there is which
    // shell and how to get back out, not an invitation to ask a question.
    //
    // Behind the same prompt character the typed line gets, so the words stand exactly where the
    // first one typed will land and nothing on the row moves when it does.
    let lines = if session.input().is_empty() && !session.shell {
        let mut spans = vec![Span::styled(
            lead_for(0, session.shell, session.shell_private()),
            Style::default().fg(colour),
        )];
        // A suggestion takes the invitation's place, drawn the same way: dimmed, with the caret on
        // its first character, since that is where the first character typed would land.
        spans.extend(placeholder_spans(
            colour,
            session.suggestion().unwrap_or(placeholder()),
        ));
        vec![Line::from(spans)]
    } else {
        lines
    };

    // The position sits in the border while browsing, labelling the box without taking a row
    // away from the text.
    //
    // Dimmed while something is in flight. The line can be typed and edited then, but it cannot
    // be sent, and the border is what says which of the two the box is currently good for.
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(if session.status != Status::Idle {
            dim()
        } else if session.shell {
            Style::default().fg(theme::accent())
        } else {
            Style::default().fg(theme::brand_primary())
        });

    if let Some((index, total)) = session.history.position() {
        let scope_name = match session.history.scope() {
            crate::history::Scope::Session => t!(input_history_this_session),
            crate::history::Scope::All => t!(input_history_all),
        };
        let position = format!(
            " {} ",
            t!(
                input_history_position,
                scope = scope_name,
                index = index,
                total = total
            )
        );
        // The other ways in, said where somebody has just shown they are looking for an old prompt.
        // A search nobody can find is a search nobody has, and walking back one at a time is what
        // a person does until they learn there is something better.
        //
        // Given up in order where the row will not hold all of it, the way the hint line under the
        // box is: the narrower scope first, then the search itself. Never cut instead, since the
        // border is one row and a right-aligned title long enough to reach the left one is drawn
        // over it, which leaves the position cut mid-word and reads as a rendering fault. Which
        // prompt is being shown is what only this row says, so it is the part that stays.
        let room = area.width.saturating_sub(2) as usize;
        let taken = wrap::display_width(&position);
        let search = t!(
            input_history_search,
            chord = session.bindings().history_name()
        );
        let switch = if session.history.can_narrow() {
            Some(t!(
                input_history_narrow,
                chord = session.bindings().scope_name()
            ))
        } else if session.history.can_widen() {
            Some(t!(
                input_history_widen,
                chord = session.bindings().scope_name()
            ))
        } else {
            None
        };
        let ways_in = switch
            .map(|switch| {
                format!(
                    " {switch}  ·  {search}  ·  {} ",
                    t!(input_history_scope, chord = session.bindings().stash_name())
                )
            })
            .into_iter()
            .chain([
                format!(
                    " {search}  ·  {} ",
                    t!(input_history_scope, chord = session.bindings().stash_name())
                ),
                format!(" {search} "),
            ])
            .find(|title| taken + wrap::display_width(title) <= room);
        block = block.title_top(Line::from(Span::styled(position, dim())));
        if let Some(ways_in) = ways_in {
            block = block.title_top(Line::from(Span::styled(ways_in, dim())).right_aligned());
        }
    }

    frame.render_widget(Paragraph::new(lines).block(block), area);
}
