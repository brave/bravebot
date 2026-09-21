//! Asking the user about a write or a run, in the terminal.
//!
//! Turns run synchronously, so this can draw a prompt and block on a keypress from inside
//! the turn that requested the write. The alternative, collecting writes and asking
//! afterwards, would mean the model continuing on the assumption a write had happened.
//!
//! Nothing is approved by default. An unreadable terminal, an unexpected key, or a lost
//! event all resolve to refusal.

use bravebot_agent::confirm::{
    Confirmer, Decision, FetchRequest, Intent, ManifestRequest, OutputRequest, RunDecision,
    RunRequest, ServerRequest, VetRequest, VouchRequest, WriteRequest,
};
use bravebot_agent::diff::Change;
use bravebot_agent::report::{Reach, Shown};
use bravebot_core::ask::{Answer as UserAnswer, Asking};
use bravebot_core::vetting::Verdict;
use bravebot_i18n::t;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::crossterm::event::{self, Event as TermEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};

use crate::render::{marked_rows, quarantined_rows};
use crate::theme;

/// Unchanged lines shown either side of a change, for orientation.
const CONTEXT_LINES: usize = 2;

/// The fewest rows a remark is given, however little room there is.
///
/// One line of it still has to be readable, and a line wider than the box is several rows, so a
/// budget that shrank below this would draw a heading and nothing under it.
const MIN_REMARK_ROWS: usize = 6;

/// Prompts in the terminal for each write.
pub struct TerminalConfirmer<'t, B: Backend> {
    terminal: &'t mut Terminal<B>,
}

impl<'t, B: Backend> TerminalConfirmer<'t, B> {
    pub fn new(terminal: &'t mut Terminal<B>) -> Self {
        Self { terminal }
    }
}

impl<B: Backend> Confirmer for TerminalConfirmer<'_, B> {
    fn confirm_write(&mut self, request: &WriteRequest) -> Decision {
        ask(self.terminal, request).decision()
    }

    fn confirm_run(&mut self, request: &RunRequest) -> RunDecision {
        ask_run(self.terminal, request).decision()
    }

    fn confirm_read_output(&mut self, request: &OutputRequest) -> Decision {
        ask_output(self.terminal, request).decision()
    }

    fn confirm_vetted_read(&mut self, request: &VetRequest) -> Decision {
        ask_vet(self.terminal, request).decision()
    }

    fn confirm_fetch(&mut self, request: &FetchRequest) -> Decision {
        ask_fetch(self.terminal, request).decision()
    }

    fn confirm_server(&mut self, request: &ServerRequest) -> Decision {
        ask_server(self.terminal, request).decision()
    }

    fn confirm_vouch(&mut self, request: &VouchRequest) -> Decision {
        ask_vouch(self.terminal, request).decision()
    }

    fn confirm_manifest(&mut self, request: &ManifestRequest) -> Decision {
        ask_manifest(self.terminal, request).decision()
    }

    fn ask_user(&mut self, asking: &Asking) -> Vec<UserAnswer> {
        crate::ask::ask(self.terminal, asking)
    }

    /// Never anything. This confirmer runs the turn on the thread that owns the terminal, so while
    /// one is running there is no box to type into and nothing can have arrived. Interjecting
    /// belongs to [`crate::remote_confirm::RemoteConfirmer`], where the turn is off on its own
    /// thread and the interface is still taking keys.
    fn interjection(&mut self) -> Option<String> {
        None
    }
}

/// What the user did with the question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Approve,
    Reject,
    /// Refuse the write and stop the turn that asked for it.
    Interrupt,
}

impl Answer {
    /// What to tell the waiting turn. Interrupting refuses, since a turn being stopped is not
    /// consent to the write it was stopped at.
    pub fn decision(self) -> Decision {
        match self {
            Answer::Approve => Decision::Approve,
            Answer::Reject | Answer::Interrupt => Decision::Reject,
        }
    }

    /// Whether the turn that asked stops as well as being refused.
    ///
    /// The one place this is decided, so that the difference between saying no and interrupting is
    /// a property of the answer rather than a comparison repeated at every prompt the event loop
    /// waits on. [`Self::decision`] cannot carry it: both answers refuse, and refusing is all the
    /// turn is told.
    pub fn stops_the_turn(self) -> bool {
        match self {
            Answer::Approve | Answer::Reject => false,
            Answer::Interrupt => true,
        }
    }
}

/// Draw the prompt and wait for an answer.
///
/// Standalone as well as available through [`TerminalConfirmer`], because a turn running on a
/// worker thread cannot hold the terminal: the main thread calls this on its behalf.
pub fn ask<B: Backend>(terminal: &mut Terminal<B>, request: &WriteRequest) -> Answer {
    let mut scroll = 0u16;
    loop {
        // A terminal that cannot be drawn to cannot carry a question, so refuse rather
        // than proceed unseen.
        // How far the body can scroll is only knowable once it has been laid out at the width
        // it will be drawn at, so it comes back out of the closure.
        let mut most = 0u16;
        if terminal
            .draw(|frame| most = draw(frame, request, scroll))
            .is_err()
        {
            return Answer::Reject;
        }

        match event::read() {
            // Presses only: asking for disambiguated keys reports releases too, and a release
            // taken for a press approves whatever the press had just approved, twice.
            Ok(TermEvent::Key(key)) if key.kind != event::KeyEventKind::Press => continue,
            Ok(TermEvent::Key(key)) => match answer_for(key) {
                Some(Response::Answer(answer)) => return answer,
                Some(Response::Scroll(by)) => {
                    scroll = scroll.saturating_add_signed(by).min(most);
                }
                None => continue,
            },
            Ok(_) => continue,
            // Losing the event stream must not approve anything.
            Err(_) => return Answer::Reject,
        }
    }
}

/// What a key press did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Response {
    Answer(Answer),
    /// Move the body by this many rows, positive being further down.
    Scroll(i16),
}

/// Interpret one key press, or `None` for a key that answers nothing.
///
/// Separated from the loop so it can be tested without a terminal.
fn answer_for(key: KeyEvent) -> Option<Response> {
    // The prompt blocks the whole interface, so without this Ctrl-C would do nothing at the one
    // moment a user is most likely to press it. It stops the turn as well as refusing, because
    // someone reaching for the interrupt wants the work to stop, not just this write.
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('c') => Some(Response::Answer(Answer::Interrupt)),
            _ => None,
        };
    }

    match key.code {
        KeyCode::Char('y' | 'Y') => Some(Response::Answer(Answer::Approve)),
        KeyCode::Char('n' | 'N') | KeyCode::Esc => Some(Response::Answer(Answer::Reject)),
        // A diff longer than the box is the one most worth reading before answering.
        KeyCode::Up => Some(Response::Scroll(-1)),
        KeyCode::Down => Some(Response::Scroll(1)),
        KeyCode::PageUp => Some(Response::Scroll(-10)),
        KeyCode::PageDown => Some(Response::Scroll(10)),
        KeyCode::Home => Some(Response::Scroll(i16::MIN)),
        KeyCode::End => Some(Response::Scroll(i16::MAX)),
        // Enter is deliberately not an approval: it is the key most likely to
        // be pressed out of habit.
        _ => None,
    }
}

/// Draw the confirmation over the session, returning how far its body can be scrolled.
///
/// The keys are drawn as a row of their own rather than as the last line of the body. They used
/// to be the last line, kept on screen by capping the diff, and the cap counted lines while the
/// paragraph drew wrapped rows: a diff with long lines pushed the question off the bottom, so the
/// prompt asked nothing and the answer went to a screen that never showed what it was for.
fn draw(frame: &mut ratatui::Frame, request: &WriteRequest, scroll: u16) -> u16 {
    let area = centred(frame.area());
    let inside = panel(frame, area, theme::brand_primary(), t!(write_title));

    let (verb, colour) = match request.intent {
        Intent::Create => (t!(write_create), theme::ok()),
        Intent::Overwrite => (t!(write_overwrite), theme::running()),
        Intent::Edit => (t!(write_edit), theme::brand_primary()),
    };

    let diff = request.diff();

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

    // All of it. What does not fit is scrolled to, rather than dropped: the hunks nobody shows
    // you are exactly the ones an approval is supposed to cover.
    //
    // Broken to the width here rather than by the paragraph, so a hunk wider than the box
    // continues on another marked row instead of at column 0 outside the margin.
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

    let keys = Line::from(vec![
        Span::styled(
            "  y",
            Style::default()
                .fg(theme::ok())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {}    ", t!(write_yes))),
        Span::styled(
            "n",
            Style::default()
                .fg(theme::fail())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {}    ", t!(write_no))),
        Span::styled(
            "ctrl-c",
            Style::default()
                .fg(theme::muted())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {}", t!(stop_the_turn)),
            Style::default().fg(theme::muted()),
        ),
    ]);

    // One row for the keys, the rest for the diff. Split before the body is laid out, so the
    // question keeps its row whatever the body turns out to be.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inside);

    let body = Paragraph::new(lines).wrap(Wrap { trim: false });
    // Rows, not lines: the paragraph wraps, and the difference between the two is what used to
    // push the question off the screen.
    let drawn = body.line_count(rows[0].width) as u16;
    let furthest = drawn.saturating_sub(rows[0].height);
    let offset = scroll.min(furthest);
    frame.render_widget(body.scroll((offset, 0)), rows[0]);

    let mut keys = keys;
    if furthest > 0 {
        let below = furthest - offset;
        keys.push_span(Span::styled(
            // Short, because the row is as wide as the box and the keys come first: a hint
            // that gets clipped in half tells the reviewer less than no hint at all.
            scroll_hint(below),
            Style::default().fg(theme::brand_primary()),
        ));
    }
    frame.render_widget(Paragraph::new(keys), rows[1]);

    furthest
}

/// What the user did with a run question.
///
/// Four answers rather than the write prompt's two. "Yes, and stop asking" is a different thing
/// from "yes", and it is the one that changes what happens next time, so it is a key of its own
/// rather than a follow-up question nobody would read. "Yes, and stop asking tomorrow as well" is a
/// different thing again, and it is a third key rather than a wider reading of the second: the two
/// grant different things and last different lengths of time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunAnswer {
    Approve,
    /// Run it, and vouch for its programs for the rest of the session.
    ApproveAlways,
    /// Run it, and record this exact line so every session in this directory runs it unasked.
    ///
    /// A different thing from `ApproveAlways` and never a wider version of it: it decides how long
    /// an answer lasts, where the other decides what a label is. The row says so in as many words,
    /// because one word saying `always` is what could be read as either.
    ApproveAndRecord,
    Reject,
    /// Refuse the run and stop the turn that asked for it.
    Interrupt,
}

impl RunAnswer {
    /// What to tell the waiting turn. Interrupting refuses and vouches for nothing, since a turn
    /// being stopped is not consent to what it was stopped at.
    pub fn decision(self) -> RunDecision {
        match self {
            RunAnswer::Approve => RunDecision::approve(),
            RunAnswer::ApproveAlways => RunDecision::approve_always(),
            RunAnswer::ApproveAndRecord => RunDecision::approve_and_record(),
            RunAnswer::Reject | RunAnswer::Interrupt => RunDecision::reject(),
        }
    }

    /// Whether the turn that asked stops as well as being refused. As [`Answer::stops_the_turn`],
    /// and for the same reason: the run prompt offers more answers, and all but one of them leave
    /// the turn running.
    pub fn stops_the_turn(self) -> bool {
        match self {
            RunAnswer::Approve
            | RunAnswer::ApproveAlways
            | RunAnswer::ApproveAndRecord
            | RunAnswer::Reject => false,
            RunAnswer::Interrupt => true,
        }
    }
}

/// Interpret one key press at a run prompt, or `None` for a key that answers nothing.
///
/// Separated from the loop so it can be tested without a terminal.
///
/// Takes the request and not only the key, because which keys the prompt offers depends on what
/// is being asked. A run an entry could not record is asked about every time whatever is remembered,
/// so the prompt neither draws `a` nor promises anything about it, and the answer has to agree with
/// the drawing: a key that grants a standing permission the same screen says cannot be granted is
/// worse than an unbound one. Unbound is what it becomes, for the reason Enter is: this prompt starts
/// a program, and a key pressed out of habit from the previous prompt must not. Which runs those are
/// is [`RunRequest::can_be_remembered`], asked here and again where the prompt is drawn.
///
/// `r` is the same rule over the wider of the two lifetimes: it is bound only where the request
/// says where the answer would be written, which is the driver having decided the key would stop a
/// later prompt at all.
fn run_answer_for(key: KeyEvent, request: &RunRequest) -> Option<RunResponse> {
    // The prompt blocks the whole interface, so without this Ctrl-C would do nothing at the one
    // moment a user is most likely to press it.
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('c') => Some(RunResponse::Answer(RunAnswer::Interrupt)),
            _ => None,
        };
    }

    match key.code {
        KeyCode::Char('y' | 'Y') => Some(RunResponse::Answer(RunAnswer::Approve)),
        KeyCode::Char('a' | 'A') if request.can_be_remembered() => {
            Some(RunResponse::Answer(RunAnswer::ApproveAlways))
        }
        // Unbound where the prompt did not draw it, for the reason `a` is: a key granting something
        // the same screen says cannot be granted is worse than an unbound one, and this key's grant
        // outlives the session that could have corrected it.
        KeyCode::Char('r' | 'R') if request.may_record() => {
            Some(RunResponse::Answer(RunAnswer::ApproveAndRecord))
        }
        KeyCode::Char('n' | 'N') | KeyCode::Esc => Some(RunResponse::Answer(RunAnswer::Reject)),
        KeyCode::Up => Some(RunResponse::Scroll(-1)),
        KeyCode::Down => Some(RunResponse::Scroll(1)),
        KeyCode::PageUp => Some(RunResponse::Scroll(-10)),
        KeyCode::PageDown => Some(RunResponse::Scroll(10)),
        KeyCode::Home => Some(RunResponse::Scroll(i16::MIN)),
        KeyCode::End => Some(RunResponse::Scroll(i16::MAX)),
        // Enter is deliberately not an approval: it is the key most likely to be pressed out of
        // habit, and this prompt starts a program.
        _ => None,
    }
}

