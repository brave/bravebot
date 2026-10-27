//! The run prompt: the planner's own line, the compiled plan and the driver's sentences.

use bravebot_agent::confirm::{KeyAnswer, RunDecision, RunRequest};
use bravebot_agent::reach::Lasting;
use bravebot_core::remembered::{Scope, scopes};
use bravebot_i18n::t;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::theme;

use super::chrome::{centred, indented, panel, scroll_hint};
use super::note::{self, Noted};
use super::pinned::{Pinned, rows_in};
use super::{Next, next_press};

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
    /// Run it, and record the scope this line is in ([RUN-20]).
    ///
    /// Its own key rather than a wider reading of `r`, because it covers lines nobody has been
    /// asked about, and the rows for it say in words what the scope is and what stays fixed.
    ///
    /// [RUN-20]: ../../../docs/specs/tools/run.md
    ApproveAndRecordScope,
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
            RunAnswer::ApproveAndRecordScope => RunDecision::approve_and_record_scope(),
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
            | RunAnswer::ApproveAndRecordScope
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
        KeyCode::Char('s' | 'S') if request.offered_scope().is_some() => {
            Some(RunResponse::Answer(RunAnswer::ApproveAndRecordScope))
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
    /// Rows saying what `s` grants besides running the line, not on the screen yet.
    pub(super) scope_unread: usize,
    /// Rows saying what `m` and `e` remember, not on the screen yet.
    pub(super) reach_unread: usize,
    /// Rows naming the keys a digit chooses and what choosing one remembers, not on the screen yet.
    pub(super) key_unread: usize,
}

