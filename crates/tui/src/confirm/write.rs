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

    let diff = &request.diff;

    let mut lines = heading_rows(request);

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

    // The sentence saying what the tokens in the rows below are, where the lines this changes hold
    // characters that draw as nothing. A row the answer waits on, in the colour the same sentence
    // has over a command's output: the file is written with them, and the tokens are where they
    // are.
    if diff.hidden() > 0 {
        lines.extend(marked_rows(
            &margin,
            &[Span::styled(
                t!(write_hidden_characters, count = diff.hidden()),
                marked,
            )],
            inside.width as usize,
        ));
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

/// The rows that open the prompt: what the write does to which file with how many lines added
/// and removed, a blank row, and where the diff could not be computed the sentence saying so.
fn heading_rows(request: &WriteRequest) -> Vec<Line<'static>> {
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
    lines
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::confirm::tests::*;
    use crate::confirm::*;
    use bravebot_agent::confirm::Intent;
    use bravebot_agent::confirm::Remark;
    use bravebot_agent::diff::Diff;
    use ratatui::backend::TestBackend;

    fn rendered(request: &WriteRequest) -> String {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        terminal
            .draw(|frame| {
                draw(frame, request, 0, &mut Seen::default());
            })
            .expect("draw");
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    /// PROMPT-13, WRITE-4: where the lines a write changes hold characters that draw as nothing, the
    /// prompt says how many above the rows, and a write with none draws no such sentence.
    #[test]
    fn the_write_prompt_says_how_many_characters_draw_as_nothing_above_the_rows() {
        // The first sentence, which fits on one row, so no border column sits inside it.
        let said = t!(write_hidden_characters, count = 2usize).to_string();
        let said = said.split('.').next().unwrap().to_string();
        let squeezed = |text: &str| text.split_whitespace().collect::<Vec<_>>().join("");
        let mut request = request("run(\"ok\") \u{200b}\u{e0069}\n", None);
        request.diff = Diff::compute("", "run(\"ok\") \u{200b}\u{e0069}\n").spelled();
        let drawn = squeezed(&fully_rendered(&request));
        assert!(drawn.contains(&squeezed(&said)), "{drawn}");
        assert!(drawn.contains("\u{2039}U+200B\u{203a}"), "{drawn}");
        let plain = squeezed(&fully_rendered(&super::tests::request("plain\n", None)));
        assert!(!plain.contains(&squeezed(&said)), "{plain}");
    }

    /// The turn is told the same thing by both approvals. What the second one also does is
    /// configuration the interface holds, and a turn that saw a different answer would be a second
    /// place the mode was decided.
    #[test]
    fn the_standing_answer_tells_the_turn_what_a_plain_yes_tells_it() {
        assert_eq!(VetAnswer::ApproveAlways.decision(), Decision::Approve);
        assert_eq!(VetAnswer::Approve.decision(), Decision::Approve);
        assert_eq!(VetAnswer::Reject.decision(), Decision::Reject);
        assert_eq!(VetAnswer::Interrupt.decision(), Decision::Reject);
    }

    /// The same drawing on a terminal tall enough to hold the whole body.
    fn fully_rendered(request: &WriteRequest) -> String {
        let mut terminal = Terminal::new(TestBackend::new(160, 48)).expect("terminal");
        terminal
            .draw(|frame| {
                draw(frame, request, 0, &mut Seen::default());
            })
            .expect("draw");
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    /// CRED-13: `a` and `r` are bound only where the driver offered them, so a key the screen
    /// does not draw grants nothing. Holding Ctrl reaches neither, as at the run prompt.
    #[test]
    fn the_write_keys_bind_only_the_standing_answers_the_prompt_offers() {
        let press = |code, modifiers, request: &WriteRequest| {
            write_answer_for(
                KeyEvent::new(code, modifiers),
                request,
                &Drawn::taking_yes(),
            )
        };
        for (request, always, remember) in [
            (request("new\n", None), None, None),
            (
                a_credential_write(true, false),
                Some(Response::Answer(WriteAnswer::ApproveAlways)),
                None,
            ),
            (
                a_credential_write(true, true),
                Some(Response::Answer(WriteAnswer::ApproveAlways)),
                Some(Response::Answer(WriteAnswer::ApproveAndRecord)),
            ),
        ] {
            assert_eq!(
                press(KeyCode::Char('a'), KeyModifiers::NONE, &request),
                always,
                "{request:?}"
            );
            assert_eq!(
                press(KeyCode::Char('r'), KeyModifiers::NONE, &request),
                remember,
                "{request:?}"
            );
            for key in ['a', 'r'] {
                assert_eq!(
                    press(KeyCode::Char(key), KeyModifiers::CONTROL, &request),
                    None,
                    "ctrl-{key} answered: {request:?}"
                );
            }
            assert_eq!(
                press(KeyCode::Char('y'), KeyModifiers::NONE, &request),
                Some(Response::Answer(WriteAnswer::Approve)),
            );
        }
    }

    /// CRED-13: each key tells the turn the one lifetime it was drawn with, and refusing keeps
    /// nothing.
    #[test]
    fn the_write_keys_separate_this_session_from_every_session() {
        let always = WriteAnswer::ApproveAlways.decision();
        assert!(always.approved() && always.remember && !always.record);
        let recorded = WriteAnswer::ApproveAndRecord.decision();
        assert!(recorded.approved() && recorded.record && !recorded.remember);
        for answer in [
            WriteAnswer::Approve,
            WriteAnswer::Reject,
            WriteAnswer::Interrupt,
        ] {
            let decision = answer.decision();
            assert!(
                !decision.remember && !decision.record,
                "{answer:?} kept an answer"
            );
        }
    }

    /// CRED-13: the prompt draws each standing answer it offers with what it settles, and none it
    /// does not. `r` names the record it would be written to, since deleting the line there is
    /// the way back.
    #[test]
    fn a_write_prompt_draws_the_standing_answers_it_offers_and_no_other() {
        let plain = fully_rendered(&request("new\n", None));
        assert!(!plain.contains("always this session"), "{plain}");
        assert!(!plain.contains("remember it"), "{plain}");
        assert!(!plain.contains("may hold a secret"), "{plain}");

        let always = fully_rendered(&a_credential_write(true, false));
        assert!(always.contains("always this session"), "{always}");
        assert!(
            always.contains("for the rest of this session") && always.contains("this file only"),
            "`a` was offered without saying what it settles: {always}"
        );
        assert!(
            always.contains("it settles the secret only"),
            "`a` was offered without saying the write's own question stands: {always}"
        );
        assert!(!always.contains("remember it"), "{always}");
        assert!(!always.contains(".bravebot/remembered"), "{always}");

        let remember = fully_rendered(&a_credential_write(true, true));
        assert!(remember.contains("remember it"), "{remember}");
        assert!(
            remember.contains("every session started in this directory"),
            "{remember}"
        );
        assert!(
            remember.contains("/home/someone/.bravebot/remembered/project"),
            "`r` was offered without saying where it is written: {remember}"
        );
    }

    /// Both standing answers on offer make the row of keys wider than the box a 100-column
    /// terminal draws, and every key is still drawn whole, the one that stops the turn included.
    #[test]
    fn a_write_prompt_offering_both_standing_answers_draws_every_key_whole() {
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
        terminal
            .draw(|frame| {
                draw(
                    frame,
                    &a_credential_write(true, true),
                    0,
                    &mut Seen::default(),
                );
            })
            .expect("draw");
        let screen: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        for key in [
            "y write it",
            "a always this session",
            "r remember it",
            "n leave it alone",
            "ctrl-c stop the turn",
        ] {
            assert!(
                screen.contains(key),
                "`{key}` was not drawn whole: {screen}"
            );
        }
    }

    /// A prompt offering neither standing answer keeps its keys on one row in a box too narrow for
    /// their usual gaps, rather than giving a row of the diff to the key that stops the turn.
    #[test]
    fn a_narrow_write_prompt_with_no_standing_answer_keeps_its_keys_on_one_row() {
        let rows = rows_of(60, 20, |frame| {
            draw(
                frame,
                &a_credential_write(false, false),
                0,
                &mut Seen::default(),
            );
        });
        assert!(
            rows.iter().any(|row| row.contains("y write it")
                && row.contains("n leave it alone")
                && row.contains("ctrl-c stop the turn")),
            "the keys were not on one row:\n{}",
            rows.join("\n")
        );
    }

    #[test]
    fn a_new_file_prompt_shows_the_path_and_body() {
        let output = rendered(&request("fn main() {}", None));
        assert!(output.contains("Create"));
        assert!(output.contains("src/main.rs"));
        assert!(output.contains("fn main()"));
        assert!(output.contains("write it"));
    }

    /// Overwriting is the dangerous case, so the prompt must show the lines it discards,
    /// not merely count them.
    #[test]
    fn an_overwrite_prompt_shows_what_it_replaces() {
        let output = rendered(&request("new", Some("a\nb\nc")));
        assert!(output.contains("Overwrite"));
        assert!(output.contains("+1 -3"), "no change counts: {output}");
        for lost in ["-a", "-b", "-c"] {
            assert!(
                output.contains(lost),
                "the discarded line {lost} was not shown: {output}"
            );
        }
        assert!(output.contains("+new"), "the new line was not shown");
    }

    /// A large body must not push the question off screen, and must not be cut short either.
    ///
    /// It used to be capped so the keys would fit, and the cap counted lines while the box drew
    /// wrapped rows, so a diff with long lines pushed the question off anyway: the prompt asked
    /// nothing, and a key pressed at it answered a question that was never on the screen.
    #[test]
    fn a_long_body_keeps_the_question_on_screen_and_offers_the_rest() {
        let body = (0..200)
            .map(|n| format!("line {n} {}", "wrapping words ".repeat(8)))
            .collect::<Vec<_>>()
            .join("\n");
        let output = rendered(&request(&body, None));

        assert!(output.contains("write it"), "the question was pushed off");
        assert!(
            output.contains("more"),
            "the reviewer was not told there is more to read: {output}"
        );
    }

    /// Scrolling reaches what the box could not show, which is the whole point of having it.
    #[test]
    fn the_rest_of_a_long_body_can_be_scrolled_to() {
        let body = (0..200)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let request = request(&body, None);

        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        let mut furthest = 0;
        terminal
            .draw(|frame| furthest = draw(frame, &request, 0, &mut Seen::default()).furthest())
            .expect("draw");
        assert!(furthest > 0, "a 200 line body reported nothing to scroll");

        terminal
            .draw(|frame| {
                draw(frame, &request, furthest, &mut Seen::default());
            })
            .expect("draw");
        let drawn: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();

        assert!(
            drawn.contains("line 199"),
            "the end of the diff could not be reached: {drawn}"
        );
        assert!(drawn.contains("write it"), "the question scrolled away");
    }

    /// CHECKOUT-14. The prompt for a file brought back from a checkout says the session wrote the
    /// path in the working directory since, and a prompt for any other write says nothing of it.
    #[test]
    fn a_write_since_the_checkout_is_said_in_the_question() {
        let drawn = |since: bool| {
            rendered(&WriteRequest {
                written_since_checkout: since,
                path: "out.txt".into(),
                diff: Diff::compute("mine\n", "theirs\n"),
                contents: "theirs\n".into(),
                existing: Some("mine\n".into()),
                intent: Intent::Overwrite,
                untrusted: false,
                remark: None,
                protected: false,
                credentials: Vec::new(),
                may_always: false,
                record: None,
                may_edit: false,
            })
        };
        assert!(
            drawn(true).contains("in the working directory"),
            "the question does not say it: {}",
            drawn(true)
        );
        assert!(!drawn(false).contains("in the working directory"));
    }

    /// The diff compares lines without their terminators, so an edit that keeps a file's CRLF
    /// endings looks the same as one that does not. The prompt says which it is.
    #[test]
    fn an_edit_to_a_crlf_file_says_its_line_endings_are_kept() {
        let drawn = |before: &str, after: &str| {
            rendered(&WriteRequest {
                written_since_checkout: false,
                path: "out.txt".into(),
                diff: Diff::compute(before, after),
                contents: after.into(),
                existing: Some(before.into()),
                intent: Intent::Edit,
                untrusted: false,
                remark: None,
                protected: false,
                credentials: Vec::new(),
                may_always: false,
                record: None,
                may_edit: false,
            })
        };
        assert!(
            drawn("a\r\nb\r\n", "a\r\nc\r\n").contains("line endings: CRLF kept"),
            "the question does not say it: {}",
            drawn("a\r\nb\r\n", "a\r\nc\r\n")
        );
        assert!(!drawn("a\nb\n", "a\nc\n").contains("line endings"));
    }

    /// The reason this exists: a one-line change to a large file must show that one line
    /// rather than a screenful of unchanged text.
    #[test]
    fn a_small_edit_in_a_large_file_shows_only_the_change() {
        let before: String = (0..300).map(|n| format!("line {n}\n")).collect::<String>();
        let after = before.replace("line 150\n", "line 150 changed\n");

        let output = rendered(&WriteRequest {
            written_since_checkout: false,
            path: "src/main.rs".into(),
            diff: Diff::compute(&before, &after),
            contents: after,
            existing: Some(before),
            intent: Intent::Edit,
            untrusted: false,
            remark: None,
            protected: false,
            credentials: Vec::new(),
            may_always: false,
            record: None,
            may_edit: false,
        });

        assert!(output.contains("Edit"));
        assert!(output.contains("+1 -1"), "wrong counts: {output}");
        assert!(
            output.contains("+line 150 changed"),
            "the change was not shown: {output}"
        );
        assert!(
            output.contains("unchanged lines"),
            "the unchanged bulk was not elided: {output}"
        );
        assert!(output.contains("write it"), "the question was pushed off");
    }

    /// A body out of a quarantined file is the one thing on the screen nobody has read. The
    /// person approving it is the only party who ever will, so the prompt says so and marks the
    /// hunks the same way the transcript marks everything else the model was not shown.
    #[test]
    fn an_untrusted_body_is_marked_in_the_prompt() {
        let output = rendered(&WriteRequest {
            written_since_checkout: false,
            path: "game.js".into(),
            contents: "const SPEED = 50;\n".into(),
            existing: Some("const SPEED = 100;\n".into()),
            diff: Diff::compute("const SPEED = 100;\n", "const SPEED = 50;\n"),
            intent: Intent::Overwrite,
            untrusted: true,
            remark: None,
            protected: false,
            credentials: Vec::new(),
            may_always: false,
            record: None,
            may_edit: false,
        });

        assert!(
            output.contains("untrusted"),
            "the reviewer was not told what they are reading: {output}"
        );
        assert!(output.contains("┃"), "the hunks were not marked: {output}");

        // A write of the model's own words is not marked, or the mark would mean nothing.
        let ordinary = rendered(&request("new\n", Some("old\n")));
        assert!(
            !ordinary.contains("┃"),
            "an ordinary write was marked as untrusted: {ordinary}"
        );
    }

    /// A diff too large to compute must not render as an empty change.
    #[test]
    fn an_uncomputable_diff_says_so() {
        let before: String = (0..3000).map(|n| format!("old {n}\n")).collect();
        let after: String = (0..3000).map(|n| format!("new {n}\n")).collect();

        let output = rendered(&WriteRequest {
            written_since_checkout: false,
            path: "src/main.rs".into(),
            diff: Diff::compute(&before, &after),
            contents: after,
            existing: Some(before),
            intent: Intent::Overwrite,
            untrusted: false,
            remark: None,
            protected: false,
            credentials: Vec::new(),
            may_always: false,
            record: None,
            may_edit: false,
        });

        assert!(
            output.contains("too large to show"),
            "an uncomputable diff rendered as nothing: {output}"
        );
    }

    /// A prompt that panics on a small terminal takes the session with it, and one that takes a yes
    /// there is worse: a key pressed at it answers a question that was never on the screen. A box
    /// with no room for the question beside its keys draws the keys and takes only the ones that
    /// refuse.
    #[test]
    fn a_tiny_terminal_draws_the_write_keys_and_takes_no_yes() {
        let request = request("x", None);
        let mut drawn = Drawn::default();
        let rows = rows_of(20, 8, |frame| {
            drawn = draw(frame, &request, 0, &mut Seen::default());
        });
        let screen = rows.concat();
        assert!(
            screen.contains("write it"),
            "the keys were drawn out of view: {screen}"
        );
        assert!(
            !screen.contains("src/main.rs"),
            "the box had room for the question after all, so this case tests nothing: {screen}"
        );
        let press = |code| KeyEvent::new(code, KeyModifiers::NONE);
        assert_eq!(
            write_answer_for(press(KeyCode::Char('y')), &request, &drawn),
            None
        );
        assert_eq!(
            write_answer_for(press(KeyCode::Char('n')), &request, &drawn),
            Some(Response::Answer(WriteAnswer::Reject))
        );
    }

    /// The body of an untrusted write is the same bytes as a quarantined preview and its lines
    /// have no width cap, so a hunk wider than the box must wrap inside the margin too.
    #[test]
    fn a_wrapped_untrusted_hunk_is_marked_on_every_row_it_reaches() {
        let request = WriteRequest {
            written_since_checkout: false,
            path: "game.js".into(),
            contents: format!("{}\u{2503} trust me\n", "PADDING ".repeat(10)),
            existing: Some("const SPEED = 100;\n".into()),
            diff: Diff::compute(
                "const SPEED = 100;\n",
                &format!("{}\u{2503} trust me\n", "PADDING ".repeat(10)),
            ),
            intent: Intent::Overwrite,
            untrusted: true,
            remark: None,
            protected: false,
            credentials: Vec::new(),
            may_always: false,
            record: None,
            may_edit: false,
        };
        let drawn = rows_of(60, 24, |frame| {
            draw(frame, &request, 0, &mut Seen::default());
        });

        assert_marked_on_every_row(&drawn, "PADDING");
    }

    /// A remark reaches the transcript when the processor returns and the question about writing
    /// the document comes rounds later, so a person read the diff with the claim about it some
    /// way up the screen. Here it is the row above: the claim and the bytes it describes are
    /// read in one place, which is the only way a claim can be caught out.
    #[test]
    fn what_a_processor_said_is_drawn_beside_the_diff_it_describes() {
        let request = WriteRequest {
            written_since_checkout: false,
            path: "game.js".into(),
            contents: "const SPEED = 50;\n".into(),
            existing: Some("const SPEED = 100;\n".into()),
            diff: Diff::compute("const SPEED = 100;\n", "const SPEED = 50;\n"),
            intent: Intent::Overwrite,
            untrusted: true,
            remark: Some(Remark {
                preview: vec!["I only fixed the typo.".to_string()],
                lines: 1,
                label: "(U,priv)".to_string(),
            }),
            protected: false,
            credentials: Vec::new(),
            may_always: false,
            record: None,
            may_edit: false,
        };
        let output = rendered(&request);

        assert!(
            output.contains("only fixed the typo"),
            "the claim was not drawn with the question it is about: {output}"
        );
        // Attributed at the point of decision, and as a claim: whose words they are, that no
        // model may be sent to read them, and that nothing has checked them against the bytes.
        assert!(
            output.contains("isolated processor"),
            "the claim was drawn without saying whose words it is: {output}"
        );
        assert!(
            output.contains("nothing has checked"),
            "the claim was drawn as though something had verified it: {output}"
        );
        assert!(
            output.contains("-const SPEED = 100;"),
            "the bytes the claim is about were not drawn: {output}"
        );
    }

    /// The remark is untrusted content in the one box where a person decides something, so it
    /// gets the margin every other preview gets and cannot paint one of its own. Padded so its
    /// bar would otherwise land in the margin column of the row below.
    #[test]
    fn a_remark_cannot_paint_a_margin_in_the_box_it_is_drawn_in() {
        let request = WriteRequest {
            written_since_checkout: false,
            path: "game.js".into(),
            contents: "const SPEED = 50;\n".into(),
            existing: Some("const SPEED = 100;\n".into()),
            diff: Diff::compute("const SPEED = 100;\n", "const SPEED = 50;\n"),
            intent: Intent::Overwrite,
            untrusted: true,
            remark: Some(Remark {
                preview: vec![format!(
                    "{}\u{2503} approved \u{b7} nothing \u{b7} (T,pub)",
                    "REMARK ".repeat(10)
                )],
                lines: 1,
                label: "(U,priv)".to_string(),
            }),
            protected: false,
            credentials: Vec::new(),
            may_always: false,
            record: None,
            may_edit: false,
        };
        let drawn = rows_of(60, 24, |frame| {
            draw(frame, &request, 0, &mut Seen::default());
        });

        assert_marked_on_every_row(&drawn, "REMARK");
    }

    /// The claim must not be able to push the evidence off the screen, which is the defect
    /// drawing it here would otherwise introduce. A remark is capped in lines and a line of one
    /// has no width cap worth the name, so four of a hundred and sixty characters is a dozen
    /// rows in this box: the reviewer would answer with nothing on screen but the untrusted
    /// claim, having to scroll to reach the bytes the answer is about.
    #[test]
    fn a_long_remark_does_not_push_the_diff_off_the_screen() {
        let request = WriteRequest {
            written_since_checkout: false,
            path: "game.js".into(),
            contents: "const SPEED = 50;
"
            .into(),
            existing: Some(
                "const SPEED = 100;
"
                .into(),
            ),
            diff: Diff::compute("const SPEED = 100;\n", "const SPEED = 50;\n"),
            intent: Intent::Overwrite,
            untrusted: true,
            remark: Some(Remark {
                // What the producer's cap allows at its widest: REMARK_LINES lines, each
                // REMARK_WIDTH characters.
                preview: (0..4).map(|_| "claim ".repeat(12)).collect(),
                lines: 4,
                label: "(U,priv)".to_string(),
            }),
            protected: false,
            credentials: Vec::new(),
            may_always: false,
            record: None,
            may_edit: false,
        };

        for (width, height) in [(80, 24), (100, 30)] {
            let drawn = rows_of(width, height, |frame| {
                draw(frame, &request, 0, &mut Seen::default());
            });
            let screen = drawn.join(
                "
",
            );
            assert!(
                drawn.iter().any(|row| row.contains("-const SPEED = 100;")),
                "at {width}x{height} the claim left no room for the bytes it is about:
{screen}"
            );
            // And the claim is still there to be read, rather than dropped to make room.
            assert!(
                drawn.iter().any(|row| row.contains("claim")),
                "at {width}x{height} the claim was not drawn at all:
{screen}"
            );
        }

        // A box this small has no room for the change under the claim, so the yes waits until the
        // change has been scrolled to.
        let yes = press(KeyCode::Char('y'));
        let mut seen = Seen::default();
        let mut drawn = Drawn::default();
        let top = rows_of(60, 20, |frame| drawn = draw(frame, &request, 0, &mut seen));
        assert!(top.iter().any(|row| row.contains("claim")));
        assert!(!top.iter().any(|row| row.contains("-const SPEED = 100;")));
        assert_eq!(write_answer_for(yes, &request, &drawn), None);
        let end = drawn.furthest();
        let bottom = rows_of(60, 20, |frame| {
            drawn = draw(frame, &request, end, &mut seen)
        });
        assert!(bottom.iter().any(|row| row.contains("-const SPEED = 100;")));
        assert_eq!(
            write_answer_for(yes, &request, &drawn),
            Some(Response::Answer(WriteAnswer::Approve))
        );
    }

    /// Neutralised rather than dropped, as everywhere else: a remark that could clear the line
    /// the margin was drawn on would erase the one mark it can never imitate.
    #[test]
    fn a_control_character_in_a_remark_is_replaced() {
        let request = WriteRequest {
            written_since_checkout: false,
            path: "game.js".into(),
            contents: "const SPEED = 50;\n".into(),
            existing: Some("const SPEED = 100;\n".into()),
            diff: Diff::compute("const SPEED = 100;\n", "const SPEED = 50;\n"),
            intent: Intent::Overwrite,
            untrusted: true,
            remark: Some(Remark {
                preview: vec!["before\u{1b}[2Kafter".to_string()],
                lines: 1,
                label: "(U,priv)".to_string(),
            }),
            protected: false,
            credentials: Vec::new(),
            may_always: false,
            record: None,
            may_edit: false,
        };
        let output = rendered(&request);

        assert!(
            !output.contains("\u{1b}[2K"),
            "a remark could clear the line the margin was drawn on: {output}"
        );
        assert!(
            output.contains("before\u{241b}"),
            "the escape in the remark was not neutralised: {output}"
        );
    }

    /// PROMPT-1: the write prompt, for a create and for an overwrite, draws a name that would draw
    /// as nothing quoted with its characters spelled, so a yes to `note.md ` cannot be read as one
    /// to `note.md`.
    #[test]
    fn the_write_prompt_marks_an_unreadable_name() {
        let screen = |path: &str, existing: Option<&str>| {
            let mut asked = request("a line", existing);
            asked.path = path.into();
            rows_of(100, 30, |frame| {
                draw(frame, &asked, 0, &mut Seen::default());
            })
            .join("\n")
        };
        for existing in [None, Some("old")] {
            let plain = screen("note.md", existing);
            assert!(plain.contains("note.md"), "{plain}");
            for (name, spelled) in UNSEEN_NAMES {
                let drawn = screen(name, existing);
                assert_ne!(drawn, plain, "{name:?}");
                assert!(drawn.contains(spelled), "{name:?}\n{drawn}");
            }
        }
    }

    /// WRITE-7: `e` opens the editor only at a prompt that offers it, so a key the screen does not
    /// draw is unbound. A control chord does not count, since Ctrl-E is a line editor's, and the
    /// answer it gives approves nothing: it is a refusal for any caller that cannot act on it.
    #[test]
    fn the_edit_key_is_bound_only_where_the_request_offers_it() {
        let offered = an_editable_write();
        for key in [KeyCode::Char('e'), KeyCode::Char('E')] {
            assert_eq!(
                write_answer_for(press(key), &offered, &Drawn::taking_yes()),
                Some(Response::Answer(WriteAnswer::Edit)),
                "{key:?}"
            );
        }
        let not_offered = request("new line\n", Some("old line\n"));
        assert_eq!(
            write_answer_for(
                press(KeyCode::Char('e')),
                &not_offered,
                &Drawn::taking_yes()
            ),
            None,
            "a key the prompt did not draw opened an editor"
        );
        assert_eq!(
            write_answer_for(
                KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL),
                &offered,
                &Drawn::taking_yes()
            ),
            None
        );

        assert!(!WriteAnswer::Edit.approves());
        assert!(!WriteAnswer::Edit.stops_the_turn());
        assert!(!WriteAnswer::Edit.decision().approved());
        assert_eq!(WriteAnswer::Edit.decision().edited(), None);
    }

    /// WRITE-7: the keys row says `e` where it is bound, and says nothing of it where it is not,
    /// so the screen and the keys agree.
    #[test]
    fn the_key_row_offers_the_editor_only_where_the_request_does() {
        let offered = rendered(&an_editable_write());
        assert!(offered.contains("e edit it first"), "{offered}");
        let not_offered = rendered(&request("new line\n", Some("old line\n")));
        assert!(!not_offered.contains("edit it first"), "{not_offered}");
    }

    /// WRITE-7: a prompt offering the editor beside both standing answers still draws every key's
    /// label on one row at the width a hundred-column terminal gives it, since a label broken
    /// across two rows reads as two keys.
    #[test]
    fn every_key_label_is_drawn_whole_beside_the_editor_and_both_standing_answers() {
        let request = WriteRequest {
            may_edit: true,
            ..a_credential_write(true, true)
        };
        let rows = rows_of(100, 30, |frame| {
            draw(frame, &request, 0, &mut Seen::default());
        });
        let keys = rows
            .iter()
            .find(|row| row.contains("write it"))
            .expect("the key row was drawn");
        for label in [
            "y write it",
            "e edit it first",
            "a always this session",
            "r remember it",
            "n leave it alone",
        ] {
            assert!(
                keys.contains(label),
                "{label:?} was broken across rows: {rows:#?}"
            );
        }
    }

    /// What a person does at a write prompt that can open an editor, with `edit` standing in for
    /// the editor: each call is recorded with the body and the path it was given.
    fn editing_write(
        script: &[(KeyEvent, bool)],
        request: &WriteRequest,
        edits: &mut Vec<(String, String)>,
        mut returns: impl FnMut() -> Result<String, String>,
    ) -> (write::Asked, String) {
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
        let (next, alone) = scripted(script);
        let asked = write::ask_editing_reading(
            &mut terminal,
            request,
            next,
            alone,
            |_terminal, body, path| {
                edits.push((body.to_string(), path.to_string()));
                returns()
            },
        );
        let buffer = terminal.backend().buffer().clone();
        let screen = (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        (asked, screen)
    }

    /// WRITE-7: what the person saved comes back as their answer and approves nothing, and the
    /// editor is given the body as it was drawn and the path it is for.
    #[test]
    fn text_saved_from_the_editor_is_returned_and_approves_nothing() {
        let mut edits = Vec::new();
        let (asked, _) = editing_write(
            &[lone(KeyCode::Char('e'))],
            &an_editable_write(),
            &mut edits,
            || Ok("saved line\n".to_string()),
        );
        assert_eq!(asked, write::Asked::Edited("saved line\n".to_string()));
        assert!(!asked.stops_the_turn());
        let decision = asked.decision();
        assert!(!decision.approved(), "saving was taken for a yes");
        assert_eq!(decision.edited(), Some("saved line\n"));
        assert_eq!(
            edits,
            [("new line\n".to_string(), "src/main.rs".to_string())]
        );
    }

    /// WRITE-7: an editor that hands back what it was given changed nothing, so the question is
    /// still there with its own diff and the next key answers it.
    #[test]
    fn an_unchanged_body_leaves_the_question_where_it_was() {
        let mut edits = Vec::new();
        let (asked, _) = editing_write(
            &[lone(KeyCode::Char('e')), lone(KeyCode::Char('n'))],
            &an_editable_write(),
            &mut edits,
            || Ok("new line\n".to_string()),
        );
        assert_eq!(
            asked,
            write::Asked::Answered(Noted::plain(WriteAnswer::Reject))
        );
        assert_eq!(edits.len(), 1);
    }

    /// WRITE-7: an editor that would not start is said above the question, which stays open, since
    /// a key that fails and a key that does nothing look the same.
    #[test]
    fn an_editor_that_fails_is_said_above_the_question() {
        let mut edits = Vec::new();
        let (asked, screen) = editing_write(
            &[lone(KeyCode::Char('e')), lone(KeyCode::Char('n'))],
            &an_editable_write(),
            &mut edits,
            || Err("no editor found: set $VISUAL or $EDITOR to the one you want".to_string()),
        );
        assert_eq!(
            asked,
            write::Asked::Answered(Noted::plain(WriteAnswer::Reject))
        );
        assert!(screen.contains("no editor found"), "{screen}");
        assert!(
            screen.contains("approve this write?"),
            "the question was not drawn again: {screen}"
        );
    }

    /// WRITE-7: a final newline the editor adds on save is not a change, since most editors add one
    /// and the person changed nothing, while any other difference is.
    #[test]
    fn a_final_newline_the_editor_added_is_not_an_edit() {
        let bare = WriteRequest {
            may_edit: true,
            ..request("new line", Some("old line"))
        };
        let mut edits = Vec::new();
        let (asked, _) = editing_write(
            &[lone(KeyCode::Char('e')), lone(KeyCode::Char('n'))],
            &bare,
            &mut edits,
            || Ok("new line\n".to_string()),
        );
        assert_eq!(
            asked,
            write::Asked::Answered(Noted::plain(WriteAnswer::Reject)),
            "a newline the editor added was taken for an edit"
        );

        for (shown, saved) in [
            ("new line", "newer line"),
            ("new line", "new line\n\n"),
            ("new line\n", "new line\n\n"),
            ("new line\n", "new line"),
        ] {
            let request = WriteRequest {
                may_edit: true,
                ..request(shown, Some("old line"))
            };
            let mut edits = Vec::new();
            let (asked, _) =
                editing_write(&[lone(KeyCode::Char('e'))], &request, &mut edits, || {
                    Ok(saved.to_string())
                });
            assert_eq!(
                asked,
                write::Asked::Edited(saved.to_string()),
                "{shown:?} -> {saved:?}"
            );
        }
    }

    /// WRITE-7: an `e` that arrived with other keys opens no editor. An editor reads whatever
    /// arrives after the key that started it, so text another program writes into the terminal
    /// could otherwise drive it, as it could drive a note field.
    #[test]
    fn an_e_that_arrived_with_other_keys_opens_no_editor() {
        let mut edits = Vec::new();
        let (asked, _) = editing_write(
            &[crowded(KeyCode::Char('e')), lone(KeyCode::Char('n'))],
            &an_editable_write(),
            &mut edits,
            || Ok("changed\n".to_string()),
        );
        assert_eq!(
            asked,
            write::Asked::Answered(Noted::plain(WriteAnswer::Reject))
        );
        assert!(edits.is_empty(), "an editor was opened by a crowded key");
    }

    /// WRITE-7: coming back from an editor that returned nothing leaves the person where they were
    /// in the diff, since the question they are looking at has not changed.
    #[test]
    fn the_place_in_a_long_diff_survives_an_editor_that_returns_nothing() {
        let long: String = (1..=80).map(|n| format!("line {n:02}\n")).collect();
        let request = WriteRequest {
            may_edit: true,
            ..request(&long, None)
        };
        let mut edits = Vec::new();
        let (_, screen) = editing_write(
            &[
                lone(KeyCode::PageDown),
                lone(KeyCode::Char('e')),
                lone(KeyCode::Char('n')),
            ],
            &request,
            &mut edits,
            || Ok(long.clone()),
        );
        assert_eq!(edits.len(), 1);
        assert!(
            !screen.contains("+line 01"),
            "the question came back at the top of the diff: {screen}"
        );
    }

    /// WRITE-7: the editor key does not break the key row on a narrow terminal. The stop key and
    /// its note hint stay on one row together, below the answers where they do not fit beside them.
    #[test]
    fn the_stop_key_stays_whole_beside_the_editor_key_on_a_narrow_terminal() {
        let rows = rows_of(80, 30, |frame| {
            draw(frame, &an_editable_write(), 0, &mut Seen::default());
        });
        let answers = rows
            .iter()
            .find(|row| row.contains("y write it"))
            .expect("the answers were drawn");
        assert!(answers.contains("n leave it alone"), "{rows:#?}");
        let stop = rows
            .iter()
            .find(|row| row.contains("ctrl-c"))
            .expect("the stop key was drawn");
        assert!(stop.contains("tab note"), "{rows:#?}");
    }
}