/// What a key press did at a run prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunResponse {
    Answer(RunAnswer),
    Scroll(i16),
}

/// Draw the prompt for a run and wait for an answer.
///
/// Standalone as well as available through [`TerminalConfirmer`], for the same reason the write
/// prompt is: a turn on a worker thread cannot hold the terminal, so the main thread calls this on
/// its behalf.
pub fn ask_run<B: Backend>(terminal: &mut Terminal<B>, request: &RunRequest) -> RunAnswer {
    let mut scroll = 0u16;
    loop {
        let mut most = 0u16;
        // A terminal that cannot be drawn to cannot carry a question, so refuse rather than run
        // something unseen.
        if terminal
            .draw(|frame| most = draw_run(frame, request, scroll))
            .is_err()
        {
            return RunAnswer::Reject;
        }

        match event::read() {
            // Presses only: asking for disambiguated keys reports releases too, and a release
            // taken for a press approves whatever the press had just approved, twice.
            Ok(TermEvent::Key(key)) if key.kind != event::KeyEventKind::Press => continue,
            Ok(TermEvent::Key(key)) => match run_answer_for(key, request) {
                Some(RunResponse::Answer(answer)) => return answer,
                Some(RunResponse::Scroll(by)) => {
                    scroll = scroll.saturating_add_signed(by).min(most);
                }
                None => continue,
            },
            Ok(_) => continue,
            // Losing the event stream must not run anything.
            Err(_) => return RunAnswer::Reject,
        }
    }
}

/// Draw the run confirmation, returning how far its body can be scrolled.
///
/// One line per stage, rendered by [`bravebot_core::Stage::display`], which quotes unambiguously: two
/// different argument vectors cannot come out looking alike, so what the reviewer reads names
/// exactly the argv the endorsement will be bound to.
fn draw_run(frame: &mut ratatui::Frame, request: &RunRequest, scroll: u16) -> u16 {
    let area = centred(frame.area());
    let inside = panel(frame, area, theme::accent(), t!(run_title));

    // Worked out once and read by both the explanation and the key row, so a drawing cannot offer a
    // key it has just said is unavailable. [`run_answer_for`] asks the same question again rather
    // than being told the answer, because a grant must not rest on a drawing.
    let offers_always = request.can_be_remembered();

    let steps = request.plan.steps();
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{} ", t!(run_verb)),
                Style::default()
                    .fg(theme::accent())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                t!(run_stages, count = steps.len()),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "  {}",
                    t!(run_in_directory, directory = &request.directory())
                ),
                Style::default().fg(theme::muted()),
            ),
        ]),
        Line::raw(""),
    ];

    // The line the planner wrote, above the plan and marked as context. It is not what the answer
    // binds to: two spellings that compile alike are one thing to agree to, and the plan below is
    // the one being agreed to. Shown all the same, because a reader comparing the two is what
    // would catch a compiler that got the line wrong.
    if !request.plan.line.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  {}", t!(run_line_sent)),
            Style::default().fg(theme::muted()),
        )));
        lines.push(Line::from(Span::styled(
            format!("       {}", request.plan.line),
            Style::default().fg(theme::muted()),
        )));
        lines.push(Line::raw(""));
    }

    for (index, step) in steps.iter().enumerate() {
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {}  ", index + 1),
                Style::default().fg(theme::muted()),
            ),
            Span::styled(
                step.as_written(),
                Style::default()
                    .fg(theme::text())
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        // The binary, under the name. A name is not a program: $PATH decides what `grep` means,
        // and a person about to vouch for one should be looking at what they are vouching for.
        lines.push(Line::from(Span::styled(
            format!("       {}", step.resolved.display()),
            Style::default().fg(theme::muted()),
        )));
    }

    // Every file the line would create or replace, listed rather than left to be worked out from
    // the steps above. This is the half of a plan that a shell string hides, so it is the half a
    // reader most needs spelled out.
    if !request.plan.writes.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            format!("  {}", t!(run_writes)),
            Style::default().fg(theme::running()),
        )));
        for path in &request.plan.writes {
            lines.push(Line::from(Span::styled(
                format!("       {}", path.display()),
                Style::default()
                    .fg(theme::text())
                    .add_modifier(Modifier::BOLD),
            )));
        }
    }

    // What goes into the first program, where the call named a reference for it. Listed like the
    // write set and for the same reason: it is the half of a plan that the steps above do not show,
    // and a person told only that something is being fed in has not been told what.
    if let Some(reference) = &request.stdin {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            format!("  {}", t!(run_is_fed)),
            Style::default().fg(theme::running()),
        )));
        lines.push(Line::from(Span::styled(
            format!("       {reference}"),
            Style::default()
                .fg(theme::text())
                .add_modifier(Modifier::BOLD),
        )));
    }

    lines.push(Line::raw(""));

    // Said every time, because it is true every time and it is the thing a reviewer is most likely
    // to assume otherwise. A program here is not sandboxed and runs with the access the user's own
    // shell would give it.
    lines.push(Line::from(Span::styled(
        format!("  {}", t!(run_not_sandboxed)),
        Style::default().fg(theme::running()),
    )));

    // The second and independent reason to be careful, on confidentiality rather than integrity.
    // Only said when it applies, so it does not become noise that hides the case it is for.
    if request.releases_private() {
        lines.push(Line::from(Span::styled(
            format!("  {}", t!(run_releases_private)),
            Style::default().fg(theme::fail()),
        )));
    }

    // What `a` would actually grant, in as many words. It is two things, not one, and the second
    // is the one nothing else in the interface would tell them: what the command prints stops
    // being quarantined and the model reads it. Nothing checks that assertion, so the person
    // making it has to be asked for it in those terms.
    if offers_always {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            format!("  {}", t!(run_always_explained)),
            Style::default().fg(theme::muted()),
        )));
        // The command first, then what trusting it means. The claims are about this, so a reader
        // should have it in front of them before reading them.
        //
        // With the tree, because the entry holds one (RUN-8) and this line is the entry: the header
        // above says where the line runs, and saying it again here is what makes the drawing and
        // the entry agree about what `a` covers. Rendered from the entry's own directory rather
        // than the plan's, so a drawing cannot claim a tree the record would not hold.
        for command in request.would_vouch_for() {
            lines.push(Line::from(Span::styled(
                format!(
                    "       {}  {}",
                    command.display(),
                    t!(
                        run_in_directory,
                        directory = command.directory.display().to_string()
                    )
                ),
                Style::default().add_modifier(Modifier::BOLD),
            )));
        }
        lines.push(Line::from(Span::styled(
            format!("     {}", t!(run_always_means_both)),
            Style::default().fg(theme::muted()),
        )));
        lines.push(Line::from(Span::styled(
            format!("       {}", t!(run_always_runs_again)),
            Style::default().fg(theme::muted()),
        )));
        // The half nothing else in the interface would reveal, so it is the half that is coloured.
        lines.push(Line::from(Span::styled(
            format!("       {}", t!(run_always_output_trusted)),
            Style::default().fg(theme::running()),
        )));
        // Exact arguments, so the narrowness is visible rather than assumed the other way.
        lines.push(Line::from(Span::styled(
            format!("     {}", t!(run_always_exact_arguments)),
            Style::default().fg(theme::muted()),
        )));
        // The narrowness of the tree, in the same breath as the narrowness of the arguments, since
        // the two are one claim about one entry. No path in it: the entry above names the tree, and
        // a sentence repeating it would be a third copy of a path already on the screen twice and
        // long enough to overflow the panel.
        lines.push(Line::from(Span::styled(
            format!("     {}", t!(run_always_this_directory)),
            Style::default().fg(theme::muted()),
        )));
    } else {
        // Each of these asks every time whatever is remembered, so offering to stop asking would be
        // offering something that will not happen. Every reason that holds is given, not the first
        // one: a reader who acts on the only reason they were shown and still sees no `a` has been
        // told to change the wrong thing about the line.
        lines.push(Line::raw(""));
        let mut why = Vec::new();
        if request.releases_private() {
            why.push(t!(run_private_not_remembered));
        }
        if request.carries_an_assignment() {
            why.push(t!(run_assignment_not_remembered));
        }
        if request.writes_a_file() {
            why.push(t!(run_write_not_remembered));
        }
        for reason in why {
            lines.push(Line::from(Span::styled(
                format!("  {reason}"),
                Style::default().fg(theme::muted()),
            )));
        }
    }

    // What `r` would grant, where it is offered. Three things a reader cannot get from the key's
    // one word: that it lasts past this session, that every session in this directory reads it, and
    // that it stops the asking without making anything readable. The place it is written down is
    // part of the grant rather than a footnote, because nobody can endorse a record they were not
    // shown, and deleting the line from that file is the way back.
    if let Some(path) = &request.record {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            format!("  {}", t!(run_remember_explained)),
            Style::default().fg(theme::muted()),
        )));
        lines.push(Line::from(Span::styled(
            format!("     {}", t!(run_remember_every_session)),
            Style::default().fg(theme::muted()),
        )));
        // The half a person is most likely to assume the other way, since the key beside it does
        // grant it, so it is the half that is coloured.
        lines.push(Line::from(Span::styled(
            format!("     {}", t!(run_remember_only_asking)),
            Style::default().fg(theme::running()),
        )));
        lines.push(Line::from(Span::styled(
            format!("     {}", t!(run_remember_where)),
            Style::default().fg(theme::muted()),
        )));
        lines.push(Line::from(Span::styled(
            format!("       {}", path.display()),
            Style::default().add_modifier(Modifier::BOLD),
        )));
    }

    // What answers a line whose arguments differ from one run to the next, since no key on this
    // screen does. Drawn only where the person has already answered a prompt for this binary under
    // other arguments, so it is not a sentence every prompt carries. It names the file rather than
    // a pattern to put in it: which argument held the message is a judgment about the program, and
    // a box with a pattern already filled in would be this system making that judgment. The costs
    // are given with it because a pattern grants more than anything here, and somebody answering
    // the same shape of prompt all day would otherwise learn the durable form from nowhere.
    if let Some(path) = &request.pattern {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            format!("  {}", t!(run_pattern_varies)),
            Style::default().fg(theme::muted()),
        )));
        lines.push(Line::from(Span::styled(
            format!("     {}", t!(run_pattern_where)),
            Style::default().fg(theme::muted()),
        )));
        lines.push(Line::from(Span::styled(
            format!("       {}", path.display()),
            Style::default().add_modifier(Modifier::BOLD),
        )));
        // The half that makes a pattern a wider grant than any key here, so it is the half that is
        // coloured.
        lines.push(Line::from(Span::styled(
            format!("     {}", t!(run_pattern_covers_unread)),
            Style::default().fg(theme::running()),
        )));
        lines.push(Line::from(Span::styled(
            format!("     {}", t!(run_pattern_only_asking)),
            Style::default().fg(theme::muted()),
        )));
    }

    let mut key_spans = vec![
        Span::styled(
            "  y",
            Style::default()
                .fg(theme::ok())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {}    ", t!(run_yes))),
    ];
    // Offered only where it would do something. A line no entry could record asks every time
    // whatever is remembered, so the key would promise something that will not happen.
    if offers_always {
        key_spans.push(Span::styled(
            "a",
            Style::default()
                .fg(theme::running())
                .add_modifier(Modifier::BOLD),
        ));
        key_spans.push(Span::raw(format!(" {}    ", t!(run_always))));
    }
    // Drawn only where the driver said where the answer would go. `a` keeps its place on a prompt
    // that offers no `r`, and its label still says which of the two lifetimes it is: the relabelling
    // is what stops one word meaning either, so it is not conditional on this key being there.
    if request.may_record() {
        key_spans.push(Span::styled(
            "r",
            Style::default()
                .fg(theme::running())
                .add_modifier(Modifier::BOLD),
        ));
        key_spans.push(Span::raw(format!(" {}    ", t!(run_remember))));
    }
    key_spans.extend([
        Span::styled(
            "n",
            Style::default()
                .fg(theme::fail())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {}    ", t!(run_no))),
        Span::styled(
            "ctrl-c",
            Style::default()
                .fg(theme::muted())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {}", t!(stop_the_turn)),
            Style::default().fg(theme::muted()),
        ),
    ]);
    let keys = Line::from(key_spans);

    // One row for the keys, the rest for the stages, split before the body is laid out so the
    // question keeps its row whatever the body turns out to be.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inside);

    let body = Paragraph::new(lines).wrap(Wrap { trim: false });
    let drawn = body.line_count(rows[0].width) as u16;
    let furthest = drawn.saturating_sub(rows[0].height);
    let offset = scroll.min(furthest);
    frame.render_widget(body.scroll((offset, 0)), rows[0]);

    let mut keys = keys;
    if furthest > 0 {
        let below = furthest - offset;
        keys.push_span(Span::styled(
            scroll_hint(below),
            Style::default().fg(theme::accent()),
        ));
    }
    frame.render_widget(Paragraph::new(keys), rows[1]);

    furthest
}

/// Prose the panels indent by two columns, wrapped so every row keeps the indent.
///
/// The paragraph would wrap it too, but back to column zero, and these sentences sit beside
/// lines that are indented on purpose. Wrapped here rather than broken in the catalog, since a
/// translation does not break where the English did.
fn indented(text: impl Into<String>, style: Style, width: usize) -> Vec<Line<'static>> {
    marked_rows(&Span::raw("  "), &[Span::styled(text.into(), style)], width)
}