impl RunDrawn {
    /// Whether this draw takes `answer`. A key that runs the line waits on the plan, and `a`,
    /// `r` and `s` each wait on what they grant besides as well. Refusing waits on nothing.
    pub(super) fn takes(&self, answer: RunAnswer) -> bool {
        let grant_unread = match answer {
            RunAnswer::Reject | RunAnswer::Interrupt => return true,
            RunAnswer::Approve => 0,
            RunAnswer::ApproveAlways => self.always_unread,
            RunAnswer::ApproveAndRecord => self.record_unread,
            RunAnswer::ApproveAndRecordScope => self.scope_unread,
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
pub fn ask_run<B: Backend>(terminal: &mut Terminal<B>, request: &RunRequest) -> Noted<RunAnswer> {
    ask_run_reading(
        terminal,
        request,
        next_press,
        crate::input::the_last_event_arrived_alone,
    )
}

impl Noted<RunAnswer> {
    /// What to tell the waiting turn: the answer's decision, with the words typed beside it.
    pub fn decision(self) -> RunDecision {
        match &self.note {
            Some(note) => self.answer.decision().with_note(note),
            None => self.answer.decision(),
        }
    }

    /// Whether the turn that asked stops as well as being refused. As [`RunAnswer::stops_the_turn`].
    pub fn stops_the_turn(&self) -> bool {
        self.answer.stops_the_turn()
    }
}

/// [`ask_run`], taking each event from `next` and asking `alone` straight after each one, so a test
/// can stand in for the terminal.
pub(super) fn ask_run_reading<B: Backend>(
    terminal: &mut Terminal<B>,
    request: &RunRequest,
    mut next: impl FnMut() -> Next,
    mut alone: impl FnMut() -> bool,
) -> Noted<RunAnswer> {
    let mut scroll = 0u16;
    let mut shown = RowsShown::default();
    let mut drawn = RunDrawn::default();
    let mut ignored = false;
    loop {
        // A terminal that cannot be drawn to cannot carry a question, so refuse rather than run
        // something unseen. Not again for a report that changed nothing (MOUSE-44).
        if !ignored
            && terminal
                .draw(|frame| drawn = draw_run(frame, request, scroll, &mut shown))
                .is_err()
        {
            return Noted::plain(RunAnswer::Reject);
        }
        ignored = false;

        match next() {
            Next::Ignored => ignored = true,
            Next::Skip => continue,
            // Losing the event stream must not run anything.
            Next::Lost => return Noted::plain(RunAnswer::Reject),
            Next::Wheel(by) => scroll = drawn.moved(scroll, i32::from(by)),
            Next::Press(key) if note::opens(key, alone()) => {
                // A yes from the line is a yes only where this draw takes the question's own.
                let may_yes = matches!(
                    drawn.response_to(
                        KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
                        request
                    ),
                    Some(RunResponse::Answer(RunAnswer::Approve))
                );
                if let Some(noted) = note::take_reading(terminal, may_yes, &mut next, &mut alone)
                    .settle(RunAnswer::Approve, RunAnswer::Reject, RunAnswer::Interrupt)
                {
                    return noted;
                }
            }
            Next::Press(key) => match drawn.response_to(key, request) {
                Some(RunResponse::Answer(answer)) => return Noted::plain(answer),
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
                    t!(run_in_directory, directory = &request.shown_directory())
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
    let mut scope = 0..0;
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

    // What `s` would grant, where the table lists the line: a named group of lines in the same
    // record. It is put in words, with no argument list and no slot, and the rows under the name
    // say what is covered and what stays fixed, because "any number" and "a group" are the halves
    // a person is most likely to read as wider than they are.
    if let Some(offered) = request.offered_scope() {
        lines.push(Line::raw(""));
        let from = measure(&lines);
        lines.extend(scope_rows(&offered));
        scope = from..measure(&lines);
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
        scope_unread: shown.unseen(scope),
        reach_unread: shown.unseen(reach),
        key_unread: shown.unseen(key),
    };

    // Said in place of how far there is to scroll while a key waits on it, since what the person
    // needs to know then is why the key does nothing.
    let waiting = drawn.always_unread
        + drawn.record_unread
        + drawn.scope_unread
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
            format!("       {}", bravebot_agent::confirm::shown_binary(step)),
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
                format!("       {}", bravebot_agent::confirm::shown_path(path)),
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
                    format!("       {}", bravebot_agent::confirm::shown_path(directory)),
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
                    bravebot_agent::confirm::shown_command(&command),
                    t!(
                        run_in_directory,
                        directory = bravebot_agent::confirm::shown_path(&command.directory)
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
        format!("       {}", bravebot_agent::confirm::shown_path(path)),
        Style::default().add_modifier(Modifier::BOLD),
    )));
    lines
}

/// What a scope is called, in the words the prompt and the reading back put to it.
pub(crate) fn scope_name(scope: &Scope) -> String {
    match scope.kind {
        scopes::Kind::ReadPullRequestsAndIssues => t!(
            scope_read_pull_requests_and_issues,
            repository = scope.repository.as_str()
        ),
    }
}

/// What the key that records a scope says it does, in the same words as [`scope_name`] with its
/// first letter small, as the other keys' labels are.
fn scope_label(scope: &Scope) -> String {
    match scope.kind {
        scopes::Kind::ReadPullRequestsAndIssues => t!(
            run_remember_scope_read_pull_requests_and_issues,
            repository = scope.repository.as_str()
        ),
    }
}

/// The rows for `s`: the scope in words, what it covers and what stays fixed.
fn scope_rows(scope: &Scope) -> Vec<Line<'static>> {
    let (covers, fixed) = match scope.kind {
        scopes::Kind::ReadPullRequestsAndIssues => (
            t!(run_remember_scope_read_covers),
            t!(run_remember_scope_read_fixed),
        ),
    };
    vec![
        Line::from(Span::styled(
            format!("  {}", t!(run_remember_scope_explained)),
            Style::default().fg(theme::muted()),
        )),
        Line::from(Span::styled(
            format!("       {}", scope_name(scope)),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!("     {covers}"),
            Style::default().fg(theme::muted()),
        )),
        Line::from(Span::styled(
            format!("     {fixed}"),
            Style::default().fg(theme::running()),
        )),
        Line::from(Span::styled(
            format!("     {}", t!(run_remember_scope_one_entry)),
            Style::default().fg(theme::muted()),
        )),
    ]
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
        format!("       {}", bravebot_agent::confirm::shown_path(path)),
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
            format!(
                "       {}  {}",
                at + 1,
                bravebot_agent::confirm::shown_path(candidate)
            ),
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
        format!(
            "       {}",
            bravebot_agent::confirm::shown_path(&choice.record)
        ),
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
        format!("       {}", bravebot_agent::confirm::shown_path(path)),
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
    if let Some(offered) = request.offered_scope() {
        key_spans.push(Span::styled(
            "s",
            Style::default()
                .fg(colour(RunAnswer::ApproveAndRecordScope, theme::running()))
                .add_modifier(Modifier::BOLD),
        ));
        key_spans.push(Span::raw(format!(" {}    ", scope_label(&offered))));
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
        Span::raw("    "),
    ]);
    key_spans.extend(note::hint());
    Line::from(key_spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::confirm::tests::*;
    use crate::confirm::*;
    use bravebot_agent::reach::Lasting;
    use bravebot_i18n::t;
    use ratatui::backend::TestBackend;

    /// A plan from a command line, with a destination and a shape, as the compiler would produce.
    fn a_compiled_run() -> RunRequest {
        let step = |program: &str, args: &[&str]| bravebot_core::command::Step {
            program: program.to_string(),
            resolved: std::path::PathBuf::from(format!("/usr/bin/{program}")),
            started_as: std::path::PathBuf::from(format!("/usr/bin/{program}")),
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
            confined: None,
            reach_record: None,
            unconfined: false,
            key_choice: None,
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
                draw_run(frame, request, 0, &mut RowsShown::default());
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

    /// A name that reached its file through a link is drawn with the link beside the file, since
    /// the link is what starts and an entry for the line is keyed on both.
    #[test]
    fn a_run_prompt_shows_the_link_a_program_is_started_by() {
        let mut request = a_run(false);
        if let bravebot_core::command::Steps::Pipeline(steps) = &mut request.plan.steps {
            steps[0].started_as = "/home/someone/project/.venv/bin/git".into();
        }
        let drawn = rendered_run(&request);
        let rows: Vec<String> = drawn
            .chars()
            .collect::<Vec<_>>()
            .chunks(160)
            .map(|row| row.iter().collect())
            .collect();
        assert!(
            rows.iter().any(|row| {
                row.trim_matches(|c: char| c.is_whitespace() || c == '│')
                    == "/home/someone/project/.venv/bin/git -> /usr/bin/git"
            }),
            "the line under the name does not show the link: {drawn}"
        );
    }

    /// A run the turn starts unconfined, as on Windows, is said to be unconfined, and says nothing
    /// of the directories or the credentials a confined one does.
    #[test]
    fn a_run_prompt_says_it_is_not_sandboxed_where_the_turn_does_not_confine() {
        let drawn = rendered_run(&a_run(false));
        assert!(drawn.contains("not sandboxed"), "{drawn}");
        assert!(!drawn.contains("confined to"), "{drawn}");
    }

    /// A run the turn confines is not said to be unsandboxed, and says what it is held to: the
    /// directories, and what each stage brings beyond them. A person who then meets a refusal from
    /// the kernel has been told a profile is in force.
    #[test]
    fn a_run_prompt_says_what_a_confined_run_is_held_to() {
        let mut request = a_run(false);
        request.confined = Some(bravebot_agent::Confined {
            reads_the_machine: false,
            directories: vec![
                "/home/someone/project".into(),
                "/var/scratch/session".into(),
            ],
            network: bravebot_sandbox::network::Network::Open,
            filesystem: Default::default(),
            requested: Vec::new(),
            requested_reaches: Vec::new(),
            carried: vec![
                bravebot_agent::Carried {
                    program: "git".into(),
                    toolchain: None,
                    scope: Some(bravebot_sandbox::scope::Scope::Remote),
                    reaches: Vec::new(),
                    identity: Some(bravebot_agent::confirm::Offered::Configured(
                        "/home/someone/.ssh/work.pub".into(),
                    )),
                    network: false,
                    remembered: Vec::new(),
                },
                bravebot_agent::Carried {
                    program: "docker".into(),
                    toolchain: None,
                    scope: Some(bravebot_sandbox::scope::Scope::Docker),
                    reaches: vec![bravebot_sandbox::scope::Reach {
                        variable: "DOCKER_CONFIG",
                        path: "/home/someone/.local/share/docker-work".into(),
                    }],
                    identity: None,
                    network: false,
                    remembered: Vec::new(),
                },
                bravebot_agent::Carried {
                    program: "sed".into(),
                    toolchain: Some(bravebot_sandbox::toolchain::Toolchain::Cargo),
                    scope: None,
                    reaches: Vec::new(),
                    identity: None,
                    network: false,
                    remembered: Vec::new(),
                },
            ],
            writes_git_directories: false,
        });

        let drawn = rendered_run(&request);

        assert!(!drawn.contains("not sandboxed"), "{drawn}");
        assert!(drawn.contains("confined to these directories"), "{drawn}");
        assert!(drawn.contains("/var/scratch/session"), "{drawn}");
        assert!(
            drawn.contains("git also reads your git and gh logins"),
            "{drawn}"
        );
        assert!(drawn.contains("never a private key"), "{drawn}");
        assert!(
            drawn.contains(
                "git has ssh offer the key /home/someone/.ssh/work.pub first, which your git.identities names"
            ),
            "{drawn}"
        );
        assert!(
            drawn.contains(
                "docker also reads /home/someone/.local/share/docker-work, where your DOCKER_CONFIG points"
            ),
            "{drawn}"
        );
        assert!(
            drawn.contains("sed also reaches the install and the cache of the cargo toolchain"),
            "{drawn}"
        );
        assert!(!drawn.contains("hooks"), "{drawn}");
    }

    /// PROMPT-1: the run prompt draws a file the line writes, and a directory it is confined to,
    /// with a name that would draw as nothing, or as another name, quoted and spelled, so a yes to
    /// `note.md ` cannot be read as one to `note.md`.
    #[test]
    fn the_run_prompt_marks_an_unreadable_written_file_and_directory() {
        let screen = |written: &str, directory: &str| {
            let mut request = a_run(false);
            request.plan.writes = vec![written.into()];
            request.confined = Some(bravebot_agent::Confined {
                reads_the_machine: false,
                directories: vec![directory.into()],
                network: bravebot_sandbox::network::Network::Open,
                filesystem: Default::default(),
                requested: Vec::new(),
                requested_reaches: Vec::new(),
                carried: Vec::new(),
                writes_git_directories: false,
            });
            run_screen(&request, (160, 40), 0, &mut RowsShown::default())
                .0
                .join("\n")
        };
        let plain = screen("note.md", "/work");
        assert!(plain.contains("note.md"), "{plain}");
        assert!(plain.contains("/work"), "{plain}");
        for (name, spelled) in UNSEEN_NAMES {
            let written = screen(name, "/work");
            assert_ne!(written, plain, "{name:?}");
            assert!(written.contains(spelled), "{name:?}\n{written}");
            let confined = screen("note.md", name);
            assert_ne!(confined, plain, "{name:?}");
            assert!(confined.contains(spelled), "{name:?}\n{confined}");
        }
    }

    /// Whether `screen` has a row that holds `text` and nothing else inside the box.
    fn has_row(screen: &str, text: &str) -> bool {
        screen
            .lines()
            .any(|row| row.trim_matches(|c: char| c == ' ' || c == '│') == text)
    }

    /// The run question at a size that holds every row of it, one string per row.
    fn whole_run_screen(request: &RunRequest) -> String {
        run_screen(request, (160, 90), 0, &mut RowsShown::default())
            .0
            .join("\n")
    }

    /// PROMPT-1: the run prompt draws the directory the line runs in, in its header and beside the
    /// command `a` would vouch for, and the file each step runs, quoted and spelled where the path
    /// would draw as nothing or as another, so a yes to a line in `/work/state ` is not read as one
    /// in `/work/state`, and the same for the binary it starts.
    #[test]
    fn the_run_prompt_marks_an_unreadable_directory_and_binary() {
        let screen = |directory: &str, binary: &str| {
            let mut request = a_run(false);
            request.plan.directory = directory.into();
            let bravebot_core::command::Steps::Pipeline(steps) = &mut request.plan.steps else {
                panic!("a pipeline");
            };
            steps[0].resolved = binary.into();
            steps[0].started_as = binary.into();
            // The header, and the row of each command `a` would vouch for.
            let rows = 1 + request.would_vouch_for().len();
            (whole_run_screen(&request), rows)
        };
        let (plain, _) = screen("/work", "/usr/bin/git");
        assert!(plain.contains("/work"), "{plain}");
        for (name, spelled) in UNSEEN_NAMES {
            let (directory, rows) = screen(name, "/usr/bin/git");
            assert_ne!(directory, plain, "{name:?}");
            assert_eq!(
                directory.matches(spelled).count(),
                rows,
                "{name:?}\n{directory}"
            );
            let (binary, _) = screen("/work", name);
            assert_ne!(binary, plain, "{name:?}");
            // The row under the step, and the command `a` would vouch for.
            assert!(has_row(&binary, spelled), "{name:?}\n{binary}");
            assert!(
                binary.contains(&format!("{spelled} log --oneline")),
                "{name:?}\n{binary}"
            );
        }
    }

    /// PROMPT-1: a step started through a link is drawn as the link and the file it leads to, each
    /// spelled on its own, so a link and a file that differ only in a trailing space are two paths.
    #[test]
    fn the_run_prompt_spells_the_link_and_the_file_a_step_starts_through() {
        let mut request = a_run(false);
        let bravebot_core::command::Steps::Pipeline(steps) = &mut request.plan.steps else {
            panic!("a pipeline");
        };
        steps[0].started_as = "/usr/local/bin/git ".into();
        steps[0].resolved = "/usr/bin/git\u{202e}".into();
        let drawn = whole_run_screen(&request);
        let spelled = "\"/usr/local/bin/git\u{2423}\" -> \"/usr/bin/git\\u{202e}\"";
        assert!(has_row(&drawn, spelled), "{drawn}");
        assert!(
            drawn.contains(&format!("{spelled} log --oneline")),
            "{drawn}"
        );
    }

    /// PROMPT-1, PROMPT-2: the file a yes is recorded in is drawn in each row that names one, the
    /// remembered-lines record, the settings file a pattern would go in, the record of a kept reach
    /// and the record of a chosen key with the key files on offer, spelled where the path would draw
    /// as another.
    #[test]
    fn the_run_prompt_marks_an_unreadable_record_path() {
        /// A run whose row under test names the file it is given.
        type Naming = fn(&str) -> RunRequest;
        let rows: [(&str, Naming); 5] = [
            ("record", |name| RunRequest {
                record: Some(name.into()),
                ..a_recordable_run()
            }),
            ("pattern", |name| RunRequest {
                pattern: Some(name.into()),
                ..a_recordable_run()
            }),
            ("reach record", |name| RunRequest {
                reach_record: Some(name.into()),
                ..a_run_keeping_remote()
            }),
            ("key record", |name| {
                let mut request = a_run_offering_two_keys();
                request.key_choice.as_mut().expect("a choice").record = name.into();
                request
            }),
            ("key candidate", |name| {
                let mut request = a_run_offering_two_keys();
                request.key_choice.as_mut().expect("a choice").candidates[1] = name.into();
                request
            }),
        ];
        for (row, request) in rows {
            let plain = whole_run_screen(&request("/home/someone/.bravebot/file"));
            assert!(
                plain.contains("/home/someone/.bravebot/file"),
                "{row}\n{plain}"
            );
            for (name, spelled) in UNSEEN_NAMES {
                let drawn = whole_run_screen(&request(name));
                assert_ne!(drawn, plain, "{row}: {name:?}");
                assert!(drawn.contains(spelled), "{row}: {name:?}\n{drawn}");
            }
        }
    }

    /// PROMPT-1, PATH-003: a directory, a binary and a file a record is written to whose names hold a
    /// byte that is not UTF-8 are drawn with that byte spelled, so none of them draws as the one its
    /// replacement-character rendering spells.
    #[cfg(unix)]
    #[test]
    fn the_run_prompt_draws_a_directory_binary_and_record_that_are_not_text_as_their_bytes() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let screen = |byte: &[u8]| {
            let path = |stem: &str| {
                let mut bytes = stem.as_bytes().to_vec();
                bytes.extend_from_slice(byte);
                std::path::PathBuf::from(OsStr::from_bytes(&bytes))
            };
            let mut request = a_recordable_run();
            request.plan.directory = path("/work/dir-");
            request.record = Some(path("/record/file-"));
            let bravebot_core::command::Steps::Pipeline(steps) = &mut request.plan.steps else {
                panic!("a pipeline");
            };
            steps[0].resolved = path("/bin/binary-");
            steps[0].started_as = path("/bin/binary-");
            whole_run_screen(&request)
        };
        let invalid = screen(b"\xff");
        let lookalike = screen("\u{fffd}".as_bytes());
        assert_ne!(invalid, lookalike);
        for spelled in ["\"/work/dir-\\xff\"", "\"/record/file-\\xff\""] {
            assert!(invalid.contains(spelled), "{spelled}\n{invalid}");
        }
        assert!(has_row(&invalid, "\"/bin/binary-\\xff\""), "{invalid}");
        assert!(!lookalike.contains("\\xff"), "{lookalike}");
    }

    /// PROMPT-1, PATH-003: the run prompt draws a directory it is confined to, and a file the line
    /// writes, under a directory whose name holds a byte that is not UTF-8 with that byte spelled, so
    /// neither is drawn as the one its replacement-character rendering spells.
    #[cfg(unix)]
    #[test]
    fn the_run_prompt_draws_a_path_that_is_not_text_as_the_bytes_it_holds() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let screen = |directory: &[u8]| {
            let directory = std::path::PathBuf::from(OsStr::from_bytes(directory));
            let mut request = a_run(false);
            request.plan.writes = vec![directory.join("note.md")];
            request.confined = Some(bravebot_agent::Confined {
                reads_the_machine: false,
                directories: vec![directory],
                network: bravebot_sandbox::network::Network::Open,
                filesystem: Default::default(),
                requested: Vec::new(),
                requested_reaches: Vec::new(),
                carried: Vec::new(),
                writes_git_directories: false,
            });
            run_screen(&request, (160, 40), 0, &mut RowsShown::default())
                .0
                .join("\n")
        };
        let invalid = screen(b"/work/a\xffb");
        let lookalike = screen("/work/a\u{fffd}b".as_bytes());
        assert_ne!(invalid, lookalike);
        assert!(invalid.contains("\"/work/a\\xffb\""), "{invalid}");
        assert!(invalid.contains("\"/work/a\\xffb/note.md\""), "{invalid}");
        assert!(lookalike.contains("/work/a\u{fffd}b"), "{lookalike}");
        assert!(!lookalike.contains("\\xff"), "{lookalike}");
    }

    /// SANDBOX-18: where nothing holds a `.git` back from a program that is not git, the heading a
    /// person approves under says that such a program can write a repository's hooks and
    /// configuration, at the size a terminal opens at, above the directories it names.
    #[test]
    fn a_run_prompt_says_a_program_other_than_git_can_write_a_repositorys_hooks() {
        let mut request = a_run(false);
        request.confined = Some(bravebot_agent::Confined {
            reads_the_machine: true,
            directories: vec!["/home/someone/project".into()],
            network: bravebot_sandbox::network::Network::Open,
            filesystem: Default::default(),
            requested: Vec::new(),
            requested_reaches: Vec::new(),
            carried: Vec::new(),
            writes_git_directories: true,
        });

        let drawn = rendered_run(&request);

        assert!(drawn.contains("can read this machine"), "{drawn}");
        assert!(drawn.contains("can write a repository's hooks"), "{drawn}");
        assert!(drawn.contains("/home/someone/project"), "{drawn}");
    }

    /// SANDBOX-16: a run prompt for a push whose key the person chose at an earlier prompt names the
    /// owner and the day it was chosen, and the key where one was chosen. A person reading it can
    /// tell a key they chose, which `/reach keys remove` forgets, from one their `git.identities`
    /// names.
    #[test]
    fn a_run_prompt_names_the_owner_and_the_day_of_a_key_chosen_at_an_earlier_prompt() {
        let drawn_with = |key: Option<&str>| {
            let mut request = a_run(false);
            request.confined = Some(bravebot_agent::Confined {
                reads_the_machine: false,
                writes_git_directories: false,
                directories: vec!["/home/someone/project".into()],
                network: bravebot_sandbox::network::Network::Open,
                filesystem: Default::default(),
                requested: Vec::new(),
                requested_reaches: Vec::new(),
                carried: vec![bravebot_agent::Carried {
                    program: "git".into(),
                    toolchain: None,
                    scope: Some(bravebot_sandbox::scope::Scope::Remote),
                    reaches: Vec::new(),
                    identity: Some(bravebot_agent::confirm::Offered::Chosen(
                        bravebot_agent::reach::ChosenKey {
                            address: bravebot_sandbox::scope::Address::spelled(
                                "github.com",
                                "acme",
                            )
                            .expect("an address"),
                            key: key.map(Into::into),
                            allowed: "2026-10-13".to_string(),
                        },
                    )),
                    network: false,
                    remembered: Vec::new(),
                }],
            });
            rendered_run(&request)
        };

        let chosen = drawn_with(Some("/home/someone/.ssh/work.pub"));
        assert!(
            chosen.contains(
                "git has ssh offer the key /home/someone/.ssh/work.pub first, which you chose for github.com/acme on 2026-10-13"
            ),
            "{chosen}"
        );
        assert!(!chosen.contains("git.identities names"), "{chosen}");

        let left_to_ssh = drawn_with(None);
        assert!(
            left_to_ssh.contains(
                "git leaves the key to ssh, which offers your keys in order, as you chose for github.com/acme on 2026-10-13"
            ),
            "{left_to_ssh}"
        );
        assert!(
            !left_to_ssh.contains("has ssh offer the key"),
            "{left_to_ssh}"
        );
    }

    /// A prompt for a session whose network is closed says so, and names the stage that keeps it,
    /// so a yes is given knowing the one program that can reach out. An open network says nothing,
    /// as it never did.
    #[test]
    fn a_run_prompt_says_the_network_is_closed_and_which_stage_keeps_it() {
        let mut request = a_run(false);
        let held = |network, kept| bravebot_agent::Confined {
            reads_the_machine: false,
            directories: vec!["/home/someone/project".into()],
            network,
            filesystem: Default::default(),
            requested: Vec::new(),
            requested_reaches: Vec::new(),
            carried: vec![bravebot_agent::Carried {
                program: "git".into(),
                toolchain: None,
                scope: Some(bravebot_sandbox::scope::Scope::Remote),
                reaches: Vec::new(),
                identity: None,
                network: kept,
                remembered: Vec::new(),
            }],
            writes_git_directories: false,
        };
        request.confined = Some(held(bravebot_sandbox::network::Network::Closed, true));

        let drawn = rendered_run(&request);

        assert!(drawn.contains("the network is closed"), "{drawn}");
        assert!(drawn.contains("git also reaches the network"), "{drawn}");

        request.confined = Some(held(bravebot_sandbox::network::Network::Open, false));
        let drawn = rendered_run(&request);
        assert!(!drawn.contains("network"), "{drawn}");
    }

    /// A line that reaches an authority nothing here holds says which one, beside the line about
    /// not being sandboxed rather than instead of it. Both are drawn: one says what confinement
    /// there is and the other says what a yes hands over, and a person told only the first has
    /// been told that a `docker` line is unsandboxed and not that it is root on their machine.
    #[test]
    fn a_run_prompt_names_the_ambient_authority_a_line_reaches() {
        let pipeline = bravebot_core::Pipeline::new(vec![bravebot_core::Stage::new(
            "docker",
            vec!["ps".into(), "-a".into()],
        )]);
        let request =
            RunRequest::from_pipeline(&pipeline, &["/usr/bin/docker".into()], "/home/someone");

        let drawn = rendered_run(&request);
        assert!(drawn.contains("not sandboxed"), "{drawn}");
        assert!(drawn.contains("container daemon"), "{drawn}");
        assert!(drawn.contains("as root on this machine"), "{drawn}");
    }

    /// Most lines reach nothing of the sort, and the row is not drawn for them. A sentence that
    /// appeared on every prompt would be one a reader learns to skip, which is what the line
    /// above it already costs and the reason this one is not spent the same way.
    #[test]
    fn a_run_prompt_names_no_authority_where_the_line_reaches_none() {
        let drawn = rendered_run(&a_compiled_run());
        assert!(drawn.contains("not sandboxed"), "{drawn}");
        assert!(!drawn.contains("spends access"), "{drawn}");
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
            started_as: std::path::PathBuf::from("/bin/cat"),
            args: Vec::new(),
            environment: Vec::new(),
            routes: vec![bravebot_core::command::Route::Stdin {
                path: secret.clone(),
            }],
        };
        RunRequest {
            confined: None,
            reach_record: None,
            unconfined: false,
            key_choice: None,
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

    /// A line fed a reference asks every time whatever is remembered, for any label the reference
    /// has: an entry records a program, its arguments and a tree, and `python3 -` fed one public
    /// page is the same entry as `python3 -` fed another. The prompt withholds `a` and says why,
    /// with the reason for a reference and not the one for private input, since a public page
    /// releases nothing.
    #[test]
    fn a_run_fed_a_reference_offers_no_standing_permission() {
        let mut request = a_run(false);
        request.stdin = Some("ref:1".to_string());
        request.plan.stdin = Some(bravebot_core::label::Label::untrusted_public());
        assert!(!request.releases_private(), "the page was taken as private");

        let drawn = rendered_run(&request);
        assert!(
            drawn.contains("a line fed a reference is asked about every time"),
            "the prompt did not give the reason for withholding the key: {drawn}"
        );
        assert!(
            !drawn.contains("private input is asked"),
            "the prompt gave the private-input reason for a public page: {drawn}"
        );

        for key in ['a', 'A', 'r', 'R'] {
            let pressed = run_answer_for(
                KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE),
                &request,
            );
            assert_eq!(
                pressed, None,
                "`{key}` answered a prompt that does not offer it"
            );
        }
    }

    /// A compiled plan with an assignment written in front of its program, as
    /// `LD_PRELOAD=./evil.so git log` compiles.
    fn a_run_with_an_assignment() -> RunRequest {
        let step = bravebot_core::command::Step {
            program: "git".to_string(),
            resolved: std::path::PathBuf::from("/usr/bin/git"),
            started_as: std::path::PathBuf::from("/usr/bin/git"),
            args: vec!["log".to_string()],
            environment: vec![("LD_PRELOAD".to_string(), "./evil.so".to_string())],
            routes: Vec::new(),
        };
        RunRequest {
            confined: None,
            reach_record: None,
            unconfined: false,
            key_choice: None,
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

    /// SANDBOX-29: a line the planner asked to run with no sandbox says so on the prompt, offers no
    /// answer that lasts, and binds none of `a`, `r`, `f`, `m` or `e`, though the request carries
    /// every path those keys write to. The control is the same request without the flag, where
    /// `a` and `r` answer. The regression it rejects is a key that remembers the request.
    #[test]
    fn a_run_asking_to_be_unconfined_says_so_and_binds_no_lasting_key() {
        let recordable = || RunRequest {
            reach_record: Some("/home/someone/.bravebot/reach.jsonl".into()),
            ..a_recordable_run()
        };
        for key in ['a', 'r'] {
            assert!(
                run_answer_for(
                    KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE),
                    &recordable()
                )
                .is_some(),
                "the control did not take `{key}`, so the rows below prove nothing"
            );
        }

        let request = RunRequest {
            unconfined: true,
            key_choice: None,
            ..recordable()
        };
        let drawn = rendered_run(&request);
        assert!(
            drawn.contains("with no sandbox"),
            "the prompt did not say the line runs with none: {drawn}"
        );
        assert!(
            !drawn.contains("stop asking about this exact line"),
            "the prompt explained a key it does not offer: {drawn}"
        );
        assert!(
            drawn.contains("asked about every time"),
            "the prompt did not say why no lasting answer is offered: {drawn}"
        );
        for key in ['a', 'A', 'r', 'R', 'f', 'F', 'm', 'M', 'e', 'E'] {
            assert_eq!(
                run_answer_for(
                    KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE),
                    &request
                ),
                None,
                "`{key}` answered a request to run with no sandbox"
            );
        }
        let once = run_answer_for(
            KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
            &request,
        );
        assert_eq!(once, Some(RunResponse::Answer(RunAnswer::Approve)));
    }

    /// A line the planner asked to carry a scope asks every time whatever is remembered, so the
    /// prompt names that reason and `a` does not answer.
    #[test]
    fn a_run_asking_for_a_scope_offers_no_standing_permission() {
        let mut request = a_run(false);
        request.confined = Some(bravebot_agent::Confined {
            reads_the_machine: false,
            directories: vec!["/home/someone/project".into()],
            network: bravebot_sandbox::network::Network::Open,
            filesystem: Default::default(),
            requested: vec![(
                "git".into(),
                bravebot_sandbox::scope::Requested::Scope(bravebot_sandbox::scope::Scope::Aws),
            )],
            requested_reaches: Vec::new(),
            carried: Vec::new(),
            writes_git_directories: false,
        });
        assert!(request.asks_for_scopes());

        let drawn = rendered_run(&request);
        assert!(
            drawn.contains("cannot be remembered"),
            "the prompt offered to remember a run that will always ask: {drawn}"
        );
        assert!(
            drawn.contains("credential scope, toolchain list or loopback"),
            "the prompt did not say which of the reasons this is: {drawn}"
        );

        for key in ['a', 'A'] {
            let pressed = run_answer_for(
                KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE),
                &request,
            );
            assert_eq!(
                pressed, None,
                "`{key}` answered a prompt that does not offer it"
            );
        }
    }

    /// A compiled plan that redirects its output to a file, as `sh check.sh > out.txt` compiles.
    /// The argv holds the script and not the destination, which is exactly why an entry made here
    /// would cover `sh check.sh` on its own.
    fn a_run_writing_a_file() -> RunRequest {
        let destination = std::path::PathBuf::from("/home/someone/project/out.txt");
        let step = bravebot_core::command::Step {
            program: "sh".to_string(),
            resolved: std::path::PathBuf::from("/bin/sh"),
            started_as: std::path::PathBuf::from("/bin/sh"),
            args: vec!["check.sh".to_string()],
            environment: Vec::new(),
            routes: vec![bravebot_core::command::Route::Stdout {
                path: destination.clone(),
                append: false,
            }],
        };
        RunRequest {
            confined: None,
            reach_record: None,
            unconfined: false,
            key_choice: None,
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

    /// A run prompt for a line the scope table lists, in a session that can record.
    fn a_scope_run() -> RunRequest {
        RunRequest {
            record: a_recordable_run().record,
            ..RunRequest::from_pipeline(
                &bravebot_core::Pipeline::new(vec![bravebot_core::Stage::new(
                    "gh",
                    ["pr", "view", "1081", "--repo", "brave/bravebot"]
                        .map(String::from)
                        .to_vec(),
                )]),
                &["/usr/bin/gh".into()],
                "/home/someone/project",
            )
        }
    }

    /// RUN-20: the scope answer has a key of its own, approves the run, and records the scope and
    /// not the exact line. It vouches for nothing.
    #[test]
    fn the_scope_key_approves_and_records_the_scope_only() {
        assert_eq!(
            run_answer_for(press(KeyCode::Char('s')), &a_scope_run()),
            Some(RunResponse::Answer(RunAnswer::ApproveAndRecordScope))
        );
        let decision = RunAnswer::ApproveAndRecordScope.decision();
        assert!(decision.approved());
        assert!(decision.record_scope);
        assert!(
            !decision.record,
            "the scope key also recorded the exact line"
        );
        assert!(
            !decision.remember,
            "the scope key also vouched for the programs"
        );
        assert!(!RunAnswer::ApproveAndRecord.decision().record_scope);
    }

    /// RUN-20: the `f` key that answered "any number" before scopes is bound to nothing, so a
    /// person who reaches for it does not record a scope they were not shown.
    #[test]
    fn the_old_any_number_key_answers_nothing() {
        assert_eq!(
            run_answer_for(press(KeyCode::Char('f')), &a_scope_run()),
            None
        );
    }

    /// RUN-20: the key is unbound where the line is not in the table, and where the prompt cannot
    /// record at all, so no key promises what the screen did not offer.
    #[test]
    fn the_scope_key_is_unbound_where_the_prompt_does_not_offer_it() {
        let cannot_record = RunRequest {
            record: None,
            ..a_scope_run()
        };
        for request in [a_run(false), a_recordable_run(), cannot_record] {
            assert_eq!(
                run_answer_for(press(KeyCode::Char('s')), &request),
                None,
                "a key recorded a scope the prompt did not offer"
            );
        }
    }

    /// RUN-20: refusing and interrupting record no scope.
    #[test]
    fn refusing_a_run_records_no_scope() {
        for answer in [RunAnswer::Reject, RunAnswer::Interrupt] {
            assert!(!answer.decision().record_scope, "{answer:?}");
        }
    }

    /// RUN-20, PROMPT-4: the scope key waits for the rows saying what it grants, as `r` waits for
    /// its own.
    #[test]
    fn the_scope_key_waits_for_the_rows_saying_what_it_grants() {
        let seen = RunDrawn {
            whole: true,
            ..RunDrawn::default()
        };
        assert!(seen.takes(RunAnswer::ApproveAndRecordScope));
        let unread = RunDrawn {
            scope_unread: 1,
            ..seen
        };
        assert!(!unread.takes(RunAnswer::ApproveAndRecordScope));
        assert!(unread.takes(RunAnswer::Approve));
        assert!(unread.takes(RunAnswer::ApproveAndRecord));
    }

    /// RUN-20: the prompt names the scope in words, says what it covers and what stays fixed, and
    /// draws the key. It draws no argument list with a slot in it and no rule syntax, and a prompt
    /// for a line the table does not list draws none of it.
    #[test]
    fn a_prompt_offering_a_scope_asks_in_words() {
        let drawn = fully_rendered_run(&a_scope_run());
        for wanted in [
            "Read pull requests and issues in brave/bravebot",
            "read pull requests and issues in brave/bravebot",
            "viewing a pull request or an issue and reading a pull request's diff and checks",
            "everything else is still asked about: another repository, another flag, and anything that writes",
            "deleting that line takes all of it back",
        ] {
            assert!(drawn.contains(wanted), "{wanted:?} was not drawn: {drawn}");
        }
        for unwanted in ["<number>", "any number in place", "Bash(", "gh pr view *"] {
            assert!(
                !drawn.contains(unwanted),
                "{unwanted:?} was drawn at a prompt that asks in words: {drawn}"
            );
        }
        let cannot_record = RunRequest {
            record: None,
            ..a_scope_run()
        };
        for request in [a_recordable_run(), cannot_record] {
            let plain = fully_rendered_run(&request);
            assert!(!plain.contains("Read pull requests"), "{plain}");
            assert!(!plain.contains("read pull requests"), "{plain}");
        }
    }

    /// RUN-20: the scope is named for the repository the line named, so a person answering for
    /// another repository is shown that one.
    #[test]
    fn the_scope_prompt_names_the_repository_the_line_named() {
        let request = RunRequest {
            record: a_recordable_run().record,
            ..RunRequest::from_pipeline(
                &bravebot_core::Pipeline::new(vec![bravebot_core::Stage::new(
                    "gh",
                    ["issue", "view", "7", "--repo", "someone/else"]
                        .map(String::from)
                        .to_vec(),
                )]),
                &["/usr/bin/gh".into()],
                "/home/someone/project",
            )
        };
        let drawn = fully_rendered_run(&request);
        assert!(
            drawn.contains("Read pull requests and issues in someone/else"),
            "{drawn}"
        );
        assert!(!drawn.contains("brave/bravebot"), "{drawn}");
    }

    /// SANDBOX-27: `m` and `e` approve the line and ask for the requested reach to be remembered
    /// for this session or for every one, and for nothing else. The regressions it rejects: a key
    /// that also vouches for the programs or records the line, and `e` lasting only the session.
    #[test]
    fn the_keep_keys_approve_and_remember_the_reach_only() {
        let request = a_run_keeping_remote();
        assert_eq!(
            run_answer_for(press(KeyCode::Char('m')), &request),
            Some(RunResponse::Answer(RunAnswer::KeepReach))
        );
        assert_eq!(
            run_answer_for(press(KeyCode::Char('e')), &request),
            Some(RunResponse::Answer(RunAnswer::KeepReachEverySession))
        );
        for (answer, lasting) in [
            (RunAnswer::KeepReach, Lasting::ThisSession),
            (RunAnswer::KeepReachEverySession, Lasting::EverySession),
        ] {
            let decision = answer.decision();
            assert!(decision.approved());
            assert_eq!(decision.remember_reach, Some(lasting));
            assert!(!decision.remember, "{answer:?} vouched for the programs");
            assert!(!decision.record, "{answer:?} recorded the line");
            assert!(!decision.record_scope, "{answer:?} recorded a scope");
            assert!(!answer.stops_the_turn());
        }
        for answer in [
            RunAnswer::Approve,
            RunAnswer::ApproveAlways,
            RunAnswer::ApproveAndRecord,
            RunAnswer::ApproveAndRecordScope,
            RunAnswer::Reject,
            RunAnswer::Interrupt,
        ] {
            assert_eq!(answer.decision().remember_reach, None, "{answer:?}");
        }
    }

    /// SANDBOX-27: the keys are unbound where the prompt does not offer them: no place to write a
    /// record, no credential scope asked for, only a toolchain list asked for, and a stage with an
    /// option where its operation would be. The regression it rejects: a key granting what the
    /// screen did not offer.
    #[test]
    fn the_keep_keys_are_unbound_where_the_prompt_does_not_offer_them() {
        let no_record = RunRequest {
            reach_record: None,
            unconfined: false,
            key_choice: None,
            ..a_run_keeping_remote()
        };
        let no_request = a_run_asking_for(Vec::new());
        let toolchain_only = a_run_asking_for(vec![bravebot_sandbox::scope::Requested::Toolchain(
            bravebot_sandbox::toolchain::Toolchain::Cargo,
        )]);
        let loopback_only = a_run_asking_for(vec![bravebot_sandbox::scope::Requested::Loopback]);
        let mut option_first = a_run_keeping_remote();
        option_first.plan.steps =
            bravebot_core::command::Steps::Pipeline(vec![bravebot_core::command::Step {
                program: "gh".to_string(),
                resolved: "/usr/bin/gh".into(),
                started_as: "/usr/bin/gh".into(),
                args: vec!["-R".to_string(), "a/b".to_string(), "pr".to_string()],
                environment: Vec::new(),
                routes: Vec::new(),
            }]);
        for request in [
            a_run(false),
            no_record,
            no_request,
            toolchain_only,
            loopback_only,
            option_first,
        ] {
            for key in ['m', 'M', 'e', 'E'] {
                assert_eq!(
                    run_answer_for(press(KeyCode::Char(key)), &request),
                    None,
                    "`{key}` answered a prompt that does not offer it"
                );
            }
            let drawn = fully_rendered_run(&request);
            assert!(
                !drawn.contains(t!(run_keep_reach)),
                "the keys were drawn without being offered: {drawn}"
            );
        }
    }

    /// SANDBOX-27, PROMPT-4: the keys wait for the rows saying what they remember.
    #[test]
    fn the_keep_keys_wait_for_the_rows_saying_what_they_remember() {
        let seen = RunDrawn {
            whole: true,
            ..RunDrawn::default()
        };
        let unread = RunDrawn {
            reach_unread: 1,
            ..seen
        };
        for answer in [RunAnswer::KeepReach, RunAnswer::KeepReachEverySession] {
            assert!(seen.takes(answer));
            assert!(!unread.takes(answer), "{answer:?}");
        }
        assert!(unread.takes(RunAnswer::Approve));
        assert!(unread.takes(RunAnswer::Reject));
    }

    /// SANDBOX-27: the prompt names the programs and the scope, says the line is still asked about,
    /// says a toolchain list and loopback are not kept, shows where the record is, and draws both
    /// keys.
    #[test]
    fn a_prompt_offering_to_keep_a_request_names_what_would_be_kept() {
        let mut request = a_run_keeping_remote();
        request
            .confined
            .as_mut()
            .expect("confined")
            .requested
            .push((
                "gh".to_string(),
                bravebot_sandbox::scope::Requested::Toolchain(
                    bravebot_sandbox::toolchain::Toolchain::Cargo,
                ),
            ));
        request
            .confined
            .as_mut()
            .expect("confined")
            .requested
            .push((
                "gh".to_string(),
                bravebot_sandbox::scope::Requested::Loopback,
            ));
        let drawn = fully_rendered_run(&request);
        for wanted in [
            "gh pr".to_string(),
            "remote, read only".to_string(),
            "/home/someone/.bravebot/reach.jsonl".to_string(),
            "still asked about every time".to_string(),
            "cargo, loopback is not remembered".to_string(),
            t!(run_keep_reach).to_string(),
            t!(run_keep_reach_always).to_string(),
        ] {
            assert!(drawn.contains(&wanted), "{wanted:?} was not drawn: {drawn}");
        }
    }

    /// A run prompt for a push to `github.com/acme` that offers a choice between two keys.
    fn a_run_offering_two_keys() -> RunRequest {
        let mut request = RunRequest::from_pipeline(
            &bravebot_core::Pipeline::new(vec![bravebot_core::Stage::new(
                "git",
                ["push", "git@github.com:acme/widgets.git"]
                    .map(String::from)
                    .to_vec(),
            )]),
            &["/usr/bin/git".into()],
            "/home/someone/project",
        );
        request.key_choice = Some(Box::new(bravebot_agent::confirm::KeyChoice {
            address: bravebot_sandbox::scope::Address::spelled("github.com", "acme")
                .expect("an address"),
            candidates: vec![
                "/home/someone/.ssh/id_ed25519.pub".into(),
                "/home/someone/.ssh/work.pub".into(),
            ],
            record: "/home/someone/.bravebot/reach.jsonl".into(),
        }));
        request
    }

    /// SANDBOX-16: each digit the prompt numbered runs the line with that key chosen, `0` leaves the
    /// key to ssh, and a digit past the last key, or any digit at a prompt that numbered none,
    /// answers nothing. The answer vouches for nothing and records no line, and no other answer
    /// carries a key. The regressions it rejects are a digit off by one, which pushes as the other
    /// account and remembers that, and a digit bound where nothing was numbered.
    #[test]
    fn the_key_digits_choose_only_the_keys_the_prompt_numbered() {
        use bravebot_agent::confirm::KeyAnswer;
        let request = a_run_offering_two_keys();
        for (digit, answer) in [
            ('1', KeyAnswer::Candidate(0)),
            ('2', KeyAnswer::Candidate(1)),
            ('0', KeyAnswer::SshDefault),
        ] {
            assert_eq!(
                run_answer_for(press(KeyCode::Char(digit)), &request),
                Some(RunResponse::Answer(RunAnswer::ChooseKey(answer))),
                "{digit}"
            );
            let decision = RunAnswer::ChooseKey(answer).decision();
            assert!(decision.approved());
            assert_eq!(decision.key, Some(answer));
            assert!(!decision.remember && !decision.record && !decision.record_scope);
            assert_eq!(decision.remember_reach, None);
            assert!(!RunAnswer::ChooseKey(answer).stops_the_turn());
        }
        for digit in ['3', '9'] {
            assert_eq!(
                run_answer_for(press(KeyCode::Char(digit)), &request),
                None,
                "{digit}"
            );
        }
        assert_eq!(
            run_answer_for(press(KeyCode::Char('n')), &request),
            Some(RunResponse::Answer(RunAnswer::Reject))
        );
        assert_eq!(
            run_answer_for(press(KeyCode::Char('j')), &request),
            Some(RunResponse::Scroll(1))
        );
        for request in [a_run(false), a_run_keeping_remote()] {
            for digit in '0'..='9' {
                assert_eq!(
                    run_answer_for(press(KeyCode::Char(digit)), &request),
                    None,
                    "`{digit}` answered a prompt that numbered no key"
                );
            }
            let drawn = fully_rendered_run(&request);
            assert!(!drawn.contains(t!(run_key_choose)), "{drawn}");
        }
        for answer in [
            RunAnswer::Approve,
            RunAnswer::ApproveAlways,
            RunAnswer::ApproveAndRecord,
            RunAnswer::ApproveAndRecordScope,
            RunAnswer::KeepReach,
            RunAnswer::KeepReachEverySession,
            RunAnswer::Reject,
            RunAnswer::Interrupt,
        ] {
            assert_eq!(answer.decision().key, None, "{answer:?}");
        }
    }

    /// SANDBOX-16, PROMPT-4: the digits wait for the rows naming the keys and what choosing one
    /// remembers, and `y` and `n` do not.
    #[test]
    fn the_key_digits_wait_for_the_rows_naming_the_keys() {
        use bravebot_agent::confirm::KeyAnswer;
        let seen = RunDrawn {
            whole: true,
            ..RunDrawn::default()
        };
        let unread = RunDrawn {
            key_unread: 1,
            ..seen
        };
        for answer in [KeyAnswer::Candidate(0), KeyAnswer::SshDefault] {
            assert!(seen.takes(RunAnswer::ChooseKey(answer)));
            assert!(!unread.takes(RunAnswer::ChooseKey(answer)), "{answer:?}");
        }
        assert!(unread.takes(RunAnswer::Approve));
        assert!(unread.takes(RunAnswer::Reject));

        // Paged through on a short terminal, a digit is taken only once each key it could choose
        // and the place the choice is written have been drawn.
        let request = a_run_offering_two_keys();
        let mut shown = RowsShown::default();
        let (mut scroll, mut read, mut waited) = (0, String::new(), false);
        loop {
            let (rows, drawn) = run_screen(&request, (80, 14), scroll, &mut shown);
            read.push_str(&box_text(&rows));
            let digit = drawn
                .response_to(press(KeyCode::Char('2')), &request)
                .is_some();
            if digit {
                for wanted in [
                    "/home/someone/.ssh/id_ed25519.pub",
                    "/home/someone/.ssh/work.pub",
                    "/home/someone/.bravebot/reach.jsonl",
                ] {
                    assert!(
                        read.contains(wanted),
                        "`2` was taken before {wanted} was drawn"
                    );
                }
                break;
            }
            if drawn
                .response_to(press(KeyCode::Char('y')), &request)
                .is_some()
            {
                waited = true;
            }
            assert!(scroll < drawn.furthest, "paged to the end: {rows:#?}");
            scroll = paged_down(&drawn, &request, scroll);
        }
        assert!(waited, "no draw took `y` alone, so nothing here waited");
    }

    /// SANDBOX-16: the prompt names the owner, each key by the digit that chooses it, leaving the key
    /// to ssh, what `y` does instead, and where the choice is written, and draws the keys.
    #[test]
    fn a_prompt_offering_a_key_choice_names_each_key_and_where_the_choice_is_written() {
        let drawn = fully_rendered_run(&a_run_offering_two_keys());
        for wanted in [
            "github.com/acme".to_string(),
            "1  /home/someone/.ssh/id_ed25519.pub".to_string(),
            "2  /home/someone/.ssh/work.pub".to_string(),
            format!("0  {}", t!(run_key_choice_default)),
            t!(run_key_choice_yes).to_string(),
            "/home/someone/.bravebot/reach.jsonl".to_string(),
            t!(run_key_choice_undo).to_string(),
            format!("1-2 {}", t!(run_key_choose)),
            format!("0 {}", t!(run_key_leave)),
        ] {
            assert!(drawn.contains(&wanted), "{wanted:?} was not drawn: {drawn}");
        }
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
                draw_run(frame, request, 0, &mut RowsShown::default());
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

    /// RUN-20: no key here covers a pattern, so the advice must not read as one being offered. The
    /// keys on the row are the same whether the advice is drawn or not.
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
            "a key granted the pattern the advice says a file has to be edited for"
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

    /// The run question drawn at this size from `scroll`, after the draws `shown` remembers.
    fn run_screen(
        request: &RunRequest,
        (width, height): (u16, u16),
        scroll: u16,
        shown: &mut RowsShown,
    ) -> (Vec<String>, RunDrawn) {
        let mut drawn = RunDrawn::default();
        let rows = rows_of(width, height, |frame| {
            drawn = draw_run(frame, request, scroll, shown);
        });
        (rows, drawn)
    }

    /// Where the plan starts once PageDown is pressed at `drawn`.
    fn paged_down(drawn: &RunDrawn, request: &RunRequest, scroll: u16) -> u16 {
        let Some(RunResponse::Page(by)) = drawn.response_to(press(KeyCode::PageDown), request)
        else {
            panic!("PageDown did not page");
        };
        drawn.moved(scroll, i32::from(by) * i32::from(drawn.page))
    }

    /// PROMPT-1, PROMPT-4: a plan longer than the box leaves the keys whole and says how much of it
    /// is unread, where before the argument list pushed the keys off the bottom with no cue that
    /// anything was below.
    #[test]
    fn a_plan_longer_than_the_box_keeps_the_run_keys_and_says_how_many_rows_are_unread() {
        let request = a_run_with_a_long_argument_list();
        let keys = words(&format!(
            "y {} a {} r {} n {} ctrl-c {} tab {}",
            t!(run_yes),
            t!(run_always),
            t!(run_remember),
            t!(run_no),
            t!(stop_the_turn),
            t!(note_key)
        ));
        for size in [(80, 24), (64, 24), (56, 30)] {
            let (rows, drawn) = run_screen(&request, size, 0, &mut RowsShown::default());
            let text = box_text(&rows);
            assert!(
                box_words(&rows).ends_with(&keys),
                "{size:?}: the keys were not whole: {rows:#?}"
            );
            assert!(drawn.unread > 0, "{size:?}: the plan fit: {rows:#?}");
            assert!(
                text.contains(&squeezed(&t!(run_unseen, count = drawn.unread))),
                "{size:?}: nothing said rows were unread: {rows:#?}"
            );
            assert!(!text.contains("--remove-files"), "{size:?}: {rows:#?}");
            assert_eq!(taken(&drawn, &request), [], "{size:?}");
        }
    }

    /// PROMPT-4: `y`, `a` and `r` each run the line, so none is taken while a row of it has not been
    /// on the screen, and `y` is once paging has put every row there. Refusing never waits.
    #[test]
    fn no_key_runs_a_long_plan_until_every_row_of_it_has_been_on_the_screen() {
        let request = a_run_with_a_long_argument_list();
        let mut shown = RowsShown::default();
        let mut scroll = 0;
        let (mut rows, mut drawn) = run_screen(&request, (80, 24), scroll, &mut shown);
        for key in RUNS_IT {
            assert_eq!(
                drawn.response_to(press(KeyCode::Char(key)), &request),
                None,
                "`{key}` ran a line whose last argument was never drawn"
            );
        }
        assert_eq!(
            drawn.response_to(press(KeyCode::Char('n')), &request),
            Some(RunResponse::Answer(RunAnswer::Reject))
        );
        assert_eq!(
            drawn.response_to(
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                &request
            ),
            Some(RunResponse::Answer(RunAnswer::Interrupt))
        );

        let (mut drew_the_last_argument, mut drew_the_access) = (false, false);
        while drawn.unread > 0 {
            assert_eq!(taken(&drawn, &request), [], "{rows:#?}");
            assert!(
                scroll < drawn.furthest,
                "paged to the end and still taken nothing: {rows:#?}"
            );
            scroll = paged_down(&drawn, &request, scroll);
            (rows, drawn) = run_screen(&request, (80, 24), scroll, &mut shown);
            drew_the_last_argument |= box_text(&rows).contains("--remove-files");
            drew_the_access |= box_text(&rows).contains(&squeezed(t!(run_not_sandboxed)));
        }

        assert!(drew_the_last_argument);
        assert!(
            drew_the_access,
            "`y` was taken before the access it spends was drawn"
        );
        assert_eq!(
            drawn.response_to(press(KeyCode::Char('y')), &request),
            Some(RunResponse::Answer(RunAnswer::Approve))
        );
    }

    /// PROMPT-4: `a` and `r` grant more than running the line, so each also waits for the rows
    /// saying what it grants, where `y` waits only for the plan.
    #[test]
    fn a_and_r_each_wait_for_the_rows_saying_what_they_grant_besides_running_the_line() {
        let request = a_run_with_a_long_argument_list();
        let path = request.record.as_ref().expect("recordable").display();
        let path = path.to_string();
        let says = [
            (
                'a',
                [
                    t!(run_always_explained),
                    t!(run_always_output_trusted),
                    t!(run_always_this_directory),
                ],
            ),
            (
                'r',
                [
                    t!(run_remember_explained),
                    t!(run_remember_only_asking),
                    path.as_str(),
                ],
            ),
        ];
        let mut shown = RowsShown::default();
        let mut scroll = 0;
        let mut read = String::new();
        let mut waited = false;
        loop {
            let (rows, drawn) = run_screen(&request, (80, 24), scroll, &mut shown);
            read.push_str(&box_text(&rows));
            read.push('\n');
            let taken = taken(&drawn, &request);
            for (key, sentences) in &says {
                for sentence in sentences.iter().filter(|_| taken.contains(key)) {
                    assert!(
                        read.contains(&squeezed(sentence)),
                        "`{key}` was taken before {sentence:?} was drawn: {rows:#?}"
                    );
                }
            }
            if taken == RUNS_IT {
                break;
            }
            if taken.contains(&'y') {
                waited = true;
                let count = drawn.always_unread + drawn.record_unread;
                assert!(
                    box_text(&rows).contains(&squeezed(&t!(run_grant_unseen, count = count))),
                    "nothing said why {taken:?} were all that was taken: {rows:#?}"
                );
            }
            assert!(scroll < drawn.furthest, "paged to the end: {rows:#?}");
            scroll = paged_down(&drawn, &request, scroll);
        }
        assert!(waited, "no draw took `y` alone, so nothing here waited");
    }

    /// PROMPT-4: having reached the bottom is not having read what was passed over on the way, and
    /// End passes over the plan's arguments, which run as surely as the ones that were drawn.
    #[test]
    fn jumping_to_the_end_of_a_long_plan_leaves_the_rows_passed_over_unread() {
        let request = a_run_with_a_long_argument_list();
        let mut shown = RowsShown::default();
        let (_, top) = run_screen(&request, (80, 24), 0, &mut shown);
        let Some(RunResponse::Scroll(by)) = top.response_to(press(KeyCode::End), &request) else {
            panic!("End did not scroll");
        };
        let (rows, end) = run_screen(&request, (80, 24), top.moved(0, i32::from(by)), &mut shown);

        assert_eq!(top.moved(0, i32::from(by)), end.furthest);
        assert!(end.unread > 0, "End read the rows it passed over");
        assert!(
            box_text(&rows).contains(&squeezed(&t!(run_unseen, count = end.unread))),
            "the bottom said nothing was left to read: {rows:#?}"
        );
        for key in RUNS_IT {
            assert_eq!(
                end.response_to(press(KeyCode::Char(key)), &request),
                None,
                "`{key}` ran a line whose arguments were passed over"
            );
        }
    }

    /// PROMPT-4: a box too small for the question or its keys takes no key that runs the line,
    /// wherever the plan is scrolled to and whatever an earlier, larger draw showed. A short prompt
    /// in a box that holds it is answered at once, so the wait costs nothing where nothing is unread.
    #[test]
    fn a_run_prompt_too_small_for_its_question_takes_no_key_that_runs_the_line() {
        for request in [a_recordable_run(), a_run_with_a_long_argument_list()] {
            for size in [(80, 3), (80, 4), (24, 6), (2, 40), (1, 1)] {
                let mut shown = RowsShown::default();
                let (_, first) = run_screen(&request, size, 0, &mut shown);
                for scroll in 0..=first.furthest {
                    let (rows, drawn) = run_screen(&request, size, scroll, &mut shown);
                    assert_eq!(
                        taken(&drawn, &request),
                        [],
                        "{size:?} at {scroll}: {rows:#?}"
                    );
                }
            }
        }

        let request = a_recordable_run();
        let mut shown = RowsShown::default();
        let (rows, whole) = run_screen(&request, (80, 40), 0, &mut shown);
        assert_eq!(
            taken(&whole, &request),
            RUNS_IT,
            "a prompt that fits waited to be scrolled: {rows:#?}"
        );
        let (rows, cut) = run_screen(&request, (80, 4), 0, &mut shown);
        assert_eq!(
            taken(&cut, &request),
            [],
            "a prompt read in a larger box was answered from one that cut it off: {rows:#?}"
        );
    }

    /// PROMPT-4: another width wraps the plan into other rows, so a plan read at one width is unread
    /// at the next until its rows there have been on the screen.
    #[test]
    fn a_plan_read_at_one_width_is_unread_again_at_another() {
        let request = a_run_with_a_long_argument_list();
        let mut shown = RowsShown::default();
        let mut scroll = 0;
        let (_, mut drawn) = run_screen(&request, (80, 24), scroll, &mut shown);
        while drawn.unread > 0 && scroll < drawn.furthest {
            scroll = paged_down(&drawn, &request, scroll);
            (_, drawn) = run_screen(&request, (80, 24), scroll, &mut shown);
        }
        assert!(taken(&drawn, &request).contains(&'y'));

        let (rows, fresh) = run_screen(&request, (120, 24), 0, &mut RowsShown::default());
        assert!(fresh.unread > 0, "the plan fit the wider box: {rows:#?}");

        let (rows, wider) = run_screen(&request, (120, 24), 0, &mut shown);
        assert_eq!(
            wider.unread, fresh.unread,
            "rows read at 80 columns were counted at 120"
        );
        assert_eq!(taken(&wider, &request), [], "{rows:#?}");
    }

    /// PROMPT-4: a page is one row fewer than the box shows, so paging through a plan in a box
    /// shorter than ten rows puts every row on the screen and every key that runs it is then taken.
    #[test]
    fn paging_through_a_long_plan_in_a_short_box_puts_every_row_on_the_screen() {
        let request = a_run_with_a_long_argument_list();
        let mut shown = RowsShown::default();
        let mut scroll = 0;
        let (rows, mut drawn) = run_screen(&request, (80, 14), scroll, &mut shown);
        // Ten rows with the question and the keys in them leaves the plan fewer than ten.
        assert!(
            box_rows(&rows).len() <= 10,
            "the box was not short: {rows:#?}"
        );
        while taken(&drawn, &request) != RUNS_IT {
            assert!(
                scroll < drawn.furthest,
                "paging passed over {} rows: {drawn:?}",
                drawn.unread + drawn.always_unread + drawn.record_unread
            );
            scroll = paged_down(&drawn, &request, scroll);
            (_, drawn) = run_screen(&request, (80, 14), scroll, &mut shown);
        }
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
}
