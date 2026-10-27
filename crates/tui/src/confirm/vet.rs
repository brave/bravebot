//! The prompts that show quarantined bytes behind the margin: output, vetting and vouching.

use bravebot_agent::confirm::{Decision, OutputRequest, PictureShown, VetRequest, VouchRequest};
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
    lines.extend(hidden_rows(request.hidden, inside.width as usize));
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

/// The sentence saying what the tokens in the text below are, where some of it drew as nothing.
///
/// Above the first row of the text, so it is one of the rows a yes waits on, and coloured rather
/// than muted: the text a model reads holds characters the person cannot see, and the tokens are
/// where they are.
fn hidden_rows(hidden: usize, width: usize) -> Vec<Line<'static>> {
    match hidden {
        0 => Vec::new(),
        count => indented(
            t!(hidden_characters, count = count),
            Style::default().fg(theme::running()),
            width,
        ),
    }
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
    let mut drawn = Drawn::default();
    let mut ignored = false;
    loop {
        picture.settle();
        // A terminal that cannot be drawn to cannot show the content, and approving content
        // nobody was shown is the one thing this question cannot mean. The verdict does not
        // rescue it: a word from a model is not a person having read something. Not again for a
        // report that changed nothing (MOUSE-44).
        if !ignored
            && terminal
                .draw(|frame| drawn = draw_vet(frame, request, scroll, picture.thumb(), &mut seen))
                .is_err()
        {
            return VetAnswer::Reject;
        }
        ignored = false;

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
            Next::Ignored => ignored = true,
            Next::Skip => continue,
            Next::Lost => return VetAnswer::Reject,
            Next::Wheel(by) => scroll = drawn.moved(scroll, by),
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

/// Push the rows that stand for a picture on the vetting prompt onto `lines`, returning the line
/// its drawn rows begin at, or `None` where it is not drawn.
///
/// The person is given a copy to open, and where the terminal draws real pictures the picture as
/// well. The path is the driver's own, a random name under a directory only they can read, so it is
/// drawn outside the margin: nothing in it came from the file. The picture is the file, so it is
/// drawn inside the margin, on every row it reaches.
fn picture_rows(
    lines: &mut Vec<Line<'static>>,
    shown: &PictureShown,
    picture: Option<&crate::preview::Thumb>,
    margin: &Span<'static>,
    width: usize,
) -> Option<usize> {
    let mut picture_at = None;
    lines.extend(indented(
        t!(vet_picture_open),
        Style::default().fg(theme::muted()),
        width,
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
            width,
        ));
        lines.push(Line::raw(""));
    }
    // What looking at it is for. A check and a person are fooled by different things, and this is
    // the one a person is fooled by.
    lines.extend(indented(
        t!(vet_picture_words),
        Style::default().fg(theme::running()),
        width,
    ));
    if shown.is_a_pdf() {
        lines.extend(indented(
            t!(vet_pdf_hidden_text),
            Style::default().fg(theme::running()),
            width,
        ));
    }
    picture_at
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
    lines.extend(hidden_rows(request.hidden, inside.width as usize));
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
        Some(shown) => {
            picture_at = picture_rows(&mut lines, shown, picture, &margin, inside.width as usize);
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
                bravebot_agent::confirm::shown_name(&request.path),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]),
        // The rule a yes writes, under the name it was asked about: the name is what the planner
        // wrote, and the rule is what the map will hold, which can be wider than it reads.
        Line::from(Span::styled(
            t!(
                vouch_rule,
                rule = bravebot_agent::confirm::shown_name(&request.rule).as_str()
            ),
            Style::default().fg(theme::muted()),
        )),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::confirm::tests::*;
    use crate::confirm::*;
    use bravebot_core::vetting::Verdict;
    use bravebot_i18n::t;
    use ratatui::backend::TestBackend;

    fn rendered_vet(request: &VetRequest) -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 40)).expect("terminal");
        terminal
            .draw(|frame| {
                draw_vet(frame, request, 0, None, &mut Seen::default());
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

    /// PROMPT-1, CHECK-10: content holding characters that draw as nothing is shown with each one
    /// written out, and the prompt says how many there are above them. Content with none says
    /// nothing about any.
    #[test]
    fn the_vet_prompt_says_how_many_characters_draw_as_nothing_and_shows_each() {
        let text = "plain \u{2039}U+E0041 A\u{203A}\u{2039}U+200B\u{203A} text";
        let mut request = a_vetting(Verdict::Safe, None, text);
        request.hidden = 2;
        let rows = rows_of(100, 40, |frame| {
            draw_vet(frame, &request, 0, None, &mut Seen::default());
        });
        let drawn = box_text(&rows);
        assert!(
            drawn.contains(&squeezed(&t!(hidden_characters, count = 2usize))),
            "the prompt did not say characters drew as nothing: {rows:#?}"
        );
        assert!(
            drawn.replace('┃', "").contains(&squeezed(text)),
            "the characters were not written out in the text: {rows:#?}"
        );
        request.hidden = 0;
        let rows = rows_of(100, 40, |frame| {
            draw_vet(frame, &request, 0, None, &mut Seen::default());
        });
        assert!(
            !box_text(&rows).contains("U+XXXX"),
            "a prompt with nothing hidden in it said something was: {rows:#?}"
        );
    }

    /// VET-4: whatever the terminal draws, the prompt names a copy to open and says what a person
    /// looking at one is most likely to miss. A PDF says the one thing more it can hide.
    #[test]
    fn a_picture_is_put_to_the_person_as_a_copy_to_open() {
        let mut request = a_vetting(Verdict::Safe, None, "");
        request.picture = Some(bravebot_agent::confirm::PictureShown {
            path: "/cache/bravebot/vetting/0f.png".into(),
            media: "image/png".into(),
            bytes: 2048,
        });
        let drawn = rendered_vet(&request);
        assert!(drawn.contains("let the model see this?"), "{drawn}");
        assert!(drawn.contains("Show image/png, 2048 bytes"), "{drawn}");
        assert!(drawn.contains("/cache/bravebot/vetting/0f.png"), "{drawn}");
        assert!(
            drawn.contains("a model reads words in a picture"),
            "{drawn}"
        );
        assert!(drawn.contains("let it see this"), "{drawn}");
        assert!(
            !drawn.contains("(there is nothing in it)"),
            "a picture was drawn as empty text: {drawn}"
        );
        assert!(!drawn.contains("no page draws"), "{drawn}");

        request.picture = Some(bravebot_agent::confirm::PictureShown {
            path: "/cache/bravebot/vetting/0f.pdf".into(),
            media: bravebot_core::vetting::PDF.into(),
            bytes: 1,
        });
        let drawn = rendered_vet(&request);
        assert!(drawn.contains("application/pdf, 1 byte "), "{drawn}");
        assert!(drawn.contains("no page draws"), "{drawn}");
    }

    fn a_picture_request(media: &str) -> VetRequest {
        let mut request = a_vetting(Verdict::Safe, None, "");
        request.picture = Some(bravebot_agent::confirm::PictureShown {
            path: "/cache/bravebot/vetting/0f.png".into(),
            media: media.into(),
            bytes: 2048,
        });
        request
    }

    fn drawn_cells(
        request: &VetRequest,
        scroll: u16,
        picture: Option<&crate::preview::Thumb>,
        size: (u16, u16),
    ) -> Vec<(u16, u16, String)> {
        let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).expect("terminal");
        terminal
            .draw(|frame| {
                draw_vet(frame, request, scroll, picture, &mut Seen::default());
            })
            .expect("draw");
        let buffer = terminal.backend().buffer().clone();
        let mut cells = Vec::new();
        for y in 0..size.1 {
            for x in 0..size.0 {
                cells.push((x, y, buffer[(x, y)].symbol().to_string()));
            }
        }
        cells
    }

    fn text_of(cells: &[(u16, u16, String)]) -> String {
        cells.iter().map(|(_, _, symbol)| symbol.as_str()).collect()
    }

    /// The Kitty graphics escape a picture is sent in, which is what a terminal that draws real
    /// pictures is given and a block of colour never contains.
    fn has_graphics(cells: &[(u16, u16, String)]) -> bool {
        cells.iter().any(|(_, _, symbol)| symbol.contains("\x1b_G"))
    }

    /// A terminal that draws real pictures is given the picture on the prompt, and the path is
    /// still there: the person can open the copy and zoom in, which the drawing cannot do.
    #[test]
    fn a_picture_is_drawn_on_the_prompt_beside_the_path_to_its_copy() {
        let request = a_picture_request("image/png");
        let thumb = drawn_with(ratatui_image::picker::ProtocolType::Kitty);
        let cells = drawn_cells(&request, 0, Some(&thumb), (100, 50));
        assert!(has_graphics(&cells), "no graphics escape was drawn");
        let drawn = text_of(&cells);
        assert!(drawn.contains("/cache/bravebot/vetting/0f.png"), "{drawn}");
        assert!(
            drawn.contains("the picture as this terminal draws it"),
            "{drawn}"
        );
        assert!(
            drawn.contains("a model reads words in a picture"),
            "{drawn}"
        );
    }

    /// The picture is what a yes here is about, and it is painted whole or not at all, so a box too
    /// short for it takes no yes however far the rest has been scrolled.
    #[test]
    fn a_picture_the_box_cannot_paint_takes_no_yes() {
        let request = a_picture_request("image/png");
        let thumb = drawn_with(ratatui_image::picker::ProtocolType::Kitty);
        for (height, painted) in [(16, false), (50, true)] {
            let mut seen = Seen::default();
            let mut drawn = Drawn::default();
            let mut scroll = 0;
            let mut graphics = false;
            loop {
                let rows = rows_of(100, height, |frame| {
                    drawn = draw_vet(frame, &request, scroll, Some(&thumb), &mut seen)
                });
                graphics |= rows.iter().any(|row| row.contains("\x1b_G"));
                let next = drawn.moved(scroll, 1);
                if next == scroll {
                    break;
                }
                scroll = next;
            }
            assert_eq!(graphics, painted, "at 100x{height}");
            assert_eq!(drawn.rows() >= thumb.height(), painted, "at 100x{height}");
            for key in ['y', 'a'] {
                assert_eq!(
                    approves(vet_answer_for(press(KeyCode::Char(key)), &request, &drawn)),
                    painted.then_some(true),
                    "at 100x{height}, {key}"
                );
            }
        }
    }

    /// The picture is the file's, so it is drawn inside the margin on every row it reaches, and
    /// the path above it is not.
    #[test]
    fn a_drawn_picture_sits_inside_the_margin_bar() {
        let request = a_picture_request("image/png");
        let thumb = drawn_with(ratatui_image::picker::ProtocolType::Kitty);
        let cells = drawn_cells(&request, 0, Some(&thumb), (100, 50));
        let (x, y, _) = cells
            .iter()
            .find(|(_, _, symbol)| symbol.contains("\x1b_G"))
            .expect("a graphics escape");
        let bar = |row: u16, column: u16| {
            cells
                .iter()
                .find(|(cx, cy, _)| *cx == column && *cy == row)
                .map(|(_, _, symbol)| symbol.as_str())
        };
        for row in *y..*y + thumb.height() {
            assert_eq!(
                bar(row, x - 2),
                Some("\u{2503}"),
                "row {row} has no margin bar"
            );
        }
    }

    /// Without a real graphics protocol the prompt is what it was: a path, no drawing, and no
    /// sentence about a drawing that is not there.
    #[test]
    fn a_picture_is_not_drawn_without_a_real_graphics_protocol() {
        let request = a_picture_request("image/png");
        let cells = drawn_cells(&request, 0, None, (100, 50));
        assert!(!has_graphics(&cells));
        let drawn = text_of(&cells);
        assert!(drawn.contains("/cache/bravebot/vetting/0f.png"), "{drawn}");
        assert!(
            !drawn.contains("the picture as this terminal draws it"),
            "{drawn}"
        );
    }

    /// Which prompts are given a picture is decided from the protocol and the media type, and a
    /// blocks-of-colour terminal, no answer and a PDF are all refused.
    #[test]
    fn only_a_real_protocol_and_a_raster_picture_are_given_a_drawing() {
        use ratatui_image::picker::ProtocolType;
        let size = ratatui::layout::Size::new(120, 50);
        let png = a_picture_request("image/png");
        for real in [
            ProtocolType::Kitty,
            ProtocolType::Iterm2,
            ProtocolType::Sixel,
        ] {
            assert!(vetting_source(Some(real), &png, size).is_some(), "{real:?}");
        }
        assert!(vetting_source(Some(ProtocolType::Halfblocks), &png, size).is_none());
        assert!(vetting_source(None, &png, size).is_none());
        let pdf = a_picture_request(bravebot_core::vetting::PDF);
        assert!(vetting_source(Some(ProtocolType::Kitty), &pdf, size).is_none());
        let text = a_vetting(Verdict::Safe, None, "text");
        assert!(vetting_source(Some(ProtocolType::Kitty), &text, size).is_none());
    }

    /// The picture is large where there is room and absent where there is not, so a small terminal
    /// keeps the path, the verdict and the keys and loses only the drawing.
    #[test]
    fn the_drawing_is_sized_to_the_terminal_and_dropped_when_it_would_not_fit() {
        use crate::preview::Fit;
        let Some(Fit::Within(columns, rows)) = vetting_fit(ratatui::layout::Size::new(120, 50))
        else {
            panic!("a roomy terminal gets no picture");
        };
        assert!(columns > crate::preview::COLUMNS && rows > crate::preview::ROWS);
        assert!(columns <= 100 && rows <= 28);
        assert_eq!(vetting_fit(ratatui::layout::Size::new(20, 10)), None);
        assert_eq!(vetting_fit(ratatui::layout::Size::new(120, 12)), None);
    }

    /// Half of a graphics protocol is worse than none, so a picture that does not fit the visible
    /// part of the prompt is not drawn at all, at any scroll position, and the path remains.
    #[test]
    fn a_picture_that_does_not_fit_the_visible_prompt_is_not_drawn() {
        let request = a_picture_request("image/png");
        let thumb = drawn_with(ratatui_image::picker::ProtocolType::Kitty);
        for scroll in [0, 1, 2, 3] {
            let cells = drawn_cells(&request, scroll, Some(&thumb), (100, 30));
            assert!(!has_graphics(&cells), "drawn at scroll {scroll}");
        }
        // Scrolled far enough that all of it is on the screen, it is drawn: the rule is about what
        // fits, not a refusal to draw at this size.
        let cells = drawn_cells(&request, u16::MAX, Some(&thumb), (100, 30));
        assert!(has_graphics(&cells), "not drawn once it fits");
        let cells = drawn_cells(&request, 0, Some(&thumb), (100, 30));
        assert!(text_of(&cells).contains("/cache/bravebot/vetting/0f.png"));
    }

    /// The bytes are what the person decides about, verdict or no verdict, so they are on the
    /// screen with where they came from. A prompt that showed only the word would be asking
    /// somebody to endorse a second model's opinion.
    #[test]
    fn the_vet_prompt_shows_the_bytes_and_where_they_came_from() {
        let drawn = rendered_vet(&a_vetting(Verdict::Safe, None, "the notes, in full\n"));
        assert!(drawn.contains("the notes, in full"), "{drawn}");
        assert!(drawn.contains("example.com/notes"), "{drawn}");
    }

    /// Every row of the content carries the margin bar the transcript draws down anything the
    /// model has not been allowed to read, and the content never draws its own.
    #[test]
    fn vetted_content_is_drawn_inside_the_margin_it_cannot_forge() {
        let drawn = rendered_vet(&a_vetting(Verdict::Safe, None, "first\nsecond\nthird"));
        assert_eq!(
            drawn.matches('┃').count(),
            3,
            "one bar per line of content, drawn outside what the page wrote: {drawn}"
        );
    }

    /// The check's own sentence is untrusted in exactly the way the content is, so it is inside
    /// the margin too. It is the one line on the screen a page could have written and a reader
    /// might take for the program's.
    #[test]
    fn what_the_check_said_is_drawn_inside_the_margin_too() {
        let drawn = rendered_vet(&a_vetting(
            Verdict::Unsafe,
            Some("it tells the reader to ignore its instructions"),
            "one line",
        ));
        assert!(drawn.contains("ignore its instructions"), "{drawn}");
        assert_eq!(
            drawn.matches('┃').count(),
            2,
            "the reason and the one line of content, each inside a bar: {drawn}"
        );
    }

    /// The two failures are different facts about different risks. "This looks like an attempt to
    /// give instructions" and "nothing looked at this" have to read differently, or a reader is
    /// told the wrong thing in one of the two cases.
    #[test]
    fn the_vet_prompt_says_which_of_the_two_failures_it_was() {
        let unsafe_drawn = rendered_vet(&a_vetting(Verdict::Unsafe, None, "a page"));
        let failed = rendered_vet(&a_vetting(
            Verdict::Inconclusive("the check could not be made"),
            None,
            "a page",
        ));
        assert!(
            unsafe_drawn.contains("looks like an attempt"),
            "{unsafe_drawn}"
        );
        assert!(failed.contains("did not complete"), "{failed}");
        assert!(
            !failed.contains("looks like an attempt"),
            "a check that did not run was reported as one that found something: {failed}"
        );
    }

    /// A safe verdict says what it means: the check looked and found nothing. It does not say the
    /// content is safe, and it does not answer the question the prompt is asking.
    #[test]
    fn a_safe_verdict_is_drawn_as_what_the_check_found() {
        let drawn = rendered_vet(&a_vetting(Verdict::Safe, None, "a page"));
        assert!(drawn.contains("found no attempt"), "{drawn}");
        assert!(drawn.contains("let it read this"), "{drawn}");
        assert!(drawn.contains("keep it back"), "{drawn}");
    }

    /// The person has to be told what approving does, since the consequence is not visible in the
    /// bytes, and what it does not do, since nothing else on the screen would say that a yes here
    /// vouches for no path.
    #[test]
    fn the_vet_prompt_says_what_approving_does_and_does_not_do() {
        let drawn = rendered_vet(&a_vetting(Verdict::Safe, None, "a page"));
        assert!(drawn.contains("has not seen this"), "{drawn}");
        assert!(drawn.contains("No path is vouched for"), "{drawn}");
    }

    /// Nothing about the verdict changes which keys answer the question. A safe verdict is advice,
    /// so a prompt that stopped offering the refusal would be collecting a keypress rather than a
    /// decision, and one that stopped offering the approval on a warning would be deciding for the
    /// person. Both are live whatever the check said, and both mean the same thing.
    #[test]
    fn a_safe_verdict_does_not_change_which_keys_the_vet_prompt_offers() {
        for verdict in [
            Verdict::Safe,
            Verdict::Unsafe,
            Verdict::Inconclusive("the check could not be made"),
        ] {
            let request = a_vetting(verdict, None, "a page");
            let drawn = rendered_vet(&request);
            assert!(drawn.contains("let it read this"), "{verdict}: {drawn}");
            assert!(drawn.contains("keep it back"), "{verdict}: {drawn}");
            assert_eq!(
                vet_answer_for(
                    KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
                    &request,
                    &Drawn::taking_yes()
                ),
                Some(Response::Answer(VetAnswer::Approve)),
                "{verdict}"
            );
            assert_eq!(
                vet_answer_for(
                    KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE),
                    &request,
                    &Drawn::taking_yes()
                ),
                Some(Response::Answer(VetAnswer::Reject)),
                "{verdict}"
            );
        }
    }

    /// The fourth key is not an answer to the question: it turns the asking off. So it is offered
    /// only where the check completed and found nothing. The moment a check reported an injection
    /// attempt, or could not be made at all, is the worst moment to grant it.
    #[test]
    fn only_a_safe_verdict_offers_to_stop_asking() {
        let safe = rendered_vet(&a_vetting(Verdict::Safe, None, "a page"));
        assert!(safe.contains("don't ask when safe"), "{safe}");
        assert!(
            safe.contains("in this session and the next"),
            "the key was offered without saying what it turns on: {safe}"
        );
        for verdict in [
            Verdict::Unsafe,
            Verdict::Inconclusive("the check could not be made"),
        ] {
            let drawn = rendered_vet(&a_vetting(verdict, None, "a page"));
            assert!(
                !drawn.contains("don't ask when safe"),
                "{verdict} offered to stop asking: {drawn}"
            );
        }
    }

    /// The key agrees with the drawing. A key that granted a standing thing the same screen does
    /// not offer is worse than an unbound one, and this key's grant outlives the prompt.
    #[test]
    fn pressing_always_at_a_not_safe_vet_prompt_grants_nothing() {
        let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        for verdict in [
            Verdict::Unsafe,
            Verdict::Inconclusive("the check could not be made"),
        ] {
            assert_eq!(
                vet_answer_for(
                    key,
                    &a_vetting(verdict, None, "a page"),
                    &Drawn::taking_yes()
                ),
                None,
                "{verdict} bound the key that turns the asking off"
            );
        }
        assert_eq!(
            vet_answer_for(
                key,
                &a_vetting(Verdict::Safe, None, "a page"),
                &Drawn::taking_yes()
            ),
            Some(Response::Answer(VetAnswer::ApproveAlways)),
            "the key was not bound where the prompt draws it"
        );
    }

    /// Refusing turns nothing on, and neither does the interrupt. Nothing about saying no is a
    /// reason to stop being asked, and a turn being stopped is not consent to anything.
    #[test]
    fn refusing_a_vetted_read_turns_nothing_on() {
        assert!(!VetAnswer::Reject.turns_vetting_on());
        assert!(!VetAnswer::Interrupt.turns_vetting_on());
        assert!(!VetAnswer::Approve.turns_vetting_on());
        assert!(VetAnswer::ApproveAlways.turns_vetting_on());
    }

    /// Enter is the key most likely to be pressed out of habit, and this prompt puts bytes
    /// nobody vouched for into the planner's context. It reaches neither approval.
    #[test]
    fn enter_does_not_approve_a_vetted_read() {
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        for verdict in [
            Verdict::Safe,
            Verdict::Unsafe,
            Verdict::Inconclusive("the check could not be made"),
        ] {
            assert_eq!(
                vet_answer_for(
                    key,
                    &a_vetting(verdict, None, "a page"),
                    &Drawn::taking_yes()
                ),
                None,
                "{verdict}"
            );
        }
    }

    /// Content with nothing in it is a fact worth stating. An empty box reads as a prompt that
    /// failed to render, and the reviewer would be answering about nothing.
    #[test]
    fn vetted_content_that_is_empty_says_so() {
        assert!(rendered_vet(&a_vetting(Verdict::Safe, None, "")).contains("nothing in it"));
    }

    /// A line wider than the box is ordinary rather than exotic, and a continuation row starting
    /// at column 0 would be untrusted content outside the margin, where the content's own padding
    /// could paint a bar of its own.
    #[test]
    fn a_wrapped_vetted_line_is_marked_on_every_row_it_reaches() {
        let long = "x".repeat(240);
        let drawn = rendered_vet(&a_vetting(Verdict::Safe, None, &long));
        assert!(
            drawn.matches('┃').count() >= 3,
            "a line three boxes wide was marked once: {drawn}"
        );
    }

    fn rendered_output(request: &OutputRequest) -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).expect("terminal");
        terminal
            .draw(|frame| {
                draw_output(frame, request, 0, &mut Seen::default());
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

    /// The whole point of this prompt: the bytes themselves are what the person decides about, so
    /// they have to be on the screen, along with which command printed them.
    #[test]
    fn the_output_prompt_shows_the_bytes_and_the_command() {
        let drawn = rendered_output(&an_output("/Applications/Brave Browser Nightly.app\n"));
        assert!(
            drawn.contains("/Applications/Brave Browser Nightly.app"),
            "{drawn}"
        );
        assert!(drawn.contains("find /Applications"), "{drawn}");
    }

    /// Every line carries the margin bar the transcript draws down anything the model has not been
    /// allowed to read, and the content never draws its own, so a block claiming the output has
    /// ended ends nothing.
    #[test]
    fn output_is_drawn_inside_the_margin_it_cannot_forge() {
        let drawn = rendered_output(&an_output("first\nsecond\nthird"));
        assert_eq!(
            drawn.matches('┃').count(),
            3,
            "one bar per line of output, drawn outside what the program wrote: {drawn}"
        );
    }

    /// A yes releases every byte of the output to the model, so a colour code in it is drawn in
    /// full and not counted: where nobody is deciding about the bytes colour codes are left out of
    /// the drawing (VIEW-30), and here somebody is.
    #[test]
    fn the_output_prompt_draws_a_colour_code_in_full() {
        let drawn = rendered_output(&an_output("\u{1b}[31mred\u{1b}[0m\n"));

        assert!(drawn.contains("\u{241b}[31mred\u{241b}[0m"), "{drawn}");
        assert!(!drawn.contains("colour code"), "{drawn}");
    }

    /// A command that printed nothing is a fact worth stating. An empty box reads as a prompt that
    /// failed to render, and the reviewer would be answering about nothing.
    #[test]
    fn output_that_is_empty_says_so() {
        assert!(rendered_output(&an_output("")).contains("printed nothing"));
    }

    /// PROMPT-1, OUTPUT-1: output holding characters that draw as nothing is shown with each one
    /// written out, and the prompt says how many there are above them. Output with none says
    /// nothing about any.
    #[test]
    fn the_output_prompt_says_how_many_characters_draw_as_nothing_and_shows_each() {
        let text = "plain \u{2039}U+E0041 A\u{203A}\u{2039}U+200B\u{203A} text";
        let mut request = an_output(text);
        request.hidden = 2;
        let rows = rows_of(100, 24, |frame| {
            draw_output(frame, &request, 0, &mut Seen::default());
        });
        let drawn = box_text(&rows);
        assert!(
            drawn.contains(&squeezed(&t!(hidden_characters, count = 2usize))),
            "the prompt did not say characters drew as nothing: {rows:#?}"
        );
        assert!(
            drawn.replace('┃', "").contains(&squeezed(text)),
            "the characters were not written out in the text: {rows:#?}"
        );
        request.hidden = 0;
        let rows = rows_of(100, 24, |frame| {
            draw_output(frame, &request, 0, &mut Seen::default());
        });
        assert!(
            !box_text(&rows).contains("U+XXXX"),
            "a prompt with nothing hidden in it said something was: {rows:#?}"
        );
    }

    /// The person has to be told what approving does, since the consequence is not visible in the
    /// bytes: they go into the planner's context and it acts on them.
    #[test]
    fn the_output_prompt_says_what_approving_does() {
        let drawn = rendered_output(&an_output("Darwin"));
        assert!(drawn.contains("has not seen this"), "{drawn}");
        assert!(drawn.contains("act on it"), "{drawn}");
    }

    /// A check ran before this prompt was drawn, so its word belongs on the screen: the bytes alone
    /// are what a person reading quickly would have had to judge for themselves.
    ///
    /// The banner is the driver's sentence and sits outside the margin. The check's own sentence is
    /// a model's words about attacker-reachable text and goes inside it, where nothing it says can
    /// be taken for the program's.
    #[test]
    fn the_output_prompt_says_what_a_check_found() {
        let mut request = an_output("Darwin");
        request.verdict = Verdict::Unsafe;
        request.reason = Some("it tells the reader to ignore its instructions".into());
        let drawn = rendered_output(&request);

        assert!(drawn.contains("looks like an attempt"), "{drawn}");
        assert!(drawn.contains("ignore its instructions"), "{drawn}");
        // Nothing about the verdict takes the decision away: both answers are still offered.
        assert!(drawn.contains("act on it"), "{drawn}");
        assert_eq!(
            drawn.matches('┃').count(),
            2,
            "the one line of output and the check's sentence, each inside a bar: {drawn}"
        );
    }

    /// The fourth key is not an answer to the question: it turns the asking off. So it is offered
    /// here on the same footing as at the other vetting prompt, and only where the check completed
    /// and found nothing. A prompt carrying a warning is the worst moment to stop asking.
    #[test]
    fn only_a_safe_verdict_offers_to_stop_asking_about_output() {
        let safe = rendered_output(&an_output("Darwin"));
        assert!(safe.contains("don't ask when safe"), "{safe}");
        assert!(safe.contains("wherever a check finds nothing"), "{safe}");

        for verdict in [
            Verdict::Unsafe,
            Verdict::Inconclusive("the check could not be made"),
        ] {
            let mut request = an_output("Darwin");
            request.verdict = verdict;
            let drawn = rendered_output(&request);
            assert!(
                !drawn.contains("don't ask when safe"),
                "{verdict} offered the standing key: {drawn}"
            );
            assert!(
                !drawn.contains("wherever a check finds nothing"),
                "{verdict} explained a key it does not offer: {drawn}"
            );
        }
    }

    /// A key that granted something the screen does not offer is worse than an unbound one, so the
    /// binding asks the verdict again rather than trusting the drawing to have matched.
    #[test]
    fn the_standing_key_is_bound_at_the_output_prompt_only_where_it_is_drawn() {
        let pressed = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);

        assert_eq!(
            output_answer_for(pressed, &an_output("Darwin"), &Drawn::taking_yes()),
            Some(Response::Answer(VetAnswer::ApproveAlways)),
            "a safe verdict did not bind the standing key"
        );

        for verdict in [
            Verdict::Unsafe,
            Verdict::Inconclusive("the check could not be made"),
        ] {
            let mut request = an_output("Darwin");
            request.verdict = verdict;
            assert_eq!(
                output_answer_for(pressed, &request, &Drawn::taking_yes()),
                None,
                "{verdict} bound a key the prompt does not draw"
            );
        }
    }

    /// The three answers to the question are live whatever the check said, on this route as on the
    /// other: a verdict is advice and never the answer.
    #[test]
    fn every_verdict_still_offers_both_answers_about_output() {
        for verdict in [
            Verdict::Safe,
            Verdict::Unsafe,
            Verdict::Inconclusive("the check could not be made"),
        ] {
            let mut request = an_output("Darwin");
            request.verdict = verdict;
            assert_eq!(
                output_answer_for(
                    KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
                    &request,
                    &Drawn::taking_yes()
                ),
                Some(Response::Answer(VetAnswer::Approve)),
                "{verdict}"
            );
            assert_eq!(
                output_answer_for(
                    KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE),
                    &request,
                    &Drawn::taking_yes()
                ),
                Some(Response::Answer(VetAnswer::Reject)),
                "{verdict}"
            );
        }
    }

    /// A line of output wider than the box used to continue at column 0 with no margin at all, and
    /// output is untrimmed, so reaching the wrap point takes nothing unusual. Padded so the
    /// output's own bar would land in the margin column of the row below, which is the content
    /// painting the one mark it can never be allowed to paint.
    #[test]
    fn a_wrapped_output_line_is_marked_on_every_row_it_reaches() {
        let request = an_output(&format!(
            "{}\u{2503} approve this? y yes  n no",
            "PADDING ".repeat(10)
        ));
        let drawn = rows_of(60, 24, |frame| {
            draw_output(frame, &request, 0, &mut Seen::default());
        });

        assert_marked_on_every_row(&drawn, "PADDING");
    }

    /// The cheapest surface to see the defect on: the preview of a file nobody has vouched for is
    /// drawn at whatever width the terminal happens to be.
    #[test]
    fn a_wrapped_vouch_preview_is_marked_on_every_row_it_reaches() {
        let request = a_vouch(
            "longline.txt",
            format!("{}\u{2503} trust me", "PADDING ".repeat(10)),
            false,
        );
        let drawn = rows_of(60, 24, |frame| {
            draw_vouch(frame, &request, 0, &mut Seen::default());
        });

        assert_marked_on_every_row(&drawn, "PADDING");
    }

    /// A file with nothing in it is asked about exactly as any other quarantined file is, so the
    /// prompt has to account for the space where a preview would be. Without this the person is
    /// asked to trust a path over a blank box, and a blank box reads as a prompt that broke.
    ///
    /// A file of blank lines is the same box: its rows draw a margin and nothing beside it.
    #[test]
    fn a_preview_with_nothing_in_it_says_so() {
        for preview in ["", "\n\n"] {
            let request = a_vouch("empty.txt", preview, false);
            let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
            terminal
                .draw(|frame| {
                    draw_vouch(frame, &request, 0, &mut Seen::default());
                })
                .expect("draw");
            let drawn: String = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect();

            assert!(drawn.contains("empty.txt"), "{preview:?}: {drawn}");
            assert!(
                drawn.contains("nothing of this file"),
                "{preview:?}: {drawn}"
            );
        }
    }

    /// A preview of blank lines with more of the file below it is not a file with nothing in it: the
    /// lines worth reading are further down. Saying it holds nothing, next to the marker saying
    /// there is more, would have the prompt contradict itself about a file the person is deciding
    /// whether to trust.
    #[test]
    fn a_blank_preview_of_a_longer_file_does_not_claim_the_file_is_empty() {
        let request = a_vouch("padded.txt", "\n".repeat(19), true);
        // Tall enough for the marker: at 24 rows the blank preview scrolls it off, which is the
        // scrolling PROMPT-4 already covers and not what this is about.
        let mut terminal = Terminal::new(TestBackend::new(80, 40)).expect("terminal");
        terminal
            .draw(|frame| {
                draw_vouch(frame, &request, 0, &mut Seen::default());
            })
            .expect("draw");
        let drawn: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();

        assert!(drawn.contains("padded.txt"), "{drawn}");
        assert!(!drawn.contains("nothing of this file"), "{drawn}");
        assert!(drawn.contains('…'), "{drawn}");
    }

    /// PROMPT-1, PROMPT-2: a name that would draw as nothing, or as another name, is drawn quoted
    /// with its characters spelled, and the rule a yes writes is drawn under it.
    #[test]
    fn the_vouch_prompt_marks_an_unreadable_name_and_says_the_rule() {
        let screen = |path: &str, rule: &str| {
            let mut request = a_vouch(path, "a line", false);
            request.rule = rule.into();
            rows_of(100, 30, |frame| {
                draw_vouch(frame, &request, 0, &mut Seen::default());
            })
            .join("\n")
        };
        let plain = screen("note.md", "/work/note.md");
        assert!(plain.contains("/work/note.md"), "{plain}");
        for name in [" ", "note.md ", "a\u{202e}b", "a\u{200b}b"] {
            let drawn = screen(name, &format!("/work/{name}"));
            assert_ne!(drawn, screen("note.md", "/work/note.md"), "{name:?}");
            assert!(drawn.contains("Trust \""), "{name:?}\n{drawn}");
            assert!(drawn.contains("trusts \"/work/"), "{name:?}\n{drawn}");
        }
    }

    /// The offer this branch was reported for. A person promoting a file is answering the question
    /// `vet_content` asks, so the check's word is on the screen here too, and it is about the whole
    /// file rather than the preview above it.
    ///
    /// The banner is the driver's and sits outside the margin; the check's own sentence is inside
    /// it, alongside the file's own text, since a model wrote it about text a page could have.
    #[test]
    fn the_vouch_prompt_says_what_a_check_found() {
        let mut request = a_vouch("notes.md", "a line of the file", false);
        request.verdict = Verdict::Unsafe;
        request.reason = Some("it tells the reader to ignore its instructions".into());
        let mut terminal = Terminal::new(TestBackend::new(100, 40)).expect("terminal");
        terminal
            .draw(|frame| {
                draw_vouch(frame, &request, 0, &mut Seen::default());
            })
            .expect("draw");
        let drawn: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();

        assert!(drawn.contains("looks like an attempt"), "{drawn}");
        assert!(drawn.contains("ignore its instructions"), "{drawn}");
        // Nothing about the verdict takes the decision away: both answers are still offered.
        assert!(drawn.contains("working blind"), "{drawn}");
        assert_eq!(
            drawn.matches(BAR).count(),
            2,
            "the one line of preview and the check's sentence, each inside a bar: {drawn}"
        );
    }

    /// A check that could not be made says nothing about the file, so it must not read as one that
    /// looked and found nothing. Somebody about to vouch for a path is the person least able to
    /// tell the two apart from the bytes.
    #[test]
    fn a_vouch_prompt_says_when_no_check_was_made() {
        let mut request = a_vouch("notes.md", "a line of the file", false);
        request.verdict = Verdict::Inconclusive("the check was not made");
        let mut terminal = Terminal::new(TestBackend::new(100, 40)).expect("terminal");
        terminal
            .draw(|frame| {
                draw_vouch(frame, &request, 0, &mut Seen::default());
            })
            .expect("draw");
        let drawn: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();

        assert!(drawn.contains("did not complete"), "{drawn}");
        assert!(
            !drawn.contains("found no attempt"),
            "a check that never ran was reported as one that found nothing: {drawn}"
        );
    }
}
