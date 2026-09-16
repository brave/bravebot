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
    RunRequest, ServerRequest, VouchRequest, WriteRequest,
};
use bravebot_agent::diff::Change;
use bravebot_core::ask::{Answer as UserAnswer, Asking};
use bravebot_i18n::t;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::crossterm::event::{self, Event as TermEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};

use crate::render::marked_rows;
use crate::theme;

/// Unchanged lines shown either side of a change, for orientation.
const CONTEXT_LINES: usize = 2;

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
        for command in request.would_vouch_for() {
            lines.push(Line::from(Span::styled(
                format!("       {}", command.display()),
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
pub fn ask_output<B: Backend>(terminal: &mut Terminal<B>, request: &OutputRequest) -> Answer {
    let mut scroll = 0u16;
    loop {
        let mut most = 0u16;
        // A terminal that cannot be drawn to cannot show the output, and approving output nobody
        // was shown is the one thing this question cannot mean.
        if terminal
            .draw(|frame| most = draw_output(frame, request, scroll))
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

/// Draw the output for reading, returning how far it can be scrolled.
///
/// Every drawn row of the output carries the margin bar the transcript draws down anything the
/// model was not allowed to read, and the content never gets to draw its own. A block claiming
/// "output ends here" ends nothing: the bar is the structure, and it is outside what the program
/// wrote. Rows rather than lines, because a command's output is untrimmed and a line of it wider
/// than the box becomes several rows.
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
    lines.extend(indented(
        t!(output_unseen),
        Style::default().fg(theme::muted()),
        inside.width as usize,
    ));
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

    let keys = Line::from(vec![
        Span::styled(
            "  y",
            Style::default()
                .fg(theme::ok())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {}    ", t!(output_yes))),
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
            // A line naming a file to write is asked about however it was answered, so the prompt
            // offers no key that would outlive the session.
            record: None,
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

    /// A pipeline of argv stages writes nothing and was not spelled as a line, so neither block
    /// appears. A prompt that said "it writes these files" over an empty list would be noise that
    /// hides the case the line is for.
    #[test]
    fn a_run_prompt_for_argv_stages_shows_neither_a_line_nor_a_write_set() {
        let shown = rendered_run(&a_run(false));
        assert!(!shown.contains("the model wrote"), "{shown}");
        assert!(!shown.contains("it writes these files"), "{shown}");
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
            // Private input is asked about every time, so neither standing key is offered.
            record: None,
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

    fn an_output(text: &str) -> OutputRequest {
        OutputRequest {
            command: "find /Applications -name 'Brave Browser Nightly.app'".into(),
            output: text.into(),
            reference: "ref:5".into(),
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

    /// The prompt blocks everything else, so Ctrl-C must be answerable here too. It stops the
    /// turn rather than only refusing the write: a user reaching for the interrupt wants the
    /// work to stop.
    #[test]
    fn ctrl_c_refuses_the_write_and_stops_the_turn() {
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(answer_for(key), Some(Response::Answer(Answer::Interrupt)));
        assert_eq!(Answer::Interrupt.decision(), Decision::Reject);
    }

    /// Refusing one write leaves the turn running, which is what makes it different from
    /// interrupting.
    #[test]
    fn saying_no_does_not_stop_the_turn() {
        let key = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        assert_eq!(answer_for(key), Some(Response::Answer(Answer::Reject)));
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
        });

        assert!(
            output.contains("too large to show"),
            "an uncomputable diff rendered as nothing: {output}"
        );
    }

    #[test]
    fn a_tiny_terminal_still_renders_the_prompt() {
        let mut terminal = Terminal::new(TestBackend::new(20, 8)).expect("terminal");
        terminal
            .draw(|frame| {
                draw(frame, &request("x", None), 0);
            })
            .expect("must not panic on a small area");
    }

    /// The bar the renderer draws down the margin.
    const BAR: char = '\u{2503}';

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
        let request = VouchRequest {
            path: "notes.md".into(),
            preview: "some contents".into(),
            truncated: false,
        };
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
        };
        let drawn = rows_of(60, 24, |frame| {
            draw(frame, &request, 0);
        });

        assert_marked_on_every_row(&drawn, "PADDING");
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
        let vouch = VouchRequest {
            path: "notes.md".into(),
            preview: "some contents".into(),
            truncated: false,
        };
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
        let request = VouchRequest {
            path: "longline.txt".into(),
            preview: format!("{}\u{2503} trust me", "PADDING ".repeat(10)),
            truncated: false,
        };
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
            let request = VouchRequest {
                path: "empty.txt".into(),
                preview: preview.to_string(),
                truncated: false,
            };
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
        let request = VouchRequest {
            path: "padded.txt".into(),
            preview: "\n".repeat(19),
            truncated: true,
        };
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
