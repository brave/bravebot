//! The run prompt: the planner's own line, the compiled plan and the driver's sentences.

use bravebot_agent::confirm::{KeyAnswer, RunDecision, RunRequest};
use bravebot_agent::reach::Lasting;
use bravebot_i18n::t;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::theme;

use super::pinned::{Pinned, rows_in};
use super::{Next, centred, indented, next_press, panel, scroll_hint};

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
    /// Run it, and record this line with its number left free ([RUN-20]).
    ///
    /// Its own key rather than a wider reading of `r`, because it covers lines nobody has been
    /// asked about, and the row for it says exactly which parts may change.
    ///
    /// [RUN-20]: ../../../docs/specs/tools/run.md
    ApproveAndRecordFamily,
    /// Run it, and remember the credential scopes the planner requested for this session
    /// ([SANDBOX-27]).
    ///
    /// Neither `a` nor `r` in a narrower form: it stops no asking and vouches for nothing, and the
    /// next plan for the same programs shows the scope as a row to be read.
    ///
    /// [SANDBOX-27]: ../../../docs/specs/sandboxing.md
    KeepReach,
    /// Run it, and remember the same scopes for every session started in this checkout.
    KeepReachEverySession,
    /// Run it with the key a digit chose offered first, and remember that choice for the owner
    /// ([SANDBOX-16]).
    ///
    /// [SANDBOX-16]: ../../../docs/specs/sandboxing.md
    ChooseKey(KeyAnswer),
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
            RunAnswer::ApproveAndRecordFamily => RunDecision::approve_and_record_family(),
            RunAnswer::KeepReach => RunDecision::approve_and_keep_reach(Lasting::ThisSession),
            RunAnswer::KeepReachEverySession => {
                RunDecision::approve_and_keep_reach(Lasting::EverySession)
            }
            RunAnswer::ChooseKey(answer) => RunDecision::approve_with_key(answer),
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
            | RunAnswer::ApproveAndRecordFamily
            | RunAnswer::KeepReach
            | RunAnswer::KeepReachEverySession
            | RunAnswer::ChooseKey(_)
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
///
/// Whether the plan has been on the screen is not asked here: [`RunDrawn::response_to`] asks it.
pub(super) fn run_answer_for(key: KeyEvent, request: &RunRequest) -> Option<RunResponse> {
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
        // Unbound where the prompt did not draw it, for the same reason, and where the table in
        // the core does not list the line.
        KeyCode::Char('f' | 'F') if request.offers_a_family() => {
            Some(RunResponse::Answer(RunAnswer::ApproveAndRecordFamily))
        }
        // Unbound where the prompt did not draw them, for the reason `r` is: each writes a standing
        // grant, and the programs it names are worked out from the plan again here and not taken
        // from the drawing.
        KeyCode::Char('m' | 'M') if request.offers_to_keep_reach() => {
            Some(RunResponse::Answer(RunAnswer::KeepReach))
        }
        // Not `k`: that scrolls up at every prompt, and a person scrolling must not write a grant.
        KeyCode::Char('e' | 'E') if request.offers_to_keep_reach() => {
            Some(RunResponse::Answer(RunAnswer::KeepReachEverySession))
        }
        // Bound only to the digits the prompt numbered, for the reason `r` is: each writes a
        // standing choice, and the key it names is read against the acting layer's own list.
        KeyCode::Char(digit @ '0'..='9') => digit
            .to_digit(10)
            .and_then(|digit| request.key_answer_for(digit))
            .map(|answer| RunResponse::Answer(RunAnswer::ChooseKey(answer))),
        KeyCode::Char('n' | 'N') | KeyCode::Esc => Some(RunResponse::Answer(RunAnswer::Reject)),
        KeyCode::Up | KeyCode::Char('k') => Some(RunResponse::Scroll(-1)),
        KeyCode::Down | KeyCode::Char('j') => Some(RunResponse::Scroll(1)),
        KeyCode::PageUp => Some(RunResponse::Page(-1)),
        KeyCode::PageDown => Some(RunResponse::Page(1)),
        KeyCode::Home => Some(RunResponse::Scroll(i16::MIN)),
        KeyCode::End => Some(RunResponse::Scroll(i16::MAX)),
        // Enter is deliberately not an approval: it is the key most likely to be pressed out of
        // habit, and this prompt starts a program.
        _ => None,
    }
}

