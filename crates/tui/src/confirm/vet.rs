//! The prompts that show quarantined bytes behind the margin: output, vetting and vouching.

use bravebot_agent::confirm::{Decision, OutputRequest, VetRequest, VouchRequest};
use bravebot_core::vetting::Verdict;
use bravebot_i18n::t;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Rect, Size};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::render::marked_rows;
use crate::{input, theme};

use super::chrome::{
    answer_keys, approving, centred, indented, panel, refusing, stopping, verdict_rows,
};
use super::pinned::{self, Drawn, Question, Seen};
use super::{
    Answer, Approves, Next, Response, Step, answer_for, ask_plain, ask_scrolled, next_press,
};

/// Draw the prompt for reading a command's output and wait for an answer.
///
/// The one prompt whose body is the thing being decided about rather than a description of it. It
/// reuses the write prompt's keys and scrolling, because the answer is the same shape: yes, no, or
/// stop.
pub fn ask_output<B: Backend>(terminal: &mut Terminal<B>, request: &OutputRequest) -> VetAnswer {
    // A terminal that cannot be drawn to cannot show the output, and approving output nobody
    // was shown is the one thing this question cannot mean. The verdict does not rescue it: a
    // word from a model is not a person having read something.
    ask_scrolled(
        terminal,
        VetAnswer::Reject,
        |frame, scroll, seen| draw_output(frame, request, scroll, seen),
        |key, drawn| output_answer_for(key, request, drawn).map(Step::from),
    )
}

/// Draw the output for reading, returning what the draw decided for its keys.
///
/// Every drawn row of the output carries the margin bar the transcript draws down anything the
/// model was not allowed to read, and the content never gets to draw its own. A block claiming
/// "output ends here" ends nothing: the bar is the structure, and it is outside what the program
/// wrote. Rows rather than lines, because a command's output is untrimmed and a line of it wider
/// than the box becomes several rows.
///
/// The banner above them is what a check made of the same bytes. It is advice and never an answer,
/// so the three answers to the question are live whatever the verdict was. The fourth key does not
/// answer the question: it turns off the asking, and it is offered only where the check completed
/// and found nothing, exactly as at the `vet_content` prompt.
///
/// What the check said and what each key does decide the question, and the output's first row is
/// what it is about, so no key that approves is taken until those have been drawn.
pub(super) fn draw_output(
    frame: &mut ratatui::Frame,
    request: &OutputRequest,
    scroll: u16,
    seen: &mut Seen,
) -> Drawn {
    let area = centred(frame.area());
    let inside = panel(frame, area, theme::brand_primary(), t!(output_title));

    let marked = Style::default().fg(theme::running());
    let margin = Span::styled("┃ ", marked);
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{} ", t!(output_verb)),
                Style::default()
                    .fg(theme::brand_primary())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                t!(output_lines, count = request.lines),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}", t!(output_printed_by, command = &request.command)),
                Style::default().fg(theme::muted()),
            ),
        ]),
        Line::raw(""),
    ];
    lines.extend(verdict_rows(
        request.verdict,
        request.reason.as_ref(),
        &margin,
        inside.width as usize,
    ));
    lines.push(Line::raw(""));
    lines.extend(indented(
        t!(output_unseen),
        Style::default().fg(theme::muted()),
        inside.width as usize,
    ));
    // What the standing key turns on, said where it is offered and nowhere else. Coloured rather
    // than muted, because it is the one thing on this screen whose effect outlives the prompt.
    if request.verdict.is_safe() {
        lines.extend(indented(
            t!(vet_always_covers),
            Style::default().fg(theme::running()),
            inside.width as usize,
        ));
    }
    lines.push(Line::raw(""));

    // An empty result is a fact worth stating. Drawing nothing would read as a prompt that failed
    // to render, and the reviewer would be deciding about a blank box.
    let deciding = lines.len();
    if request.output.is_empty() {
        lines.extend(marked_rows(
            &margin,
            &[Span::styled(
                t!(output_empty),
                Style::default().fg(theme::muted()),
            )],
            inside.width as usize,
        ));
    }
    for line in request.output.lines() {
        lines.extend(marked_rows(
            &margin,
            &[Span::raw(line.to_string())],
            inside.width as usize,
        ));
    }

    let keys = |answerable: bool| {
        vetting_keys(
            t!(output_yes),
            t!(output_no),
            request.verdict.is_safe(),
            answerable,
        )
    };

    pinned::draw(
        frame,
        inside,
        Question::scrolled(lines, deciding, 1, &keys),
        scroll,
        seen,
    )
}

