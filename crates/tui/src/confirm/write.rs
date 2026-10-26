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
use super::note::{self, Noted};
use super::pinned::{self, Drawn, Question, Seen};
use super::{Answer, Approves, Next, Response, pressed};

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
    /// Open the body in the person's editor, where the request offered it ([WRITE-7]).
    ///
    /// Not an answer to the question: the question is put again for whatever they save. What the
    /// waiting turn is told is a refusal, which is only reached by a caller with no editor to
    /// give them, and the request offers the key to the callers that have one.
    ///
    /// [WRITE-7]: ../../../../docs/specs/tools/write-file.md
    Edit,
}

impl WriteAnswer {
    /// What to tell the waiting turn. Interrupting refuses and keeps nothing, as [`Answer::decision`].
    pub fn decision(self) -> WriteDecision {
        match self {
            WriteAnswer::Approve => WriteDecision::approve(),
            WriteAnswer::ApproveAlways => WriteDecision::approve_always(),
            WriteAnswer::ApproveAndRecord => WriteDecision::approve_and_record(),
            WriteAnswer::Reject | WriteAnswer::Interrupt | WriteAnswer::Edit => {
                WriteDecision::reject()
            }
        }
    }

    /// Whether the turn that asked stops as well as being refused. As [`Answer::stops_the_turn`].
    pub fn stops_the_turn(self) -> bool {
        matches!(self, WriteAnswer::Interrupt)
    }
}

impl Approves for WriteAnswer {
    fn approves(&self) -> bool {
        match self {
            WriteAnswer::Approve | WriteAnswer::ApproveAlways | WriteAnswer::ApproveAndRecord => {
                true
            }
            WriteAnswer::Reject | WriteAnswer::Interrupt | WriteAnswer::Edit => false,
        }
    }
}

impl Noted<WriteAnswer> {
    /// What to tell the waiting turn: the answer's decision, with the words typed beside it.
    pub fn decision(self) -> WriteDecision {
        match &self.note {
            Some(note) => self.answer.decision().with_note(note),
            None => self.answer.decision(),
        }
    }

    /// Whether the turn that asked stops as well as being refused. As [`WriteAnswer::stops_the_turn`].
    pub fn stops_the_turn(&self) -> bool {
        self.answer.stops_the_turn()
    }
}

/// Draw the prompt and wait for an answer, which carries a note where the person typed one.
///
/// Standalone as well as available through [`TerminalConfirmer`], because a turn running on a
/// worker thread cannot hold the terminal: the main thread calls this on its behalf.
///
/// A caller that cannot hand the person an editor uses this one, and the request does not offer
/// one where it is for such a caller ([`ask_editing`] is the other).
pub fn ask<B: Backend>(terminal: &mut Terminal<B>, request: &WriteRequest) -> Noted<WriteAnswer> {
    ask_reading(
        terminal,
        request,
        super::next_press,
        crate::input::the_last_event_arrived_alone,
    )
}

/// [`ask`], taking each event from `next` and asking `alone` straight after each one, so a test can
/// stand in for the terminal.
pub(super) fn ask_reading<B: Backend>(
    terminal: &mut Terminal<B>,
    request: &WriteRequest,
    mut next: impl FnMut() -> Next,
    mut alone: impl FnMut() -> bool,
) -> Noted<WriteAnswer> {
    ask_saying(
        terminal,
        request,
        &mut next,
        &mut alone,
        None,
        &mut 0,
        &mut Seen::default(),
    )
}

/// What a person who may open their editor at a write prompt did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// An answer to the question, as [`ask`] gives it.
    Answered(Noted<WriteAnswer>),
    /// The body as the person saved it from their editor, which is no answer: the turn puts the
    /// question again for this text ([WRITE-7]).
    ///
    /// [WRITE-7]: ../../../../docs/specs/tools/write-file.md
    Edited(String),
}

impl Asked {
    /// What to tell the waiting turn.
    pub fn decision(self) -> WriteDecision {
        match self {
            Asked::Answered(noted) => noted.decision(),
            Asked::Edited(body) => WriteDecision::edit(body),
        }
    }

    /// Whether the turn that asked stops as well as being refused. An edit stops nothing.
    pub fn stops_the_turn(&self) -> bool {
        match self {
            Asked::Answered(noted) => noted.stops_the_turn(),
            Asked::Edited(_) => false,
        }
    }
}