/// What a key press did at a run prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RunResponse {
    Answer(RunAnswer),
    Scroll(i16),
    /// Move by this many of the last draw's pages.
    Page(i16),
}

/// What one draw of the run question decided for the keys that answer it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct RunDrawn {
    /// How far the plan can be scrolled.
    pub(super) furthest: u16,
    /// The rows a page moves: one fewer than the body's rows on the screen, so paging from the top
    /// puts every row on the screen. A page longer than the box would pass over rows that a key
    /// then waits on.
    pub(super) page: u16,
    /// Whether the question, the keys and a row of the body are on the screen.
    pub(super) whole: bool,
    /// Rows of the plan not on the screen yet during this question. Every key that runs the line
    /// waits on them.
    pub(super) unread: usize,
    /// Rows saying what `a` grants besides running the line, not on the screen yet.
    pub(super) always_unread: usize,
    /// Rows saying what `r` grants besides running the line, not on the screen yet.
    pub(super) record_unread: usize,
    /// Rows saying what `f` grants besides running the line, not on the screen yet.
    pub(super) family_unread: usize,
    /// Rows saying what `m` and `e` remember, not on the screen yet.
    pub(super) reach_unread: usize,
    /// Rows naming the keys a digit chooses and what choosing one remembers, not on the screen yet.
    pub(super) key_unread: usize,
}

impl RunDrawn {
    /// Whether this draw takes `answer`. A key that runs the line waits on the plan, and `a`,
    /// `r` and `f` each wait on what they grant besides as well. Refusing waits on nothing.
    pub(super) fn takes(&self, answer: RunAnswer) -> bool {
        let grant_unread = match answer {
            RunAnswer::Reject | RunAnswer::Interrupt => return true,
            RunAnswer::Approve => 0,
            RunAnswer::ApproveAlways => self.always_unread,
            RunAnswer::ApproveAndRecord => self.record_unread,
            RunAnswer::ApproveAndRecordFamily => self.family_unread,
            RunAnswer::KeepReach | RunAnswer::KeepReachEverySession => self.reach_unread,
            RunAnswer::ChooseKey(_) => self.key_unread,
        };
        self.whole && self.unread == 0 && grant_unread == 0
    }

    /// One key pressed at the question this draw put on the screen.
    pub(super) fn response_to(&self, key: KeyEvent, request: &RunRequest) -> Option<RunResponse> {
        match run_answer_for(key, request) {
            Some(RunResponse::Answer(answer)) if !self.takes(answer) => None,
            response => response,
        }
    }

    /// Where the plan starts once moved `by` rows from `scroll`.
    ///
    /// Moved from where this draw put it rather than from `scroll`, which a terminal made taller
    /// since can leave past the bottom, where a press of Up would move nothing.
    pub(super) fn moved(&self, scroll: u16, by: i32) -> u16 {
        let from = i32::from(scroll.min(self.furthest));
        let to = from.saturating_add(by).clamp(0, i32::from(self.furthest));
        u16::try_from(to).unwrap_or(self.furthest)
    }
}

/// Which rows of a run question's body have been on the screen during one question.
///
/// Kept across draws rather than read off the last one, because a plan longer than the box is read
/// a part at a time: what a key that runs it needs is that each row was on the screen, not all at
/// once. Counted at the width they were wrapped to, since another width wraps the body into other
/// rows and a row read at the old one is not a row of the new.
#[derive(Debug, Default)]
pub(super) struct RowsShown {
    pub(super) width: u16,
    pub(super) rows: Vec<bool>,
}