/// The key row of a prompt about a slot a check looked at, as a function of whether a yes is taken
/// yet. `yes` and `no` are the prompt's own words for those keys.
///
/// `a` is offered only where the check completed and found nothing. It is not an answer to the
/// question on the screen: it turns off the asking, so the moment the check reported an injection
/// attempt, or could not be made at all, is the worst moment to draw it. [`vetting_answer_for`] asks
/// the same question again rather than being told the answer, because a grant must not rest on a
/// drawing.
fn vetting_keys(yes: &str, no: &str, offers_always: bool, answerable: bool) -> Vec<Line<'static>> {
    let mut key_spans = vec![
        Span::styled("  y", approving(theme::ok(), answerable)),
        Span::raw(format!(" {yes}    ")),
    ];
    if offers_always {
        key_spans.push(Span::styled("a", approving(theme::running(), answerable)));
        key_spans.push(Span::raw(format!(" {}    ", t!(vet_always))));
    }
    key_spans.extend([
        Span::styled("n", refusing()),
        Span::raw(format!(" {no}    ")),
    ]);
    key_spans.extend(stopping());
    vec![Line::from(key_spans)]
}

/// What the user decided about being shown one quarantined slot.
///
/// Four answers rather than three, because "yes" and "yes, and stop asking me about a check that
/// finds nothing" are different things and the second is the one that changes what happens next
/// time. It is not a standing answer about these bytes or about this path: there is no such thing
/// here, since a promotion covers one slot once and writes no rule. What it turns on is
/// auto-vetting, which is the mode [CHECK-11] governs.
///
/// [CHECK-11]: ../../../docs/specs/vetting.md
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VetAnswer {
    Approve,
    /// Read this one, and let a check that finds nothing answer from here on.
    ApproveAlways,
    Reject,
    /// Refuse the read and stop the turn that asked for it.
    Interrupt,
}

impl VetAnswer {
    /// What to tell the waiting turn. The two approvals are the same answer to it: what the second
    /// one also does is configuration the interface holds, and the turn in flight keeps the mode
    /// it began with either way.
    pub fn decision(self) -> Decision {
        match self {
            VetAnswer::Approve | VetAnswer::ApproveAlways => Decision::Approve,
            VetAnswer::Reject | VetAnswer::Interrupt => Decision::Reject,
        }
    }

    /// Whether the person asked to stop being asked about a check that finds nothing.
    ///
    /// Never true of a refusal or of an interrupt: nothing about saying no is a reason to turn a
    /// mode on, and a turn being stopped is not consent to anything it was stopped at.
    pub fn turns_vetting_on(self) -> bool {
        matches!(self, VetAnswer::ApproveAlways)
    }

    /// Whether the turn that asked stops as well as being refused. As [`Answer::stops_the_turn`].
    pub fn stops_the_turn(self) -> bool {
        matches!(self, VetAnswer::Interrupt)
    }
}

impl Approves for VetAnswer {
    fn approves(&self) -> bool {
        match self {
            VetAnswer::Approve | VetAnswer::ApproveAlways => true,
            VetAnswer::Reject | VetAnswer::Interrupt => false,
        }
    }
}

/// Interpret one key press at the `vet_content` prompt, or `None` for a key that answers nothing.
pub(super) fn vet_answer_for(
    key: KeyEvent,
    request: &VetRequest,
    drawn: &Drawn,
) -> Option<Response<VetAnswer>> {
    vetting_answer_for(key, request.verdict, drawn)
}

/// Interpret one key press at the `read_output` prompt, or `None` for a key that answers nothing.
pub(super) fn output_answer_for(
    key: KeyEvent,
    request: &OutputRequest,
    drawn: &Drawn,
) -> Option<Response<VetAnswer>> {
    vetting_answer_for(key, request.verdict, drawn)
}

