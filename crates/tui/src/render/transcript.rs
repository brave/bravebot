//! The transcript: the lines each entry is drawn as, laid out against the area they fill.

use bravebot_i18n::t;
use bravebot_session::audit::TrailLine;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};
use std::sync::{Arc, OnceLock};

use crate::colour_codes;
use crate::logo;
use crate::markdown;
use crate::select::{Mark, View, Window, fingerprint_after};
use crate::state::{Bar, Entry, Hit, Laid, Session, Space, Speaker, Target, Tone, TopOfView};
use crate::table;
use crate::text_links::{Found, Marks};
use crate::theme::{self, Palette};
use crate::wrap;

use super::{
    DETAIL_MARKER, LEAD, MAX_DIFF_LINES, TURN_MARKER, activity_lines, call_rows, delegate_lines,
    dim, plan_lines, printable, quarantined_lines, reported_lines, returned_lines, todo_lines,
};

/// Draw what the model said, styled rather than shown with its markdown markers.
///
/// Used for a finished turn in the transcript and for the reply still arriving beneath it, which
/// is what makes the handover invisible: the same words in the same shape in the same place, so
/// the moment a round ends nothing on the screen moves.
///
/// `marks` says what is marked for the caller to read off the cells once the lines are drawn:
/// each `#<number>` it will link (VIEW-32) and each web address a click opens (MOUSE-40). A table
/// cell marks only the numbers, since the table cuts a cell's text into rows of its own and an
/// address cut that way is not one anybody can see the whole of.
///
/// A fenced block is drawn as a block of its own (VIEW-38): a header row where its opening fence
/// was, then each line as it was written, in the ink of inline code. Nothing in one is read as
/// markdown, a table or a number, so it is drawn before any of those is tried.
pub(super) fn assistant_lines(text: &str, width: u16, marks: Marks) -> Vec<Line<'static>> {
    assistant_laid(text, width, marks, false).0
}

/// A reply as the screen shows it, with none of the marks of the markdown it was written in, none of
/// the marker and margin the screen draws before it, and no wrapping (MOUSE-52).
///
/// Drawn by the widget that draws the transcript, at a width no line of a reply reaches, so a line
/// is a line of what the planner wrote and what is copied is read off the same lines the screen
/// draws. Trailing blanks go, since they are the padding of a row.
pub(crate) fn reply_as_text(text: &str) -> String {
    const NEVER_WRAPPED: u16 = 4000;
    assistant_laid(text, NEVER_WRAPPED, Marks::references(false), false)
        .0
        .iter()
        .map(|line| {
            let row: String = line
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect();
            row.chars()
                .skip(LEAD)
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Where the word that copies a fenced block is drawn, in the lines [`assistant_laid`] returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct BlockAt {
    /// The line of the reply's lines the word is on, which is the header of the block.
    pub line: usize,
    /// The column it begins at.
    pub column: u16,
    /// How many cells it takes.
    pub width: u16,
    /// The blanks drawn before it, between the fence's name and it.
    pub padding: u16,
    /// Which of the reply's blocks it copies, counted from none.
    pub number: usize,
}

/// [`assistant_lines`], and where the header of each block that can be copied has its word for it
/// (MOUSE-27).
///
/// `copyable` draws the word at the right of each header where there is room for it beside what
/// the fence named, and for a block that holds something. It is asked for the replies that are
/// entries of the transcript. A reply still arriving and an answer drawn beside the work draw no
/// word, since neither is something a click copies.
pub(super) fn assistant_laid(
    text: &str,
    width: u16,
    marks: Marks,
    copyable: bool,
) -> (Vec<Line<'static>>, Vec<BlockAt>) {
    let source: Vec<&str> = text.lines().collect();
    let room = (width as usize).saturating_sub(LEAD);
    let blocks = markdown::fences(&source);
    let mut blocks = blocks.iter().enumerate().peekable();
    let mut placed = Vec::new();
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
        if let Some((number, block)) = blocks.next_if(|(_, block)| block.open == at) {
            let body = block.body(&source);
            let word = t!(transcript_copy_block);
            let wide = wrap::display_width(word);
            // The word ends one column short of the edge, with a blank before it, and the fence's
            // name has what is left. A row with no room for both draws the name alone.
            let column = (width as usize).saturating_sub(1 + wide);
            let word_fits = copyable && block.holds_words(&source) && column > LEAD;
            let name = if word_fits {
                fitted(&printable(&block.info), column - LEAD - 1)
            } else {
                fitted(&printable(&block.info), room)
            };
            let mut header = vec![lead(at), Span::styled(name.clone(), dim())];
            if word_fits {
                let used = LEAD + wrap::display_width(&name);
                let padding = column.saturating_sub(used);
                header.push(Span::raw(" ".repeat(padding)));
                header.push(Span::styled(word, dim().add_modifier(Modifier::UNDERLINED)));
                placed.push(BlockAt {
                    line: lines.len(),
                    column: u16::try_from(column).unwrap_or(u16::MAX),
                    width: u16::try_from(wide).unwrap_or(u16::MAX),
                    padding: u16::try_from(padding).unwrap_or(u16::MAX),
                    number,
                });
            }
            lines.push(Line::from(header));
            let ink = Style::default().fg(markdown::code());
            for (offset, line) in body.into_iter().enumerate() {
                // A tab is laid out as width here, since the screen would draw none of it.
                let words = printable(&line.replace('\t', "    "));
                lines.push(Line::from(vec![
                    lead(at + 1 + offset),
                    Span::raw("  "),
                    Span::styled(words, ink),
                ]));
            }
            at = block.end(source.len());
            continue;
        }
        // A table is several source lines drawn as one block, so it is tried first and the lines
        // it consumed are skipped. Anything that is not one falls through to the styling every
        // other line gets. It stops short of the line that opens the next fenced block, which is the
        // block's whatever the line looks like, so that every block is reached.
        let before_the_next_block = blocks.peek().map_or(source.len(), |(_, block)| block.open);
        let after = source.get(at..before_the_next_block).unwrap_or_default();
        match table::table(after, room, Style::default(), marks.references) {
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
                spans.extend(markdown::spans_with(source[at], Style::default(), marks));
                lines.push(Line::from(spans));
                at += 1;
            }
        }
    }
    (lines, placed)
}

/// `text` cut to `room` cells, with an ellipsis where it was cut.
///
/// Cut by grapheme cluster and measured as the widget that draws the row measures, so the cells it
/// takes are the cells the draw gives it: a cluster can be wider than its characters add up to.
fn fitted(text: &str, room: usize) -> String {
    if wrap::display_width(text) <= room {
        return text.to_string();
    }
    let mut kept = String::new();
    let mut used = 0;
    for (cluster, columns) in wrap::cluster_strs(text) {
        if used + columns + 1 > room {
            break;
        }
        kept.push_str(cluster);
        used += columns;
    }
    if room == 0 {
        return kept;
    }
    kept.push('\u{2026}');
    kept
}

/// Build the transcript as lines, so height is known before rendering.
///
/// `width` is the terminal's, and a table is laid out against what is left of it after the lead.
/// `height` is the transcript area's, which only the opening mark uses: it floats down from the
/// top edge by a share of whatever room there is.
#[cfg(test)]
pub(super) fn transcript_lines(session: &Session, width: u16, height: u16) -> Vec<Line<'static>> {
    with_prompts(session, width, height).lines
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
                            ))
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

/// The lines of a transcript, the line each entry and each prompt begins at, the first line, last
/// line and index of each call with more to show than it draws, the cells of a line that a click
/// acts on, and the line that closes a delegate's view.
struct Drawn {
    lines: Vec<Line<'static>>,
    /// The line each entry begins at, in the order of the entries.
    entries: Vec<usize>,
    prompts: Vec<usize>,
    calls: Vec<(usize, usize, usize)>,
    pins: Vec<LinePin>,
    closing: Option<usize>,
}

/// Cells at the start of a line that a click acts on, before the line is wrapped into rows: the
/// column they begin at, how many there are, and what a click on them does.
///
/// Placed on the first row of the line, which is where they are drawn.
#[derive(Debug, Clone, Copy)]
struct LinePin {
    line: usize,
    column: u16,
    width: u16,
    /// The blanks drawn before the cells only to put them where they are.
    padding: u16,
    hit: Hit,
}

impl LinePin {
    /// The columns of the line that only the interface put there, as [`Target::drawn_for_the_click`]
    /// says of a target placed on a row.
    fn drawn_for_the_click(&self) -> Option<(u16, u16)> {
        self.hit
            .chrome_columns(self.column, self.width, self.padding)
    }
}

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

/// A note, in the ink its `tone` says, one or more rows to each line of `text`.
///
/// A line longer than `width` less the margin continues on the next row under the margin, so every
/// row of a note starts in the column the first does (VIEW-19), the way a refusal's rows do. The
/// rows are broken here and not left to the paragraph that draws them, which continues a row at
/// column 0. The words are drawn as they are, like a refusal's: a note is the session's own voice.
///
/// The one caller is the `Speaker::System` entry. What commands typed during a turn answered is
/// pushed as those entries, so it is drawn by this too, being the same voice. The ink is the tone's
/// and is not worked out from the words. Under `brave` none of the three is the ink that marks
/// untrusted content, and under the plain palette every ink is the terminal's own (VIEW-8).
fn note_lines(text: &str, tone: Tone, width: u16) -> Vec<Line<'static>> {
    let ink = match tone {
        Tone::Note => theme::note(),
        Tone::Done => theme::ok(),
        Tone::Refused => theme::fail(),
    };
    let room = (width as usize).saturating_sub(LEAD).max(8);
    text.lines()
        .flat_map(|text| wrap::wrap(text, room, 0).rows)
        .map(|row| {
            Line::from(Span::styled(
                format!("{:LEAD$}{row}", ""),
                Style::default().fg(ink),
            ))
        })
        .collect()
}

/// The lines an entry is drawn as, which depend on who it is from.
#[cfg(test)]
fn speaker_lines(session: &Session, entry: &Entry, width: u16) -> Vec<Line<'static>> {
    speaker_laid(session, entry, width).0
}