impl RowsShown {
    /// Marks the rows `laid` puts on the screen, of a body wrapped into `total` rows at `width`.
    pub(super) fn mark(&mut self, width: u16, total: usize, laid: &Pinned) {
        if self.width != width || self.rows.len() != total {
            *self = RowsShown {
                width,
                rows: vec![false; total],
            };
        }
        // A draw that cuts off the question or its keys leaves the body no rows, so a row marked
        // here was on the screen under both.
        let from = usize::from(laid.offset).min(total);
        let to = (usize::from(laid.offset) + usize::from(laid.body.height)).min(total);
        self.rows[from..to].fill(true);
    }

    /// How many of these rows have not been on the screen.
    pub(super) fn unseen(&self, rows: std::ops::Range<usize>) -> usize {
        self.rows
            .get(rows)
            .map_or(0, |rows| rows.iter().filter(|shown| !**shown).count())
    }
}

/// Draw the prompt for a run and wait for an answer.
///
/// Standalone as well as available through [`TerminalConfirmer`], for the same reason the write
/// prompt is: a turn on a worker thread cannot hold the terminal, so the main thread calls this on
/// its behalf.
pub fn ask_run<B: Backend>(terminal: &mut Terminal<B>, request: &RunRequest) -> RunAnswer {
    let mut scroll = 0u16;
    let mut shown = RowsShown::default();
    loop {
        let mut drawn = RunDrawn::default();
        // A terminal that cannot be drawn to cannot carry a question, so refuse rather than run
        // something unseen.
        if terminal
            .draw(|frame| drawn = draw_run(frame, request, scroll, &mut shown))
            .is_err()
        {
            return RunAnswer::Reject;
        }

        match next_press() {
            Next::Skip => continue,
            // Losing the event stream must not run anything.
            Next::Lost => return RunAnswer::Reject,
            Next::Press(key) => match drawn.response_to(key, request) {
                Some(RunResponse::Answer(answer)) => return answer,
                Some(RunResponse::Scroll(by)) => scroll = drawn.moved(scroll, i32::from(by)),
                Some(RunResponse::Page(by)) => {
                    scroll = drawn.moved(scroll, i32::from(by) * i32::from(drawn.page));
                }
                None => continue,
            },
        }
    }
}

/// What one ambient authority is, in the words a person reads.
///
/// The sentence is per authority rather than one sentence with a name substituted into it,
/// because what each of them costs is different: a container daemon is root on this machine, a
/// logged-in tool is an account elsewhere, the agent is a signature, the metadata service is a
/// role. The word that named it is the driver's own, from the table that recognised it, so
/// nothing of the command line is put into this sentence.
pub(super) fn authority(spent: &bravebot_core::ambient::Spent) -> String {
    let named = spent.named;
    match spent.authority {
        bravebot_core::ambient::Authority::ContainerDaemon => {
            t!(run_authority_container, named = named)
        }
        bravebot_core::ambient::Authority::LoggedInTool => {
            t!(run_authority_logged_in, named = named)
        }
        bravebot_core::ambient::Authority::AgentSocket => t!(run_authority_agent, named = named),
        bravebot_core::ambient::Authority::MetadataService => {
            t!(run_authority_metadata, named = named)
        }
    }
    .to_string()
}