/// [`ask`] for a caller that can hand the person an editor, which `edit` does: it is given the
/// terminal, the body as it is drawn and the path it is bound for, and returns what they saved or
/// the sentence saying why nothing came back.
///
/// An editor that returns what it was given, or none at all, leaves the question where it was, with
/// the failure said above it where there was one, since a key that does nothing and a key that
/// fails look the same.
pub fn ask_editing<B: Backend>(
    terminal: &mut Terminal<B>,
    request: &WriteRequest,
    edit: impl FnMut(&mut Terminal<B>, &str, &str) -> Result<String, String>,
) -> Asked {
    ask_editing_reading(
        terminal,
        request,
        super::next_press,
        crate::input::the_last_event_arrived_alone,
        edit,
    )
}

/// [`ask_editing`], taking each event from `next` and asking `alone` straight after each one, so a
/// test can stand in for the terminal.
pub(super) fn ask_editing_reading<B: Backend>(
    terminal: &mut Terminal<B>,
    request: &WriteRequest,
    mut next: impl FnMut() -> Next,
    mut alone: impl FnMut() -> bool,
    mut edit: impl FnMut(&mut Terminal<B>, &str, &str) -> Result<String, String>,
) -> Asked {
    let mut said: Option<String> = None;
    // Kept across the editor, so a person who comes back with nothing is where they were in the
    // diff, and a yes that had been taken from a draw is still taken.
    let mut scroll = 0u16;
    let mut seen = Seen::default();
    loop {
        let answer = ask_saying(
            terminal,
            request,
            &mut next,
            &mut alone,
            said.as_deref(),
            &mut scroll,
            &mut seen,
        );
        if answer.answer != WriteAnswer::Edit {
            return Asked::Answered(answer);
        }
        said = match edit(terminal, &request.contents, &request.path) {
            Ok(saved) if changed(&saved, &request.contents) => return Asked::Edited(saved),
            Ok(_) => None,
            Err(failure) => Some(failure),
        };
    }
}

/// Whether `saved` is a different body from `shown`.
///
/// A final newline the editor added to a body that had none is not a difference. Most editors add
/// one on save, so a person who opened the body and changed nothing would otherwise be put the same
/// question again for it.
fn changed(saved: &str, shown: &str) -> bool {
    saved != shown && (shown.ends_with('\n') || saved.strip_suffix('\n') != Some(shown))
}

/// The question, drawn with `said` above the body where the last thing tried failed, until it has
/// an answer.
fn ask_saying<B: Backend>(
    terminal: &mut Terminal<B>,
    request: &WriteRequest,
    next: &mut impl FnMut() -> Next,
    alone: &mut impl FnMut() -> bool,
    said: Option<&str>,
    scroll: &mut u16,
    seen: &mut Seen,
) -> Noted<WriteAnswer> {
    let mut drawn = Drawn::default();
    let mut ignored = false;
    loop {
        // A terminal that cannot be drawn to cannot carry a question, so refuse rather than
        // proceed unseen. How far the body can scroll, and whether a yes is taken, are only
        // knowable once it has been laid out at the width it will be drawn at, so they come back
        // out of the draw. Not again for a report that changed nothing (MOUSE-44).
        if !ignored
            && terminal
                .draw(|frame| drawn = draw_saying(frame, request, *scroll, seen, said))
                .is_err()
        {
            return Noted::plain(WriteAnswer::Reject);
        }
        ignored = false;

        match next() {
            Next::Ignored => ignored = true,
            Next::Skip => continue,
            Next::Lost => return Noted::plain(WriteAnswer::Reject),
            Next::Wheel(by) => *scroll = drawn.moved(*scroll, by),
            Next::Press(key) if note::opens(key, alone()) => {
                // A yes from the line is a yes only where this draw takes the question's own.
                let may_yes = matches!(
                    write_answer_for(
                        KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
                        request,
                        &drawn
                    ),
                    Some(Response::Answer(WriteAnswer::Approve))
                );
                if let Some(noted) = note::take_reading(terminal, may_yes, &mut *next, &mut *alone)
                    .settle(
                        WriteAnswer::Approve,
                        WriteAnswer::Reject,
                        WriteAnswer::Interrupt,
                    )
                {
                    return noted;
                }
            }
            Next::Press(key) => match write_answer_for(key, request, &drawn) {
                // An editor reads the keys that arrive after the one that started it, so the key
                // that starts one has to have arrived alone, as the one that opens a note does.
                Some(Response::Answer(WriteAnswer::Edit)) if !alone() => continue,
                Some(Response::Answer(answer)) => return Noted::plain(answer),
                Some(Response::Scroll(by)) => *scroll = drawn.moved(*scroll, by),
                None => continue,
            },
        }
    }
}

