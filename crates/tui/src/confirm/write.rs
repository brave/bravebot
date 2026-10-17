//! The write prompt: the diff, the standing rows and the keys that answer for a write.

use bravebot_agent::confirm::{Intent, WriteDecision, WriteRequest};
use bravebot_agent::diff::Change;
use bravebot_agent::report::{Reach, Shown};
use bravebot_i18n::t;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::render::{marked_rows, quarantined_rows};
use crate::theme;

use super::chrome::{approving, centred, panel, refusing, stopping};
use super::pinned::{self, Drawn, Question, Seen};
use super::{Answer, Response, Step, ask_scrolled, pressed};

/// Unchanged lines shown either side of a change, for orientation.
const CONTEXT_LINES: usize = 2;

/// The fewest rows a remark is given, however little room there is.
///
/// One line of it still has to be readable, and a line wider than the box is several rows, so a
/// budget that shrank below this would draw a heading and nothing under it.
const MIN_REMARK_ROWS: usize = 6;

/// What the user did with a write question.
///
/// Two answers more than the other prompts give, and only where the scan put a secret to the person:
/// each settles that question for the file the write lands in, one for the session and one past it
/// (CRED-13). Neither approves a later write by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteAnswer {
    Approve,
    /// Write it, and stop asking whether this file may hold a secret for the rest of the session.
    ApproveAlways,
    /// Write it, and record the file so every session in this directory stops asking that too.
    ApproveAndRecord,
    Reject,
    /// Refuse the write and stop the turn that asked for it.
    Interrupt,
}

impl WriteAnswer {
    /// What to tell the waiting turn. Interrupting refuses and keeps nothing, as [`Answer::decision`].
    pub fn decision(self) -> WriteDecision {
        match self {
            WriteAnswer::Approve => WriteDecision::approve(),
            WriteAnswer::ApproveAlways => WriteDecision::approve_always(),
            WriteAnswer::ApproveAndRecord => WriteDecision::approve_and_record(),
            WriteAnswer::Reject | WriteAnswer::Interrupt => WriteDecision::reject(),
        }
    }

    /// Whether the turn that asked stops as well as being refused. As [`Answer::stops_the_turn`].
    pub fn stops_the_turn(self) -> bool {
        matches!(self, WriteAnswer::Interrupt)
    }
}

/// Draw the prompt and wait for an answer.
///
/// Standalone as well as available through [`TerminalConfirmer`], because a turn running on a
/// worker thread cannot hold the terminal: the main thread calls this on its behalf.
pub fn ask<B: Backend>(terminal: &mut Terminal<B>, request: &WriteRequest) -> WriteAnswer {
    ask_scrolled(
        terminal,
        WriteAnswer::Reject,
        |frame, scroll, seen| draw(frame, request, scroll, seen),
        |key, drawn| write_answer_for(key, request, drawn).map(Step::from),
    )
}

/// What a key press did at a write prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WriteResponse {
    Answer(WriteAnswer),
    Scroll(i16),
}

/// Interpret one key press at a write prompt, or `None` for a key that answers nothing.
///
/// Takes the request for the reason [`run_answer_for`] does: `a` is bound only where the driver
/// offered it and `r` only where it said where the answer would be written, so a key the screen
/// does not draw is unbound rather than granting what the screen never offered. Takes the draw
/// because none of `y`, `a` and `r` approves from one that did not show what it decides.
pub(super) fn write_answer_for(
    key: KeyEvent,
    request: &WriteRequest,
    drawn: &Drawn,
) -> Option<WriteResponse> {
    drawn.take(match key.code {
        KeyCode::Char('a' | 'A')
            if request.may_always && !key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            Some(WriteResponse::Answer(WriteAnswer::ApproveAlways))
        }
        KeyCode::Char('r' | 'R')
            if request.record.is_some() && !key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            Some(WriteResponse::Answer(WriteAnswer::ApproveAndRecord))
        }
        _ => pressed(key, drawn.page()).map(|response| match response {
            Response::Answer(Answer::Approve) => WriteResponse::Answer(WriteAnswer::Approve),
            Response::Answer(Answer::Reject) => WriteResponse::Answer(WriteAnswer::Reject),
            Response::Answer(Answer::Interrupt) => WriteResponse::Answer(WriteAnswer::Interrupt),
            Response::Scroll(by) => WriteResponse::Scroll(by),
        }),
    })
}