/// Draw the run confirmation, marking in `shown` the rows of the body this draw put on the screen.
///
/// One line per stage, rendered by [`bravebot_core::Stage::display`], which quotes unambiguously: two
/// different argument vectors cannot come out looking alike, so what the reviewer reads names
/// exactly the argv the endorsement will be bound to.
///
/// Drawn is not enough where the plan is longer than the box, since an argument below the bottom
/// edge runs as surely as the first. So the keys and the row saying how much is unread are pinned
/// where the body cannot push them off, and a key is taken only once every row it waits on has
/// been on the screen.
pub(super) fn draw_run(
    frame: &mut ratatui::Frame,
    request: &RunRequest,
    scroll: u16,
    shown: &mut RowsShown,
) -> RunDrawn {
    let area = centred(frame.area());
    let inside = panel(frame, area, theme::accent(), t!(run_title));

    // Worked out once and read by both the explanation and the key row, so a drawing cannot offer a
    // key it has just said is unavailable. [`run_answer_for`] asks the same question again rather
    // than being told the answer, because a grant must not rest on a drawing.
    let offers_always = request.can_be_remembered();

    let steps = request.plan.steps();
    let header = vec![
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
    let mut lines = plan_rows(request);
    lines.push(Line::raw(""));
    lines.extend(effect_rows(request, inside.width as usize));

    let wrapped = |lines: Vec<Line<'static>>| Paragraph::new(lines).wrap(Wrap { trim: false });
    // Each line wraps on its own, so the rows of a run of lines are where measuring before and
    // after it puts them.
    let measure = |lines: &[Line<'static>]| wrapped(lines.to_vec()).line_count(inside.width);
    // Everything above is what running the line does, so it is what every key that runs it waits
    // on. What `a` and `r` grant on top of that is below, and each of them waits on its own part.
    let plan = 0..measure(&lines);
    let mut always = 0..0;
    let mut record = 0..0;
    let mut family = 0..0;
    let mut reach = 0..0;
    let mut key = 0..0;

    // What `a` would actually grant, in as many words. It is two things, not one, and the second
    // is the one nothing else in the interface would tell them: what the command prints stops
    // being quarantined and the model reads it. Nothing checks that assertion, so the person
    // making it has to be asked for it in those terms.
    lines.push(Line::raw(""));
    let from = measure(&lines);
    lines.extend(always_rows(request, offers_always));
    if offers_always {
        always = from..measure(&lines);
    }

    // What `r` would grant, where it is offered. Three things a reader cannot get from the key's
    // one word: that it lasts past this session, that every session in this directory reads it, and
    // that it stops the asking without making anything readable. The place it is written down is
    // part of the grant rather than a footnote, because nobody can endorse a record they were not
    // shown, and deleting the line from that file is the way back.
    if let Some(path) = request.record.as_ref().filter(|_| request.may_record()) {
        lines.push(Line::raw(""));
        let from = measure(&lines);
        lines.extend(record_rows(path));
        record = from..measure(&lines);
    }

    // What `f` would grant, where the table lists the line: the same record, with the number left
    // free. The line is drawn as the entry will hold it, so the reader sees the slot, and the
    // sentence under it names what stays fixed, because "any number" is the half a person is most
    // likely to read as wider than it is.
    if let Some(family_line) = request.family_display() {
        lines.push(Line::raw(""));
        let from = measure(&lines);
        lines.extend(family_rows(&family_line));
        family = from..measure(&lines);
    }

    // What `m` and `e` would remember, where they are offered: the programs by name, the scopes, and
    // the place the record is written. The second sentence is the half a person is most likely to
    // assume the other way round, since the plan they are looking at asked for the scope: it is
    // asked about again next time, with the scope on it, and nothing is vouched for.
    let kept = request.kept_reach_shapes();
    if let Some(path) = request.reach_record.as_ref().filter(|_| !kept.is_empty()) {
        lines.push(Line::raw(""));
        let from = measure(&lines);
        lines.extend(reach_rows(request, &kept, path));
        reach = from..measure(&lines);
    }

    // The keys a digit chooses between for the owner this push leaves to ssh, numbered as the
    // digits are, with what `y` does instead and where the choice is written. The owner is the
    // host and account the argv names, which the plan above already shows.
    if let Some(choice) = &request.key_choice {
        lines.push(Line::raw(""));
        let from = measure(&lines);
        lines.extend(key_choice_rows(choice, inside.width as usize));
        key = from..measure(&lines);
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
        lines.extend(pattern_rows(path));
    }

    let total = measure(&lines);
    let header = wrapped(header);
    let body = wrapped(lines);
    // Measured with the keys live, which is the same text: whether they are taken changes how they
    // are coloured and not how many rows they take, so it can be decided after the layout.
    let laid = Pinned::new(
        inside,
        (
            rows_in(&header, inside.width),
            u16::try_from(total).unwrap_or(u16::MAX),
            0,
            rows_in(
                &wrapped(vec![run_keys(request, offers_always, |_| true)]),
                inside.width,
            ),
        ),
        scroll,
    );
    // Rows past the most the layout scrolls to are never marked, so a body that long is never
    // taken rather than taken unread.
    shown.mark(inside.width, total, &laid);
    let drawn = RunDrawn {
        furthest: laid.furthest,
        page: laid.body.height.saturating_sub(1).max(1),
        whole: laid.whole && laid.body.height > 0,
        unread: shown.unseen(plan),
        always_unread: shown.unseen(always),
        record_unread: shown.unseen(record),
        family_unread: shown.unseen(family),
        reach_unread: shown.unseen(reach),
        key_unread: shown.unseen(key),
    };

    // Said in place of how far there is to scroll while a key waits on it, since what the person
    // needs to know then is why the key does nothing.
    let waiting = drawn.always_unread
        + drawn.record_unread
        + drawn.family_unread
        + drawn.reach_unread
        + drawn.key_unread;
    let hint = if drawn.unread > 0 {
        Line::styled(
            format!("   {}", t!(run_unseen, count = drawn.unread)),
            Style::default().fg(theme::running()),
        )
    } else if waiting > 0 {
        Line::styled(
            format!("   {}", t!(run_grant_unseen, count = waiting)),
            Style::default().fg(theme::running()),
        )
    } else {
        Line::styled(
            scroll_hint(laid.furthest - laid.offset),
            Style::default().fg(theme::brand_primary()),
        )
    };
    laid.render(
        frame,
        header,
        body,
        hint,
        wrapped(Vec::new()),
        wrapped(vec![run_keys(request, offers_always, |answer| {
            drawn.takes(answer)
        })]),
    );

    drawn
}

/// The rows naming what the plan runs: the line the planner wrote, one row pair per stage, the files
/// it writes and the reference fed to its first program.
fn plan_rows(request: &RunRequest) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let steps = request.plan.steps();
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
            format!("       {}", step.binary()),
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
    lines
}

/// The rows saying what confinement the line runs under, whether the planner asked to leave it,
/// which ambient authority a yes hands over and whether the output reaches the model.
fn effect_rows(request: &RunRequest, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    // Said every time, because it is the thing a reviewer is most likely to assume otherwise. Where
    // the turn confines what it starts, the programs are held to the directories listed and to what
    // each stage brings; where it does not, they run with the access the user's own shell has.
    match &request.confined {
        Some(confined) => {
            lines.extend(indented(
                confined.heading(),
                Style::default().fg(theme::running()),
                width,
            ));
            for directory in &confined.directories {
                lines.push(Line::from(Span::styled(
                    format!("       {}", directory.display()),
                    Style::default()
                        .fg(theme::text())
                        .add_modifier(Modifier::BOLD),
                )));
            }
            for sentence in confined.sentences() {
                lines.extend(indented(
                    sentence,
                    Style::default().fg(theme::running()),
                    width,
                ));
            }
        }
        None => lines.push(Line::from(Span::styled(
            format!("  {}", t!(run_not_sandboxed)),
            Style::default().fg(theme::running()),
        ))),
    }
    // The planner's request, which the answer is to. Drawn in the colour of a failure, since the
    // line is about to be given what the sandbox was holding back.
    if request.unconfined {
        lines.extend(indented(
            t!(run_unconfined),
            Style::default().fg(theme::fail()),
            width,
        ));
    }

    // Which access in particular a yes hands over, where the line reaches one nothing here holds:
    // a container daemon, a tool already logged in, the ssh agent, the metadata service. The line
    // above says what confinement there is and is said every time; this says what is being
    // granted, which is the half a person cannot read off an argument list. Said only where there
    // is something to say, so it never becomes a row that is always there.
    let spends = request.ambient_authority();
    if !spends.is_empty() {
        // Wrapped with the indent carried down, like every other sentence in this panel that runs
        // past the border: a second row starting at the border reads as a line of its own.
        lines.extend(indented(
            t!(run_spends_authority),
            Style::default().fg(theme::fail()),
            width,
        ));
        for spent in &spends {
            lines.push(Line::from(Span::styled(
                format!("       {}", authority(spent)),
                Style::default()
                    .fg(theme::text())
                    .add_modifier(Modifier::BOLD),
            )));
        }
    }

    // The second and independent reason to be careful, on confidentiality rather than integrity.
    // Only said when it applies, so it does not become noise that hides the case it is for.
    if request.releases_private() {
        lines.push(Line::from(Span::styled(
            format!("  {}", t!(run_releases_private)),
            Style::default().fg(theme::fail()),
        )));
    }
    lines
}

/// The rows for `a`: what it grants where it is offered, or every reason there is no `a`.
fn always_rows(request: &RunRequest, offers_always: bool) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if offers_always {
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
        if request.feeds_a_reference() {
            why.push(t!(run_stdin_not_remembered));
        }
        if request.asks_for_scopes() {
            why.push(t!(run_scopes_not_remembered));
        }
        if request.unconfined {
            why.push(t!(run_unconfined_not_remembered));
        }
        for reason in why {
            lines.push(Line::from(Span::styled(
                format!("  {reason}"),
                Style::default().fg(theme::muted()),
            )));
        }
    }
    lines
}