/// Interpret one key press at either prompt a check runs for and a promotion follows, or `None`
/// for a key that answers nothing.
///
/// Separated from the loop so it can be tested without a terminal.
///
/// Takes the verdict and not only the key, for the reason [`run_answer_for`] takes the request:
/// `a` is bound only where the check completed and found nothing, and the answer has to agree with
/// the drawing. The moment a check reported an injection attempt, or could not be made at all, is
/// the worst moment to turn off the asking, and a key that granted something the same screen does
/// not offer is worse than an unbound one.
///
/// One function for both prompts because they ask the same question of the same person about the
/// same kind of grant, and the standing answer is the same answer. Two copies of this would be two
/// places for the set of bound keys to drift apart.
fn vetting_answer_for(
    key: KeyEvent,
    verdict: Verdict,
    drawn: &Drawn,
) -> Option<Response<VetAnswer>> {
    // The prompt blocks the whole interface, so without this Ctrl-C would do nothing at the one
    // moment a user is most likely to press it.
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('c') => Some(Response::Answer(VetAnswer::Interrupt)),
            _ => None,
        };
    }

    let page = drawn.page();
    drawn.take(match key.code {
        KeyCode::Char('y' | 'Y') => Some(Response::Answer(VetAnswer::Approve)),
        KeyCode::Char('a' | 'A') if verdict.is_safe() => {
            Some(Response::Answer(VetAnswer::ApproveAlways))
        }
        KeyCode::Char('n' | 'N') | KeyCode::Esc => Some(Response::Answer(VetAnswer::Reject)),
        KeyCode::Up | KeyCode::Char('k') => Some(Response::Scroll(-1)),
        KeyCode::Down | KeyCode::Char('j') => Some(Response::Scroll(1)),
        KeyCode::PageUp => Some(Response::Scroll(-page)),
        KeyCode::PageDown => Some(Response::Scroll(page)),
        KeyCode::Home => Some(Response::Scroll(i16::MIN)),
        KeyCode::End => Some(Response::Scroll(i16::MAX)),
        // Enter is deliberately not an approval: it is the key most likely to be pressed out of
        // habit, and this prompt puts bytes nobody vouched for into the planner's context.
        _ => None,
    })
}

/// Draw the prompt for a slot a check has looked at, and wait for an answer.
///
/// The bytes are the body, as they are at the output prompt: the person deciding is the person
/// reading. What is new is the banner above them, which says what a second model made of the same
/// bytes. It is advice and never an answer, so the three answers to the question are live whatever
/// the verdict was. The fourth key does not answer the question: it turns off the asking, and it
/// is offered only where the check completed and found nothing.
pub fn ask_vet<B: Backend>(terminal: &mut Terminal<B>, request: &VetRequest) -> VetAnswer {
    let mut scroll = 0u16;
    let mut seen = Seen::default();
    let mut picture = vetting_preview(terminal, request);
    loop {
        picture.settle();
        let mut drawn = Drawn::default();
        // A terminal that cannot be drawn to cannot show the content, and approving content
        // nobody was shown is the one thing this question cannot mean. The verdict does not
        // rescue it: a word from a model is not a person having read something.
        if terminal
            .draw(|frame| drawn = draw_vet(frame, request, scroll, picture.thumb(), &mut seen))
            .is_err()
        {
            return VetAnswer::Reject;
        }

        // While the picture is still being made, wait a moment rather than for a key, so it is
        // drawn when it arrives and not at the next press.
        if picture.is_pending() {
            match input::poll(std::time::Duration::from_millis(50)) {
                Ok(false) => continue,
                Ok(true) => {}
                Err(_) => return VetAnswer::Reject,
            }
        }

        match next_press() {
            Next::Skip => continue,
            Next::Lost => return VetAnswer::Reject,
            Next::Press(key) => match vet_answer_for(key, request, &drawn) {
                Some(Response::Answer(answer)) => return answer,
                Some(Response::Scroll(by)) => scroll = drawn.moved(scroll, by),
                None => continue,
            },
        }
    }
}