impl pinned::Approving for WriteResponse {
    fn approves(&self) -> bool {
        match self {
            WriteResponse::Answer(answer) => match answer {
                WriteAnswer::Approve
                | WriteAnswer::ApproveAlways
                | WriteAnswer::ApproveAndRecord => true,
                WriteAnswer::Reject | WriteAnswer::Interrupt => false,
            },
            WriteResponse::Scroll(_) => false,
        }
    }
}

impl From<WriteResponse> for Step<WriteAnswer> {
    fn from(response: WriteResponse) -> Self {
        match response {
            WriteResponse::Answer(answer) => Step::Answer(answer),
            WriteResponse::Scroll(by) => Step::Scroll(by),
        }
    }
}

/// Draw the confirmation over the session, returning what the draw decided for its keys.
///
/// The keys are drawn as a row of their own rather than as the last line of the body. They used
/// to be the last line, kept on screen by capping the diff, and the cap counted lines while the
/// paragraph drew wrapped rows: a diff with long lines pushed the question off the bottom, so the
/// prompt asked nothing and the answer went to a screen that never showed what it was for.
///
/// Everything above the diff decides the question, the findings and what `a` and `r` settle most
/// of all, so no key that approves is taken until all of it and the diff's first row have been
/// drawn.
pub(super) fn draw(
    frame: &mut ratatui::Frame,
    request: &WriteRequest,
    scroll: u16,
    seen: &mut Seen,
) -> Drawn {
    let area = centred(frame.area());
    let inside = panel(frame, area, theme::brand_primary(), t!(write_title));

    let (verb, colour) = match request.intent {
        Intent::Create => (t!(write_create), theme::ok()),
        Intent::Overwrite => (t!(write_overwrite), theme::running()),
        Intent::Edit => (t!(write_edit), theme::brand_primary()),
    };

    let diff = &request.diff;

    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{verb} "),
                Style::default().fg(colour).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                request.path.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "  {}",
                    t!(write_tally, added = diff.added(), removed = diff.removed())
                ),
                Style::default().fg(theme::muted()),
            ),
        ]),
        Line::raw(""),
    ];

    // A diff that could not be computed must say so. Rendering nothing would read as
    // "this write changes nothing", which is the opposite of the truth.
    if !diff.is_exact() {
        lines.push(Line::from(Span::styled(
            format!(
                "  {}",
                t!(
                    write_too_large_to_show,
                    added = diff.added(),
                    removed = diff.removed()
                )
            ),
            Style::default().fg(theme::running()),
        )));
    }

    // The same margin the transcript draws down everything the model was not allowed to read.
    // A body out of a quarantined file is that, and the person about to approve it is the only
    // one who will ever see it: they should be able to tell which kind of review this is.
    let marked = Style::default().fg(theme::running());
    let margin = Span::styled(if request.untrusted { "┃ " } else { "  " }, marked);
    if request.untrusted {
        lines.extend(marked_rows(
            &margin,
            &[Span::styled(t!(write_untrusted), marked)],
            inside.width as usize,
        ));
    }

    // What the processor that produced the body said about it, beside the lines it describes.
    // The remark reached the transcript rounds ago, when the processor returned, so a person
    // was reading this diff with the claim about it some way up the screen. It is a claim and
    // nothing more: no gate reads it, nothing checked it against the bytes below, and it is
    // drawn through the transcript's own block so that it carries the margin it cannot forge
    // and its control characters are replaced.
    if let Some(remark) = &request.remark {
        // The prompt's own margin column, the one the hunks below are drawn against: two of
        // them on one screen is a screen where the column stops meaning anything.
        let bar = Span::styled("┃ ", marked);
        // Bounded in **rows**, here, because only here is the width known. The producer caps
        // the remark in lines, and a line of a remark has no width cap worth the name: four
        // lines of a hundred and sixty characters is a dozen rows in this box, which is the
        // diff below the fold and a reviewer answering with nothing but the claim on screen.
        // That is the defect showing the remark here exists to prevent, and it is the same
        // line-for-row confusion that once pushed the question itself off the bottom.
        //
        // Whole preview lines are dropped rather than trimmed, and the block says how many it
        // is not showing: the transcript above keeps the fuller preview either way.
        // A third of the body, which is the box less the row the keys keep.
        let budget = ((inside.height.saturating_sub(1) as usize) / 3).max(MIN_REMARK_ROWS);
        let mut kept = remark.preview.len();
        let block = loop {
            let block = quarantined_rows(
                &Shown {
                    origin: t!(write_remark).to_string(),
                    reach: Reach::NoModel,
                    label: remark.label.clone(),
                    preview: remark.preview[..kept].to_vec(),
                    lines: remark.lines,
                },
                &bar,
                inside.width as usize,
            );
            if block.len() <= budget || kept <= 1 {
                break block;
            }
            kept -= 1;
        };
        lines.extend(block);
        lines.push(Line::raw(""));
    }

    // What the scan inferred about this body, above the lines it read it from. Drawn plainly
    // rather than behind the quarantine margin: these are the driver's own words about its own
    // findings, not content out of a file, and each is already a kind, a location and a masked
    // preview, so nothing here repeats a character of the value.
    //
    // Unbounded on purpose. A finding is one short line, and a body holding several is one whose
    // every finding the person wants in front of them; the alternative is a prompt that hides the
    // reason it is asking.
    if !request.credentials.is_empty() {
        let warn = Style::default().fg(theme::fail());
        lines.extend(marked_rows(
            &margin,
            &[Span::styled(t!(write_credentials), warn)],
            inside.width as usize,
        ));
        for found in &request.credentials {
            lines.extend(marked_rows(
                &margin,
                &[Span::styled(format!("  {found}"), warn)],
                inside.width as usize,
            ));
        }
        lines.push(Line::raw(""));
    }

    // The driver's record that the working directory's file was written after the checkout this
    // body comes from was made. A record of names, so it says nothing of whether the bytes differ:
    // that is read from the diff below (CHECKOUT-14).
    if request.written_since_checkout {
        lines.extend(marked_rows(
            &margin,
            &[Span::styled(
                t!(write_since_checkout),
                Style::default().fg(theme::fail()),
            )],
            inside.width as usize,
        ));
        lines.push(Line::raw(""));
    }

    // What the write does to the file's line terminators. The diff below compares lines without
    // them, so a write that only swaps `\r\n` for `\n` shows no changed line, and an edit that
    // matched the file's `\r\n` for the model has to say it did.
    if let Some(note) = request.line_endings_note() {
        lines.push(Line::from(Span::styled(
            format!("  {note}"),
            Style::default().fg(theme::muted()),
        )));
        lines.push(Line::raw(""));
    }

    // What `a` and `r` would settle, where they are offered, under the findings they settle and
    // above a diff that may push anything below it out of sight: which file, how long, and that
    // the rest of the write's question is untouched. Where `r` is written down is part of what it
    // grants, so the path is on the screen.
    lines.extend(standing_rows(request));

    // All of it. What does not fit is scrolled to, rather than dropped: the hunks nobody shows
    // you are exactly the ones an approval is supposed to cover.
    //
    // Broken to the width here rather than by the paragraph, so a hunk wider than the box
    // continues on another marked row instead of at column 0 outside the margin.
    let deciding = lines.len();
    let changes = diff.condensed(CONTEXT_LINES);
    for change in changes.iter() {
        let body = match change {
            Change::Added(text) => {
                Span::styled(format!("+{text}"), Style::default().fg(theme::ok()))
            }
            Change::Removed(text) => {
                Span::styled(format!("-{text}"), Style::default().fg(theme::fail()))
            }
            Change::Kept(text) => {
                Span::styled(format!(" {text}"), Style::default().fg(theme::muted()))
            }
            Change::Elided(count) => Span::styled(
                format!(" {}", t!(write_unchanged, count = *count)),
                Style::default().fg(theme::muted()),
            ),
        };
        lines.extend(marked_rows(&margin, &[body], inside.width as usize));
    }

    let keys = write_keys(request, inside.width as usize);

    pinned::draw(
        frame,
        inside,
        Question::scrolled(lines, deciding, 1, &keys),
        scroll,
        seen,
    )
}