/// The rows for `r`: what it records and where.
fn record_rows(path: &std::path::Path) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
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
    lines
}

/// The rows for `f`: the line with its number left free and what stays fixed.
fn family_rows(family_line: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        format!("  {}", t!(run_remember_family_explained)),
        Style::default().fg(theme::muted()),
    )));
    lines.push(Line::from(Span::styled(
        format!("       {family_line}"),
        Style::default().add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        format!("     {}", t!(run_remember_family_only_number)),
        Style::default().fg(theme::running()),
    )));
    lines
}

/// The rows for `m` and `e`: the programs and scopes they keep, and where the record is written.
fn reach_rows(request: &RunRequest, kept: &[String], path: &std::path::Path) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        format!("  {}", t!(run_keep_reach_explained)),
        Style::default().fg(theme::muted()),
    )));
    for shape in kept {
        lines.push(Line::from(Span::styled(
            format!("       {shape}"),
            Style::default().add_modifier(Modifier::BOLD),
        )));
    }
    lines.push(Line::from(Span::styled(
        format!(
            "     {}",
            t!(
                run_keep_reach_scopes,
                scopes = request
                    .requested_credential_scopes()
                    .iter()
                    .map(|scope| scope.name())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        ),
        Style::default().fg(theme::running()),
    )));
    lines.push(Line::from(Span::styled(
        format!("     {}", t!(run_keep_reach_still_asked)),
        Style::default().fg(theme::muted()),
    )));
    lines.push(Line::from(Span::styled(
        format!("     {}", t!(run_keep_reach_lifetimes)),
        Style::default().fg(theme::muted()),
    )));
    let asked_again = request.requested_not_remembered();
    if !asked_again.is_empty() {
        lines.push(Line::from(Span::styled(
            format!(
                "     {}",
                t!(run_keep_reach_asked_again, names = asked_again.join(", "))
            ),
            Style::default().fg(theme::muted()),
        )));
    }
    lines.push(Line::from(Span::styled(
        format!("     {}", t!(run_keep_reach_where)),
        Style::default().fg(theme::muted()),
    )));
    lines.push(Line::from(Span::styled(
        format!("       {}", path.display()),
        Style::default().add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        format!("     {}", t!(run_keep_reach_undo)),
        Style::default().fg(theme::muted()),
    )));
    lines
}