/// [`speaker_lines`], and where the blocks of a reply have their word for copying them.
fn speaker_laid(
    session: &Session,
    entry: &Entry,
    width: u16,
) -> (Vec<Line<'static>>, Vec<BlockAt>) {
    let mut lines = Vec::new();
    let mut blocks = Vec::new();
    match entry.speaker {
        // The user's own words, echoed the way they were typed.
        Speaker::User => {
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
            let held = lines.len();
            let (reply, placed) = assistant_laid(
                &entry.text,
                width,
                transcript_links(session),
                session.mouse_captured(),
            );
            lines.extend(reply);
            blocks.extend(placed.into_iter().map(|at| BlockAt {
                line: held + at.line,
                ..at
            }));
        }
        // The session in its own voice, indented to the column the rest of the transcript's
        // text starts in. Not behind the detail marker: that says the line belongs to the
        // entry above it, and the notes starting up leaves are drawn before there is one.
        Speaker::System => lines.extend(note_lines(&entry.text, entry.tone, width)),
        // In the same column as a note and in the colour of a failure, wrapped rather than
        // clipped: a reason is the one line in the transcript somebody reads to the end.
        Speaker::Failure | Speaker::Stopped => {
            for text in entry.text.lines() {
                for row in wrap::wrap(text, (width as usize).saturating_sub(LEAD).max(8), 0).rows {
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
        // A message to or from another session, apart from the prompts: the heading in the accent
        // colour and every word of the message quoted under it, wrapped and made printable so
        // neither a name nor the words can move the cursor (MSG-11).
        Speaker::Message => {
            for (index, text) in entry.text.lines().enumerate() {
                let style = if index == 0 {
                    Style::default()
                        .fg(theme::accent())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let width = (width as usize).saturating_sub(LEAD).max(8);
                for row in wrap::wrap(&printable(text), width, 0).rows {
                    lines.push(Line::from(Span::styled(
                        format!("{:LEAD$}{row}", ""),
                        style,
                    )));
                }
            }
        }
        // Plainly, and indented to sit under the command. No marker down the margin and no
        // markdown: this is a terminal's output, and the user is reading it as one.
        //
        // Not a quarantine block either, which is the point of the whole feature: the user
        // typed the command, so the kernel labelled what it printed trusted, and dressing it as
        // untrusted would say the opposite of what is true.
        //
        // Its colour codes are taken out and counted rather than drawn (VIEW-30): a program that
        // colours every word would otherwise be a screen of escape pictures.
        Speaker::Output => output_lines(&entry.text, &mut lines),
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
    (lines, blocks)
}

/// What follows an entry's own lines: what the call handed the planner, what the person alone may
/// read, the plan the turn worked to, and the trail.
///
/// `began` is where the entry's own lines began in `lines`, and `index` is the entry's place in
/// the sequence being drawn.
fn entry_extras(
    session: &Session,
    entry: &Entry,
    width: u16,
    began: usize,
    lines: &mut Vec<Line<'static>>,
    calls: &mut Vec<(usize, usize, usize)>,
    index: usize,
) {
    if let Some(returned) = &entry.returned {
        lines.extend(returned_lines(
            returned,
            entry.expanded && session.scrolling(),
            width as usize,
        ));
    }

    // The expand key acts on the rows from the call to the end of what it returned, so the end is
    // taken here, before anything that hangs beneath the call is drawn.
    //
    // A delegate's block is recorded the same way, as the rows a click on it opens its view for
    // (MOUSE-10). Nothing about it can be expanded, so [`lay_out`] tells the two apart.
    if has_more_to_show(entry) || entry.delegate.is_some() {
        calls.push((began, lines.len(), index));
    }

    // What the model was not allowed to read, for the person who is.
    if let Some(shown) = &entry.shown {
        lines.extend(quarantined_lines(shown, width as usize));
    }

    // The plan the turn worked to, kept next to what it produced. The turn that reported it has
    // already ended, so a row still outstanding is drawn left open rather than as work going on.
    lines.extend(todo_lines(&entry.todos, true));

    // The whole plan, where the person asked for it to be printed (VIEW-37).
    lines.extend(plan_lines(
        &entry.whole_plan,
        entry.whole_plan_left_open,
        width as usize,
    ));

    if session.show_trail && !entry.trail.is_empty() {
        for recorded in &entry.trail {
            lines.push(trail_line(recorded));
        }
    }
}

/// What a command printed, drawn plainly under the command that printed it.
///
/// The entry a finished command leaves and the rows a running one has so far are both drawn here, so
/// one step takes the colour codes out and one replaces every other control character, and neither
/// can be drawn by a path that skips it. It is given the whole text each time and not a piece of
/// it: a code or an escape that a read stopped inside is complete in the text a later frame draws,
/// and until then it is drawn as the pictures `printable` makes of it (SHELL-7).
fn output_lines(text: &str, lines: &mut Vec<Line<'static>>) {
    let mut dropped = 0;
    for text in text.lines() {
        let (plain, taken) = colour_codes::without(text);
        dropped += taken;
        lines.push(Line::from(Span::raw(format!("  {}", printable(&plain)))));
    }
    if let Some(note) = colour_codes::note(dropped) {
        lines.push(Line::from(Span::styled(format!("  {note}"), dim())));
    }
}

/// The lines each entry is drawn as, appended to `lines`, with where each prompt and each call
/// with more to show begins. The line each entry begins at is given back.
///
/// Everything in it is a function of the entries, the width, the theme and whether the trail and
/// the scroller are showing, which is what a [`EntryLines`] is kept against.
fn push_entries(
    session: &Session,
    viewed: &[Entry],
    width: u16,
    lines: &mut Vec<Line<'static>>,
    prompts: &mut Vec<usize>,
    calls: &mut Vec<(usize, usize, usize)>,
    pins: &mut Vec<LinePin>,
) -> Vec<usize> {
    let mut entries = Vec::with_capacity(viewed.len());
    for (index, entry) in viewed.iter().enumerate() {
        let began = lines.len();
        entries.push(began);
        if entry.speaker == Speaker::User {
            prompts.push(began);
        }
        let (own, blocks) = speaker_laid(session, entry, width);
        // The rows of a reply's words are a place a right click finds the reply (MOUSE-52). They are
        // recorded as the rows of a call and a delegate's block are, and told apart from them by the
        // entry, since only a reply has no call to expand and no delegate to open.
        if entry.speaker == Speaker::Assistant && !entry.text.trim().is_empty() {
            calls.push((began, began + own.len(), index));
        }
        lines.extend(own);
        // The marker that leads a reply copies it (MOUSE-26). It is on the first line of the words,
        // under the line that says which definition answered where one did.
        if entry.speaker == Speaker::Assistant && !entry.text.trim().is_empty() {
            pins.push(LinePin {
                line: began + usize::from(entry.answered_as.is_some()),
                column: 0,
                width: u16::try_from(LEAD).unwrap_or(u16::MAX),
                padding: 0,
                hit: Hit::Reply(index),
            });
        }
        // And the word at the right of a fenced block's header copies the block (MOUSE-27).
        pins.extend(blocks.into_iter().map(|at| LinePin {
            line: began + at.line,
            column: at.column,
            width: at.width,
            padding: at.padding,
            hit: Hit::Block(index, at.number),
        }));
        entry_extras(session, entry, width, began, lines, calls, index);

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
    entries
}

/// How many of the last lines of the model's thinking are drawn, so that thinking takes at most
/// this much height from the reply under it.
const THINKING_ROWS: usize = 8;

/// The model's thinking, dimmed and italic under a heading, the last few lines of it.
fn thinking_lines(text: &str, lines: &mut Vec<Line<'static>>) {
    let style = dim().add_modifier(Modifier::ITALIC);
    lines.push(Line::from(Span::styled(
        format!("  {}", t!(transcript_thinking)),
        dim(),
    )));
    let all: Vec<&str> = text.lines().collect();
    let from = all.len().saturating_sub(THINKING_ROWS);
    for line in &all[from..] {
        lines.push(Line::from(Span::styled(
            format!("  {}", printable(line)),
            style,
        )));
    }
}

/// What follows the entries: the reply still arriving, what a command that is running has printed,
/// what a command typed during the turn answered, the invitation on the opening screen, and the
/// close of a delegate's view.
///
/// Drawn from the session's live state rather than from entries, so it is built afresh on every
/// frame and is as long as one reply, never as long as the conversation.
///
/// Says which line is the sentence that closes a delegate's view, where one is drawn.
fn push_tail(
    session: &Session,
    width: u16,
    opening: bool,
    lines: &mut Vec<Line<'static>>,
) -> Option<usize> {
    let mut closing = None;
    // The model's thinking above the reply, where it came first, while a person has asked to see
    // it. Plain text under a heading of its own and never markdown, so what the model wrote cannot
    // draw as structure, and gone when the round ends: the reply replaces it, and nothing recorded
    // held it.
    if !session.thinking_so_far().is_empty() && !session.watching_a_delegate() {
        thinking_lines(session.thinking_so_far(), lines);
        lines.push(Line::raw(""));
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
        lines.extend(reply_lines(session, width).lines.iter().cloned());
        lines.push(Line::raw(""));
    }

    // What a command is printing right now, under the command, in the shape its entry will have when
    // it ends. Drawn from the session for the reason the reply above is: it is not an entry yet,
    // and the entry arrives in this same place as the tail is dropped. Not drawn over a delegate's
    // view, which is a different sequence than the one the command is in.
    //
    // Without the blank lines at its end, as the entry is drawn, so what is on the screen does not
    // move when the entry replaces it.
    if !session.watching_a_delegate()
        && let Some((text, earlier)) = session.command_so_far()
        && !text.trim().is_empty()
    {
        if earlier > 0 {
            lines.push(Line::from(Span::raw(format!(
                "  {}",
                t!(transcript_earlier_lines, count = earlier)
            ))));
        }
        output_lines(text.trim_end(), lines);
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
                closing = Some(lines.len());
                lines.push(Line::from(Span::styled(
                    format!("{TURN_MARKER} {note}"),
                    Style::default().fg(if delegate.failed() {
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
    closing
}

/// The transcript, and the index of the line each prompt the person typed begins at.
///
/// Two answers from one pass, because working the second out afterwards would mean deciding which
/// drawn lines were prompts by looking at them, and the thing that knows is the pass that drew
/// them.
fn with_prompts(session: &Session, width: u16, height: u16) -> Drawn {
    if let Some(view) = session
        .scroller()
        .and_then(|scroller| scroller.request.as_deref())
    {
        let (lines, prompts) = request_lines(view);
        return Drawn {
            lines,
            entries: Vec::new(),
            prompts,
            calls: Vec::new(),
            pins: Vec::new(),
            closing: None,
        };
    }
    let mut prompts: Vec<usize> = Vec::new();
    let mut calls: Vec<(usize, usize, usize)> = Vec::new();
    let mut pins: Vec<LinePin> = Vec::new();
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

    let cacheable = !opening && is_the_transcript(session, viewed);
    let entries = if cacheable {
        let kept = cached_entries(session, width);
        lines.extend(kept.lines.iter().cloned());
        prompts.extend_from_slice(&kept.prompts);
        calls.extend_from_slice(&kept.calls);
        pins.extend_from_slice(&kept.pins);
        kept.entries.clone()
    } else {
        push_entries(
            session,
            viewed,
            width,
            &mut lines,
            &mut prompts,
            &mut calls,
            &mut pins,
        )
    };

    let closing = push_tail(session, width, opening, &mut lines);

    Drawn {
        lines,
        entries,
        prompts,
        calls,
        pins,
        closing,
    }
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
///
/// A word the interface drew on a line for a click, with the blanks before it, is no text of the
/// transcript: a match that takes in any of it is not one, so searching for `copy` finds the word a
/// reply says and not the one that copies its block.
fn highlight(lines: &mut [Line<'static>], needle: &str, pins: &[LinePin]) -> Vec<usize> {
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
        let mut found = crate::state::matched(&text, needle);
        let chrome = chrome_among(pins, index);
        if !found.is_empty() && !chrome.is_empty() {
            let drawn = wrap::chars_by_cells(line, &chrome);
            found.retain(|&(from, to)| {
                !drawn
                    .get(from..to)
                    .unwrap_or_default()
                    .iter()
                    .any(|(_, drawn_for_a_click)| *drawn_for_a_click)
            });
        }
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
    #[cfg(test)]
    counted::asked();
    if line.width() <= width as usize {
        return 1;
    }
    #[cfg(test)]
    counted::wrapped();
    let borrowed: Vec<Span<'_>> = line
        .spans
        .iter()
        .map(|span| Span::styled(span.content.as_ref(), span.style))
        .collect();
    Paragraph::new(Line::from(borrowed))
        .wrap(Wrap { trim: false })
        .line_count(width) as u16
}

/// How many rows at the end of `lines` hold no text, each line with the rows it takes: the blank row
/// that follows the last entry or the reply being written.
///
/// Only the lines from the end are looked at, up to the last that has a word.
fn blank_rows_at_the_end<'a>(
    lines: impl DoubleEndedIterator<Item = (&'a Line<'static>, usize)>,
) -> u16 {
    let blank: usize = lines
        .rev()
        .take_while(|(line, _)| line.spans.iter().all(|span| span.content.trim().is_empty()))
        .map(|(_, rows)| rows)
        .sum();
    u16::try_from(blank).unwrap_or(u16::MAX)
}

/// The items of `sorted` whose `key` is `at`, found by bisection where `sorted` is in order of key.
fn those_at<T>(sorted: &[T], at: usize, key: impl Fn(&T) -> usize) -> &[T] {
    let first = sorted.partition_point(|item| {
        #[cfg(test)]
        counted::compared();
        key(item) < at
    });
    let held = sorted.get(first..).unwrap_or_default();
    let run = held.partition_point(|item| {
        #[cfg(test)]
        counted::compared();
        key(item) <= at
    });
    held.get(..run).unwrap_or_default()
}

/// The columns of the line `line` that only the interface put there, among the pins of a layout.
///
/// The pins are in the order the lines are drawn in, so the ones on a line are found by bisection and
/// not by a look at every pin for every line.
fn chrome_among(pins: &[LinePin], line: usize) -> Vec<(u16, u16)> {
    those_at(pins, line, |pin| pin.line)
        .iter()
        .filter_map(LinePin::drawn_for_the_click)
        .collect()
}

/// What tells the row `row` by, where `lines` are the lines from the row `start` on, each with the
/// rows it takes, and `above` is the fingerprint of everything before them. `len` is how much of the
/// row's line is read, where an earlier frame read some of it. `chrome` says where the line that
/// begins at a row holds cells drawn for a click, which are no part of what tells the row.
fn mark_in_lines<'a>(
    row: usize,
    len: Option<usize>,
    start: usize,
    above: u64,
    lines: impl Iterator<Item = (&'a Line<'static>, usize)>,
    chrome: impl Fn(usize) -> Vec<(u16, u16)>,
) -> Mark {
    let (mut at, mut above) = (start, above);
    for (line, rows) in lines {
        let drawn_for_a_click = chrome(at);
        if row < at + rows {
            return Mark::of(row - at, above, line, len, &drawn_for_a_click);
        }
        above = fingerprint_after(above, line, &drawn_for_a_click);
        at += rows;
    }
    // A row past the last, which no selection holds: the frame that drew the transcript has
    // fewer rows than the selection reaches, and the selection is dropped for it.
    Mark::past_the_end(above)
}

/// The lines the entries of the transcript are drawn as, kept from one frame to the next.
///
/// Drawing the entries is the expensive half of a frame: markdown, tables and diffs for every one
/// of them. None of it depends on what is being typed, so a keystroke that touches no entry reuses
/// the lines instead of making them again. It is kept against everything it was made from: the
/// entries' stamp, the width, the palette, and the flags that change how an entry is drawn.
#[derive(Debug)]
pub(crate) struct EntryLines {
    stamp: u64,
    width: u16,
    palette: Palette,
    show_trail: bool,
    scrolling: bool,
    /// Whether the lines draw the word that copies a block, which is drawn only where the mouse is
    /// reported.
    copyable: bool,
    /// What the lines mark for the scans of the cells they are drawn in.
    marks: Marks,
    lines: Vec<Line<'static>>,
    /// The line each entry begins at.
    entries: Vec<usize>,
    prompts: Vec<usize>,
    calls: Vec<(usize, usize, usize)>,
    pins: Vec<LinePin>,
    /// The row each line begins at once wrapped, and the row after the last. Worked out on first
    /// use, since the lay-out for the scroller measures rows its own way and nothing at rest
    /// needs them.
    starts: OnceLock<Vec<usize>>,
    /// The fingerprint of the lines before each line, and of all of them after the last. Worked out
    /// on first use, which is a frame with a selection of the transcript held.
    prints: OnceLock<Vec<u64>>,
}

impl EntryLines {
    fn prints(&self) -> &[u64] {
        self.prints.get_or_init(|| {
            let mut above = 0u64;
            let mut prints = Vec::with_capacity(self.lines.len() + 1);
            for (index, line) in self.lines.iter().enumerate() {
                prints.push(above);
                above = fingerprint_after(above, line, &self.chrome_of(index));
            }
            prints.push(above);
            prints
        })
    }

    /// The columns of the line `line` that only the interface put there.
    ///
    /// The pins are in the order the lines are drawn in, so the ones on a line are found by
    /// bisection and not by a look at every pin for every line.
    fn chrome_of(&self, line: usize) -> Vec<(u16, u16)> {
        chrome_among(&self.pins, line)
    }

    /// What tells the row `row` of the transcript by: the kept lines, and what is drawn after them.
    fn mark_of_row(&self, row: u16, len: Option<usize>, tail: &[(Line<'static>, usize)]) -> Mark {
        let (starts, prints) = (self.starts(), self.prints());
        let row = usize::from(row);
        let head_rows = starts.last().copied().unwrap_or(0);
        if row < head_rows {
            let line = starts.partition_point(|&start| start <= row) - 1;
            return Mark::of(
                row - starts[line],
                prints[line],
                &self.lines[line],
                len,
                &self.chrome_of(line),
            );
        }
        // Nothing drawn after the kept lines has a word drawn on it for a click: a reply still
        // arriving has none until its round ends.
        mark_in_lines(
            row,
            len,
            head_rows,
            prints.last().copied().unwrap_or(0),
            tail.iter().map(|(line, rows)| (line, *rows)),
            |_| Vec::new(),
        )
    }

    fn starts(&self) -> &[usize] {
        self.starts.get_or_init(|| {
            let mut at = 0usize;
            let mut starts = Vec::with_capacity(self.lines.len() + 1);
            for line in &self.lines {
                starts.push(at);
                at += usize::from(rows_of(line, self.width));
            }
            starts.push(at);
            starts
        })
    }
}

/// The reply still arriving, drawn, and kept against the words it was drawn from.
///
/// A turn redraws on every pass whether or not the reply moved, and drawing markdown is the cost,
/// so a pass that carries a keystroke and no new words reuses the lines it already has.
#[derive(Debug)]
pub(crate) struct ReplyLines {
    text: String,
    width: u16,
    palette: Palette,
    marks: Marks,
    lines: Vec<Line<'static>>,
}

fn reply_lines(session: &Session, width: u16) -> Arc<ReplyLines> {
    let text = session.reply_so_far();
    let palette = theme::palette();
    let marks = transcript_links(session);
    session.reply_layout.get(
        |kept| {
            kept.text == text
                && kept.width == width
                && kept.palette == palette
                && kept.marks == marks
        },
        || ReplyLines {
            text: text.to_string(),
            width,
            lines: assistant_lines(text, width, marks),
            palette: palette.clone(),
            marks,
        },
    )
}

/// Whether `viewed` is the session's own transcript, which is the only sequence kept: a delegate's
/// lines are a different sequence with no stamp.
fn is_the_transcript(session: &Session, viewed: &[Entry]) -> bool {
    std::ptr::eq(viewed.as_ptr(), session.transcript.as_ptr())
        && viewed.len() == session.transcript.len()
}

fn cached_entries(session: &Session, width: u16) -> Arc<EntryLines> {
    let palette = theme::palette();
    let scrolling = session.scrolling();
    let copyable = session.mouse_captured();
    let marks = transcript_links(session);
    session.layout.get(
        |kept| {
            kept.stamp == session.transcript.stamp()
                && kept.width == width
                && kept.palette == palette
                && kept.show_trail == session.show_trail
                && kept.scrolling == scrolling
                && kept.copyable == copyable
                && kept.marks == marks
        },
        || {
            let (mut lines, mut prompts, mut calls, mut pins) =
                (Vec::new(), Vec::new(), Vec::new(), Vec::new());
            let entries = push_entries(
                session,
                &session.transcript,
                width,
                &mut lines,
                &mut prompts,
                &mut calls,
                &mut pins,
            );
            EntryLines {
                stamp: session.transcript.stamp(),
                width,
                palette: palette.clone(),
                show_trail: session.show_trail,
                scrolling,
                copyable,
                marks,
                lines,
                entries,
                prompts,
                calls,
                pins,
                starts: OnceLock::new(),
                prints: OnceLock::new(),
            }
        },
    )
}

/// The transcript's lines as the plain frame holds them at one width, and how many rows they and
/// the reply being written come to.
struct Measured {
    entries: Arc<EntryLines>,
    tail: Vec<(Line<'static>, usize)>,
    total: usize,
}

fn measure_kept(session: &Session, width: u16) -> Measured {
    let entries = cached_entries(session, width);
    let head_rows = entries.starts().last().copied().unwrap_or(0);
    let mut tail: Vec<Line<'static>> = Vec::new();
    let _ = push_tail(session, width, false, &mut tail);
    let tail: Vec<(Line<'static>, usize)> = tail
        .into_iter()
        .map(|line| {
            let rows = usize::from(rows_of(&line, width));
            (line, rows)
        })
        .collect();
    let total = head_rows + tail.iter().map(|(_, rows)| rows).sum::<usize>();
    Measured {
        entries,
        tail,
        total,
    }
}

/// The transcript drawn from what is kept, without making a line of it again and without handing
/// the terminal any line that is off the screen. `None` where the transcript is not the plain
/// sequence of entries: the scroller, the opening screen and a delegate's view draw it the
/// long way.
///
/// Where the transcript is longer than the pane, the last column of the pane is the scrollbar and
/// the lines are wrapped to the columns left (VIEW-39). Whether it is longer is read at that
/// narrower width: a transcript that overflows at the full width overflows at the narrower one, and
/// one that fits at the full width but not the narrower is drawn with the bar, so the width the
/// lines are wrapped to and the bar are never in disagreement.
fn draw_kept(frame: &mut Frame, area: Rect, session: &Session) -> Option<Laid> {
    let viewed = session.viewed();
    if session.scrolling() || opening_screen(session) || !is_the_transcript(session, viewed) {
        return None;
    }
    if session.shows_a_scrollbar() && area.width > 1 && area.height > 2 {
        let text = Rect {
            width: area.width - 1,
            ..area
        };
        let measured = measure_kept(session, text.width);
        if measured.total > usize::from(area.height) {
            let (mut laid, offset) = draw_measured(frame, text, session, measured);
            let track = Rect::new(area.right() - 1, area.y, 1, area.height);
            let bar = Bar {
                track,
                rows: laid.rows,
                top: u16::try_from(offset).unwrap_or(u16::MAX),
            };
            draw_scrollbar(frame, &bar);
            laid.targets
                .push(Target::on_screen(bar.track, Hit::Scrollbar));
            laid.bar = Some(bar);
            return Some(laid);
        }
    }
    let measured = measure_kept(session, area.width);
    Some(draw_measured(frame, area, session, measured).0)
}

/// The scrollbar's thumb, in the colour of the other things that are there to be found and not
/// read. The rest of the track is left blank.
fn draw_scrollbar(frame: &mut Frame, bar: &Bar) {
    let (first, length) = bar.thumb();
    let style = Style::default().fg(theme::muted());
    for row in first..first + length {
        let cell = &mut frame.buffer_mut()[(bar.track.x, bar.track.y + row)];
        cell.set_symbol(SCROLLBAR_THUMB);
        cell.set_style(style);
    }
}

/// What the thumb of the scrollbar is drawn with: the right half of a cell, so that it sits against
/// the edge of the pane and not in the middle of the column.
pub(super) const SCROLLBAR_THUMB: &str = "\u{2590}";

/// The lines [`measure_kept`] measured, drawn in `area`, with the row of the transcript the area
/// starts at.
fn draw_measured(
    frame: &mut Frame,
    area: Rect,
    session: &Session,
    measured: Measured,
) -> (Laid, usize) {
    let width = area.width;
    let Measured {
        entries,
        tail,
        total,
    } = measured;
    let starts = entries.starts();

    let height = usize::from(area.height);
    let max_offset = total.saturating_sub(height);
    let offset = if session.scroll > 0 {
        // The words a scroller was closed on are put at the top, where they were: the row they were
        // on is a row of another layout (SCROLL-1).
        session
            .words_held()
            .and_then(|top| row_of_words(top, starts, &entries.entries, total))
            .unwrap_or_else(|| usize::from(session.top_row()))
            .min(max_offset)
    } else {
        max_offset
    };

    // The line holding the first row on the screen, and how far into it the screen starts. Lines
    // wrap alone, so wrapping from that line gives the rows a wrap of everything would.
    let first = starts
        .partition_point(|&start| start <= offset)
        .saturating_sub(1)
        .min(entries.lines.len());
    let mut at = starts[first];
    let mut shown: Vec<Line<'static>> = Vec::new();
    let mut skip = 0;
    let mut continued = Vec::new();
    let head = entries.lines[first..]
        .iter()
        .zip(starts[first..].windows(2).map(|pair| pair[1] - pair[0]));
    for (line, rows) in head.chain(tail.iter().map(|(line, rows)| (line, *rows))) {
        if at + rows <= offset {
            at += rows;
            continue;
        }
        if shown.is_empty() {
            skip = offset - at;
        }
        shown.push(line.clone());
        note_continued(
            &mut continued,
            at,
            (rows, line),
            offset,
            height,
            area.y,
            width,
        );
        at += rows;
        if at >= offset + height {
            break;
        }
    }
    let paragraph = Paragraph::new(shown)
        .wrap(Wrap { trim: false })
        .scroll((skip as u16, 0));
    frame.render_widget(paragraph, area);
    link_references(frame, area, session);
    let links = scan_links(frame, area, &continued);

    let marks = session
        .selection
        .and_then(|selection| selection.marked_rows())
        .map(|rows| rows.map(|(row, len)| entries.mark_of_row(row, len, &tail)));
    let closing_blank_rows = blank_rows_at_the_end(
        entries
            .lines
            .iter()
            .zip(starts.windows(2).map(|pair| pair[1] - pair[0]))
            .chain(tail.iter().map(|(line, rows)| (line, *rows))),
    );
    let mut laid = Laid {
        width,
        height: area.height,
        rows: total.min(usize::from(u16::MAX)) as u16,
        closing_blank_rows,
        continued,
        marks,
        window: Some(Window {
            area,
            top: u16::try_from(offset).unwrap_or(u16::MAX),
            view: View::of(session),
        }),
        ..Laid::default()
    };
    // Each is a row a click can be answered against, so they are recorded at rest as the scroller
    // records them, from the rows the lines were already measured to.
    let row_of = |line: usize| u16::try_from(starts[line]).unwrap_or(u16::MAX);
    place_calls(&mut laid, session.viewed(), width, &entries.calls, row_of);
    place_pins(&mut laid, &entries.pins, row_of);
    laid.chrome = chrome_on_screen(&laid.targets, offset, area);
    place_links(&mut laid, links);
    (laid, offset)
}

/// The transcript's rows drawn on their own, for a copy to read rows that are not on the screen.
///
/// Made from the lines the frame draws and wrapped by the same widget at the same width, so a row
/// is the cells the screen holds for it while it is in view. Nothing is read from anywhere the draw
/// does not read: these are the lines released for display, and the copy is of what was shown.
pub(crate) struct Rows {
    lines: Vec<Line<'static>>,
    /// The row each line begins at, and the row after the last.
    starts: Vec<usize>,
    /// The cells drawn for a click, by the row of the transcript they are on.
    chrome: Vec<Rect>,
    x: u16,
    width: u16,
}

impl Rows {
    /// The rows of the transcript as the view the window describes draws them.
    pub(crate) fn new(session: &Session, window: Window) -> Self {
        let width = window.area.width;
        let Drawn { lines, pins, .. } = with_prompts(session, width, window.area.height);
        let mut at = 0usize;
        let mut starts = Vec::with_capacity(lines.len() + 1);
        for line in &lines {
            starts.push(at);
            at += usize::from(rows_of(line, width));
        }
        starts.push(at);
        // What a frame leaves out of a copy, found the way the frame finds it.
        let mut placed = Laid::default();
        place_pins(&mut placed, &pins, |line| {
            u16::try_from(starts[line]).unwrap_or(u16::MAX)
        });
        let chrome = chrome_on_screen(
            &placed.targets,
            0,
            Rect::new(window.area.x, 0, width, u16::MAX),
        );
        Self {
            lines,
            starts,
            chrome,
            x: window.area.x,
            width,
        }
    }

    /// The rows from `first` to `last`, drawn in the columns they have on the screen. Rows past the
    /// end of the transcript are blank.
    pub(crate) fn window(&self, first: u16, last: u16) -> crate::select::Rendered {
        let count = last.saturating_sub(first).saturating_add(1);
        let area = Rect::new(self.x, 0, self.width, count);
        let chrome = self
            .chrome
            .iter()
            .filter(|cells| (first..=last).contains(&cells.y))
            .map(|cells| Rect::new(cells.x, cells.y - first, cells.width, 1))
            .collect();
        let mut buffer = Buffer::empty(area);
        let mut continued = Vec::new();
        let (first, last) = (usize::from(first), usize::from(last));
        let total = self.starts.last().copied().unwrap_or(0);
        if first < total {
            let holding = |row: usize| self.starts.partition_point(|&start| start <= row) - 1;
            let (from, to) = (holding(first), holding(last.min(total - 1)));
            for index in from..=to {
                note_continued(
                    &mut continued,
                    self.starts[index],
                    (
                        self.starts[index + 1] - self.starts[index],
                        &self.lines[index],
                    ),
                    first,
                    usize::from(count),
                    0,
                    self.width,
                );
            }
            Paragraph::new(self.lines[from..=to].to_vec())
                .wrap(Wrap { trim: false })
                .scroll((
                    u16::try_from(first - self.starts[from]).unwrap_or(u16::MAX),
                    0,
                ))
                .render(area, &mut buffer);
        }
        crate::select::Rendered {
            buffer,
            continued,
            chrome,
        }
    }
}

/// The width the words of a reply are drawn at for [`links_of_the_latest_reply`] before any frame
/// has been drawn and recorded one.
const WIDTH_BEFORE_A_FRAME: u16 = 80;

/// The links drawn in the words of the latest reply that holds any, in the order they are drawn
/// (CMD-30). Where the reply still arriving holds one it is that reply, and otherwise the newest
/// reply of the transcript that does.
///
/// Found as a click finds them. Each line of a reply is drawn into a buffer by the widget that
/// draws the transcript, at the width the last frame drew it at, and [`crate::text_links::scan`]
/// reads the marks the matcher left in the cells, so which characters are an address or a path, how
/// a wrapped one is joined and which are left out are the click's rules and no second set. Where a
/// reply is longer than the screen the click reaches the links in view and these are the links of
/// the whole reply.
pub(crate) fn links_of_the_latest_reply(session: &Session) -> Vec<Found> {
    let width = match session.laid.width {
        0 => WIDTH_BEFORE_A_FRAME,
        width => width,
    };
    if !session.reply_so_far().is_empty() && !session.watching_a_delegate() {
        let found = links_in_lines(&reply_lines(session, width).lines, width);
        if !found.is_empty() {
            return found;
        }
    }
    let entries = cached_entries(session, width);
    entries
        .calls
        .iter()
        .rev()
        .filter(|&&(_, _, index)| {
            session
                .transcript
                .get(index)
                .is_some_and(|entry| entry.speaker == Speaker::Assistant)
        })
        .map(|&(first, last, _)| links_in_lines(&entries.lines[first..last], width))
        .find(|found| !found.is_empty())
        .unwrap_or_default()
}

/// The links in `lines` as they are drawn at `width`, one line at a time: a link is never wider than
/// the line it is in, so a line is drawn into a buffer of its own and what is read off it is what a
/// frame reads off the same rows.
///
/// A blank row is left under each line, so a link that ends in the last column of the last row is
/// not taken for one that goes on below it, which is how a frame reads a link at the foot of the
/// area it was drawn in.
fn links_in_lines(lines: &[Line<'static>], width: u16) -> Vec<Found> {
    let width = width.max(1);
    let mut found = Vec::new();
    for line in lines {
        let rows = rows_of(line, width);
        let area = Rect::new(0, 0, width, rows.saturating_add(1));
        let mut buffer = Buffer::empty(area);
        Paragraph::new(line.clone())
            .wrap(Wrap { trim: false })
            .render(area, &mut buffer);
        let mut continued = Vec::new();
        note_continued(
            &mut continued,
            0,
            (usize::from(rows), line),
            0,
            usize::from(area.height),
            0,
            width,
        );
        found.extend(crate::text_links::scan(&mut buffer, area, &continued));
    }
    found
}

/// What the assistant's words in the transcript mark, for [`scan_links`] and [`link_references`] to
/// read off the cells they are drawn in.
///
/// Every place that draws these lines scans them, since a mark nobody takes off would reach the
/// terminal. The numbers are marked only where the session can link them (VIEW-32).
pub(super) fn transcript_links(session: &Session) -> Marks {
    Marks {
        references: session.repository().is_some(),
        addresses: true,
        paths: true,
    }
}

/// Find the links drawn in `area` (MOUSE-40), which also takes the marks the matcher left off the
/// cells. Called after [`link_references`], which must not read the digits of an address as a
/// number, and by every path that draws the transcript.
pub(super) fn scan_links(
    frame: &mut Frame,
    area: Rect,
    continued: &[(u16, bool)],
) -> Vec<crate::text_links::Found> {
    crate::text_links::scan(frame.buffer_mut(), area, continued)
}

/// Link each `#<number>` drawn in `area` to the issue or pull request it names (VIEW-32), where the
/// session's checkout has a GitHub `origin`.
///
/// Called after the widget has wrapped the lines, since only then is it known which cells a
/// number landed in. Every path that draws the transcript or an assistant's words calls it.
pub(super) fn link_references(frame: &mut Frame, area: Rect, session: &Session) {
    if let Some(repository) = session.repository() {
        crate::issue_link::link_references(frame.buffer_mut(), area, repository);
    }
}

/// Record the screen rows, among those a line of `rows` rows starting at row `at` of the
/// transcript is drawn over, that are not the first of it: the ones the wrap made. Each comes with
/// whether the wrap broke the line at a blank, which a copy puts back as a space, or inside a word.
///
/// `offset` is the first row of the transcript on the screen, `height` how many it shows, `top`
/// the screen row the transcript starts at and `width` the width the line was wrapped to.
fn note_continued(
    out: &mut Vec<(u16, bool)>,
    at: usize,
    (rows, line): (usize, &Line<'_>),
    offset: usize,
    height: usize,
    top: u16,
    width: u16,
) {
    if rows < 2 || at + rows <= offset || at >= offset + height {
        return;
    }
    // Only as far as the rows on the screen reach.
    let wanted = (at + rows).min(offset + height).saturating_sub(at + 1);
    let breaks = blank_breaks(line, width, rows, wanted);
    for row in (at + 1)..(at + rows) {
        if row >= offset && row < offset + height {
            let at_blank = breaks.get(row - at - 1).copied().unwrap_or(false);
            out.push((top + (row - offset) as u16, at_blank));
        }
    }
}

/// For each of the first `wanted` breaks the wrap made in `line` at `width`, whether it fell on a
/// blank.
///
/// The screen alone cannot say. A word that ends in the last column has its blank dropped and the
/// next word starts the next row, which looks like a row cut inside a word. A run of blanks can be
/// drawn on the row it ends, dropped, or left at the start of the next row, and no count of cells tells
/// which. So the line is wrapped by itself, through the same wrapping that draws it, and a row is told
/// by what stands in the text before its first drawn character: the break is at a blank where any
/// character between that and the last drawn character of the rows before is one the wrap takes for a
/// blank.
///
/// `rows` is how many rows the whole line takes. Only the start of it that holds the rows wanted is
/// wrapped, so a line of a million characters costs what the rows shown of it do and not the line.
fn blank_breaks(line: &Line<'_>, width: u16, rows: usize, wanted: usize) -> Vec<bool> {
    let width = width.max(1);
    let wanted = wanted.min(rows.saturating_sub(1));
    if wanted == 0 {
        return Vec::new();
    }
    let needed = wanted + 1;
    let columns = usize::from(width);
    let area = Rect::new(0, 0, width, u16::try_from(needed).unwrap_or(u16::MAX));
    let mut budget = needed.saturating_mul(columns).saturating_mul(2);
    loop {
        // The graphemes that fill `budget` columns, which the wrap drew in this order, and whether
        // that is the whole line.
        let mut graphemes = Vec::new();
        let mut used = 0;
        let mut taken = line.styled_graphemes(Style::default());
        for grapheme in taken.by_ref() {
            used += wrap::cell_width(grapheme.symbol);
            graphemes.push(grapheme);
            if used >= budget {
                break;
            }
        }
        let whole = taken.next().is_none();
        #[cfg(test)]
        counted::read(graphemes.len());

        let mut buffer = Buffer::empty(area);
        let text: String = graphemes.iter().map(|grapheme| grapheme.symbol).collect();
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .render(area, &mut buffer);

        // Of the characters the wrap draws, in the order it draws them, whether a blank came before
        // each since the one before it. A blank is a character the wrap breaks at, which is not the
        // zero-width space, since that is no blank in what is copied.
        let mut blank_before = Vec::new();
        let mut blank = false;
        for grapheme in &graphemes {
            let drawn = wrap::cell_width(grapheme.symbol);
            if grapheme.is_whitespace() {
                blank |= grapheme.symbol != "\u{200b}";
            } else if drawn > 0 && drawn <= columns {
                blank_before.push(blank);
                blank = false;
            }
        }

        // How many of them each row holds: the cells that are not blank, a wide one once.
        let mut first_of_row = vec![0usize];
        for row in 0..area.height {
            let mut column = 0;
            let mut held = 0;
            while column < width {
                let symbol = buffer[(column, row)].symbol();
                if symbol != " " {
                    held += 1;
                }
                column += u16::try_from(wrap::cell_width(symbol).max(1)).unwrap_or(1);
            }
            first_of_row.push(first_of_row.last().copied().unwrap_or(0) + held);
        }

        // The row wanted last is complete once it holds something, and the line cannot give more
        // than it has.
        let complete = first_of_row.get(needed) > first_of_row.get(wanted);
        if complete || whole {
            return (1..needed)
                .map(|row| {
                    first_of_row
                        .get(row)
                        .and_then(|&first| blank_before.get(first))
                        .copied()
                        .unwrap_or(true)
                })
                .collect();
        }
        budget = budget.saturating_mul(2);
    }
}

/// The row, counted from the first row of the transcript, where task `item` of the plan a turn left
/// in the transcript is drawn at the width of the last frame (MOUSE-31). `None` where the plan is
/// not in the transcript, the task is not one of its rows, or the view is not the transcript.
///
/// Worked out from where the lines kept for the last frame say the plan's entry begins and the lines
/// this one draws before the task, through the functions that draw them, so a row that wraps counts
/// as the rows it takes. Only that entry is laid out again: the entries above it are the rows the
/// frame already measured, however many there are.
pub(crate) fn plan_item_row(session: &Session, item: usize) -> Option<u16> {
    if session.watching_a_delegate() {
        return None;
    }
    let width = session.laid.width;
    let at = session.plan_entry()?;
    let entry = session.transcript.get(at)?;
    if item >= entry.todos.len() {
        return None;
    }
    let kept = cached_entries(session, width);
    let above = *kept.starts().get(*kept.entries.get(at)?)?;
    let mut lines = speaker_laid(session, entry, width).0;
    let without_the_plan = Entry {
        todos: Vec::new(),
        trail: Vec::new(),
        ..entry.clone()
    };
    entry_extras(
        session,
        &without_the_plan,
        width,
        lines.len(),
        &mut lines,
        &mut Vec::new(),
        at,
    );
    lines.extend(todo_lines(&entry.todos[..item], true));
    let within = lines
        .iter()
        .map(|line| rows_of(line, width))
        .fold(0, u16::saturating_add);
    Some(
        u16::try_from(above)
            .unwrap_or(u16::MAX)
            .saturating_add(within),
    )
}

/// The rows each of `lines` begins at once drawn at `width`, and the row after the last.
fn row_starts(lines: &[Line<'static>], width: u16) -> Vec<usize> {
    let mut at = 0usize;
    let mut starts = Vec::with_capacity(lines.len() + 1);
    for line in lines {
        starts.push(at);
        at += usize::from(rows_of(line, width));
    }
    starts.push(at);
    starts
}

/// The words at the top of the view the last frame drew: the entry the first row on the screen
/// belongs to, and how many rows into that entry's rows it is (SCROLL-1). `None` where no frame has
/// been drawn or the first row is above every entry, as the opening screen's is.
///
/// Read in the layout the frame drew: the plain one, whose width is what the info panel and the
/// scrollbar leave and in which no call is expanded, or the scroller's.
pub(crate) fn top_of_view(session: &Session) -> Option<TopOfView> {
    let width = session.laid.width;
    if width == 0 {
        return None;
    }
    let Drawn { lines, entries, .. } = with_prompts(session, width, session.laid.height);
    let starts = row_starts(&lines, width);
    let begins: Vec<usize> = entries
        .iter()
        .filter_map(|&line| starts.get(line).copied())
        .collect();
    let top = usize::from(session.top_row());
    let entry = begins.partition_point(|&row| row <= top).checked_sub(1)?;
    Some(TopOfView {
        entry,
        rows_in: top - begins.get(entry).copied()?,
    })
}

/// Where the words at the top of the view are in the layout the scroller draws at `width` by
/// `height`, with that layout, so that opening the scroller moves nothing (SCROLL-1).
///
/// The scroller draws the transcript across the whole screen and with the calls the person expanded
/// open, so the row of the words is not the row they were on. A row that is past the end of the entry
/// there, because the entry takes fewer rows, is the entry's last.
pub(crate) fn place_top_of_view(
    session: &Session,
    top: TopOfView,
    width: u16,
    height: u16,
) -> Option<(Laid, u16)> {
    let Drawn { lines, entries, .. } = with_prompts(session, width, height);
    let starts = row_starts(&lines, width);
    let row = row_of_words(top, &starts, &entries, *starts.last()?)?;
    Some((lay_out(session, width, height).1, u16::try_from(row).ok()?))
}

/// The row the words `top` are at, where `starts` is the row each line begins at, `entries` the line
/// each entry begins at and `end` the row after the last. A row past the end of the entry, because
/// the entry takes fewer rows here than it did, is the entry's last.
fn row_of_words(top: TopOfView, starts: &[usize], entries: &[usize], end: usize) -> Option<usize> {
    let first = *starts.get(*entries.get(top.entry)?)?;
    let after = match entries.get(top.entry + 1) {
        Some(&line) => *starts.get(line)?,
        None => end,
    };
    Some(first + top.rows_in.min(after.saturating_sub(first + 1)))
}

/// The transcript laid out at a width: the lines to draw, and where the rows worth reaching are.
///
/// Where each line begins is only worked out while the scroller is open. It costs a wrap of every
/// line, and at rest nothing asks the question: the wheel moves by rows and never has to know
/// which row is which.
pub(super) fn lay_out(session: &Session, width: u16, height: u16) -> (Vec<Line<'static>>, Laid) {
    let Drawn {
        mut lines,
        prompts,
        calls,
        pins,
        closing,
        ..
    } = with_prompts(session, width, height);
    let held = highlight(&mut lines, session.needle(), &pins);

    let mut laid = Laid {
        width,
        height,
        window: Some(Window {
            area: Rect::new(0, 0, width, height),
            top: 0,
            view: View::of(session),
        }),
        ..Laid::default()
    };
    // A delegate's view is measured too, since the sentence that closes it is a place to click.
    if !session.scrolling() && !session.watching_a_delegate() {
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
    place_calls(&mut laid, session.viewed(), width, &calls, |line| {
        starts[line]
    });
    place_pins(&mut laid, &pins, |line| starts[line]);
    if let Some(line) = closing {
        // Every row the sentence is drawn over, across the width, as a call's rows are.
        laid.targets.push(Target::in_transcript(
            Rect::new(
                0,
                starts[line],
                width,
                starts[line + 1].saturating_sub(starts[line]),
            ),
            Hit::Back,
            0,
        ));
    }
    laid.rows = at;
    (lines, laid)
}

/// The transcript as the text of its lines, one to a line, as the frame draws them but without the
/// cells drawn for a click: what is handed to an editor is the text and not the interface's words on
/// it.
pub(super) fn plain_rows(session: &Session, width: u16, height: u16) -> Vec<String> {
    let Drawn { lines, pins, .. } = with_prompts(session, width, height);
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| wrap::without_chrome(line, &chrome_among(&pins, index)))
        .collect()
}

/// The cells the targets in the transcript draw for a click and no part of the text around them, as
/// the screen holds them, where `offset` is the first row of the transcript on it.
fn chrome_on_screen(targets: &[Target], offset: usize, area: Rect) -> Vec<Rect> {
    targets
        .iter()
        .filter(|target| target.space == Space::Transcript && target.hit.is_chrome())
        .filter_map(|target| {
            let row = usize::from(target.area.y).checked_sub(offset)?;
            (row < usize::from(area.height)).then(|| {
                Rect::new(
                    area.x + target.area.x,
                    area.y
                        .saturating_add(u16::try_from(row).unwrap_or(u16::MAX)),
                    target.area.width,
                    1,
                )
            })
        })
        .collect()
}

/// Record the cells of each line a click acts on, on the row the line begins at.
fn place_pins(laid: &mut Laid, pins: &[LinePin], row_of: impl Fn(usize) -> u16) {
    laid.targets.extend(pins.iter().map(|pin| {
        Target::in_transcript(
            Rect::new(pin.column, row_of(pin.line), pin.width, 1),
            pin.hit,
            pin.padding,
        )
    }));
}

/// Record the cells of each link the scan found, as the screen holds them, and what each opens.
fn place_links(laid: &mut Laid, found: Vec<crate::text_links::Found>) {
    for link in found {
        let index = laid.links.len();
        laid.targets
            .extend(link.cells.iter().map(|&(row, from, to)| {
                Target::on_screen(
                    Rect::new(from, row, to.saturating_sub(from), 1),
                    Hit::Link(index),
                )
            }));
        laid.links.push(link.target);
    }
}

/// Record where each call with more to show and each delegate's block is drawn, as the rows of the
/// transcript each covers across `width` columns.
///
/// `calls` holds the lines each covers and the index of its entry, and `row_of` says which row a
/// line begins at, the row after the last line included. A delegate's block can be opened and a
/// call expanded, and nothing about one can do the other, so the two are kept apart here.
fn place_calls(
    laid: &mut Laid,
    viewed: &[Entry],
    width: u16,
    calls: &[(usize, usize, usize)],
    row_of: impl Fn(usize) -> u16,
) {
    for &(first, last, index) in calls {
        let (start, after) = (row_of(first), row_of(last));
        let Some(rows) = after.checked_sub(start).filter(|rows| *rows > 0) else {
            continue;
        };
        let hit = match viewed.get(index) {
            Some(entry) if entry.delegate.is_some() => Hit::Delegate(index),
            Some(entry) if entry.speaker == Speaker::Assistant => Hit::Words(index),
            _ => Hit::Call(index),
        };
        laid.targets.push(Target::in_transcript(
            Rect::new(0, start, width, rows),
            hit,
            0,
        ));
    }
}

pub(super) fn draw_transcript(frame: &mut Frame, area: Rect, session: &Session) -> Laid {
    if let Some(laid) = draw_kept(frame, area, session) {
        return laid;
    }
    let (lines, mut laid) = lay_out(session, area.width, area.height);
    let line_rows: Vec<(usize, Line<'static>)> = lines
        .iter()
        .map(|line| {
            let rows = usize::from(rows_of(line, area.width));
            // Only a line the wrap cut needs its text kept to say how.
            (
                rows,
                if rows > 1 {
                    line.clone()
                } else {
                    Line::default()
                },
            )
        })
        .collect();
    // The cells drawn for a click, by the row they begin at, which are in the order the rows are.
    let mut chrome_rows: Vec<(usize, (u16, u16))> = laid
        .targets
        .iter()
        .filter(|target| target.space == Space::Transcript)
        .filter_map(|target| Some((usize::from(target.area.y), target.drawn_for_the_click()?)))
        .collect();
    chrome_rows.sort_by_key(|(row, _)| *row);
    laid.closing_blank_rows =
        blank_rows_at_the_end(lines.iter().zip(line_rows.iter().map(|(rows, _)| *rows)));
    let marks = session
        .selection
        .and_then(|selection| selection.marked_rows())
        .map(|rows| {
            if opening_screen(session) {
                return [Mark::OPENING_SCREEN; 2];
            }
            rows.map(|(row, len)| {
                let heights = line_rows.iter().map(|(rows, _)| *rows);
                mark_in_lines(
                    usize::from(row),
                    len,
                    0,
                    0,
                    lines.iter().zip(heights),
                    |at| {
                        those_at(&chrome_rows, at, |(row, _)| *row)
                            .iter()
                            .map(|(_, cells)| *cells)
                            .collect()
                    },
                )
            })
        });
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

    let (mut at, height) = (0usize, usize::from(area.height));
    for (rows, line) in line_rows {
        if at >= usize::from(offset) + height {
            break;
        }
        note_continued(
            &mut laid.continued,
            at,
            (rows, &line),
            usize::from(offset),
            height,
            area.y,
            area.width,
        );
        at += rows;
    }

    frame.render_widget(paragraph.scroll((offset, 0)), area);
    link_references(frame, area, session);
    let links = scan_links(frame, area, &laid.continued);

    // The paragraph's own count rather than the sum of the rows measured line by line, so what a
    // key is answered against is the number the view was actually drawn with.
    laid.rows = total;
    laid.marks = marks;
    laid.window = Some(Window {
        area,
        top: offset,
        view: View::of(session),
    });
    laid.chrome = chrome_on_screen(&laid.targets, usize::from(offset), area);
    place_links(&mut laid, links);
    laid
}

/// Render one line of the audit trail.
///
/// Refusals are coloured differently from passes: a blocked gate is the most important
/// thing on the screen when it happens. The wording is settled in [`bravebot_session::audit`], so a line
/// that happened in this session and one read back off disk are drawn the same way.
///
/// The text holds paths and listing entry names a checkout chose, so it goes through [`printable`]
/// (LAYER-3): a control character is drawn as its picture instead of being dropped by the buffer.
fn trail_line(recorded: &TrailLine) -> Line<'static> {
    let style = if recorded.blocked {
        Style::default()
            .fg(theme::fail())
            .add_modifier(Modifier::BOLD)
    } else {
        dim()
    };
    Line::from(Span::styled(
        format!("  {DETAIL_MARKER} {}", printable(&recorded.text)),
        style,
    ))
}

/// What [`rows_of`] did on this thread, for a test that bounds the work of laying a transcript
/// out. A clock reading includes whatever else the machine was running, so a bound on elapsed
/// time fails under load on code that did no more work.
#[cfg(test)]
pub(super) mod counted {
    use std::cell::Cell;

    /// Lines [`super::rows_of`] was asked about, how many of them it wrapped to answer, how many
    /// places a line was looked up by were compared to find the cells drawn for a click on it, and how
    /// many graphemes of a line were read to tell where its wrap broke at a blank.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub(crate) struct Work {
        pub(crate) asked: usize,
        pub(crate) wrapped: usize,
        pub(crate) compared: usize,
        pub(crate) read: usize,
    }

    thread_local! {
        static ASKED: Cell<usize> = const { Cell::new(0) };
        static WRAPPED: Cell<usize> = const { Cell::new(0) };
        static COMPARED: Cell<usize> = const { Cell::new(0) };
        static READ: Cell<usize> = const { Cell::new(0) };
    }

    pub(super) fn compared() {
        COMPARED.set(COMPARED.get() + 1);
    }

    pub(super) fn read(graphemes: usize) {
        READ.set(READ.get() + graphemes);
    }

    pub(super) fn asked() {
        ASKED.set(ASKED.get() + 1);
    }

    pub(super) fn wrapped() {
        WRAPPED.set(WRAPPED.get() + 1);
    }

    /// Runs `body` and returns what it did, counting from nothing on this thread.
    pub(crate) fn during<R>(body: impl FnOnce() -> R) -> (R, Work) {
        ASKED.set(0);
        WRAPPED.set(0);
        COMPARED.set(0);
        READ.set(0);
        let result = body();
        let work = Work {
            asked: ASKED.get(),
            wrapped: WRAPPED.get(),
            compared: COMPARED.get(),
            read: READ.get(),
        };
        (result, work)
    }
}

#[cfg(test)]
mod tests {
    /// A word ending in the last column has its blank dropped, and the row after it looks like
    /// one cut inside a word. The layout says which it was.
    #[test]
    fn a_break_is_told_from_one_inside_a_word() {
        let at_blank = Line::raw("aaaa bbbbb ccccccccc eeee");
        assert_eq!(super::blank_breaks(&at_blank, 20, 2, 1), vec![true]);
        let inside = Line::raw("abcdefghijklmnopqrstuvw");
        assert_eq!(super::blank_breaks(&inside, 20, 2, 1), vec![false]);
        let spaced = Line::raw("aaaa  bbbbb ccccccccc  eeee ffff");
        assert_eq!(super::blank_breaks(&spaced, 20, 2, 1), vec![true]);
        let mixed = Line::raw("one two three abcdefghijklmnopqrstuvw");
        assert_eq!(super::blank_breaks(&mixed, 20, 3, 2), vec![true, false]);
    }

    use super::*;
    use crate::panel::LEFT_OPEN_MARKER;
    use crate::render::QUARANTINE_BAR;
    use bravebot_agent::diff::Change;
    use bravebot_agent::report::{Activity, Landing, Reach, Returned, Shown};
    use bravebot_core::todo::{Row, Status};

    fn text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    fn position(lines: &[Line<'_>], needle: &str) -> usize {
        lines
            .iter()
            .position(|line| text(line).contains(needle))
            .unwrap_or_else(|| panic!("{needle} was not drawn"))
    }

    /// Both are the session speaking. Drawn in different places or colours, the same words would
    /// read as coming from two sources depending on whether the person typed the command before
    /// the turn or during it.
    #[test]
    fn a_note_and_a_command_answered_mid_turn_are_drawn_alike() {
        let _held = crate::theme::exclusive();
        let said = "first line\nsecond line";

        let mut noted = Session::new("kernel-enforced");
        noted.transcript.push(Entry::user("hello"));
        noted.note(said);
        let noted = with_prompts(&noted, 90, 24).lines;

        let mut answered = Session::new("kernel-enforced");
        answered.transcript.push(Entry::user("hello"));
        answered.answer_while_working(|session| session.note(said));
        let answered = with_prompts(&answered, 90, 24).lines;

        let first = position(&noted, "first line");
        let rows: Vec<String> = noted[first..first + 2].iter().map(text).collect();
        assert_eq!(rows, ["  first line", "  second line"]);
        for line in &noted[first..first + 2] {
            assert_eq!(line.spans.len(), 1, "{line:?}");
            assert_eq!(line.spans[0].style, Style::default().fg(theme::note()));
        }
        assert_eq!(answered, noted);
    }

    /// MSG-11: a message to or from another session is drawn as a heading and the words quoted,
    /// neither behind the person's prompt marker nor as a reply, and nothing in a name or in the
    /// words can move the cursor.
    #[test]
    fn a_message_between_sessions_is_drawn_under_its_heading_with_the_words_quoted() {
        let _held = crate::theme::exclusive();
        let session = Session::new("kernel-enforced");
        let rows: Vec<String> = speaker_lines(
            &session,
            &Entry::session_message(
                "message from session 11111111 (x\u{1b}[2Jy) \u{b7} (T,pub)".to_string(),
                Some("first\n\u{1b}[2Ksecond"),
            ),
            90,
        )
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect();
        assert_eq!(
            rows,
            [
                "  message from session 11111111 (x\u{241b}[2Jy) \u{b7} (T,pub)",
                "  > first",
                "  > \u{241b}[2Ksecond"
            ]
        );
    }

    /// Who an entry is from decides how it reads: the person's words behind the marker they were
    /// typed with, a failure in its own colour and wrapped so the whole reason can be read, a
    /// command's output plainly, and a reply under the definition's name where a person addressed
    /// one.
    #[test]
    fn each_speaker_is_drawn_in_its_own_shape() {
        let _held = crate::theme::exclusive();
        let session = Session::new("kernel-enforced");
        let drawn = |entry: Entry, width: u16| -> Vec<Line<'static>> {
            speaker_lines(&session, &entry, width)
        };
        let styled = |lines: &[Line<'_>]| -> Vec<(String, Style)> {
            lines
                .iter()
                .map(|line| (text(line), line.spans[0].style))
                .collect()
        };
        let person = Style::default().fg(theme::brand_primary());
        let fail = Style::default().fg(theme::fail());

        assert_eq!(
            styled(&drawn(Entry::user("first\nsecond"), 90)),
            [
                ("> first".to_string(), person),
                ("  second".to_string(), person)
            ]
        );

        let accent = Style::default().fg(theme::accent());
        assert_eq!(
            styled(&drawn(Entry::shell("ls\n| wc"), 90)),
            [("! ls".to_string(), accent), ("  | wc".to_string(), accent)]
        );

        assert_eq!(
            styled(&drawn(Entry::output("one\ntwo"), 90)),
            [
                ("  one".to_string(), Style::default()),
                ("  two".to_string(), Style::default())
            ]
        );

        assert_eq!(
            styled(&drawn(Entry::stopped("stopped by the person"), 90)),
            [("  stopped by the person".to_string(), fail)]
        );

        let reason = "alpha beta gamma delta epsilon zeta";
        let rows = drawn(Entry::failure(reason), 20);
        assert!(rows.len() >= 2, "a reason wider than the room was clipped");
        for row in &rows {
            assert!(text(row).starts_with("  ") && row.width() <= 20, "{row:?}");
            assert_eq!(row.spans[0].style, fail);
        }
        let words: Vec<String> = rows
            .iter()
            .map(|row| text(row).trim().to_string())
            .collect();
        assert_eq!(words.join(" "), reason);

        // A terminal narrower than the lead and a word still wraps at a width a word fits.
        let narrow: Vec<String> = drawn(Entry::failure("abcdefg hijklmn"), 4)
            .iter()
            .map(|row| text(row).trim().to_string())
            .collect();
        assert_eq!(narrow, ["abcdefg", "hijklmn"]);

        let named = drawn(
            Entry::assistant("hello", Vec::new()).answered_as(Some("reader".to_string())),
            90,
        );
        assert_eq!(
            styled(&named[..1]),
            [(
                "  reader answered".to_string(),
                Style::default().add_modifier(Modifier::BOLD)
            )]
        );
        assert!(text(&named[1]).ends_with("hello"), "{named:?}");
    }

    /// The scroller's previous-prompt and next-prompt keys move between these lines, so each has
    /// to be the one its own words begin on. A shell command is the person's own words too and is
    /// not a prompt.
    #[test]
    fn each_prompt_is_found_at_the_line_it_begins_on() {
        let _held = crate::theme::exclusive();
        let mut session = Session::new("kernel-enforced");
        session.note("starting up\nand a second line");
        session
            .transcript
            .push(Entry::user("first prompt\nover two lines"));
        session.transcript.push(Entry::assistant(
            "an answer\n\nover three lines",
            Vec::new(),
        ));
        session.transcript.push(Entry::shell("ls"));
        session.transcript.push(Entry::user("second prompt"));

        let Drawn { lines, prompts, .. } = with_prompts(&session, 90, 24);

        let begins: Vec<String> = prompts.iter().map(|&at| text(&lines[at])).collect();
        assert_eq!(begins, ["> first prompt", "> second prompt"]);
    }

    /// The expand key reveals a call and what it returned, so the rows it acts on end where the
    /// quarantined block under the call begins. That block, the plan and the trail hang under the
    /// call in that order and are not part of it.
    #[test]
    fn a_calls_expandable_rows_end_where_what_hangs_under_it_begins() {
        let _held = crate::theme::exclusive();
        let mut session = Session::new("kernel-enforced");
        session.show_trail = true;
        session.transcript.push(Entry::user("run it"));

        let mut call = Entry::tool(Activity::running("Run", "make").done("ok"));
        call.landing = Some(Landing::Context);
        let output: Vec<String> = (0..40).map(|n| format!("output {n}")).collect();
        call.returned = Some(Returned {
            lines: output[..5].to_vec(),
            total: 40,
            from_the_end: false,
            whole: output,
        });
        call.shown = Some(Shown {
            origin: "a reader delegate".to_string(),
            reach: Reach::NotThePlanner,
            label: "untrusted".to_string(),
            preview: vec!["quarantined body".to_string()],
            lines: 1,
        });
        call.todos = vec![Row {
            content: "read the parser".to_string(),
            marker: "✓",
            status: Status::Done,
        }];
        call.trail = vec![TrailLine {
            text: "read allowed".to_string(),
            blocked: false,
        }];
        session.transcript.push(call);

        session.transcript.push(Entry::tool(
            Activity::running("Write", "out.txt")
                .done("wrote 17 lines")
                .with_changes(
                    (0..MAX_DIFF_LINES + 5)
                        .map(|n| Change::Added(format!("change {n}")))
                        .collect(),
                ),
        ));

        let Drawn { lines, calls, .. } = with_prompts(&session, 90, 24);

        let run = position(&lines, "Run(make)");
        let block = position(&lines, QUARANTINE_BAR);
        let plan = position(&lines, "read the parser");
        let trail = position(&lines, "read allowed");
        let write = position(&lines, "Write(out.txt)");
        assert!(
            position(&lines, "… 35 more lines") < block && block < plan && plan < trail,
            "what hangs under the call is not drawn in order: {lines:?}"
        );
        assert_eq!(
            calls,
            [
                (run, block, 1),
                (write, position(&lines, "… 5 more lines") + 1, 2)
            ]
        );
    }

    /// A path or a listing entry's name in the trail is text a checkout chose. The buffer drops a
    /// control character without a trace, so it is drawn as its picture first (LAYER-3).
    #[test]
    fn the_trail_a_person_opens_draws_a_checkouts_names_as_pictures() {
        let drawn = trail_line(&TrailLine {
            text: "slot ref:1 holds x\u{1b}]0;T\u{7}\u{1b}[31mred.txt, unread, at (U,priv)"
                .to_string(),
            blocked: false,
        });
        let text: String = drawn.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            text.contains("holds x\u{241b}]0;T\u{2407}\u{241b}[31mred.txt, unread"),
            "the name is not drawn as pictures: {text:?}"
        );
        assert!(!text.chars().any(char::is_control), "{text:?}");
    }

    /// The rows of a reply as text, each with the ink of its first span after the lead.
    fn reply_rows(text: &str, width: u16, references: bool) -> Vec<(String, Option<Style>)> {
        assistant_lines(text, width, Marks::references(references))
            .iter()
            .map(|line| (self::text(line), line.spans.last().map(|span| span.style)))
            .collect()
    }

    /// VIEW-38: a fenced block is a header where the opening fence was and then its lines as they
    /// were written, in the ink of inline code, and the closing fence is not drawn. Markers inside
    /// it are characters and a tab is laid out as width.
    #[test]
    fn a_fenced_block_is_drawn_under_a_header_with_its_lines_as_written() {
        let _held = crate::theme::exclusive();
        let rows = reply_rows(
            "Run it:\n\n```sh\ncargo test *x* a_b_c **y**\n\techo \"$HOME\" `z`\n\n```\nDone.",
            60,
            true,
        );
        let drawn: Vec<&str> = rows.iter().map(|(text, _)| text.as_str()).collect();
        assert_eq!(
            drawn,
            [
                "\u{23fa} Run it:",
                "  ",
                "  sh",
                "    cargo test *x* a_b_c **y**",
                "        echo \"$HOME\" `z`",
                "    ",
                "  Done."
            ]
        );
        let ink = Style::default().fg(markdown::code());
        assert_eq!(rows[3].1, Some(ink));
        assert_eq!(rows[4].1, Some(ink));
        assert_eq!(rows[2].1, Some(dim()));
    }

    /// VIEW-38: a block with no closing fence, which is what a reply still arriving is, is drawn the
    /// way it will be once the fence is written, so nothing moves when it closes. What follows a
    /// closed block is prose again, and a number in a block is not marked as a reference.
    #[test]
    fn an_unclosed_block_is_drawn_as_it_will_be_closed_and_a_number_in_one_is_not_a_link() {
        let _held = crate::theme::exclusive();
        let open = reply_rows("```\nfixes #12\nmore", 40, true);
        let closed = reply_rows("```\nfixes #12\nmore\n```", 40, true);
        assert_eq!(open, closed);
        let lines = assistant_lines("```\nfixes #12\n```\nsee #12", 40, Marks::references(true));
        assert!(
            !lines[1]
                .spans
                .iter()
                .any(|span| span.style.add_modifier.contains(crate::issue_link::MARK)),
            "a number in a block was marked"
        );
        assert!(
            lines[2]
                .spans
                .iter()
                .any(|span| span.style.add_modifier.contains(crate::issue_link::MARK)),
            "a number in the prose after it was not"
        );
    }

    /// VIEW-38: what a block names is cut to the room there is, and a table, a heading and a fence of
    /// the other character inside a block are lines of it.
    #[test]
    fn what_a_block_holds_is_never_a_table_a_heading_or_a_fence() {
        let _held = crate::theme::exclusive();
        let rows = reply_rows(
            "```a-language-name-that-is-far-too-long\n| a | b |\n|---|---|\n# not a heading\n~~~\n```",
            24,
            false,
        );
        let drawn: Vec<&str> = rows.iter().map(|(text, _)| text.as_str()).collect();
        // Exactly as wide as the room, so the row is one row.
        assert_eq!(drawn[0].chars().count(), 24, "{drawn:?}");
        assert!(drawn[0].ends_with('\u{2026}'), "{drawn:?}");
        assert_eq!(
            &drawn[1..],
            [
                "    | a | b |",
                "    |---|---|",
                "    # not a heading",
                "    ~~~"
            ]
        );
    }

    /// MOUSE-27: the word that copies a block is at the right of its header, one column short of the
    /// edge, and the fence's name takes what is left. At every width the header is no wider than
    /// the room, so it is one row, and the word is on the column the layout recorded. A header with no
    /// room for both draws the name alone, and a block with nothing in it draws no word.
    #[test]
    fn the_word_that_copies_a_block_sits_where_the_layout_says_at_every_width() {
        let _held = crate::theme::exclusive();
        for info in [
            "",
            "sh",
            "a-long-name-for-what-this-is-written-in",
            "日本語日本語日本語",
        ] {
            for width in 1..=44u16 {
                let text = format!("```{info}\nx\n```");
                let (lines, placed) = assistant_laid(&text, width, Marks::references(false), true);
                let header = &lines[0];
                assert!(
                    header.width() <= usize::from(width).max(super::LEAD),
                    "{info:?} at {width}: {header:?}"
                );
                for at in &placed {
                    let row = self::text(&lines[at.line]);
                    let from: String = {
                        let mut seen = 0usize;
                        row.chars()
                            .skip_while(|c| {
                                let before = seen;
                                seen += unicode_width::UnicodeWidthChar::width(*c).unwrap_or(0);
                                before < usize::from(at.column)
                            })
                            .collect()
                    };
                    assert_eq!(from.trim_end(), "copy", "{info:?} at {width}: {row:?}");
                    assert_eq!(
                        header.width(),
                        usize::from(width) - 1,
                        "{info:?} at {width}"
                    );
                }
            }
        }
        assert_eq!(
            assistant_laid("```sh\nx\n```", 6, Marks::references(false), true).1,
            []
        );
        assert_eq!(
            assistant_laid("```sh\nx\n```", 40, Marks::references(false), false).1,
            []
        );
        assert_eq!(
            assistant_laid("```sh\n```", 40, Marks::references(false), true).1,
            []
        );
        assert_eq!(
            assistant_laid("```sh\n\n  \n```", 40, Marks::references(false), true).1,
            []
        );
        let (lines, placed) = assistant_laid(
            "a\n```sh\nx\n```\n```\ny\n```",
            40,
            Marks::references(false),
            true,
        );
        assert_eq!(
            placed
                .iter()
                .map(|at| (at.line, at.number))
                .collect::<Vec<_>>(),
            [(1, 0), (3, 1)]
        );
        assert_eq!(lines.len(), 5);
    }

    /// A line that opens a fenced block is never a row of the table above it. The table's rows are
    /// the lines under its header that read as cells, and a fence written with a pipe in what it
    /// names reads as one, which took the opening line out of the block and left every block after it
    /// drawn as prose with no word, numbered differently from the blocks `/copy block` counts
    /// (VIEW-38, MOUSE-27).
    #[test]
    fn a_fence_under_a_table_still_opens_its_block_and_the_blocks_after_it_are_numbered_alike() {
        let _held = crate::theme::exclusive();
        let reply = "| a | b |\n|---|---|\n| 1 | 2 |\n~~~ x|y\nbody\n~~~\n```sh\necho\n```";
        let (lines, placed) = assistant_laid(reply, 60, Marks::references(false), true);
        let rows: Vec<String> = lines.iter().map(text).collect();
        let header = |name: &str| position(&lines, name);
        assert_eq!(
            placed
                .iter()
                .map(|at| (at.line, at.number))
                .collect::<Vec<_>>(),
            [(header("x|y"), 0), (header("sh"), 1)],
            "{rows:?}"
        );
        assert!(
            rows.iter().any(|row| row == "    echo"),
            "the block after the table was drawn as prose: {rows:?}"
        );
        assert_eq!(
            markdown::copyable_blocks(reply),
            ["body", "echo"],
            "the blocks that have a word are the blocks `/copy block` counts"
        );
        assert_eq!(placed.len(), 2);
    }

    /// The name a fence gives is cut to the room by the columns the screen draws each cluster in,
    /// which is not the sum of its characters' widths: an emoji presentation sequence is two columns
    /// for one column of characters, a halfwidth voiced mark takes a cell of its own, and a combining
    /// mark takes none. Cut by characters, a header came out wider than its room, so it wrapped to a
    /// second row, and where the word that copies the block was to follow it the padding between them
    /// was a subtraction past zero (VIEW-38, MOUSE-27).
    #[test]
    fn a_fence_name_of_wide_clusters_is_cut_to_the_room_at_every_width() {
        let _held = crate::theme::exclusive();
        let names = [
            "a".repeat(100),
            "日本語".repeat(40),
            format!("\u{2764}\u{fe0f}\u{2764}\u{fe0f}{}", "a".repeat(100)),
            "\u{2764}\u{fe0f}".repeat(60),
            "\u{ff76}\u{ff9e}".repeat(60),
            "e\u{301}".repeat(100),
        ];
        for info in &names {
            for width in 1..=110u16 {
                for copyable in [true, false] {
                    let source = format!("```{info}\nx\n```");
                    let (lines, placed) =
                        assistant_laid(&source, width, Marks::references(false), copyable);
                    let drawn = wrap::display_width(&text(&lines[0]));
                    let at = format!("{info:?} at {width}, copyable {copyable}");
                    assert!(
                        drawn <= usize::from(width).max(super::LEAD),
                        "the header is {drawn} columns wide, past its room: {at}"
                    );
                    for block in &placed {
                        assert_eq!(drawn, usize::from(width) - 1, "{at}");
                        let spans = &lines[block.line].spans;
                        let before: String = spans[..spans.len() - 1]
                            .iter()
                            .map(|span| span.content.as_ref())
                            .collect();
                        assert_eq!(
                            wrap::display_width(&before),
                            usize::from(block.column),
                            "the word is not where the layout says: {at}"
                        );
                        assert_eq!(
                            usize::from(block.column) - usize::from(block.padding),
                            wrap::display_width(
                                &spans[..spans.len() - 2]
                                    .iter()
                                    .map(|span| span.content.as_ref())
                                    .collect::<String>()
                            ),
                            "the padding is not the blanks between the name and the word: {at}"
                        );
                    }
                }
            }
        }
    }

    /// The same names through the draw, the plain text an editor is handed and the rows a copy reads,
    /// at the widths where a header once ended the program.
    #[test]
    fn a_reply_with_a_wide_cluster_fence_name_is_drawn_exported_and_read_for_a_copy() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        let _held = crate::theme::exclusive();
        let reply = format!(
            "```\u{2764}\u{fe0f}\u{2764}\u{fe0f}{}\nx\n```\n```{}\ny\n```",
            "a".repeat(100),
            "\u{ff76}\u{ff9e}".repeat(60)
        );
        for width in [40u16, 80, 100] {
            let mut session = Session::new("kernel-enforced");
            session
                .transcript
                .push(Entry::assistant(&reply, Vec::new()));
            session.laid.width = width;
            session.laid.height = 24;
            let mut terminal = Terminal::new(TestBackend::new(width, 24)).expect("terminal");
            terminal
                .draw(|frame| {
                    draw_transcript(frame, frame.area(), &session);
                })
                .expect("draw");
            let exported = crate::render::as_text(&session);
            assert!(
                exported.contains('x') && exported.contains('y'),
                "{exported}"
            );
            let window = Window {
                area: Rect::new(0, 0, width, 24),
                top: 0,
                view: View::of(&session),
            };
            let rows = Rows::new(&session, window).window(0, 23);
            assert_eq!(rows.buffer.area.width, width);
        }
    }

    /// The rows a line is drawn over at `width`, as the widget that draws the transcript draws them,
    /// with the blanks at the end of each as blanks.
    fn drawn_rows(line: &Line<'_>, width: u16) -> Vec<String> {
        let rows = rows_of(line, width);
        let area = Rect::new(0, 0, width, rows);
        let mut buffer = Buffer::empty(area);
        Paragraph::new(line.clone())
            .wrap(Wrap { trim: false })
            .render(area, &mut buffer);
        (0..rows)
            .map(|row| {
                (0..width)
                    .map(|column| buffer[(column, row)].symbol().to_string())
                    .collect()
            })
            .collect()
    }

    /// MOUSE-8: whether a row of a wrapped line was broken at a blank is told by what stands between
    /// the characters either side of the break, however many blanks it was. The wrap draws a run of
    /// blanks on the row it ends where they fit, drops one where they do not, and leaves the rest at the
    /// start of the next row, so a break after two blanks that both fit looks like one inside a word.
    /// Each fixture is drawn first, so the rows it is about are in the test.
    #[test]
    fn a_break_is_at_a_blank_whatever_the_wrap_did_with_the_run_of_them() {
        for (text, rows, breaks) in [
            // Two blanks that both fit on the row, and three, one of which is dropped.
            (
                "aaaaaaaa  bbbbbbbb cccccccc dddddddd",
                vec!["aaaaaaaa  ", "bbbbbbbb  ", "cccccccc  ", "dddddddd  "],
                vec![true, true, true],
            ),
            (
                "aaaaaaaa   bbbbbbbb cccccccc dddddddd",
                vec!["aaaaaaaa  ", "bbbbbbbb  ", "cccccccc  ", "dddddddd  "],
                vec![true, true, true],
            ),
            // One blank dropped and the others left at the start of the next row, which then breaks
            // inside a word and ends at a blank.
            (
                "aaaaaaaaaa    bbbbbbbb cccccccc dddddddd",
                vec!["aaaaaaaaaa", "   bbbbbbb", "b cccccccc", "dddddddd  "],
                vec![true, false, true],
            ),
            // No blank at the first break, which is inside a word.
            (
                "aaaaaaaabbbbbbbb cccccccc dddddddd",
                vec!["aaaaaaaabb", "bbbbbb    ", "cccccccc  ", "dddddddd  "],
                vec![false, true, true],
            ),
            // A no-break space is no place to break, so a break near one is inside a word, and the
            // blank after the word is one.
            (
                "aaaaaaa\u{a0}bbbbbbbb cccccccc",
                vec!["aaaaaaa\u{a0}bb", "bbbbbb    ", "cccccccc  "],
                vec![false, true],
            ),
        ] {
            let line = Line::raw(text);
            assert_eq!(drawn_rows(&line, 10), rows, "{text:?}");
            let all = blank_breaks(&line, 10, rows.len(), rows.len() - 1);
            assert_eq!(all, breaks, "{text:?}");
            // Only as many as are wanted, and the same ones.
            for wanted in 1..rows.len() {
                assert_eq!(
                    blank_breaks(&line, 10, rows.len(), wanted),
                    breaks[..wanted],
                    "{text:?} up to {wanted}"
                );
            }
        }
    }

    /// A line of `words` in order, each as many characters wide as it says and separated by the blanks
    /// that follow it, written as the characters a reply may hold: letters, a character two cells wide,
    /// and a no-break space inside a word.
    fn a_line_of_words(seed: &mut u64) -> String {
        let mut next = |below: u64| {
            *seed ^= *seed << 13;
            *seed ^= *seed >> 7;
            *seed ^= *seed << 17;
            *seed % below
        };
        let mut line = String::new();
        for _ in 0..(2 + next(7)) {
            for at in 0..(1 + next(9)) {
                line.push(match next(12) {
                    0 => '\u{65e5}',
                    1 if at > 0 => '\u{a0}',
                    n => char::from(b'a' + u8::try_from(n).unwrap_or(0)),
                });
            }
            for _ in 0..(1 + next(4)) {
                line.push(' ');
            }
        }
        line.truncate(line.trim_end().len());
        line
    }

    /// What a run of blanks is copied as: one, and none at the end.
    fn with_runs_of_blanks_as_one(text: &str) -> String {
        let mut collapsed = String::new();
        for c in text.chars() {
            if !(c == ' ' && collapsed.ends_with(' ')) {
                collapsed.push(c);
            }
        }
        collapsed.trim_end().to_string()
    }

    /// MOUSE-8, over lines written to wrap in every way: a selection of all the rows a line is drawn
    /// over copies the line, with a run of blanks as one and none lost, glued or added, at every width a
    /// word may or may not fit, whatever the run of blanks at a break, a no-break space inside a word
    /// or a character two cells wide at the edge. Rejects: a bookkeeping that counts one blank dropped
    /// at every break, which glues the words either side of a break after two blanks and adds a blank
    /// where a break fell inside a word.
    #[test]
    fn a_wrapped_line_copies_as_the_line_it_is_whatever_the_blanks_at_its_breaks() {
        let mut seed = 0x2545_f491_4f6c_dd1d_u64;
        let mut broken = 0;
        for _ in 0..3000 {
            let text = a_line_of_words(&mut seed);
            for width in 6..=14u16 {
                let line = Line::raw(text.clone());
                let rows = rows_of(&line, width);
                let area = Rect::new(0, 0, width, rows);
                let mut buffer = Buffer::empty(area);
                Paragraph::new(line.clone())
                    .wrap(Wrap { trim: false })
                    .render(area, &mut buffer);
                let mut continued = Vec::new();
                note_continued(
                    &mut continued,
                    0,
                    (usize::from(rows), &line),
                    0,
                    usize::from(rows),
                    0,
                    width,
                );
                let mut selection =
                    crate::select::Selection::in_pane(0, 0, crate::select::Pane::Transcript, area);
                selection.extend_to(rows - 1, width);
                let copied = crate::select::text_wrapping(&buffer, &selection, &[], &continued);
                broken += usize::from(rows > 1);
                assert_eq!(
                    with_runs_of_blanks_as_one(&copied),
                    with_runs_of_blanks_as_one(&text),
                    "{text:?} at {width}: {:?}",
                    drawn_rows(&line, width)
                );
            }
        }
        assert!(broken > 10_000, "the lines hardly wrapped: {broken}");
    }

    /// The breaks of the rows on the screen are told from the start of a line and not from all of it,
    /// so a line of half a million characters that is drawn every frame costs the rows shown and not
    /// the line. Counted in the graphemes read and not timed: a window of one screen at the top of the
    /// line reads a few screens' worth, and what it tells is what the whole line gives for those rows.
    #[test]
    fn the_breaks_of_the_rows_shown_of_a_very_long_line_are_read_from_its_start() {
        let line = Line::raw("word ".repeat(100_000));
        let (width, height) = (80u16, 24usize);
        let rows = usize::from(rows_of(&line, width));
        assert!(rows > 6000, "the line is not long: {rows} rows");

        let (shown, work) = counted::during(|| {
            let mut shown = Vec::new();
            note_continued(&mut shown, 0, (rows, &line), 0, height, 0, width);
            shown
        });
        assert_eq!(shown.len(), height - 1);
        assert!(
            (1..=height * usize::from(width) * 8).contains(&work.read),
            "{} graphemes were read to tell {height} rows of a line of {rows}",
            work.read
        );

        let whole = blank_breaks(&line, width, rows, rows - 1);
        assert_eq!(
            shown
                .iter()
                .map(|(_, at_blank)| *at_blank)
                .collect::<Vec<_>>(),
            whole[..height - 1]
        );
    }

    /// A finished entry's open tasks are drawn through `entry_extras`, the transcript's own call
    /// site for `todo_lines`, not through `todo_lines` directly. A row the model left pending or
    /// in progress has to be drawn hollow and dim there, not filled, running and bold, or the
    /// wiring at that call site could revert without any test here catching it (#2897).
    #[test]
    fn a_finished_entrys_open_task_is_drawn_left_open_at_its_call_site() {
        let _held = crate::theme::exclusive();
        let mut session = Session::new("kernel-enforced");
        session.transcript.push(Entry::user("run it"));

        let mut call = Entry::tool(Activity::running("Run", "make").done("ok"));
        call.todos = vec![Row {
            content: "still to do".to_string(),
            marker: "■",
            status: Status::Active,
        }];
        session.transcript.push(call);

        let Drawn { lines, .. } = with_prompts(&session, 90, 24);
        let row = &lines[position(&lines, "still to do")];

        assert_eq!(
            row.spans[1].content,
            format!("{LEFT_OPEN_MARKER} "),
            "the row kept the filled mark of work being done: {row:?}"
        );
        assert_eq!(
            row.spans[1].style,
            dim(),
            "the marker kept the running colour: {row:?}"
        );
        assert_eq!(
            row.spans[2].style,
            dim(),
            "the text kept the bold a row being worked on is drawn with: {row:?}"
        );
    }
}