/// How large the picture on a vetting prompt is drawn on a terminal of `size`, or `None` where the
/// prompt has no room to draw one that means anything.
///
/// Large, because a person is looking for writing that is small: as much of the width as the prompt
/// has, and as many rows as are left once the rows above it (the verdict, what a yes does, where the
/// picture came from and the path) are on the screen. A terminal without that many still gets a
/// picture of a usable height, which is drawn once it is scrolled into view.
pub(super) fn vetting_fit(size: ratatui::layout::Size) -> Option<crate::preview::Fit> {
    /// The rows a prompt about a picture spends above the picture, and the keys below it.
    const AROUND: u16 = 24;
    const LEAST: u16 = 8;
    const MOST: u16 = 28;
    let inside = centred(Rect::new(0, 0, size.width, size.height));
    // The frame of the prompt, then the margin bar and the space after it.
    let columns = inside.width.saturating_sub(2 + 2 + 2).min(100);
    let rows = inside.height.saturating_sub(AROUND).clamp(LEAST, MOST);
    (columns >= 16 && inside.height >= LEAST + 6)
        .then_some(crate::preview::Fit::Within(columns, rows))
}

/// The picture to draw on a vetting prompt, or none where it stays a path to open.
///
/// Drawn only where the terminal draws real pictures ([`crate::preview::draws_in_a_vetting_prompt`]),
/// from the private copy the prompt names: it is the file the person is told to open, so what they
/// see here is what they would see there.
fn vetting_preview<B: Backend>(
    terminal: &Terminal<B>,
    request: &VetRequest,
) -> crate::preview::Preview {
    match terminal
        .size()
        .ok()
        .and_then(|size| vetting_source(crate::preview::protocol(), request, size))
    {
        Some((source, fit)) => crate::preview::Preview::start(source, fit),
        None => crate::preview::Preview::nothing(),
    }
}

/// What a vetting prompt draws, or `None` where it stays a path to open.
pub(super) fn vetting_source(
    protocol: Option<ratatui_image::picker::ProtocolType>,
    request: &VetRequest,
    size: ratatui::layout::Size,
) -> Option<(crate::preview::Source, crate::preview::Fit)> {
    let picture = request.picture.as_ref()?;
    if !crate::preview::draws_in_a_vetting_prompt(protocol, &picture.media) {
        return None;
    }
    Some((
        crate::preview::Source::File(picture.path.clone()),
        vetting_fit(size)?,
    ))
}