/// The rows for the digit keys: the candidate keys, what `y` does instead and where the choice is written.
fn key_choice_rows(
    choice: &bravebot_agent::confirm::KeyChoice,
    width: usize,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.extend(indented(
        t!(
            run_key_choice_explained,
            first = "1",
            last = choice.candidates.len().to_string(),
            owner = choice.owner()
        ),
        Style::default().fg(theme::muted()),
        width,
    ));
    for (at, candidate) in choice.candidates.iter().enumerate() {
        lines.push(Line::from(Span::styled(
            format!("       {}  {}", at + 1, candidate.display()),
            Style::default().add_modifier(Modifier::BOLD),
        )));
    }
    lines.push(Line::from(Span::styled(
        format!("       0  {}", t!(run_key_choice_default)),
        Style::default().add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        format!("     {}", t!(run_key_choice_yes)),
        Style::default().fg(theme::running()),
    )));
    lines.push(Line::from(Span::styled(
        format!("     {}", t!(run_keep_reach_where)),
        Style::default().fg(theme::muted()),
    )));
    lines.push(Line::from(Span::styled(
        format!("       {}", choice.record.display()),
        Style::default().add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        format!("     {}", t!(run_key_choice_undo)),
        Style::default().fg(theme::muted()),
    )));
    lines
}