/// The rows saying what `a` and `r` settle, where they are offered, with the blank row that
/// follows them.
fn standing_rows(request: &WriteRequest) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if request.may_always {
        let muted = Style::default().fg(theme::muted());
        lines.push(Line::from(Span::styled(
            format!("  {}", t!(write_always_explained)),
            muted,
        )));
        lines.push(Line::from(Span::styled(
            format!("     {}", t!(write_always_this_file)),
            muted,
        )));
        lines.push(Line::from(Span::styled(
            format!("     {}", t!(write_always_only_the_secret)),
            Style::default().fg(theme::running()),
        )));
        if let Some(path) = &request.record {
            lines.push(Line::raw(""));
            lines.push(Line::from(Span::styled(
                format!("  {}", t!(write_remember_explained)),
                muted,
            )));
            lines.push(Line::from(Span::styled(
                format!("     {}", t!(write_remember_every_session)),
                muted,
            )));
            lines.push(Line::from(Span::styled(
                format!("     {}", t!(write_remember_where)),
                muted,
            )));
            lines.push(Line::from(Span::styled(
                format!("       {}", path.display()),
                Style::default().add_modifier(Modifier::BOLD),
            )));
        }
        lines.push(Line::raw(""));
    }
    lines
}

/// The write prompt's key rows as a function of whether a yes is taken yet. A prompt offering both
/// standing answers makes the row wider than the box, so the key that stops the turn gets a row of
/// its own.
fn write_keys(request: &WriteRequest, width: usize) -> impl Fn(bool) -> Vec<Line<'static>> {
    let answers = |answerable: bool, gap: &str| {
        let mut key_spans = vec![
            Span::styled("  y", approving(theme::ok(), answerable)),
            Span::raw(format!(" {}{gap}", t!(write_yes))),
        ];
        if request.may_always {
            key_spans.push(Span::styled("a", approving(theme::running(), answerable)));
            key_spans.push(Span::raw(format!(" {}{gap}", t!(write_always))));
        }
        if request.record.is_some() {
            key_spans.push(Span::styled("r", approving(theme::running(), answerable)));
            key_spans.push(Span::raw(format!(" {}{gap}", t!(write_remember))));
        }
        key_spans.extend([
            Span::styled("n", refusing()),
            Span::raw(format!(" {}{gap}", t!(write_no))),
        ]);
        Line::from(key_spans)
    };
    let stop = Line::from(Vec::from(stopping()));
    let wide = answers(true, "    ").width() + stop.width() > width;
    // Both standing answers make the row wider than the box on a 100-column terminal, so the key
    // that stops the turn gets a row of its own rather than being broken across two. A prompt
    // offering neither keeps its one row, closing its gaps where it is still too wide, and with it
    // every row it had for the body.
    let stop_below = request.may_always && wide;
    let gap = if wide && !stop_below { "  " } else { "    " };
    move |answerable: bool| {
        let answers = answers(answerable, gap);
        let stop = stop.clone();
        if stop_below {
            let mut row = stop;
            row.spans.insert(0, Span::raw("  "));
            vec![answers, row]
        } else {
            let mut row = answers;
            row.spans.extend(stop.spans);
            vec![row]
        }
    }
}