/// What a check said, for the head of a prompt whose answer would promote content.
///
/// Shared by all three of them, so the word reads the same wherever it is drawn and a prompt cannot
/// be given one without the other. Which of the three banners is drawn is the one thing decided from
/// what the check said, and it decides nothing further: the bytes are below it either way, and the
/// keys are the same three.
///
/// The banner is the driver's own words and is drawn outside the margin. The sentence under it came
/// out of content nobody vouched for and is drawn inside it, on every row it reaches, which is the
/// distinction the bar exists to make: a reader can tell which line the program wrote and which line
/// came out of the page. It is free text about content an attacker may own and it can lie; what
/// stops that mattering is that the bytes it describes are on the same screen.
///
/// The two failures are told apart rather than collapsed. "This looks like an attempt to give
/// instructions" and "nothing looked at this" are different facts about different risks, and one
/// sentence covering both would be wrong about one of them.
///
/// What went wrong is not said. The driver's word for it is English and goes in the audit trail;
/// putting it in this sentence would splice an untranslated fragment into a translated one, and the
/// difference between a backend that was down and a reply nobody could read a verdict out of is the
/// same fact to the person answering.
fn verdict_rows(
    verdict: Verdict,
    reason: Option<&String>,
    margin: &Span<'static>,
    width: usize,
) -> Vec<Line<'static>> {
    let (banner, colour) = match verdict {
        Verdict::Safe => (t!(check_safe), theme::ok()),
        Verdict::Unsafe => (t!(check_unsafe), theme::fail()),
        Verdict::Inconclusive(_) => (t!(check_inconclusive), theme::running()),
    };
    let mut rows = indented(
        banner,
        Style::default().fg(colour).add_modifier(Modifier::BOLD),
        width,
    );
    if let Some(reason) = reason {
        rows.extend(marked_rows(
            margin,
            &[Span::styled(
                reason.clone(),
                Style::default().fg(theme::muted()),
            )],
            width,
        ));
    }
    rows
}

/// How much further the body goes, or that there is nothing below.
fn scroll_hint(below: u16) -> String {
    if below > 0 {
        format!("   {}", t!(scroll_more, count = below))
    } else {
        format!("   {}", t!(scroll_back))
    }
}

/// Draw the prompt for reading a command's output and wait for an answer.
///
/// The one prompt whose body is the thing being decided about rather than a description of it. It
/// reuses the write prompt's keys and scrolling, because the answer is the same shape: yes, no, or
/// stop.
pub fn ask_output<B: Backend>(terminal: &mut Terminal<B>, request: &OutputRequest) -> VetAnswer {
    let mut scroll = 0u16;
    loop {
        let mut most = 0u16;
        // A terminal that cannot be drawn to cannot show the output, and approving output nobody
        // was shown is the one thing this question cannot mean. The verdict does not rescue it: a
        // word from a model is not a person having read something.
        if terminal
            .draw(|frame| most = draw_output(frame, request, scroll))
            .is_err()
        {
            return VetAnswer::Reject;
        }

        match event::read() {
            // Presses only: asking for disambiguated keys reports releases too, and a release
            // taken for a press approves whatever the press had just approved, twice.
            Ok(TermEvent::Key(key)) if key.kind != event::KeyEventKind::Press => continue,
            Ok(TermEvent::Key(key)) => match output_answer_for(key, request) {
                Some(VetResponse::Answer(answer)) => return answer,
                Some(VetResponse::Scroll(by)) => {
                    scroll = scroll.saturating_add_signed(by).min(most);
                }
                None => continue,
            },
            Ok(_) => continue,
            Err(_) => return VetAnswer::Reject,
        }
    }
}

/// Draw the output for reading, returning how far it can be scrolled.
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
fn draw_output(frame: &mut ratatui::Frame, request: &OutputRequest, scroll: u16) -> u16 {
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
                t!(output_lines, count = request.lines()),
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

    let mut key_spans = vec![
        Span::styled(
            "  y",
            Style::default()
                .fg(theme::ok())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {}    ", t!(output_yes))),
    ];
    // Offered only where the check completed and found nothing. It is not an answer to the
    // question on the screen: it turns off the asking, so the moment the check reported an
    // injection attempt, or could not be made at all, is the worst moment to draw it.
    // [`vetting_answer_for`] asks the same question again rather than being told the answer,
    // because a grant must not rest on a drawing.
    if request.verdict.is_safe() {
        key_spans.push(Span::styled(
            "a",
            Style::default()
                .fg(theme::running())
                .add_modifier(Modifier::BOLD),
        ));
        key_spans.push(Span::raw(format!(" {}    ", t!(vet_always))));
    }
    key_spans.extend([
        Span::styled(
            "n",
            Style::default()
                .fg(theme::fail())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {}    ", t!(output_no))),
        Span::styled(
            "ctrl-c",
            Style::default()
                .fg(theme::muted())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {}", t!(stop_the_turn)),
            Style::default().fg(theme::muted()),
        ),
    ]);
    let keys = Line::from(key_spans);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inside);

    let body = Paragraph::new(lines).wrap(Wrap { trim: false });
    let drawn = body.line_count(rows[0].width) as u16;
    let furthest = drawn.saturating_sub(rows[0].height);
    let offset = scroll.min(furthest);
    frame.render_widget(body.scroll((offset, 0)), rows[0]);

    let mut keys = keys;
    if furthest > 0 {
        let below = furthest - offset;
        keys.push_span(Span::styled(
            scroll_hint(below),
            Style::default().fg(theme::brand_primary()),
        ));
    }
    frame.render_widget(Paragraph::new(keys), rows[1]);

    furthest
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

/// What a key press did at a vetting prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VetResponse {
    Answer(VetAnswer),
    Scroll(i16),
}

/// Interpret one key press at the `vet_content` prompt, or `None` for a key that answers nothing.
fn vet_answer_for(key: KeyEvent, request: &VetRequest) -> Option<VetResponse> {
    vetting_answer_for(key, request.verdict)
}

/// Interpret one key press at the `read_output` prompt, or `None` for a key that answers nothing.
fn output_answer_for(key: KeyEvent, request: &OutputRequest) -> Option<VetResponse> {
    vetting_answer_for(key, request.verdict)
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
fn vetting_answer_for(key: KeyEvent, verdict: Verdict) -> Option<VetResponse> {
    // The prompt blocks the whole interface, so without this Ctrl-C would do nothing at the one
    // moment a user is most likely to press it.
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('c') => Some(VetResponse::Answer(VetAnswer::Interrupt)),
            _ => None,
        };
    }

    match key.code {
        KeyCode::Char('y' | 'Y') => Some(VetResponse::Answer(VetAnswer::Approve)),
        KeyCode::Char('a' | 'A') if verdict.is_safe() => {
            Some(VetResponse::Answer(VetAnswer::ApproveAlways))
        }
        KeyCode::Char('n' | 'N') | KeyCode::Esc => Some(VetResponse::Answer(VetAnswer::Reject)),
        KeyCode::Up => Some(VetResponse::Scroll(-1)),
        KeyCode::Down => Some(VetResponse::Scroll(1)),
        KeyCode::PageUp => Some(VetResponse::Scroll(-10)),
        KeyCode::PageDown => Some(VetResponse::Scroll(10)),
        KeyCode::Home => Some(VetResponse::Scroll(i16::MIN)),
        KeyCode::End => Some(VetResponse::Scroll(i16::MAX)),
        // Enter is deliberately not an approval: it is the key most likely to be pressed out of
        // habit, and this prompt puts bytes nobody vouched for into the planner's context.
        _ => None,
    }
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
    loop {
        let mut most = 0u16;
        // A terminal that cannot be drawn to cannot show the content, and approving content
        // nobody was shown is the one thing this question cannot mean. The verdict does not
        // rescue it: a word from a model is not a person having read something.
        if terminal
            .draw(|frame| most = draw_vet(frame, request, scroll))
            .is_err()
        {
            return VetAnswer::Reject;
        }

        match event::read() {
            // Presses only: asking for disambiguated keys reports releases too, and a release
            // taken for a press approves whatever the press had just approved, twice.
            Ok(TermEvent::Key(key)) if key.kind != event::KeyEventKind::Press => continue,
            Ok(TermEvent::Key(key)) => match vet_answer_for(key, request) {
                Some(VetResponse::Answer(answer)) => return answer,
                Some(VetResponse::Scroll(by)) => {
                    scroll = scroll.saturating_add_signed(by).min(most);
                }
                None => continue,
            },
            Ok(_) => continue,
            Err(_) => return VetAnswer::Reject,
        }
    }
}