/// The rows for the pattern box: where to write one and what it covers.
fn pattern_rows(path: &std::path::Path) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
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
    lines
}

/// The run question's keys, with each that `live` says is not taken drawn muted. A key not taken
/// yet is muted rather than dropped, so the row keeps its shape and the row above it says why.
fn run_keys(
    request: &RunRequest,
    offers_always: bool,
    live: impl Fn(RunAnswer) -> bool,
) -> Line<'static> {
    let colour = |answer, colour| if live(answer) { colour } else { theme::muted() };
    let mut key_spans = vec![
        Span::styled(
            "  y",
            Style::default()
                .fg(colour(RunAnswer::Approve, theme::ok()))
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
                .fg(colour(RunAnswer::ApproveAlways, theme::running()))
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
                .fg(colour(RunAnswer::ApproveAndRecord, theme::running()))
                .add_modifier(Modifier::BOLD),
        ));
        key_spans.push(Span::raw(format!(" {}    ", t!(run_remember))));
    }
    if request.offers_a_family() {
        key_spans.push(Span::styled(
            "f",
            Style::default()
                .fg(colour(RunAnswer::ApproveAndRecordFamily, theme::running()))
                .add_modifier(Modifier::BOLD),
        ));
        key_spans.push(Span::raw(format!(" {}    ", t!(run_remember_family))));
    }
    if request.offers_to_keep_reach() {
        for (key, answer, label) in [
            ("m", RunAnswer::KeepReach, t!(run_keep_reach)),
            (
                "e",
                RunAnswer::KeepReachEverySession,
                t!(run_keep_reach_always),
            ),
        ] {
            key_spans.push(Span::styled(
                key,
                Style::default()
                    .fg(colour(answer, theme::running()))
                    .add_modifier(Modifier::BOLD),
            ));
            key_spans.push(Span::raw(format!(" {label}    ")));
        }
    }
    // Every digit waits on the same rows, so the first key's state is every key's.
    if let Some(choice) = &request.key_choice {
        let live_colour = colour(
            RunAnswer::ChooseKey(KeyAnswer::SshDefault),
            theme::running(),
        );
        for (keys, label) in [
            (format!("1-{}", choice.candidates.len()), t!(run_key_choose)),
            ("0".to_string(), t!(run_key_leave)),
        ] {
            key_spans.push(Span::styled(
                keys,
                Style::default()
                    .fg(live_colour)
                    .add_modifier(Modifier::BOLD),
            ));
            key_spans.push(Span::raw(format!(" {label}    ")));
        }
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
    Line::from(key_spans)
}