/// Draw the vetted read for review, returning what the draw decided for its keys.
///
/// Two things on this screen came from somewhere nobody vouched for: the content, and the
/// sentence the check wrote about it. Both are drawn inside the margin the transcript draws down
/// anything the model was not allowed to read, on every row they reach. The banner saying which
/// verdict it was is the driver's own words and is outside the margin, which is the distinction
/// the bar exists to make: a reader can tell which line the program wrote and which line came out
/// of the page.
///
/// Everything above the content decides the question, and so does the content's first row, or
/// with a picture everything said about it, so no key that approves is taken until those have
/// been drawn.
pub(super) fn draw_vet(
    frame: &mut ratatui::Frame,
    request: &VetRequest,
    scroll: u16,
    picture: Option<&crate::preview::Thumb>,
    seen: &mut Seen,
) -> Drawn {
    let area = centred(frame.area());
    let title = match &request.picture {
        Some(_) => t!(vet_picture_title),
        None => t!(vet_title),
    };
    let inside = panel(frame, area, theme::brand_primary(), title);
    let (verb, what) = match &request.picture {
        Some(picture) => (
            t!(vet_picture_verb),
            t!(
                vet_picture_file,
                media = &picture.media,
                bytes = picture.bytes
            ),
        ),
        None => (t!(vet_verb), t!(vet_lines, count = request.lines)),
    };

    let marked = Style::default().fg(theme::running());
    let margin = Span::styled("┃ ", marked);
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{verb} "),
                Style::default()
                    .fg(theme::brand_primary())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(what, Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(
                format!("  {}", t!(vet_from, origin = &request.origin)),
                Style::default().fg(theme::muted()),
            ),
        ]),
        Line::raw(""),
    ];

    lines.extend(verdict_rows(
        request.verdict,
        request.reason.as_ref(),
        &margin,
        inside.width as usize,
    ));
    lines.push(Line::raw(""));

    lines.extend(indented(
        t!(vet_unseen),
        Style::default().fg(theme::muted()),
        inside.width as usize,
    ));
    // What a yes does not do, which is the half nothing else on the screen would say: this covers
    // these bytes and writes no rule, so the same file read again asks again.
    lines.extend(indented(
        t!(vet_covers_this_only),
        Style::default().fg(theme::muted()),
        inside.width as usize,
    ));
    // What the standing key turns on, said where it is offered and nowhere else. Coloured rather
    // than muted, because it is the one thing on this screen whose effect outlives the prompt.
    if request.verdict.is_safe() {
        lines.extend(indented(
            t!(vet_always_covers),
            Style::default().fg(theme::running()),
            inside.width as usize,
        ));
    }
    lines.push(Line::raw(""));

    // Why the planner wanted it, in the planner's own words. It is not what the answer binds to:
    // the slot is, and the bytes below are what the reader is agreeing about.
    if !request.expects.is_empty() {
        lines.extend(indented(
            t!(vet_expected, expects = &request.expects),
            Style::default().fg(theme::muted()),
            inside.width as usize,
        ));
        lines.push(Line::raw(""));
    }

    let mut picture_at = None;
    let deciding = match &request.picture {
        Some(_) => usize::MAX,
        None => lines.len(),
    };
    match &request.picture {
        // The person is given a copy to open, and where the terminal draws real pictures the
        // picture as well. The path is the driver's own, a random name under a directory only they
        // can read, so it is drawn outside the margin: nothing in it came from the file. The
        // picture is the file, so it is drawn inside the margin, on every row it reaches.
        Some(shown) => {
            lines.extend(indented(
                t!(vet_picture_open),
                Style::default().fg(theme::muted()),
                inside.width as usize,
            ));
            lines.push(Line::from(Span::styled(
                format!("  {}", shown.path.display()),
                Style::default().add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::raw(""));
            if let Some(thumb) = picture {
                picture_at = Some(lines.len());
                for _ in 0..thumb.height() {
                    lines.push(Line::from(margin.clone()));
                }
                lines.extend(indented(
                    t!(vet_picture_drawn),
                    Style::default().fg(theme::muted()),
                    inside.width as usize,
                ));
                lines.push(Line::raw(""));
            }
            // What looking at it is for. A check and a person are fooled by different things,
            // and this is the one a person is fooled by.
            lines.extend(indented(
                t!(vet_picture_words),
                Style::default().fg(theme::running()),
                inside.width as usize,
            ));
            if shown.is_a_pdf() {
                lines.extend(indented(
                    t!(vet_pdf_hidden_text),
                    Style::default().fg(theme::running()),
                    inside.width as usize,
                ));
            }
        }
        None => {
            // Empty content is a fact worth stating. Drawing nothing would read as a prompt that
            // failed to render, and the reviewer would be deciding about a blank box.
            if request.content.is_empty() {
                lines.extend(marked_rows(
                    &margin,
                    &[Span::styled(
                        t!(vet_empty),
                        Style::default().fg(theme::muted()),
                    )],
                    inside.width as usize,
                ));
            }
            for line in request.content.lines() {
                lines.extend(marked_rows(
                    &margin,
                    &[Span::raw(line.to_string())],
                    inside.width as usize,
                ));
            }
        }
    }

    let keys = |answerable: bool| {
        let yes = match &request.picture {
            Some(_) => t!(vet_picture_yes),
            None => t!(vet_yes),
        };
        vetting_keys(yes, t!(vet_no), request.verdict.is_safe(), answerable)
    };

    // Where the picture's rows begin once the rows above it are wrapped, which is not the line
    // they were pushed at.
    let picture_row = picture_at.map(|at| {
        Paragraph::new(lines[..at].to_vec())
            .wrap(Wrap { trim: false })
            .line_count(inside.width) as u16
    });
    let question = Question {
        picture: picture_row
            .zip(picture)
            .map(|(row, thumb)| (row, Size::new(thumb.width(), thumb.height()))),
        ..Question::scrolled(lines, deciding, 1, &keys)
    };
    let drawn = pinned::draw(frame, inside, question, scroll, seen);
    // Whole or not at all: half of a graphics protocol is worse than none, and the path is above.
    if let (Some(at), Some(thumb)) = (drawn.picture(), picture) {
        thumb.draw(frame, at);
    }

    drawn
}

/// Draw the offer to vouch for a quarantined file, and wait for an answer.
pub fn ask_vouch<B: Backend>(terminal: &mut Terminal<B>, request: &VouchRequest) -> Answer {
    ask_plain(
        terminal,
        |frame, scroll, seen| draw_vouch(frame, request, scroll, seen),
        answer_for,
    )
}

/// Draw the vouch offer, returning what the draw decided for its keys.
///
/// The preview carries the same margin bar as everything else the model has not been allowed to
/// read, because that is exactly what it is until this question is answered.
///
/// The banner above it is what a check made of the file. It is advice and never an answer: a yes
/// writes the trust rule whatever the word was, and a no writes nothing whatever the word was.
///
/// The path, the check and what a yes grants decide the question, and the preview's first row is
/// what it is about, so no yes is taken until those have been drawn.
pub(super) fn draw_vouch(
    frame: &mut ratatui::Frame,
    request: &VouchRequest,
    scroll: u16,
    seen: &mut Seen,
) -> Drawn {
    let area = centred(frame.area());
    let inside = panel(frame, area, theme::ok(), t!(vouch_title));

    let marked = Style::default().fg(theme::running());
    let margin = Span::styled("┃ ", marked);
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{} ", t!(vouch_verb)),
                Style::default()
                    .fg(theme::ok())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                request.path.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::raw(""),
    ];
    // What a check made of the whole file, above the head of it the person can read. The two are
    // about different amounts of the same file on purpose: what a yes here grants is that the file's
    // text may be read, so the check is over all of it, and an attempt to give instructions is least
    // likely to be in the first few lines.
    lines.extend(verdict_rows(
        request.verdict,
        request.reason.as_ref(),
        &margin,
        inside.width as usize,
    ));
    lines.push(Line::raw(""));
    lines.extend(indented(
        t!(vouch_explained),
        Style::default().fg(theme::muted()),
        inside.width as usize,
    ));
    lines.push(Line::raw(""));

    // A file with nothing to show is asked about like any other, so the prompt has to say that is
    // what it is. Drawing nothing would read as a prompt that failed to render, and the person
    // would be answering about a blank box. Blank lines are nothing to show too: they draw rows of
    // bare margin, which is the same blank box with more of it.
    //
    // An empty file and one whose text will not decode arrive here identically, so what is said has
    // to be true of both: that nothing of the file can be shown, not that there is nothing in it.
    // A person told a file was empty would be answering a different question about it.
    //
    // Only where that is the whole file, though. A preview that is blank because the lines with
    // something on them are further down is a file with plenty to show, and saying it holds nothing
    // while the marker below says there is more would be the prompt contradicting itself over a
    // file the person is about to trust. So the blank rows are drawn, and the marker speaks for
    // them. Either the message or the rows, never both: two of them is the blank box again.
    let deciding = lines.len();
    if request.preview.trim().is_empty() && !request.truncated {
        lines.extend(marked_rows(
            &margin,
            &[Span::styled(
                t!(vouch_nothing),
                Style::default().fg(theme::muted()),
            )],
            inside.width as usize,
        ));
    } else {
        for line in request.preview.lines() {
            lines.extend(marked_rows(
                &margin,
                &[Span::raw(line.to_string())],
                inside.width as usize,
            ));
        }
    }
    if request.truncated {
        lines.extend(marked_rows(
            &margin,
            &[Span::styled("…", Style::default().fg(theme::muted()))],
            inside.width as usize,
        ));
    }

    let keys = |answerable| vec![answer_keys(t!(vouch_yes), t!(vouch_no), answerable)];
    pinned::draw(
        frame,
        inside,
        Question::scrolled(lines, deciding, 1, &keys),
        scroll,
        seen,
    )
}