/// Draw the vetted read for review, returning how far it can be scrolled.
///
/// Two things on this screen came from somewhere nobody vouched for: the content, and the
/// sentence the check wrote about it. Both are drawn inside the margin the transcript draws down
/// anything the model was not allowed to read, on every row they reach. The banner saying which
/// verdict it was is the driver's own words and is outside the margin, which is the distinction
/// the bar exists to make: a reader can tell which line the program wrote and which line came out
/// of the page.
fn draw_vet(frame: &mut ratatui::Frame, request: &VetRequest, scroll: u16) -> u16 {
    let area = centred(frame.area());
    let inside = panel(frame, area, theme::brand_primary(), t!(vet_title));

    let marked = Style::default().fg(theme::running());
    let margin = Span::styled("┃ ", marked);
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{} ", t!(vet_verb)),
                Style::default()
                    .fg(theme::brand_primary())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                t!(vet_lines, count = request.lines()),
                Style::default().add_modifier(Modifier::BOLD),
            ),
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

    // Empty content is a fact worth stating. Drawing nothing would read as a prompt that failed
    // to render, and the reviewer would be deciding about a blank box.
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

    let mut key_spans = vec![
        Span::styled(
            "  y",
            Style::default()
                .fg(theme::ok())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {}    ", t!(vet_yes))),
    ];
    // Offered only where the check completed and found nothing. It is not an answer to the
    // question on the screen: it turns off the asking, so the moment the check reported an
    // injection attempt, or could not be made at all, is the worst moment to draw it.
    // [`vet_answer_for`] asks the same question again rather than being told the answer, because
    // a grant must not rest on a drawing.
    if request.verdict.is_safe() {
        key_spans.push(Span::styled(
            "a",
            Style::default()
                .fg(theme::running())
                .add_modifier(Modifier::BOLD),
        ));
        key_spans.push(Span::raw(format!(" {}    ", t!(vet_always))));
    }
    key_spans.extend([
        Span::styled(
            "n",
            Style::default()
                .fg(theme::fail())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {}    ", t!(vet_no))),
        Span::styled(
            "ctrl-c",
            Style::default()
                .fg(theme::muted())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {}", t!(stop_the_turn)),
            Style::default().fg(theme::muted()),
        ),
    ]);
    let keys = Line::from(key_spans);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inside);

    let body = Paragraph::new(lines).wrap(Wrap { trim: false });
    let drawn = body.line_count(rows[0].width) as u16;
    let furthest = drawn.saturating_sub(rows[0].height);
    let offset = scroll.min(furthest);
    frame.render_widget(body.scroll((offset, 0)), rows[0]);

    let mut keys = keys;
    if furthest > 0 {
        let below = furthest - offset;
        keys.push_span(Span::styled(
            scroll_hint(below),
            Style::default().fg(theme::brand_primary()),
        ));
    }
    frame.render_widget(Paragraph::new(keys), rows[1]);

    furthest
}

/// Draw the offer to vouch for a quarantined file, and wait for an answer.
/// Ask whether to fetch a URL, blocking until answered.
pub fn ask_fetch<B: Backend>(terminal: &mut Terminal<B>, request: &FetchRequest) -> Answer {
    loop {
        if terminal.draw(|frame| draw_fetch(frame, request)).is_err() {
            return Answer::Reject;
        }

        match event::read() {
            Ok(TermEvent::Key(key)) if key.kind != event::KeyEventKind::Press => continue,
            Ok(TermEvent::Key(key)) => match answer_for(key) {
                Some(Response::Answer(answer)) => return answer,
                // Nothing here scrolls: a URL and a host are two lines, and there is no body to
                // page through because none has been fetched yet.
                Some(Response::Scroll(_)) => continue,
                None => continue,
            },
            Ok(_) => continue,
            Err(_) => return Answer::Reject,
        }
    }
}

/// Put the language-server question to the user.
///
/// Its own prompt rather than a run's, because what a yes grants has a different shape: a process
/// that lives for the session rather than one argv that exits. LSP-5 is where that is settled.
pub fn ask_server<B: Backend>(terminal: &mut Terminal<B>, request: &ServerRequest) -> Answer {
    loop {
        if terminal.draw(|frame| draw_server(frame, request)).is_err() {
            return Answer::Reject;
        }

        match event::read() {
            Ok(TermEvent::Key(key)) if key.kind != event::KeyEventKind::Press => continue,
            Ok(TermEvent::Key(key)) => match answer_for(key) {
                Some(Response::Answer(answer)) => return answer,
                // Nothing here scrolls: the whole question is a binary, a directory and two
                // sentences, and there is no body because nothing has been read yet.
                Some(Response::Scroll(_)) => continue,
                None => continue,
            },
            Ok(_) => continue,
            Err(_) => return Answer::Reject,
        }
    }
}

/// Draw the language-server question.
///
/// What a person is answering about is what the process will be allowed to do, so the build-tooling
/// sentence is drawn where it cannot be missed rather than left inside "with your own access". A
/// server that only reads says that instead, because the two are genuinely different propositions
/// and a prompt that warned about both would teach the reader to skim.
fn draw_server(frame: &mut ratatui::Frame, request: &ServerRequest) {
    let area = centred(frame.area());
    let inside = panel(frame, area, theme::ok(), t!(server_title));

    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{} ", t!(server_verb)),
                Style::default()
                    .fg(theme::ok())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                request.program.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::styled(
            format!(
                "  {}",
                t!(server_workspace, workspace = request.workspace.as_str())
            ),
            Style::default().fg(theme::muted()),
        ),
        Line::raw(""),
    ];

    // The consequential half. Drawn in the warning colour where dependency code runs, since that is
    // the part of this question a person could not have inferred from the word "start".
    let (sentence, style) = if request.runs_build_tooling {
        (t!(server_build_tooling), Style::default().fg(theme::note()))
    } else {
        (t!(server_reads_only), Style::default().fg(theme::muted()))
    };
    lines.extend(indented(sentence, style, inside.width as usize));
    lines.push(Line::raw(""));
    lines.extend(indented(
        t!(server_explained),
        Style::default().fg(theme::muted()),
        inside.width as usize,
    ));

    let keys = Line::from(vec![
        Span::styled(
            "  y",
            Style::default()
                .fg(theme::ok())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {}    ", t!(server_yes))),
        Span::styled(
            "n",
            Style::default()
                .fg(theme::fail())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {}    ", t!(server_no))),
        Span::styled(
            "ctrl-c",
            Style::default()
                .fg(theme::muted())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {}", t!(stop_the_turn)),
            Style::default().fg(theme::muted()),
        ),
    ]);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inside);

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), rows[0]);
    frame.render_widget(Paragraph::new(keys), rows[1]);
}

/// Draw the fetch question.
///
/// The host is drawn on its own line rather than left inside the URL. A person skimming
/// `https://example.com@evil.test/` reads the first name and the request goes to the second, so
/// what they are actually answering about is put where it cannot be misread.
fn draw_fetch(frame: &mut ratatui::Frame, request: &FetchRequest) {
    let area = centred(frame.area());
    let inside = panel(frame, area, theme::ok(), t!(fetch_title));

    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{} ", t!(fetch_verb)),
                Style::default()
                    .fg(theme::ok())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                request.url.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::styled(
            format!("  {}", t!(fetch_host, host = request.host.as_str())),
            Style::default().fg(theme::muted()),
        ),
        Line::raw(""),
    ];
    lines.extend(indented(
        t!(fetch_explained),
        Style::default().fg(theme::muted()),
        inside.width as usize,
    ));

    let keys = Line::from(vec![
        Span::styled(
            "  y",
            Style::default()
                .fg(theme::ok())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {}    ", t!(fetch_yes))),
        Span::styled(
            "n",
            Style::default()
                .fg(theme::fail())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {}    ", t!(fetch_no))),
        Span::styled(
            "ctrl-c",
            Style::default()
                .fg(theme::muted())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {}", t!(stop_the_turn)),
            Style::default().fg(theme::muted()),
        ),
    ]);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inside);

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), rows[0]);
    frame.render_widget(Paragraph::new(keys), rows[1]);
}

pub fn ask_vouch<B: Backend>(terminal: &mut Terminal<B>, request: &VouchRequest) -> Answer {
    let mut scroll = 0u16;
    loop {
        let mut most = 0u16;
        if terminal
            .draw(|frame| most = draw_vouch(frame, request, scroll))
            .is_err()
        {
            return Answer::Reject;
        }

        match event::read() {
            // Presses only: asking for disambiguated keys reports releases too, and a release
            // taken for a press approves whatever the press had just approved, twice.
            Ok(TermEvent::Key(key)) if key.kind != event::KeyEventKind::Press => continue,
            Ok(TermEvent::Key(key)) => match answer_for(key) {
                Some(Response::Answer(answer)) => return answer,
                Some(Response::Scroll(by)) => {
                    scroll = scroll.saturating_add_signed(by).min(most);
                }
                None => continue,
            },
            Ok(_) => continue,
            Err(_) => return Answer::Reject,
        }
    }
}

/// Draw the vouch offer, returning how far it can be scrolled.
///
/// The preview carries the same margin bar as everything else the model has not been allowed to
/// read, because that is exactly what it is until this question is answered.
///
/// The banner above it is what a check made of the file. It is advice and never an answer: a yes
/// writes the trust rule whatever the word was, and a no writes nothing whatever the word was.
fn draw_vouch(frame: &mut ratatui::Frame, request: &VouchRequest, scroll: u16) -> u16 {
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

    let keys = Line::from(vec![
        Span::styled(
            "  y",
            Style::default()
                .fg(theme::ok())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {}    ", t!(vouch_yes))),
        Span::styled(
            "n",
            Style::default()
                .fg(theme::fail())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {}    ", t!(vouch_no))),
        Span::styled(
            "ctrl-c",
            Style::default()
                .fg(theme::muted())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {}", t!(stop_the_turn)),
            Style::default().fg(theme::muted()),
        ),
    ]);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inside);

    let body = Paragraph::new(lines).wrap(Wrap { trim: false });
    let drawn = body.line_count(rows[0].width) as u16;
    let furthest = drawn.saturating_sub(rows[0].height);
    let offset = scroll.min(furthest);
    frame.render_widget(body.scroll((offset, 0)), rows[0]);

    let mut keys = keys;
    if furthest > 0 {
        let below = furthest - offset;
        keys.push_span(Span::styled(
            scroll_hint(below),
            Style::default().fg(theme::ok()),
        ));
    }
    frame.render_widget(Paragraph::new(keys), rows[1]);

    furthest
}

/// Put a whole frozen plan to the person, blocking until answered.
///
/// The only prompt here about a run rather than about one effect, and the only one raised before
/// anything has happened at all. A manifest run fixes every destination while the task string is
/// the only input in existence, so there is no later moment at which any of this could be asked
/// again and nothing a step reads can add to it: what is on the screen is the whole of what will
/// happen. MANIFEST-10 is where that is settled.
pub fn ask_manifest<B: Backend>(terminal: &mut Terminal<B>, request: &ManifestRequest) -> Answer {
    let mut scroll = 0u16;
    loop {
        let mut most = 0u16;
        // A terminal that cannot be drawn to cannot show the plan, and running a program nobody was
        // shown is the one thing this question cannot mean.
        if terminal
            .draw(|frame| most = draw_manifest(frame, request, scroll))
            .is_err()
        {
            return Answer::Reject;
        }

        match event::read() {
            Ok(TermEvent::Key(key)) if key.kind != event::KeyEventKind::Press => continue,
            Ok(TermEvent::Key(key)) => match answer_for(key) {
                Some(Response::Answer(answer)) => return answer,
                // A plan longer than the box is the one most worth reading before answering, since
                // approving it approves the steps below the fold as well.
                Some(Response::Scroll(by)) => {
                    scroll = scroll.saturating_add_signed(by).min(most);
                }
                None => continue,
            },
            Ok(_) => continue,
            Err(_) => return Answer::Reject,
        }
    }
}

/// Draw the plan, returning how far its body can be scrolled.
///
/// No margin bar down the steps, unlike every other body in this file. The others are somebody
/// else's bytes; this is the driver's own rendering of a program that came from a context holding
/// the task string and the driver's words. A bar here would mark the steps as content nobody may
/// trust, which is the opposite of why they can be shown at all.
fn draw_manifest(frame: &mut ratatui::Frame, request: &ManifestRequest, scroll: u16) -> u16 {
    let area = centred(frame.area());
    let inside = panel(frame, area, theme::brand_primary(), t!(plan_title));

    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{} ", t!(plan_verb)),
                Style::default()
                    .fg(theme::brand_primary())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                t!(plan_steps, count = request.steps.len()),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}", t!(plan_goal, task = &request.task)),
                Style::default().fg(theme::muted()),
            ),
        ]),
        Line::raw(""),
    ];

    // Every step, never a count of them and never the first few. A person cannot endorse a step
    // they were not shown, and the one that walks off the bottom of the box is as binding as the
    // first: what does not fit is scrolled to.
    for step in &request.steps {
        lines.extend(indented(
            step.clone(),
            Style::default(),
            inside.width as usize,
        ));
    }
    lines.push(Line::raw(""));

    // What a yes settles, then the two things it does not. Each write in the plan is still put to
    // the person as it comes up, and nothing has happened yet, so declining costs nothing.
    for sentence in [
        t!(plan_explained),
        t!(plan_not_its_writes),
        t!(plan_nothing_yet),
    ] {
        lines.extend(indented(
            sentence,
            Style::default().fg(theme::muted()),
            inside.width as usize,
        ));
    }

    let keys = Line::from(vec![
        Span::styled(
            "  y",
            Style::default()
                .fg(theme::ok())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {}    ", t!(plan_yes))),
        Span::styled(
            "n",
            Style::default()
                .fg(theme::fail())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {}    ", t!(plan_no))),
        Span::styled(
            "ctrl-c",
            Style::default()
                .fg(theme::muted())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {}", t!(stop_the_turn)),
            Style::default().fg(theme::muted()),
        ),
    ]);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inside);

    let body = Paragraph::new(lines).wrap(Wrap { trim: false });
    let drawn = body.line_count(rows[0].width) as u16;
    let furthest = drawn.saturating_sub(rows[0].height);
    let offset = scroll.min(furthest);
    frame.render_widget(body.scroll((offset, 0)), rows[0]);

    let mut keys = keys;
    if furthest > 0 {
        let below = furthest - offset;
        keys.push_span(Span::styled(
            scroll_hint(below),
            Style::default().fg(theme::brand_primary()),
        ));
    }
    frame.render_widget(Paragraph::new(keys), rows[1]);

    furthest
}