/// Interpret one key press at a write prompt, or `None` for a key that answers nothing.
///
/// Takes the request for the reason [`run_answer_for`] does: `a` is bound only where the driver
/// offered it, `r` only where it said where the answer would be written and `e` only where it
/// offered the editor, so a key the screen does not draw is unbound rather than granting what the
/// screen never offered. Takes the draw
/// because none of `y`, `a` and `r` approves from one that did not show what it decides.
pub(super) fn write_answer_for(
    key: KeyEvent,
    request: &WriteRequest,
    drawn: &Drawn,
) -> Option<Response<WriteAnswer>> {
    drawn.take(match key.code {
        KeyCode::Char('a' | 'A')
            if request.may_always && !key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            Some(Response::Answer(WriteAnswer::ApproveAlways))
        }
        KeyCode::Char('r' | 'R')
            if request.record.is_some() && !key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            Some(Response::Answer(WriteAnswer::ApproveAndRecord))
        }
        KeyCode::Char('e' | 'E')
            if request.may_edit && !key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            Some(Response::Answer(WriteAnswer::Edit))
        }
        _ => pressed(key, drawn.page()).map(|response| match response {
            Response::Answer(Answer::Approve) => Response::Answer(WriteAnswer::Approve),
            Response::Answer(Answer::Reject) => Response::Answer(WriteAnswer::Reject),
            Response::Answer(Answer::Interrupt) => Response::Answer(WriteAnswer::Interrupt),
            Response::Scroll(by) => Response::Scroll(by),
        }),
    })
}

/// [`draw_saying`] with nothing to say, which is what the tests of the prompt's own rows draw.
#[cfg(test)]
pub(super) fn draw(
    frame: &mut ratatui::Frame,
    request: &WriteRequest,
    scroll: u16,
    seen: &mut Seen,
) -> Drawn {
    draw_saying(frame, request, scroll, seen, None)
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
///
/// `said` is drawn above the body where the last thing the person tried at this question failed:
/// the editor would not start, or returned nothing.
pub(super) fn draw_saying(
    frame: &mut ratatui::Frame,
    request: &WriteRequest,
    scroll: u16,
    seen: &mut Seen,
    said: Option<&str>,
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
                bravebot_agent::confirm::shown_name(&request.path),
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

    // The driver's own words about what the person just tried, where it did not work. Drawn in the
    // margin column like the other rows the answer waits on, so it is on the screen with them.
    if let Some(said) = said {
        lines.extend(marked_rows(
            &margin,
            &[Span::styled(
                said.to_string(),
                Style::default().fg(theme::fail()),
            )],
            inside.width as usize,
        ));
        lines.push(Line::raw(""));
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
        // Not an approving key, so it is drawn however the yes is: opening the editor writes
        // nothing, and the question comes back for whatever the person saves.
        if request.may_edit {
            key_spans.push(Span::styled(
                "e",
                Style::default().fg(theme::brand_primary()),
            ));
            key_spans.push(Span::raw(format!(" {}{gap}", t!(write_edit_first))));
        }
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
    let mut stop = Line::from(Vec::from(stopping()));
    stop.spans.push(Span::raw("    "));
    stop.spans.extend(note::hint());
    let roomy = answers(true, "    ").width();
    let wide = roomy + stop.width() > width;
    // Both standing answers make the row wider than the box on a 100-column terminal, and so can
    // the editor beside either, so the key that stops the turn gets a row of its own rather than
    // being broken across two. A prompt offering none of them keeps its one row, closing its gaps
    // where it is still too wide, and with it every row it had for the body.
    let stop_below = (request.may_always || request.may_edit) && wide;
    // Closed where the answers alone are wider than the box, which is a prompt offering the
    // editor beside both standing answers: a label broken across two rows reads as two keys.
    let gap = if (wide && !stop_below) || roomy > width {
        "  "
    } else {
        "    "
    };
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