/// Draw the outer box of a prompt, and return the area inside its border.
///
/// The theme's own background as well as its border, because `Clear` empties cells without
/// colouring them: a panel that painted only its border would be a hole in the palette, with the
/// themed transcript still drawn around it, and the boundary between what the system is asking and
/// what somebody else's bytes say is exactly what a person is reading when they answer. The text
/// colour comes with it, so a span that sets none of its own is the theme's rather than the
/// terminal's.
fn panel(frame: &mut ratatui::Frame, area: Rect, border: Color, title: &str) -> Rect {
    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border))
        .title(format!(" {title} "))
        .style(Style::default().bg(theme::background()).fg(theme::text()));
    // Measured before the block is handed over, because the bodies are laid out against the width
    // they will be drawn at: a margin decided without knowing the width is a margin the first
    // wrapped row escapes.
    let inside = block.inner(area);
    frame.render_widget(block, area);
    inside
}

/// A centred box, sized to the terminal but never larger than it.
fn centred(area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(10),
            Constraint::Percentage(80),
            Constraint::Percentage(10),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(5),
            Constraint::Percentage(90),
            Constraint::Percentage(5),
        ])
        .split(vertical[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_agent::confirm::Remark;

    use ratatui::backend::TestBackend;

    fn request(contents: &str, existing: Option<&str>) -> WriteRequest {
        WriteRequest {
            path: "src/main.rs".into(),
            contents: contents.into(),
            intent: if existing.is_some() {
                Intent::Overwrite
            } else {
                Intent::Create
            },
            existing: existing.map(str::to_string),
            untrusted: false,
            remark: None,
        }
    }

    fn rendered(request: &WriteRequest) -> String {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        terminal
            .draw(|frame| {
                draw(frame, request, 0);
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

    fn a_run(private: bool) -> RunRequest {
        let pipeline = bravebot_core::Pipeline::new(vec![
            bravebot_core::Stage::new("git", vec!["log".into(), "--oneline".into()]),
            bravebot_core::Stage::new("sed", vec!["-n".into(), "1,10p".into()]),
        ]);
        RunRequest::from_pipeline(
            &if private {
                pipeline.with_stdin(bravebot_core::label::Label::trusted_private())
            } else {
                pipeline
            },
            &["/usr/bin/git".into(), "/usr/bin/sed".into()],
            "/home/someone/project",
        )
    }

    /// A plan from a command line, with a destination and a shape, as the compiler would produce.
    fn a_compiled_run() -> RunRequest {
        let step = |program: &str, args: &[&str]| bravebot_core::command::Step {
            program: program.to_string(),
            resolved: std::path::PathBuf::from(format!("/usr/bin/{program}")),
            args: args.iter().map(|arg| (*arg).to_string()).collect(),
            environment: Vec::new(),
            routes: Vec::new(),
        };
        let mut writing = step("tee", &[]);
        writing.routes = vec![bravebot_core::command::Route::Stdout {
            path: std::path::PathBuf::from("/home/someone/project/out.txt"),
            append: false,
        }];
        RunRequest {
            stdin: None,
            // A line naming a file to write is asked about however it was answered, so the prompt
            // offers no key that would outlive the session.
            record: None,
            pattern: None,
            plan: bravebot_core::command::Plan {
                line: "git log --oneline | tee > out.txt".to_string(),
                directory: std::path::PathBuf::from("/home/someone/project"),
                steps: bravebot_core::command::Steps::Pipeline(vec![
                    step("git", &["log", "--oneline"]),
                    writing,
                ]),
                writes: vec![std::path::PathBuf::from("/home/someone/project/out.txt")],
                reads: Vec::new(),
                stdin: None,
            },
        }
    }

    /// The answer binds to the plan, so the plan is what the prompt puts in front of a reader: the
    /// name they recognise, the binary that will actually run, and where each argument ends.
    #[test]
    fn a_run_prompt_shows_the_plan_it_would_endorse() {
        let shown = rendered_run(&a_compiled_run());
        assert!(shown.contains("git log --oneline"), "{shown}");
        assert!(shown.contains("/usr/bin/git"), "{shown}");
        assert!(shown.contains("/usr/bin/tee"), "{shown}");
    }

    /// The line is context and not the thing agreed to, but it is shown: a reader comparing it
    /// against the plan is what would catch a compiler that read the line wrong.
    #[test]
    fn a_run_prompt_shows_the_line_the_model_wrote_as_context() {
        let shown = rendered_run(&a_compiled_run());
        assert!(shown.contains("the model wrote"), "{shown}");
        assert!(
            shown.contains("git log --oneline | tee > out.txt"),
            "{shown}"
        );
    }

    /// Where the bytes land is the half of a plan that a shell string hides, so it is the half a
    /// reader most needs spelled out rather than left to be worked out from the steps.
    #[test]
    fn a_run_prompt_lists_every_file_the_line_would_write() {
        let shown = rendered_run(&a_compiled_run());
        assert!(shown.contains("it writes these files"), "{shown}");
        assert!(shown.contains("/home/someone/project/out.txt"), "{shown}");
    }

    /// What goes into the first program is the other half a shell string hides, and a person
    /// endorsing a release has to be able to read which reference it is: told only that something
    /// is being fed in, they have been told nothing they could weigh. The reference name and never
    /// a byte of what it holds; reading that is `read_output`'s own prompt.
    #[test]
    fn a_run_prompt_names_the_reference_it_would_be_fed() {
        let mut request = a_compiled_run();
        request.stdin = Some("ref:1".to_string());
        request.plan.stdin = Some(bravebot_core::label::Label::untrusted_public());

        let shown = rendered_run(&request);
        assert!(shown.contains("it is fed the contents of"), "{shown}");
        assert!(shown.contains("ref:1"), "{shown}");
    }

    /// A pipeline of argv stages writes nothing and was not spelled as a line, so neither block
    /// appears. A prompt that said "it writes these files" over an empty list would be noise that
    /// hides the case the line is for. The same for what it is fed: a call that named no reference
    /// has nothing to name.
    #[test]
    fn a_run_prompt_for_argv_stages_shows_neither_a_line_nor_a_write_set() {
        let shown = rendered_run(&a_run(false));
        assert!(!shown.contains("the model wrote"), "{shown}");
        assert!(!shown.contains("it writes these files"), "{shown}");
        assert!(!shown.contains("it is fed the contents of"), "{shown}");
    }

    /// Wide enough that the lines under test are not wrapped by the box, since what is being
    /// checked is the wording rather than the layout.
    fn rendered_run(request: &RunRequest) -> String {
        let mut terminal = Terminal::new(TestBackend::new(160, 24)).expect("terminal");
        terminal
            .draw(|frame| {
                draw_run(frame, request, 0);
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

    /// A reviewer has to see the argv, the binary behind each name, and where it will run. All
    /// three change what the run means, and none of them can be inferred from the others.
    #[test]
    fn a_run_prompt_shows_the_argv_the_binary_and_the_directory() {
        let drawn = rendered_run(&a_run(false));
        assert!(drawn.contains("git log --oneline"), "{drawn}");
        assert!(drawn.contains("sed -n 1,10p"), "{drawn}");
        assert!(drawn.contains("/usr/bin/git"), "the binary is not shown");
        assert!(drawn.contains("/home/someone/project"), "{drawn}");
    }

    /// Said every time, because it is true every time and it is the thing a reviewer is most
    /// likely to assume otherwise.
    #[test]
    fn a_run_prompt_says_it_is_not_sandboxed() {
        assert!(rendered_run(&a_run(false)).contains("not sandboxed"));
    }

    /// Vouching grants two things, and the prompt has to ask for both in as many words. The
    /// second is the one nothing else in the interface would reveal: what the command prints stops
    /// being quarantined and the model reads it. Nothing checks that assertion, so the person
    /// making it must be asked for it explicitly.
    #[test]
    fn a_run_prompt_asks_for_the_side_effects_and_the_output_together() {
        let drawn = rendered_run(&a_run(false));
        assert!(
            drawn.contains("runs again unasked"),
            "the prompt does not say vouching covers running it again: {drawn}"
        );
        assert!(
            drawn.contains("side effects"),
            "the prompt does not say vouching covers the side effects: {drawn}"
        );
        assert!(
            drawn.contains("what it prints is trusted"),
            "the prompt does not say vouching trusts the output: {drawn}"
        );
    }

    /// The entry is a command, not a program, and the prompt shows it with its arguments so the
    /// narrowness is visible rather than assumed the other way around.
    #[test]
    fn a_run_prompt_names_the_exact_command_it_would_vouch_for() {
        let drawn = rendered_run(&a_run(false));
        assert!(
            drawn.contains("/usr/bin/git log --oneline"),
            "the prompt does not name the arguments being vouched for: {drawn}"
        );
        assert!(
            drawn.contains("would not cover git push"),
            "the prompt does not say the entry is one command: {drawn}"
        );
    }

    /// RUN-8: an entry records the tree it was given in, so the sentence saying what `a` covers has
    /// to name that tree. The header above already shows where the line runs; this is the claim
    /// about the grant, and a claim that named the command and its arguments alone would be telling
    /// the person the entry covers more than it does.
    #[test]
    fn a_run_prompt_names_the_tree_the_entry_would_be_given_in() {
        let request = a_run(false);
        let drawn = rendered_run(&request);
        // Once for the header, which says where the line runs, and once for each entry `a` would
        // make, which says where that entry would hold. The header alone is a screen that shows
        // the tree and still claims a grant that does not name it.
        assert_eq!(
            drawn.matches("/home/someone/project").count(),
            request.would_vouch_for().len() + 1,
            "the entries the prompt offers to make do not name the tree they would cover: {drawn}"
        );
        assert!(
            drawn.contains("this directory only"),
            "the prompt does not say the entry stops at that tree: {drawn}"
        );
    }

    /// Private input asks every time whatever is remembered, so the key that offers to stop
    /// asking is not offered: it would promise something that will not happen.
    #[test]
    fn a_run_that_releases_private_data_offers_no_standing_permission() {
        let drawn = rendered_run(&a_run(true));
        assert!(
            drawn.contains("cannot be remembered"),
            "the prompt offered to remember a run that will always ask: {drawn}"
        );
        assert!(
            drawn.contains("your own data"),
            "the confidentiality reason was not given: {drawn}"
        );
    }

    /// The drawing and the answer have to agree. The key row stopped offering `a` for a run that
    /// releases private data, but the handler went on accepting it, so a reviewer pressing it out
    /// of habit from the previous prompt granted a session-long permission the same screen had
    /// just told them could not be granted, over a command it never named.
    #[test]
    fn pressing_always_at_a_private_input_prompt_grants_nothing() {
        let pressed = run_answer_for(
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
            &a_run(true),
        );
        assert_eq!(
            pressed, None,
            "`a` answered a prompt that does not offer it"
        );

        let shouted = run_answer_for(
            KeyEvent::new(KeyCode::Char('A'), KeyModifiers::NONE),
            &a_run(true),
        );
        assert_eq!(shouted, None, "the shifted spelling still answered");
    }

    /// A plan with a `<` redirection, which is the route by which private input actually reaches
    /// a program. The drawing and the handler both have to withhold `a` here for the same reason
    /// they withhold it for supplied bytes: the run asks every time, so the key would promise
    /// something that will not happen, over a file the entry it records would not even name.
    #[test]
    fn a_run_reading_a_file_offers_no_standing_permission() {
        let drawn = rendered_run(&a_run_reading_a_file());
        assert!(
            drawn.contains("cannot be remembered"),
            "the prompt offered to remember a run that will always ask: {drawn}"
        );

        let pressed = run_answer_for(
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
            &a_run_reading_a_file(),
        );
        assert_eq!(
            pressed, None,
            "`a` answered a prompt that does not offer it"
        );
    }

    /// A compiled plan that feeds a file to a program, as `cat < ~/.ssh/id_rsa` compiles.
    fn a_run_reading_a_file() -> RunRequest {
        let secret = std::path::PathBuf::from("/home/someone/.ssh/id_rsa");
        let reading = bravebot_core::command::Step {
            program: "cat".to_string(),
            resolved: std::path::PathBuf::from("/bin/cat"),
            args: Vec::new(),
            environment: Vec::new(),
            routes: vec![bravebot_core::command::Route::Stdin {
                path: secret.clone(),
            }],
        };
        RunRequest {
            stdin: None,
            // Private input is asked about every time, so neither standing key is offered.
            record: None,
            pattern: None,
            plan: bravebot_core::command::Plan {
                line: "cat < /home/someone/.ssh/id_rsa".to_string(),
                directory: std::path::PathBuf::from("/home/someone/project"),
                steps: bravebot_core::command::Steps::Pipeline(vec![reading]),
                writes: Vec::new(),
                reads: vec![secret],
                stdin: None,
            },
        }
    }

    /// A line carrying an assignment asks every time whatever is remembered, because an entry
    /// records a program and its exact arguments and an assignment is in neither. The drawing and
    /// the handler both have to withhold `a`, and the sentence has to give this reason rather than
    /// the private-input one: a reader told the wrong reason cannot tell what to change about the
    /// line.
    #[test]
    fn a_run_carrying_an_environment_assignment_offers_no_standing_permission() {
        let drawn = rendered_run(&a_run_with_an_assignment());
        assert!(
            drawn.contains("cannot be remembered"),
            "the prompt offered to remember a run that will always ask: {drawn}"
        );
        assert!(
            drawn.contains("an assignment in front of a program"),
            "the prompt did not say which of the two reasons this is: {drawn}"
        );

        let pressed = run_answer_for(
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
            &a_run_with_an_assignment(),
        );
        assert_eq!(
            pressed, None,
            "`a` answered a prompt that does not offer it"
        );

        let shouted = run_answer_for(
            KeyEvent::new(KeyCode::Char('A'), KeyModifiers::NONE),
            &a_run_with_an_assignment(),
        );
        assert_eq!(shouted, None, "the shifted spelling still answered");
    }

    /// Both reasons at once, as `LD_PRELOAD=./evil.so cat < secret.txt` is. A reader given only the
    /// first one acts on it, sees no `a` appear, and has nothing left to tell them why: the two
    /// reasons are independent, so the screen has to name every one that holds.
    #[test]
    fn a_run_that_is_private_and_carries_an_assignment_gives_both_reasons() {
        let mut request = a_run_with_an_assignment();
        request.plan.stdin = Some(bravebot_core::label::Label::trusted_private());

        let drawn = rendered_run(&request);
        assert!(
            drawn.contains("private input is asked"),
            "the prompt dropped the private-input reason: {drawn}"
        );
        assert!(
            drawn.contains("an assignment in front of a program"),
            "the prompt dropped the assignment reason: {drawn}"
        );
    }

    /// A compiled plan with an assignment written in front of its program, as
    /// `LD_PRELOAD=./evil.so git log` compiles.
    fn a_run_with_an_assignment() -> RunRequest {
        let step = bravebot_core::command::Step {
            program: "git".to_string(),
            resolved: std::path::PathBuf::from("/usr/bin/git"),
            args: vec!["log".to_string()],
            environment: vec![("LD_PRELOAD".to_string(), "./evil.so".to_string())],
            routes: Vec::new(),
        };
        RunRequest {
            stdin: None,
            plan: bravebot_core::command::Plan {
                line: "LD_PRELOAD=./evil.so git log".to_string(),
                directory: std::path::PathBuf::from("/home/someone/project"),
                steps: bravebot_core::command::Steps::Pipeline(vec![step]),
                writes: Vec::new(),
                reads: Vec::new(),
                stdin: None,
            },
            // What the driver hands over for such a line: it is asked about whatever is recorded,
            // so there is nowhere an answer to it would be written.
            record: None,
            pattern: None,
        }
    }

    /// A line naming a file to write asks every time whatever is remembered, so `a` cannot stop the
    /// next prompt for it. What the key could do instead is the whole of the problem: an entry holds
    /// no redirection, so it would come out covering this program and these arguments with the
    /// destination gone, granting a bare line the person never read. Both layers withhold the key,
    /// and the sentence has to name this reason rather than one of the other two.
    #[test]
    fn a_run_writing_a_file_offers_no_standing_permission() {
        let drawn = rendered_run(&a_run_writing_a_file());
        assert!(
            drawn.contains("cannot be remembered"),
            "the prompt offered to remember a run that will always ask: {drawn}"
        );
        assert!(
            drawn.contains("a line naming a file to write"),
            "the prompt did not say which of the reasons this is: {drawn}"
        );

        let pressed = run_answer_for(
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
            &a_run_writing_a_file(),
        );
        assert_eq!(
            pressed, None,
            "`a` answered a prompt that does not offer it"
        );

        let shouted = run_answer_for(
            KeyEvent::new(KeyCode::Char('A'), KeyModifiers::NONE),
            &a_run_writing_a_file(),
        );
        assert_eq!(shouted, None, "the shifted spelling still answered");
    }

    /// A compiled plan that redirects its output to a file, as `sh check.sh > out.txt` compiles.
    /// The argv holds the script and not the destination, which is exactly why an entry made here
    /// would cover `sh check.sh` on its own.
    fn a_run_writing_a_file() -> RunRequest {
        let destination = std::path::PathBuf::from("/home/someone/project/out.txt");
        let step = bravebot_core::command::Step {
            program: "sh".to_string(),
            resolved: std::path::PathBuf::from("/bin/sh"),
            args: vec!["check.sh".to_string()],
            environment: Vec::new(),
            routes: vec![bravebot_core::command::Route::Stdout {
                path: destination.clone(),
                append: false,
            }],
        };
        RunRequest {
            stdin: None,
            plan: bravebot_core::command::Plan {
                line: "sh check.sh > out.txt".to_string(),
                directory: std::path::PathBuf::from("/home/someone/project"),
                steps: bravebot_core::command::Steps::Pipeline(vec![step]),
                writes: vec![destination],
                reads: Vec::new(),
                stdin: None,
            },
            record: None,
            pattern: None,
        }
    }

    /// Refusing `a` must not take the answers the prompt does offer with it: a private run can
    /// still be approved for this one time, and still refused.
    #[test]
    fn a_private_input_run_can_still_be_approved_once_or_refused() {
        assert_eq!(
            run_answer_for(
                KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
                &a_run(true)
            ),
            Some(RunResponse::Answer(RunAnswer::Approve))
        );
        assert_eq!(
            run_answer_for(
                KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE),
                &a_run(true)
            ),
            Some(RunResponse::Answer(RunAnswer::Reject))
        );
    }

    /// Three answers, and the one that grants a standing permission is a key of its own rather
    /// than a follow-up question nobody would read.
    #[test]
    fn the_run_keys_separate_running_once_from_running_always() {
        let once = run_answer_for(
            KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
            &a_run(false),
        );
        assert_eq!(once, Some(RunResponse::Answer(RunAnswer::Approve)));
        assert!(!RunAnswer::Approve.decision().remember);

        let always = run_answer_for(
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
            &a_run(false),
        );
        assert_eq!(always, Some(RunResponse::Answer(RunAnswer::ApproveAlways)));
        assert!(RunAnswer::ApproveAlways.decision().remember);
    }

    /// A run prompt that may record its answer, as one in a session with somewhere to keep the
    /// record looks. The path is what the prompt has to show, since nobody can endorse a record
    /// they were not shown.
    fn a_recordable_run() -> RunRequest {
        RunRequest {
            stdin: None,
            record: Some(std::path::PathBuf::from(
                "/home/someone/.bravebot/remembered/-home-someone-project.jsonl",
            )),
            ..a_run(false)
        }
    }

    /// RUN-19, PROMPT-6: the third answer has a key of its own. Pressing it approves the run and
    /// records the line, and it vouches for nothing, which is the half `a` grants and this does not.
    #[test]
    fn the_run_keys_separate_this_session_from_every_session() {
        let recorded = run_answer_for(
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
            &a_recordable_run(),
        );
        assert_eq!(
            recorded,
            Some(RunResponse::Answer(RunAnswer::ApproveAndRecord))
        );

        let decision = RunAnswer::ApproveAndRecord.decision();
        assert!(decision.approved());
        assert!(decision.record);
        assert!(
            !decision.remember,
            "the key that decides a lifetime also vouched for the programs"
        );
        assert!(
            !RunAnswer::ApproveAlways.decision().record,
            "the key that decides a label also recorded an answer past the session"
        );
    }

    /// RUN-19: the key is unbound where the prompt did not draw it. A key granting something the
    /// same screen says cannot be granted is worse than an unbound one, and this key's grant
    /// outlives the session that could have corrected it.
    #[test]
    fn a_prompt_that_offers_no_record_binds_no_key_to_one() {
        for request in [a_run(false), a_run(true), a_compiled_run()] {
            assert!(!request.may_record());
            assert_eq!(
                run_answer_for(
                    KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
                    &request
                ),
                None,
                "a key recorded an answer the prompt did not offer"
            );
        }
    }

    /// PROMPT-6: Enter does not reach the key whose grant outlives the session either.
    #[test]
    fn enter_does_not_record_a_run_past_the_session() {
        assert_eq!(
            run_answer_for(
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                &a_recordable_run()
            ),
            None
        );
    }

    /// RUN-19: declining and Ctrl-C record nothing, which is what every standing grant here
    /// requires. A refusal is not a reason to answer for anything past this moment.
    #[test]
    fn refusing_a_run_records_nothing_past_the_session() {
        for answer in [RunAnswer::Reject, RunAnswer::Interrupt] {
            let decision = answer.decision();
            assert!(!decision.approved());
            assert!(!decision.record, "{answer:?} recorded an answer");
        }
    }

    /// The same drawing on a terminal tall enough to hold the whole body, for a test about what the
    /// prompt says rather than about what scrolls.
    fn fully_rendered_run(request: &RunRequest) -> String {
        let mut terminal = Terminal::new(TestBackend::new(160, 48)).expect("terminal");
        terminal
            .draw(|frame| {
                draw_run(frame, request, 0);
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

    /// RUN-19: the prompt shows what would be recorded and where. The line is already on the
    /// screen; the file is the part nothing else would tell them, and deleting a line from it is
    /// the way back.
    #[test]
    fn a_prompt_offering_to_remember_says_where_the_record_goes() {
        let drawn = fully_rendered_run(&a_recordable_run());
        assert!(drawn.contains("remember it"), "{drawn}");
        assert!(
            drawn.contains("/home/someone/.bravebot/remembered"),
            "the prompt offered a record without saying where it goes: {drawn}"
        );
        assert!(
            drawn.contains("stays quarantined"),
            "the prompt did not say the key leaves the output where it was: {drawn}"
        );
    }

    /// RUN-19, PROMPT-6: the row is where the two lifetimes are told apart, and it says so on every
    /// run prompt, including the ones that offer no `r`. A bare `always` is the one word this
    /// relabelling exists to stop meaning two different things.
    #[test]
    fn the_row_says_which_lifetime_the_always_key_grants() {
        for request in [a_run(false), a_recordable_run()] {
            let drawn = rendered_run(&request);
            assert!(
                drawn.contains("always this session"),
                "the row left `always` saying either lifetime: {drawn}"
            );
        }
    }

    /// RUN-19: a prompt with no record to offer draws no key for one, so nothing on the screen
    /// promises something that will not happen.
    #[test]
    fn a_prompt_with_no_record_to_offer_draws_no_key_for_one() {
        let drawn = rendered_run(&a_run(false));
        assert!(!drawn.contains("remember it"), "{drawn}");
    }

    /// A run prompt for a line whose arguments have already differed, as the driver hands one over
    /// once the same binary has been put to the person twice.
    fn a_varying_run() -> RunRequest {
        RunRequest {
            stdin: None,
            pattern: Some(std::path::PathBuf::from(
                "/home/someone/.bravebot/settings.json",
            )),
            ..a_recordable_run()
        }
    }

    /// RUN-20: a line whose arguments differ next time is asked about again however it is answered
    /// here, so the prompt says where the durable answer is written. Naming the file is the whole
    /// of the advice: somebody told only that a pattern exists has been handed a chore without the
    /// one fact they cannot get from the screen.
    #[test]
    fn a_prompt_for_a_line_whose_arguments_vary_names_the_settings_file() {
        let drawn = fully_rendered_run(&a_varying_run());
        assert!(
            drawn.contains("/home/someone/.bravebot/settings.json"),
            "the prompt advised a pattern without saying which file holds one: {drawn}"
        );
    }

    /// RUN-20: a pattern grants more than any key on this screen, so the advice carries what it
    /// costs. Advice that named only the relief would have somebody widening a grant on the
    /// strength of a sentence that described half of it.
    #[test]
    fn a_prompt_for_a_line_whose_arguments_vary_says_what_a_pattern_costs() {
        let drawn = fully_rendered_run(&a_varying_run());
        assert!(
            drawn.contains("covers lines nobody has read"),
            "the advice left out what a pattern reaches that no key here does: {drawn}"
        );
        assert!(
            drawn.contains("stays quarantined"),
            "the advice left out that a pattern makes nothing readable: {drawn}"
        );
    }

    /// RUN-20: no key here covers a family, so the advice must not read as one being offered. The
    /// keys on the row are the same four whether the advice is drawn or not.
    #[test]
    fn advising_a_pattern_offers_no_key_that_grants_one() {
        let drawn = fully_rendered_run(&a_varying_run());
        assert!(
            !drawn.contains("git commit *"),
            "the prompt put a pattern on screen for somebody to accept: {drawn}"
        );
        assert_eq!(
            run_answer_for(
                KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE),
                &a_varying_run()
            ),
            None,
            "a key granted the family the advice says a file has to be edited for"
        );
    }

    /// RUN-20: the advice is for the line whose arguments move, and saying it on every prompt would
    /// be noise that hides the case it is for. A first prompt has nothing to compare against.
    #[test]
    fn a_prompt_for_a_line_nothing_has_varied_says_nothing_about_a_pattern() {
        let drawn = fully_rendered_run(&a_recordable_run());
        assert!(
            !drawn.contains("settings.json"),
            "a prompt advised a pattern for a line that repeats exactly: {drawn}"
        );
    }

    /// Enter is the key most likely to be pressed out of habit, and this prompt starts a program.
    #[test]
    fn enter_does_not_approve_a_run() {
        assert_eq!(
            run_answer_for(
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                &a_run(false)
            ),
            None
        );
    }

    /// Interrupting refuses and vouches for nothing: a turn being stopped is not consent to what
    /// it was stopped at, let alone standing consent.
    #[test]
    fn ctrl_c_refuses_the_run_and_vouches_for_nothing() {
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(
            run_answer_for(key, &a_run(false)),
            Some(RunResponse::Answer(RunAnswer::Interrupt))
        );
        let decision = RunAnswer::Interrupt.decision();
        assert!(!decision.approved());
        assert!(!decision.remember);
    }

    /// Saying no refuses this run without vouching for anything or stopping the turn.
    #[test]
    fn saying_no_to_a_run_vouches_for_nothing() {
        let key = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        assert_eq!(
            run_answer_for(key, &a_run(false)),
            Some(RunResponse::Answer(RunAnswer::Reject))
        );
        assert!(!RunAnswer::Reject.decision().remember);
    }

    fn a_vetting(verdict: Verdict, reason: Option<&str>, content: &str) -> VetRequest {
        VetRequest {
            origin: "example.com/notes".into(),
            expects: "the release notes for version 2".into(),
            content: content.into(),
            verdict,
            reason: reason.map(str::to_string),
        }
    }

    fn rendered_vet(request: &VetRequest) -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 40)).expect("terminal");
        terminal
            .draw(|frame| {
                draw_vet(frame, request, 0);
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
                    &request
                ),
                Some(VetResponse::Answer(VetAnswer::Approve)),
                "{verdict}"
            );
            assert_eq!(
                vet_answer_for(
                    KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE),
                    &request
                ),
                Some(VetResponse::Answer(VetAnswer::Reject)),
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
                vet_answer_for(key, &a_vetting(verdict, None, "a page")),
                None,
                "{verdict} bound the key that turns the asking off"
            );
        }
        assert_eq!(
            vet_answer_for(key, &a_vetting(Verdict::Safe, None, "a page")),
            Some(VetResponse::Answer(VetAnswer::ApproveAlways)),
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
                vet_answer_for(key, &a_vetting(verdict, None, "a page")),
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

    fn an_output(text: &str) -> OutputRequest {
        OutputRequest {
            command: "find /Applications -name 'Brave Browser Nightly.app'".into(),
            output: text.into(),
            reference: "ref:5".into(),
            verdict: Verdict::Safe,
            reason: None,
        }
    }

    fn rendered_output(request: &OutputRequest) -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).expect("terminal");
        terminal
            .draw(|frame| {
                draw_output(frame, request, 0);
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

    /// A command that printed nothing is a fact worth stating. An empty box reads as a prompt that
    /// failed to render, and the reviewer would be answering about nothing.
    #[test]
    fn output_that_is_empty_says_so() {
        assert!(rendered_output(&an_output("")).contains("printed nothing"));
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
            output_answer_for(pressed, &an_output("Darwin")),
            Some(VetResponse::Answer(VetAnswer::ApproveAlways)),
            "a safe verdict did not bind the standing key"
        );

        for verdict in [
            Verdict::Unsafe,
            Verdict::Inconclusive("the check could not be made"),
        ] {
            let mut request = an_output("Darwin");
            request.verdict = verdict;
            assert_eq!(
                output_answer_for(pressed, &request),
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
                    &request
                ),
                Some(VetResponse::Answer(VetAnswer::Approve)),
                "{verdict}"
            );
            assert_eq!(
                output_answer_for(
                    KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE),
                    &request
                ),
                Some(VetResponse::Answer(VetAnswer::Reject)),
                "{verdict}"
            );
        }
    }

    /// The prompt blocks everything else, so Ctrl-C must be answerable here too. It stops the
    /// turn rather than only refusing the write: a user reaching for the interrupt wants the
    /// work to stop.
    #[test]
    fn ctrl_c_refuses_the_write_and_stops_the_turn() {
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(answer_for(key), Some(Response::Answer(Answer::Interrupt)));
        assert_eq!(Answer::Interrupt.decision(), Decision::Reject);
        assert!(
            Answer::Interrupt.stops_the_turn(),
            "the interrupt refused the write and left the turn running"
        );
    }

    /// Refusing one write leaves the turn running, which is what makes it different from
    /// interrupting. The decision the turn is told is `Reject` either way, so the key mapping
    /// alone says nothing about which of the two happened: what separates them is here.
    #[test]
    fn saying_no_does_not_stop_the_turn() {
        let key = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        assert_eq!(answer_for(key), Some(Response::Answer(Answer::Reject)));
        assert_eq!(Answer::Reject.decision(), Decision::Reject);
        assert!(
            !Answer::Reject.stops_the_turn(),
            "saying no to one write ended the turn"
        );
    }

    /// The same at the run prompt, which has three ways of approving and one of refusing before
    /// the interrupt. Every one of them leaves the turn running, so a person who declines a
    /// command keeps the work that was going to use it.
    #[test]
    fn only_the_interrupt_stops_the_turn_at_a_run_prompt() {
        for answer in [
            RunAnswer::Approve,
            RunAnswer::ApproveAlways,
            RunAnswer::ApproveAndRecord,
            RunAnswer::Reject,
        ] {
            assert!(
                !answer.stops_the_turn(),
                "{answer:?} ended the turn that asked"
            );
        }
        assert!(RunAnswer::Interrupt.stops_the_turn());
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
            .draw(|frame| furthest = draw(frame, &request, 0))
            .expect("draw");
        assert!(furthest > 0, "a 200 line body reported nothing to scroll");

        terminal
            .draw(|frame| {
                draw(frame, &request, furthest);
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

    /// The reason this exists: a one-line change to a large file must show that one line
    /// rather than a screenful of unchanged text.
    #[test]
    fn a_small_edit_in_a_large_file_shows_only_the_change() {
        let before: String = (0..300).map(|n| format!("line {n}\n")).collect::<String>();
        let after = before.replace("line 150\n", "line 150 changed\n");

        let output = rendered(&WriteRequest {
            path: "src/main.rs".into(),
            contents: after,
            existing: Some(before),
            intent: Intent::Edit,
            untrusted: false,
            remark: None,
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
            path: "game.js".into(),
            contents: "const SPEED = 50;\n".into(),
            existing: Some("const SPEED = 100;\n".into()),
            intent: Intent::Overwrite,
            untrusted: true,
            remark: None,
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
            path: "src/main.rs".into(),
            contents: after,
            existing: Some(before),
            intent: Intent::Overwrite,
            untrusted: false,
            remark: None,
        });

        assert!(
            output.contains("too large to show"),
            "an uncomputable diff rendered as nothing: {output}"
        );
    }

    /// A prompt that panics on a small terminal takes the session with it, and one that drops the
    /// question is worse: it blocks everything else while showing nothing to answer, and a key
    /// pressed at it answers a question that was never on the screen. So the small case is held to
    /// what it asks about and the key that answers, not merely to surviving the draw.
    #[test]
    fn a_tiny_terminal_still_renders_the_prompt() {
        let mut terminal = Terminal::new(TestBackend::new(20, 8)).expect("terminal");
        terminal
            .draw(|frame| {
                draw(frame, &request("x", None), 0);
            })
            .expect("must not panic on a small area");

        let drawn: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(
            drawn.contains("src/main.rs"),
            "the prompt did not say what it was asking about: {drawn}"
        );
        assert!(
            drawn.contains("write it"),
            "the key that approves the write was drawn out of view: {drawn}"
        );
    }

    /// The bar the renderer draws down the margin.
    const BAR: char = '\u{2503}';

    /// A quarantined file the model asked to read, as `read_file` offers one: the head of the file,
    /// and the word a check said about the whole of it.
    fn a_vouch(path: &str, preview: impl Into<String>, truncated: bool) -> VouchRequest {
        VouchRequest {
            path: path.into(),
            preview: preview.into(),
            truncated,
            verdict: Verdict::Safe,
            reason: None,
        }
    }

    /// The prompt as drawn rows.
    ///
    /// Rows rather than one flattened string, because a margin is a claim about where a row
    /// *starts*, and a buffer joined end to end cannot tell a continuation row from the line it
    /// continues.
    /// The sentences these panels put above the body are indented by two columns, and a
    /// translation is not the length the English is, so they have to wrap without losing the
    /// indent. Broken by hand into lines that fit, as they were, the second row of a longer
    /// translation would start hard against the border and read as part of the body.
    #[test]
    fn explanatory_prose_keeps_its_indent_on_every_row_it_wraps_to() {
        let request = a_vouch("notes.md", "some contents", false);
        // Narrow enough that the sentence cannot fit on one row.
        let drawn = rows_of(52, 24, |frame| {
            draw_vouch(frame, &request, 0);
        });

        let wrapped: Vec<&String> = drawn
            .iter()
            .filter(|row| {
                row.contains("working blind")
                    || row.contains("rest of this session")
                    || row.contains("later read")
            })
            .collect();
        assert!(
            wrapped.len() > 1,
            "the sentence did not wrap, so this proves nothing: {drawn:#?}"
        );
        // The panel is centred, so the box's own left border is where the indent is measured
        // from rather than the start of the terminal row.
        for row in wrapped {
            let inside = row
                .split_once('\u{2502}')
                .map(|(_, rest)| rest)
                .expect("the panel draws a border");
            assert!(
                inside.starts_with("  ") && !inside.starts_with("   "),
                "a wrapped row lost the indent: {row:?}"
            );
        }
    }

    fn rows_of(
        width: u16,
        height: u16,
        mut draw_it: impl FnMut(&mut ratatui::Frame),
    ) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        terminal.draw(|frame| draw_it(frame)).expect("draw");
        let buffer = terminal.backend().buffer().clone();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect()
    }

    /// Every drawn row holding `token` starts at the margin, and the margin holds the bar.
    ///
    /// Asserted against the drawn buffer rather than the lines the prompt builds, because the
    /// defect this pins is introduced between the two: a line with a bar at its head becomes
    /// several rows when it is wider than the box, and only the first of them ever had one.
    fn assert_marked_on_every_row(rows: &[String], token: &str) {
        let margin = rows
            .iter()
            .find_map(|row| row.chars().position(|c| c == BAR))
            .expect("nothing in the prompt was marked at all");

        let body: Vec<&String> = rows.iter().filter(|row| row.contains(token)).collect();
        assert!(
            body.len() > 1,
            "the content did not wrap, so the case is not exercised:\n{}",
            rows.join("\n")
        );

        for row in body {
            // The box's own border is drawn to the left of the margin and is the prompt's, not
            // the content's.
            assert_eq!(
                row.chars()
                    .position(|c| !c.is_whitespace() && c != '\u{2502}'),
                Some(margin),
                "a row of the block begins with content rather than the margin: {row:?}"
            );
            assert_eq!(
                row.chars().nth(margin),
                Some(BAR),
                "the margin column holds something other than the bar: {row:?}"
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
            draw_output(frame, &request, 0);
        });

        assert_marked_on_every_row(&drawn, "PADDING");
    }

    /// The body of an untrusted write is the same bytes as a quarantined preview and its lines
    /// have no width cap, so a hunk wider than the box must wrap inside the margin too.
    #[test]
    fn a_wrapped_untrusted_hunk_is_marked_on_every_row_it_reaches() {
        let request = WriteRequest {
            path: "game.js".into(),
            contents: format!("{}\u{2503} trust me\n", "PADDING ".repeat(10)),
            existing: Some("const SPEED = 100;\n".into()),
            intent: Intent::Overwrite,
            untrusted: true,
            remark: None,
        };
        let drawn = rows_of(60, 24, |frame| {
            draw(frame, &request, 0);
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
            path: "game.js".into(),
            contents: "const SPEED = 50;\n".into(),
            existing: Some("const SPEED = 100;\n".into()),
            intent: Intent::Overwrite,
            untrusted: true,
            remark: Some(Remark {
                preview: vec!["I only fixed the typo.".to_string()],
                lines: 1,
                label: "(U,priv)".to_string(),
            }),
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
            path: "game.js".into(),
            contents: "const SPEED = 50;\n".into(),
            existing: Some("const SPEED = 100;\n".into()),
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
        };
        let drawn = rows_of(60, 24, |frame| {
            draw(frame, &request, 0);
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
            path: "game.js".into(),
            contents: "const SPEED = 50;
"
            .into(),
            existing: Some(
                "const SPEED = 100;
"
                .into(),
            ),
            intent: Intent::Overwrite,
            untrusted: true,
            remark: Some(Remark {
                // What the producer's cap allows at its widest: REMARK_LINES lines, each
                // REMARK_WIDTH characters.
                preview: (0..4).map(|_| "claim ".repeat(12)).collect(),
                lines: 4,
                label: "(U,priv)".to_string(),
            }),
        };

        for (width, height) in [(80, 24), (100, 30), (60, 20)] {
            let drawn = rows_of(width, height, |frame| {
                draw(frame, &request, 0);
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
    }

    /// Neutralised rather than dropped, as everywhere else: a remark that could clear the line
    /// the margin was drawn on would erase the one mark it can never imitate.
    #[test]
    fn a_control_character_in_a_remark_is_replaced() {
        let request = WriteRequest {
            path: "game.js".into(),
            contents: "const SPEED = 50;\n".into(),
            existing: Some("const SPEED = 100;\n".into()),
            intent: Intent::Overwrite,
            untrusted: true,
            remark: Some(Remark {
                preview: vec!["before\u{1b}[2Kafter".to_string()],
                lines: 1,
                label: "(U,priv)".to_string(),
            }),
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

    /// `Clear` empties cells without colouring them, so a panel that painted only its border came
    /// out as a hole in the palette: themed border, terminal-default everything else, with the
    /// themed transcript still drawn around it. These four screens are the only place a person
    /// authorises anything, and the frame around untrusted content is what they read when they
    /// decide, so it has to be wholly the theme's.
    ///
    /// All five, because the panel they share is only shared until somebody adds a sixth.
    #[test]
    fn every_prompt_paints_the_themes_background_inside_its_border() {
        let write = request("fn main() {}", None);
        let run = a_run(false);
        let output = an_output("Darwin\n");
        let vouch = a_vouch("notes.md", "some contents", false);
        let plan = a_plan(&["1. [fetch] read notes.md into notes"]);

        let _held = theme::exclusive();
        let theme = theme::find("nord").expect("nord is built in");
        theme::apply(&theme);
        let painted = theme::background();
        assert!(
            theme::paints_background(),
            "the theme under test leaves the terminal's own background alone"
        );

        // Collected while the theme is in force and asserted after it is put back, so a failing
        // assertion does not leave nord behind for whatever runs next.
        let unpainted = [
            (
                "write",
                unpainted_cell(painted, |frame| {
                    draw(frame, &write, 0);
                }),
            ),
            (
                "run",
                unpainted_cell(painted, |frame| {
                    draw_run(frame, &run, 0);
                }),
            ),
            (
                "output",
                unpainted_cell(painted, |frame| {
                    draw_output(frame, &output, 0);
                }),
            ),
            (
                "vouch",
                unpainted_cell(painted, |frame| {
                    draw_vouch(frame, &vouch, 0);
                }),
            ),
            (
                "plan",
                unpainted_cell(painted, |frame| {
                    draw_manifest(frame, &plan, 0);
                }),
            ),
        ];
        theme::apply_brave();

        for (name, cell) in unpainted {
            assert_eq!(
                cell, None,
                "the {name} prompt left a cell inside its border in the terminal's own colours"
            );
        }
    }

    /// The first cell inside a prompt's border that is not the theme's, or `None` when every one
    /// of them is.
    ///
    /// Every enclosed cell rather than a sample, including the rows the body did not reach: an
    /// unpainted row below the keys is the same hole in the palette as an unpainted one beside them.
    fn unpainted_cell(
        background: Color,
        draw_it: impl FnOnce(&mut ratatui::Frame),
    ) -> Option<(u16, u16, Color, Color)> {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        let mut draw_it = Some(draw_it);
        terminal
            .draw(|frame| {
                if let Some(draw_it) = draw_it.take() {
                    draw_it(frame);
                }
            })
            .expect("draw");

        let buffer = terminal.backend().buffer();
        let inside = centred(*buffer.area());
        (1..inside.height - 1)
            .flat_map(|y| (1..inside.width - 1).map(move |x| (x, y)))
            .find_map(|(x, y)| {
                let cell = buffer
                    .cell((inside.x + x, inside.y + y))
                    .expect("a cell inside the border");
                (cell.bg != background || cell.fg == Color::Reset)
                    .then_some((x, y, cell.bg, cell.fg))
            })
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
            draw_vouch(frame, &request, 0);
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
                    draw_vouch(frame, &request, 0);
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
                draw_vouch(frame, &request, 0);
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
                draw_vouch(frame, &request, 0);
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
                draw_vouch(frame, &request, 0);
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

    fn a_plan(steps: &[&str]) -> ManifestRequest {
        ManifestRequest {
            task: "tidy the notes".into(),
            steps: steps.iter().map(|step| (*step).to_string()).collect(),
        }
    }

    fn rendered_manifest(request: &ManifestRequest) -> String {
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).expect("terminal");
        terminal
            .draw(|frame| {
                draw_manifest(frame, request, 0);
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

    /// One answer covers the whole run, so the whole run is what the prompt shows: every step, and
    /// the task they are meant to serve. A count of steps would be a summary of what is at stake
    /// rather than the thing itself.
    #[test]
    fn the_plan_prompt_shows_the_task_and_every_step() {
        let drawn = rendered_manifest(&a_plan(&[
            "1. [fetch] read notes.md into notes",
            "2. [transform] summarise notes into summary",
            "3. [act] write summary to summary.md",
        ]));

        assert!(drawn.contains("tidy the notes"), "{drawn}");
        assert!(drawn.contains("3 steps"), "{drawn}");
        for step in [
            "read notes.md",
            "summarise notes",
            "write summary to summary.md",
        ] {
            assert!(
                drawn.contains(step),
                "the plan did not show {step}: {drawn}"
            );
        }
    }

    /// Both halves a reader would otherwise guess at, and they pull in opposite directions: a yes
    /// here does not carry the writes inside the plan, and a no costs nothing because the run has
    /// touched nothing yet.
    #[test]
    fn the_plan_prompt_says_what_approving_it_does_and_does_not_do() {
        let drawn = rendered_manifest(&a_plan(&["1. [act] write summary to summary.md"]));

        assert!(drawn.contains("not approving its writes"), "{drawn}");
        assert!(
            drawn.contains("nothing has been read or written yet"),
            "{drawn}"
        );
    }

    /// Enter is the key most likely to be pressed out of habit, and at this prompt it would start a
    /// whole program rather than one effect.
    ///
    /// The mapping is the one the write and output prompts read, which is what `ask_manifest` asks.
    /// The second half is about this prompt in particular: the keys it offers are the two answers,
    /// and it offers no third one for a reader to reach for without deciding.
    #[test]
    fn enter_does_not_approve_a_plan() {
        assert_eq!(
            answer_for(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            None
        );

        let drawn = rendered_manifest(&a_plan(&["1. [act] write summary to summary.md"]));
        assert!(drawn.contains("run it"), "{drawn}");
        assert!(drawn.contains("don't"), "{drawn}");
        assert!(
            !drawn.to_lowercase().contains("enter"),
            "the plan prompt offers Enter as an answer: {drawn}"
        );
    }

    /// A step below the fold is as binding as the first one, so a plan longer than the box is
    /// scrolled to rather than cut short, and the question stays on screen while it is.
    #[test]
    fn a_long_plan_keeps_the_question_on_screen_and_offers_the_rest() {
        let request = ManifestRequest {
            task: "read everything".into(),
            steps: (0..60)
                .map(|n| format!("{}. [fetch] read file{n}.md into slot{n}", n + 1))
                .collect(),
        };

        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        let mut furthest = 0;
        terminal
            .draw(|frame| furthest = draw_manifest(frame, &request, 0))
            .expect("draw");
        assert!(furthest > 0, "a sixty step plan reported nothing to scroll");

        terminal
            .draw(|frame| {
                draw_manifest(frame, &request, furthest);
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
            drawn.contains("read file59.md"),
            "the last step could not be reached: {drawn}"
        );
        assert!(drawn.contains("run it"), "the question scrolled away");
    }
}
