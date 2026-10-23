//! Putting the planner's question to the user, in the terminal.
//!
//! The same discipline as [`crate::confirm`], for the same reason: the turn is blocked waiting,
//! so an unreadable terminal, a lost event stream, or a key nobody defined must resolve to a
//! definite answer rather than to a hang. Here that answer is [`Answer::Declined`], which the
//! kernel turns into words the planner can act on.
//!
//! Reached through [`crate::confirm::TerminalConfirmer`], which is the terminal's whole side of
//! the human channel: it draws a diff for a write and this picker for a question.
//!
//! Declining is always one keypress away. A question the user does not want is a question they
//! can dismiss, and a picker with no exit would make the model able to stall the session.
//!
//! A person who has walked away is not declining, and the picker is told how long to wait for them
//! (ASK-9). Past that, every question still open is closed as [`Answer::TimedOut`], which the planner
//! reads as no answer from somebody who may be away.

use bravebot_core::ask::{Answer, Asking, Prompt, Row};
use bravebot_i18n::t;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::crossterm::event::{self, Event, KeyCode};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use std::io;
use std::time::{Duration, Instant};

use crate::input;
use crate::state::Session;
use crate::theme;

/// Longest answer the user can type.
///
/// Generous for a sentence and far short of anything that would take the field off screen. The
/// answer is one line, not a document: a long explanation belongs in the next prompt.
const MAX_TYPED: usize = 500;

/// Columns the marker occupies, and so the indent a detail line sits at to align under its label.
const MARKER_WIDTH: usize = 4;

/// What the row offering the free-text field says.
///
/// An entry in the list rather than only a key, because an affordance nobody can see is one most
/// people never find, and answering in your own words is the way out of a set of options that
/// does not contain the answer.
fn own_words() -> &'static str {
    t!(ask_own_words)
}

/// How many rows of the box a line takes once the paragraph has wrapped it.
///
/// Through the wrapping that will draw it, so the budget and the drawing agree. They did not:
/// the budget counted lines, and a question sentence or an option label that wrapped cost the
/// box a row nothing had reserved, which the paragraph then clipped in silence.
fn drawn_rows(line: &Line<'_>, inside: u16) -> usize {
    crate::render::rows_of(line, inside) as usize
}

/// The free-text field as it is drawn: the prompt, what has been typed, and the caret.
///
/// One place, so what the budget measures is what the box draws. They were two, and the budget's
/// copy was the constant 2 whatever had been typed.
fn typed_line(text: &str) -> Vec<Span<'_>> {
    vec![
        Span::styled(
            "  > ",
            Style::default()
                .fg(theme::brand_primary())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(text),
        Span::styled("▏", Style::default().fg(theme::brand_primary())),
    ]
}

/// A row of the list as it will be drawn: the marker's four columns, then the text.
///
/// Every marker is [`MARKER_WIDTH`] columns and ends on a space, and a detail sits at the same
/// indent, so this wraps where the row it stands for will wrap.
fn indented(text: &str) -> Line<'_> {
    Line::from(vec![Span::raw(" ".repeat(MARKER_WIDTH)), Span::raw(text)])
}

/// Widest a tag is drawn, in characters.
///
/// Capped here rather than in the kernel because it is a fact about the box, not about the
/// question: what the tag says is the model's, how much of it fits is the terminal's. A tag that
/// ran on would push the question it labels off the line and cost the person the sentence they
/// have to answer.
const CHIP_WIDTH: usize = 12;

/// Put a series of questions and wait for the answers.
///
/// Standalone because a turn running on a worker thread cannot hold the terminal: the main
/// thread calls this on its behalf.
///
/// Returns no answers wherever it cannot ask, which the kernel reads as a decline for every
/// question. Saying nothing is the one reply that cannot be wrong about how many questions there
/// were.
pub fn ask<B: Backend>(terminal: &mut Terminal<B>, asking: &Asking) -> Vec<Answer> {
    ask_within(terminal, asking, None)
}

/// [`ask`] for the planner's questions, closing them without an answer once `idle` passes with no
/// key pressed (ASK-9).
///
/// `None` waits for as long as it takes, which is [`ask`]. The time is the person's own setting and
/// the clock is the only thing it is compared with, so nothing a question or an answer says
/// reaches the decision. Any key restarts it, and a series that is answered a question at a time
/// gets the whole time again for each.
pub fn ask_within<B: Backend>(
    terminal: &mut Terminal<B>,
    asking: &Asking,
    idle: Option<Duration>,
) -> Vec<Answer> {
    ask_titled(terminal, asking, None, idle)
}

/// [`ask`] for a question the driver puts and not the planner, so the box is not called "the agent
/// is asking".
///
/// A person who sees that title takes the question for a request from the model, which is what
/// they are taught to read with suspicion. `title` is a sentence from this program's own catalog.
pub fn ask_as<B: Backend>(terminal: &mut Terminal<B>, asking: &Asking, title: &str) -> Vec<Answer> {
    ask_titled(terminal, asking, Some(title), None)
}

fn ask_titled<B: Backend>(
    terminal: &mut Terminal<B>,
    asking: &Asking,
    title: Option<&str>,
    idle: Option<Duration>,
) -> Vec<Answer> {
    if asking.prompts.is_empty() {
        return Vec::new();
    }
    let mut picker = Picker::new(asking);
    picker.named = title.map(str::to_string);
    converse(terminal, &mut picker, idle, &mut Keyboard)
}

/// Where the picker's events and its clock come from, so a test can say what arrives when.
trait Events {
    fn now(&mut self) -> Instant;

    /// The next event, or `None` once `within` has passed with none. A `within` of `None` waits for
    /// one however long that is.
    fn next(&mut self, within: Option<Duration>) -> io::Result<Option<Event>>;
}

/// The terminal and the system clock.
struct Keyboard;

impl Events for Keyboard {
    fn now(&mut self) -> Instant {
        Instant::now()
    }

    fn next(&mut self, within: Option<Duration>) -> io::Result<Option<Event>> {
        if let Some(within) = within
            && !input::poll(within)?
        {
            return Ok(None);
        }
        input::read().map(Some)
    }
}

/// Draw the question, take keys, and return the answers once there is nothing left to ask.
///
/// `idle` is how long the person may go without pressing a key before every question still open is
/// closed with [`Answer::TimedOut`] (ASK-9). It is measured from the last key press, or from when
/// the questions were put, and nothing else restarts it: a release, the pointer and a change of
/// size are not somebody answering.
fn converse<B: Backend>(
    terminal: &mut Terminal<B>,
    picker: &mut Picker<'_>,
    idle: Option<Duration>,
    events: &mut impl Events,
) -> Vec<Answer> {
    let mut quiet_since = events.now();

    loop {
        let left = idle
            .map(|idle| idle.saturating_sub(events.now().saturating_duration_since(quiet_since)));

        // A terminal that cannot show the question cannot collect a considered answer, so
        // decline rather than take a keypress against something nobody saw.
        if terminal.draw(|frame| picker.draw(frame)).is_err() {
            return Vec::new();
        }

        let key = match events.next(left) {
            Ok(Some(taken)) => match input::key_of(&taken) {
                // Presses only. The interface asks the terminal for disambiguated keys, which
                // reports releases too, and a release taken for a press answers the next question
                // with the key that answered this one.
                Some(key) if key.kind == event::KeyEventKind::Press => key.code,
                _ => continue,
            },
            // Nothing arrived in the time left. With none left, the person was not there: a key
            // pressed before the time was up is still waiting when this loop was late, and a wait
            // of no length takes it first.
            Ok(None) if left == Some(Duration::ZERO) => return picker.time_out(),
            Ok(None) => continue,
            // Losing the event stream must not invent an answer.
            Err(_) => return Vec::new(),
        };
        quiet_since = events.now();

        match picker.press(key) {
            Step::Waiting => continue,
            Step::Answered(answers) => return answers,
        }
    }
}

/// What a keypress did.
enum Step {
    Waiting,
    Answered(Vec<Answer>),
}

/// Where the person is within the question on screen.
///
/// Its own value so moving to the next question is one assignment. Marks and half-typed text
/// belong to the question they were made against, and carrying either into the next one would
/// put words in the person's mouth.
struct State {
    cursor: usize,
    /// Toggled options, in the order the user picked them, so the planner reads them back in the
    /// order they were chosen rather than in the order the model wrote them.
    chosen: Vec<usize>,
    /// The free-text field, once opened.
    typed: Option<String>,
    /// First option drawn, so a long list scrolls rather than hiding the cursor.
    offset: usize,
}

impl State {
    fn starting(prompt: &Prompt) -> Self {
        Self {
            cursor: 0,
            chosen: Vec::new(),
            // A question with no options can only be answered in the user's own words, so the
            // field opens itself rather than presenting an empty list to navigate.
            typed: prompt.rows.is_empty().then(String::new),
            offset: 0,
        }
    }
}

/// The picker's state between keypresses.
///
/// Separate from the drawing so the key discipline can be tested without a terminal: what a key
/// does is the part that must not drift.
struct Picker<'a> {
    asking: &'a Asking,
    /// Which question is on screen.
    at: usize,
    /// Settled so far, one per question, in the order they were asked.
    answers: Vec<Answer>,
    here: State,
    /// What the box is called when the driver asked and the planner did not.
    named: Option<String>,
    /// The step after the last question of a series, where every answer is listed and nothing has
    /// yet been sent (ASK-11). `at` stays on the last question while this is up.
    reviewing: bool,
    /// Set while a question is reopened from the review, so settling it goes back to the review
    /// rather than on through the questions after it.
    returning: bool,
}

impl<'a> Picker<'a> {
    fn new(asking: &'a Asking) -> Self {
        Self {
            asking,
            at: 0,
            answers: Vec::new(),
            here: State::starting(&asking.prompts[0]),
            named: None,
            reviewing: false,
            returning: false,
        }
    }

    fn prompt(&self) -> &Prompt {
        &self.asking.prompts[self.at]
    }

    /// Where the free-text row sits: one past the last option.
    fn own_words(&self) -> usize {
        self.prompt().rows.len()
    }

    fn on_own_words(&self) -> bool {
        self.here.cursor == self.own_words()
    }

    /// Take this question's answer and move on, finishing when there is nothing left to ask.
    ///
    /// [`Asking::ends_at`] decides the series is over, on the number of questions and never on
    /// what any of them was answered with. That is what makes skipping and finishing the same
    /// operation: an answer the person declined is still an answer to the question they were on.
    ///
    /// The end of a series of more than one is the review and not the return (ASK-11): nothing
    /// leaves the picker until the person confirms it there. A question reopened from the review
    /// goes back to it, since the rest of the series is already answered.
    fn settle(&mut self, answer: Answer) -> Step {
        let ends = self.asking.ends_at(self.at);
        match self.answers.get_mut(self.at) {
            Some(earlier) => *earlier = answer,
            None => self.answers.push(answer),
        }
        let several = self.asking.prompts.len() > 1;
        if several && (ends || self.returning) {
            self.reviewing = true;
            self.returning = false;
            return Step::Waiting;
        }
        if ends {
            return Step::Answered(std::mem::take(&mut self.answers));
        }
        self.reopen(self.at + 1);
        Step::Waiting
    }

    /// Put question `at` on screen, with the mark or the words the person gave it if they have
    /// answered it before. A question not answered yet starts clean.
    fn reopen(&mut self, at: usize) {
        self.at = at;
        self.reviewing = false;
        self.here = State::starting(self.prompt());
        match self.answers.get(at) {
            Some(Answer::Chosen(picked)) => {
                self.here.cursor = self
                    .prompt()
                    .rows
                    .iter()
                    .position(|row| picked.first() == Some(&row.index))
                    .unwrap_or(0);
                self.here.chosen = picked.clone();
            }
            Some(Answer::Typed(text)) => {
                self.here.cursor = self.own_words();
                self.here.typed = Some(text.clone());
            }
            _ => {}
        }
    }

    /// Close every question still open, the one on screen among them, because nobody pressed a key
    /// in time.
    ///
    /// What is already settled stays. What is marked or half typed on the question on screen is not
    /// kept: the person never confirmed it, and reporting an unconfirmed mark as their answer is
    /// the invented reply ASK-7 rules out. A question the person went back to keeps the answer it
    /// had, and so do those after it, since each was given: only a question never answered is
    /// closed.
    fn time_out(&mut self) -> Vec<Answer> {
        let mut answers = std::mem::take(&mut self.answers);
        answers.resize(self.asking.prompts.len(), Answer::TimedOut);
        answers
    }

    /// What a key does on the review: send, or go back to a question.
    ///
    /// A digit is the position of the question, as the title counts them, and escape reopens the
    /// last one. Neither declines anything: escape here is the one way out that does not send, and
    /// the answers already given are kept (ASK-11).
    fn press_review(&mut self, key: KeyCode) -> Step {
        let last = self.asking.prompts.len() - 1;
        let target = match key {
            KeyCode::Enter => return Step::Answered(std::mem::take(&mut self.answers)),
            KeyCode::Esc | KeyCode::Left | KeyCode::Char('h') => last,
            KeyCode::Char(digit) => match digit.to_digit(10) {
                Some(nth @ 1..) if nth as usize <= last + 1 => nth as usize - 1,
                _ => return Step::Waiting,
            },
            _ => return Step::Waiting,
        };
        self.returning = true;
        self.reopen(target);
        Step::Waiting
    }

    fn press(&mut self, key: KeyCode) -> Step {
        if self.reviewing {
            return self.press_review(key);
        }
        match &mut self.here.typed {
            Some(text) => match key {
                KeyCode::Char(c) if text.chars().count() < MAX_TYPED => {
                    text.push(c);
                    Step::Waiting
                }
                KeyCode::Backspace => {
                    text.pop();
                    Step::Waiting
                }
                KeyCode::Enter => {
                    let answer = text.trim().to_string();
                    if answer.is_empty() {
                        // Nothing typed is not an answer. Escape is how you leave.
                        return Step::Waiting;
                    }
                    self.settle(Answer::Typed(answer))
                }
                // Back to the list, or out altogether if there was no list to begin with.
                KeyCode::Esc => {
                    self.here.typed = None;
                    if self.prompt().rows.is_empty() {
                        return self.settle(Answer::Declined);
                    }
                    Step::Waiting
                }
                _ => Step::Waiting,
            },
            None => match key {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.here.cursor = self.here.cursor.saturating_sub(1);
                    Step::Waiting
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    // Past the last option sits the free-text row, so the range is one longer
                    // than the list.
                    self.here.cursor = (self.here.cursor + 1).min(self.own_words());
                    Step::Waiting
                }
                // Marks in both kinds of question. A key that works on one and silently does
                // nothing on the other reads as broken, and the person presses it again rather
                // than reaching for enter.
                KeyCode::Char(' ') if !self.on_own_words() => {
                    self.toggle(self.here.cursor);
                    Step::Waiting
                }
                // On the free-text row both keys mean the same thing, which is what the row is
                // for: there is nothing there to mark.
                KeyCode::Char(' ') | KeyCode::Char('o') => {
                    self.here.typed = Some(String::new());
                    Step::Waiting
                }
                KeyCode::Enter if self.on_own_words() => {
                    self.here.typed = Some(String::new());
                    Step::Waiting
                }
                // Confirms what is toggled, or the option under the cursor when nothing is. One
                // rule for both kinds of question, and no state the user cannot get out of.
                KeyCode::Enter => {
                    let picked = if self.here.chosen.is_empty() {
                        self.prompt()
                            .rows
                            .get(self.here.cursor)
                            .map(|row| vec![row.index])
                            .unwrap_or_default()
                    } else {
                        self.here.chosen.clone()
                    };
                    if picked.is_empty() {
                        return Step::Waiting;
                    }
                    self.settle(Answer::Chosen(picked))
                }
                KeyCode::Esc => self.settle(Answer::Declined),
                // Back to the question before, with what was answered there. What is marked on
                // this one is not carried, as it belongs to this one.
                KeyCode::Left | KeyCode::Char('h') if self.at > 0 => {
                    // Walking back by hand is walking the series again, so what follows is the
                    // next question and not the review.
                    self.returning = false;
                    self.reopen(self.at - 1);
                    Step::Waiting
                }
                _ => Step::Waiting,
            },
        }
    }

    /// Mark or unmark the option under the cursor.
    ///
    /// Where the question takes one answer, marking a second replaces the first: a list that let
    /// two be marked would then have to decide between them, and the person would have said one
    /// thing and been reported as saying another.
    fn toggle(&mut self, cursor: usize) {
        let Some(index) = self.prompt().rows.get(cursor).map(|row| row.index) else {
            return;
        };
        let multiple = self.prompt().multiple;
        match self.here.chosen.iter().position(|i| *i == index) {
            Some(at) => {
                self.here.chosen.remove(at);
            }
            None => {
                if !multiple {
                    self.here.chosen.clear();
                }
                self.here.chosen.push(index);
            }
        }
    }

    fn is_chosen(&self, row: &Row) -> bool {
        self.here.chosen.contains(&row.index)
    }

    fn draw(&mut self, frame: &mut ratatui::Frame) {
        if self.reviewing {
            return self.draw_review(frame);
        }
        let area = centred(frame.area());
        frame.render_widget(Clear, area);

        let mut lines = Vec::new();
        if let Some(chip) = chip(&self.prompt().header) {
            lines.push(Line::from(chip));
        }
        lines.push(Line::from(Span::styled(
            self.prompt().question.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::raw(""));

        // An option costs a line for its label and another for its detail. Where any option has
        // one, the list is spaced out as well, since two unspaced options each carrying a second
        // line read as four options rather than two.
        let spaced = self.prompt().rows.iter().any(|row| row.detail.is_some());
        let gap = usize::from(spaced);

        // Everything drawn below the list: the two borders, the spacer above the tail, and the
        // tail itself. With the field open that is the field and its own keys; otherwise it is
        // the free-text row, whatever space is kept above it, and the key hints.
        //
        // Every one of these is measured in the rows the box will draw it as rather than in the
        // lines it was written as. The hints wrap on a narrow box, and so does the question
        // sentence; a line counted as a row is a line whose second row lands on something else.
        let inside = area.width.saturating_sub(2).max(1);
        let tail = match &self.here.typed {
            // The field is one line and, past a sentence or so, several rows. Counted as one, a
            // person typing pushes their own way out of the field off the bottom of the box.
            Some(text) => {
                drawn_rows(&Line::from(typed_line(text)), inside)
                    + drawn_rows(&Line::from(self.field_keys()), inside)
            }
            None => drawn_rows(&Line::from(self.keys()), inside) + 1 + gap,
        };
        let reserved = lines
            .iter()
            .map(|line| drawn_rows(line, inside))
            .sum::<usize>()
            + 3
            + tail;
        let budget = (area.height as usize).saturating_sub(reserved).max(1);
        // An option costs the rows its label wraps onto, not one. Costed at one apiece, a list of
        // ordinary sentences fits twice over and the half that does not fit is drawn past the
        // bottom border: neither on screen, nor counted in `hidden`, nor reachable by scrolling.
        let height = |row: &Row, first: bool| {
            drawn_rows(&indented(&row.label), inside)
                + row
                    .detail
                    .as_deref()
                    .map_or(0, |detail| drawn_rows(&indented(detail), inside))
                + if first { 0 } else { gap }
        };

        self.scroll_to_cursor(budget, height);

        // Twice, because the line saying how many options were cut is itself a line, and it
        // exists only once something has been cut. Fitting the list first and then discovering
        // there is no room left to say so is how an option ends up silently off the bottom.
        let mut visible = self.fitting(budget, height);
        if visible < self.prompt().rows.len() {
            visible = self.fitting(budget.saturating_sub(1), height);
        }

        for (nth, row) in self
            .prompt()
            .rows
            .iter()
            .skip(self.here.offset)
            .take(visible)
            .enumerate()
        {
            if nth > 0 && spaced {
                lines.push(Line::raw(""));
            }

            let here = row.index == self.here.cursor;
            // A mark has to be visible or pressing space looks like nothing happened. On a
            // one-answer question the cursor arrow gives way to the mark, so the list stays
            // uncluttered until the person actually picks something.
            let marker = if self.prompt().multiple {
                if self.is_chosen(row) { "[x] " } else { "[ ] " }
            } else if self.is_chosen(row) {
                "  ◉ "
            } else if here {
                "  › "
            } else {
                "    "
            };
            let style = if here {
                Style::default()
                    .fg(theme::brand_primary())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            lines.push(Line::from(vec![
                Span::styled(marker, style),
                Span::styled(row.label.clone(), style),
            ]));

            // Beneath the label rather than beside it, aligned under it, so a long explanation
            // cannot push the option it belongs to off the edge of the box.
            if let Some(detail) = &row.detail {
                lines.push(Line::from(vec![
                    Span::raw(" ".repeat(MARKER_WIDTH)),
                    Span::styled(detail.clone(), Style::default().fg(theme::muted())),
                ]));
            }
        }

        // A list cut short must say so, or an option nobody scrolled to reads as an option that
        // was never offered.
        let hidden = self.prompt().rows.len() - visible;
        if hidden > 0 {
            lines.push(Line::from(Span::styled(
                format!("    {}", t!(ask_more_options, count = hidden)),
                Style::default().fg(theme::muted()),
            )));
        }

        // Last in the list, and drawn only while the field it opens is closed: with the field on
        // screen the person is already answering in their own words, and a row inviting them to
        // do what they are doing is one more thing to read.
        //
        // No mark of its own in either kind of question, because it is a way in rather than a
        // choice: a checkbox beside it would say it could be picked alongside the options.
        if self.here.typed.is_none() {
            if spaced {
                lines.push(Line::raw(""));
            }
            let style = if self.on_own_words() {
                Style::default()
                    .fg(theme::brand_primary())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::muted())
            };
            lines.push(Line::from(vec![
                Span::styled(
                    if self.on_own_words() {
                        "  › "
                    } else {
                        "    "
                    },
                    style,
                ),
                Span::styled(own_words(), style),
            ]));
        }

        lines.push(Line::raw(""));
        match &self.here.typed {
            Some(text) => {
                lines.push(Line::from(typed_line(text)));
                // The field needs its own keys. Drawing none, as this did, leaves a person who
                // opened it with no way out that they can see, and the way out is the hint they
                // most need.
                lines.push(Line::from(self.field_keys()));
            }
            None => lines.push(Line::from(self.keys())),
        }

        frame.render_widget(
            Paragraph::new(lines)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(theme::brand_primary()))
                        .title(self.title()),
                )
                .wrap(Wrap { trim: false }),
            area,
        );
    }

    /// The review: every question beside what the person did with it, and the keys to send or to
    /// go back (ASK-11).
    ///
    /// The tag and the options are the ones the questions were drawn with, and the words are the
    /// person's own, so nothing is drawn here that the person has not already been shown. Each
    /// question and each answer is cut to the one row it is given, by the columns it takes and not
    /// the characters, so four questions and their keys fit a box that [`centred`] has made as
    /// short as it will: an answer that wrapped would push the keys, which are how the person
    /// sends, off the bottom.
    fn draw_review(&self, frame: &mut ratatui::Frame) {
        let area = centred(frame.area());
        frame.render_widget(Clear, area);
        let inside = usize::from(area.width.saturating_sub(2).max(1));
        let room = inside.saturating_sub(MARKER_WIDTH).max(1);

        let mut lines = vec![Line::from(Span::styled(
            t!(ask_review_heading),
            Style::default().add_modifier(Modifier::BOLD),
        ))];
        lines.push(Line::raw(""));
        for (nth, prompt) in self.asking.prompts.iter().enumerate() {
            let label = match chip(&prompt.header) {
                Some(chip) => chip,
                None => Span::raw(clipped(&prompt.question, room.saturating_sub(3))),
            };
            // A tag is at most CHIP_WIDTH wide and the number takes three, so the row fits unless
            // the box is narrower than both.
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {} ", nth + 1),
                    Style::default()
                        .fg(theme::brand_primary())
                        .add_modifier(Modifier::BOLD),
                ),
                label,
            ]));
            let (said, style) = match self.answers.get(nth) {
                Some(Answer::Chosen(picked)) => (
                    picked
                        .iter()
                        .filter_map(|index| prompt.rows.iter().find(|row| row.index == *index))
                        .map(|row| row.label.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    Style::default(),
                ),
                Some(Answer::Typed(text)) => (text.clone(), Style::default()),
                _ => (
                    t!(ask_review_skipped).to_string(),
                    Style::default().fg(theme::muted()),
                ),
            };
            lines.push(Line::from(vec![
                Span::raw(" ".repeat(MARKER_WIDTH)),
                Span::styled(clipped(&said, room), style),
            ]));
        }
        lines.push(Line::raw(""));
        lines.push(Line::from(self.review_keys()));

        frame.render_widget(
            Paragraph::new(lines)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(theme::brand_primary()))
                        .title(self.title()),
                )
                .wrap(Wrap { trim: false }),
            area,
        );
    }

    /// Keys for the review. The way back is named beside the way on, since the point of the
    /// screen is that nothing has been sent.
    fn review_keys(&self) -> Vec<Span<'static>> {
        let bold = Style::default()
            .fg(theme::brand_primary())
            .add_modifier(Modifier::BOLD);
        vec![
            Span::styled("  enter", bold),
            Span::raw(format!(" {}   ", t!(ask_key_send))),
            Span::styled(format!("1-{}", self.asking.prompts.len()), bold),
            Span::raw(format!(" {}   ", t!(ask_key_change))),
            Span::styled("esc", bold),
            Span::raw(format!(" {}", t!(ask_key_back))),
        ]
    }

    /// What the box is called, and where the person is in the series.
    ///
    /// A lone question is not counted: "1 of 1" is noise on the common case, and the count is
    /// there to tell someone partway through a series how much of it is left.
    fn title(&self) -> String {
        if let Some(named) = &self.named {
            return format!(" {named} ");
        }
        if self.reviewing {
            return format!(" {} ", t!(ask_review_title));
        }
        let said = bravebot_i18n::ask::place(self.at, self.asking.prompts.len())
            .unwrap_or_else(|| t!(ask_title).to_string());
        format!(" {said} ")
    }

    fn keys(&self) -> Vec<Span<'static>> {
        let bold = Style::default()
            .fg(theme::brand_primary())
            .add_modifier(Modifier::BOLD);
        // Short enough to stay on one line in a narrow terminal. A key hint that wraps is a key
        // hint the person skims past, and the way out is the one they must not miss.
        let mut spans = vec![
            Span::styled("  ↑↓", bold),
            Span::raw(format!(" {}   ", t!(ask_key_move))),
        ];
        spans.push(Span::styled("space", bold));
        spans.push(Span::raw(format!(
            " {}   ",
            if self.prompt().multiple {
                t!(ask_key_pick_any)
            } else {
                t!(ask_key_pick_one)
            }
        )));
        spans.push(Span::styled("enter", bold));
        spans.push(Span::raw(format!(" {}   ", t!(ask_key_answer))));
        // Only where there is a question to go back to, so the first question's line is the one
        // it always was.
        if self.at > 0 {
            spans.push(Span::styled("←", bold));
            spans.push(Span::raw(format!(" {}   ", t!(ask_key_back))));
        }
        // No hint for the free-text field: it is a row in the list now, and a line this short
        // cannot afford to say twice what the person can already read.
        spans.push(Span::styled(
            "esc",
            Style::default()
                .fg(theme::fail())
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(format!(" {}", t!(ask_key_skip))));
        spans
    }

    /// Keys for the free-text field, which are not the keys for the list.
    fn field_keys(&self) -> Vec<Span<'static>> {
        let bold = Style::default()
            .fg(theme::brand_primary())
            .add_modifier(Modifier::BOLD);
        vec![
            Span::styled("  enter", bold),
            Span::raw(format!(" {}   ", t!(ask_key_answer))),
            Span::styled(
                "esc",
                Style::default()
                    .fg(theme::fail())
                    .add_modifier(Modifier::BOLD),
            ),
            // Escape goes back to the options where there are any, and out where there are none.
            Span::raw(format!(
                " {}",
                if self.prompt().rows.is_empty() {
                    t!(ask_key_skip_question)
                } else {
                    t!(ask_key_back_to_options)
                }
            )),
        ]
    }

    /// How many options fit in this many lines, counting from the first one drawn.
    ///
    /// At least one, always: a box too small for even one option should show the option and be
    /// cut off, rather than show an empty list and say everything is hidden.
    fn fitting(&self, budget: usize, height: impl Fn(&Row, bool) -> usize) -> usize {
        let mut used = 0;
        let mut visible = 0;
        for (nth, row) in self.prompt().rows.iter().skip(self.here.offset).enumerate() {
            let cost = height(row, nth == 0);
            if visible > 0 && used + cost > budget {
                break;
            }
            used += cost;
            visible += 1;
        }
        visible
    }

    /// Move the window as little as it takes to put the cursor in it.
    ///
    /// Row by row rather than against an estimate of how many options fit, because once a label
    /// wraps they are of different heights and how many fit depends on which ones. An estimate
    /// taken from the tallest of them scrolls a list of short options that had room for all of
    /// them; one taken from the average leaves the cursor below the last drawn row.
    fn scroll_to_cursor(&mut self, budget: usize, height: impl Fn(&Row, bool) -> usize) {
        let rows = &self.asking.prompts[self.at].rows;
        if rows.is_empty() {
            return;
        }

        // Room for the whole list is the one case measured against the whole budget: nothing is
        // hidden, so no line is spent saying what was cut, and there is nothing to scroll.
        let whole: usize = rows
            .iter()
            .enumerate()
            .map(|(nth, row)| height(row, nth == 0))
            .sum();
        if whole <= budget {
            self.here.offset = 0;
            return;
        }
        // Otherwise something is hidden whatever the window is, so the list is fitted a line
        // shorter to leave room for saying so, and the window is measured against that.
        let budget = budget.saturating_sub(1);

        // The free-text row is drawn below the list rather than in it, so a cursor resting there
        // scrolls the list to its end and no further.
        let cursor = self.here.cursor.min(rows.len() - 1);
        if cursor < self.here.offset {
            self.here.offset = cursor;
            return;
        }

        // Backwards from the cursor, to the earliest row the window can start at and still reach
        // it. Taking in the row before costs that row, and costs the row it displaces the spacer
        // that every row but the first carries.
        let mut earliest = cursor;
        let mut used = height(&rows[cursor], true);
        while earliest > 0 {
            let displaced = height(&rows[earliest], false) - height(&rows[earliest], true);
            let cost = used + displaced + height(&rows[earliest - 1], true);
            if cost > budget {
                break;
            }
            used = cost;
            earliest -= 1;
        }
        self.here.offset = self.here.offset.max(earliest);
    }
}

/// Put a series to the person, asking only what the session has not been answered already.
///
/// A planner that loops back over the same decision does not make the person restate it. The note
/// is what keeps that from being invisible: an answer given once and reused silently would look
/// like a question that was never asked. `ask` draws the questions still outstanding, so the count
/// in its title is the number the person has to answer. What the person answers is remembered for
/// the next time, and a question `ask` returned no answer for is drawn again (ASK-8). So is one
/// `ask` closed on the clock, which the transcript says was closed (ASK-9).
pub fn put(
    session: &mut Session,
    asking: &Asking,
    ask: impl FnOnce(&Asking) -> Vec<Answer>,
) -> Vec<Answer> {
    let settled = session.answers.read(asking);
    for prompt in settled.reused() {
        session.note(t!(session_answered_already, question = &prompt.question));
    }

    let fresh = ask(settled.outstanding());
    session.note_closed_on_the_clock(&fresh);

    settled.finish(&mut session.answers, fresh)
}

/// `text` cut to `limit` columns, the last of them an ellipsis where something was cut.
///
/// Columns and not characters, because a wide character takes two and a row measured in
/// characters wraps. Newlines and tabs are one space, since a typed answer is one line but a
/// planner's question need not be.
fn clipped(text: &str, limit: usize) -> String {
    let flat: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let width = |text: &str| Line::from(text.to_string()).width();
    if width(&flat) <= limit {
        return flat;
    }
    let mut cut = String::new();
    for c in flat.chars() {
        cut.push(c);
        if width(&cut) >= limit {
            cut.pop();
            break;
        }
    }
    cut.push('\u{2026}');
    cut
}

/// The tag a question is shown under, or nothing where it has none.
///
/// Reversed rather than merely coloured so it reads as a tag at a glance, which is the whole
/// point of it: with several questions in a series the sentence says what is being asked and the
/// tag says which of the pending decisions this one is.
fn chip(header: &str) -> Option<Span<'static>> {
    if header.is_empty() {
        return None;
    }
    let mut text: String = header.chars().take(CHIP_WIDTH).collect();
    if header.chars().count() > CHIP_WIDTH {
        text = header.chars().take(CHIP_WIDTH - 1).collect();
        text.push('\u{2026}');
    }
    Some(Span::styled(
        format!(" {text} "),
        Style::default()
            .fg(theme::brand_primary())
            .add_modifier(Modifier::REVERSED),
    ))
}

/// A centred box, sized to the terminal but never larger than it.
fn centred(area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(15),
            Constraint::Percentage(70),
            Constraint::Percentage(15),
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
    use bravebot_core::ask::{Choice, Question};
    use ratatui::backend::TestBackend;

    fn prompt(multiple: bool) -> Prompt {
        bravebot_core::ask::prompt(&Question::new(
            "Cache layer",
            "Which cache layer?",
            vec![
                Choice::new("HTTP", Some("in front of the handler".into())),
                Choice::new("Query", None),
                Choice::new("Neither", None),
            ],
            multiple,
        ))
    }

    /// A question the driver puts is not titled as the planner's. The regression it rejects is the
    /// `/sandbox off` question drawn under "the agent is asking", which reads as the model asking to
    /// be unconfined, and a title that is ignored where the planner asks.
    #[test]
    fn a_question_the_driver_puts_is_not_titled_as_the_agents() {
        let asking = one(&prompt(false));
        let top_row = |named: Option<&str>| {
            let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
            let mut picker = Picker::new(&asking);
            picker.named = named.map(str::to_string);
            terminal.draw(|frame| picker.draw(frame)).expect("drawn");
            let buffer = terminal.backend().buffer().clone();
            (0..24)
                .flat_map(|y| (0..80).map(move |x| (x, y)))
                .map(|(x, y)| buffer[(x, y)].symbol().to_string())
                .collect::<String>()
        };
        assert!(top_row(None).contains(t!(ask_title)));
        let driver = top_row(Some("sandbox"));
        assert!(driver.contains("sandbox") && !driver.contains(t!(ask_title)));
    }

    /// A series of one, which is what most of these tests are about.
    fn one(prompt: &Prompt) -> Asking {
        Asking {
            prompts: vec![prompt.clone()],
            ..Default::default()
        }
    }

    fn answer(prompt: &Prompt, keys: &[KeyCode]) -> Option<Answer> {
        answers(&one(prompt), keys).map(|mut given| given.remove(0))
    }

    fn answers(asking: &Asking, keys: &[KeyCode]) -> Option<Vec<Answer>> {
        let mut picker = Picker::new(asking);
        for key in keys {
            if let Step::Answered(given) = picker.press(*key) {
                return Some(given);
            }
        }
        None
    }

    fn rendered(prompt: &Prompt) -> String {
        screen_rows(prompt, 80, 24).join("\n")
    }

    /// The screen as separate rows, so a test can assert what sits on a line of its own rather
    /// than only that the text is somewhere on the display.
    fn screen_rows(prompt: &Prompt, width: u16, height: u16) -> Vec<String> {
        rows_after(&one(prompt), &[], width, height)
    }

    /// The screen once some keys have been pressed, for asserting what a later question in a
    /// series looks like.
    fn rows_after(asking: &Asking, keys: &[KeyCode], width: u16, height: u16) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        let mut picker = Picker::new(asking);
        for key in keys {
            picker.press(*key);
        }
        terminal.draw(|frame| picker.draw(frame)).expect("drawn");
        let buffer = terminal.backend().buffer().clone();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    /// Which column this text starts at, counted in characters rather than bytes: the border and
    /// the cursor marker are multi-byte, so byte offsets do not line up between rows.
    fn column_of(row: &str, text: &str) -> usize {
        let at = row
            .find(text)
            .unwrap_or_else(|| panic!("{text} is not on this row"));
        row[..at].chars().count()
    }

    /// Which row holds this text, for asserting on the layout rather than its contents.
    fn row_holding(rows: &[String], text: &str) -> usize {
        rows.iter()
            .position(|row| row.contains(text))
            .unwrap_or_else(|| panic!("{text} is not on screen"))
    }

    #[test]
    fn moving_and_confirming_picks_the_option_under_the_cursor() {
        assert_eq!(
            answer(&prompt(false), &[KeyCode::Down, KeyCode::Enter]),
            Some(Answer::Chosen(vec![1]))
        );
    }

    #[test]
    fn vim_keys_move_as_the_arrows_do() {
        assert_eq!(
            answer(&prompt(false), &[KeyCode::Char('j'), KeyCode::Enter]),
            answer(&prompt(false), &[KeyCode::Down, KeyCode::Enter])
        );
    }

    /// The cursor must not run off either end, or Enter would confirm nothing and the picker
    /// would look stuck. Its last stop is the free-text row, one past the options.
    #[test]
    fn the_cursor_stops_at_the_ends_of_the_list() {
        let up = [KeyCode::Up, KeyCode::Up, KeyCode::Enter];
        assert_eq!(answer(&prompt(false), &up), Some(Answer::Chosen(vec![0])));

        let down = [KeyCode::Down; 9];
        let mut keys = down.to_vec();
        keys.extend([KeyCode::Up, KeyCode::Enter]);
        assert_eq!(
            answer(&prompt(false), &keys),
            Some(Answer::Chosen(vec![2])),
            "the cursor ran past the free-text row"
        );
    }

    #[test]
    fn several_options_can_be_picked_when_the_question_allows_it() {
        let keys = [
            KeyCode::Char(' '),
            KeyCode::Down,
            KeyCode::Down,
            KeyCode::Char(' '),
            KeyCode::Enter,
        ];
        assert_eq!(
            answer(&prompt(true), &keys),
            Some(Answer::Chosen(vec![0, 2]))
        );
    }

    #[test]
    fn a_pick_can_be_taken_back() {
        let keys = [
            KeyCode::Char(' '),
            KeyCode::Char(' '),
            KeyCode::Down,
            KeyCode::Char(' '),
            KeyCode::Enter,
        ];
        assert_eq!(answer(&prompt(true), &keys), Some(Answer::Chosen(vec![1])));
    }

    /// Space marks on a one-answer question too. A key that works on one kind of question and
    /// silently does nothing on the other reads as broken, and the person presses it again
    /// instead of reaching for enter.
    #[test]
    fn space_marks_the_option_on_a_single_answer_question() {
        let keys = [KeyCode::Char(' '), KeyCode::Enter];
        assert_eq!(answer(&prompt(false), &keys), Some(Answer::Chosen(vec![0])));
    }

    /// Marking is not answering, so the cursor can still be moved afterwards and the mark is what
    /// enter confirms, not wherever the cursor ended up.
    #[test]
    fn a_mark_survives_moving_the_cursor() {
        let keys = [
            KeyCode::Char(' '),
            KeyCode::Down,
            KeyCode::Down,
            KeyCode::Enter,
        ];
        assert_eq!(answer(&prompt(false), &keys), Some(Answer::Chosen(vec![0])));
    }

    /// One answer means one mark. Two marks would leave the picker deciding between them, and the
    /// person would have said one thing and been reported as saying another.
    #[test]
    fn marking_a_second_option_replaces_the_first_when_only_one_answer_is_allowed() {
        let keys = [
            KeyCode::Char(' '),
            KeyCode::Down,
            KeyCode::Char(' '),
            KeyCode::Enter,
        ];
        assert_eq!(answer(&prompt(false), &keys), Some(Answer::Chosen(vec![1])));
    }

    /// And a mark can be taken off again, leaving enter to confirm the cursor as it would have.
    #[test]
    fn a_mark_on_a_single_answer_question_can_be_taken_back() {
        let keys = [
            KeyCode::Char(' '),
            KeyCode::Char(' '),
            KeyCode::Down,
            KeyCode::Enter,
        ];
        assert_eq!(answer(&prompt(false), &keys), Some(Answer::Chosen(vec![1])));
    }

    fn platforms() -> Prompt {
        bravebot_core::ask::prompt(&Question::new(
            "Platforms",
            "Which platforms?",
            vec![Choice::new("Linux", None), Choice::new("macOS", None)],
            true,
        ))
    }

    fn a_series() -> Asking {
        Asking {
            prompts: vec![prompt(false), platforms()],
            ..Default::default()
        }
    }

    /// The point of a series: one question is settled and the next is put, without the turn
    /// having to go back to the model in between.
    #[test]
    fn answering_one_question_moves_on_to_the_next() {
        assert_eq!(
            answers(&a_series(), &[KeyCode::Enter]),
            None,
            "the series ended on its first answer"
        );
        assert_eq!(
            answers(&a_series(), &[KeyCode::Enter, KeyCode::Enter],),
            None,
            "the series was sent before the review was confirmed"
        );
        assert_eq!(
            answers(
                &a_series(),
                &[KeyCode::Enter, KeyCode::Enter, KeyCode::Enter]
            ),
            Some(vec![Answer::Chosen(vec![0]), Answer::Chosen(vec![0])])
        );
    }

    /// The kernel pairs answers to questions by position, so the order they come back in is the
    /// order the questions were asked, whatever order the person moved through the options in.
    #[test]
    fn the_answers_come_back_in_the_order_the_questions_were_asked() {
        let given = answers(
            &a_series(),
            &[
                KeyCode::Down,
                KeyCode::Enter,
                KeyCode::Char(' '),
                KeyCode::Down,
                KeyCode::Char(' '),
                KeyCode::Enter,
                KeyCode::Enter,
            ],
        )
        .expect("the series finished");
        assert_eq!(
            given,
            vec![Answer::Chosen(vec![1]), Answer::Chosen(vec![0, 1])]
        );
    }

    /// Escape skips the question on screen rather than abandoning the series. A person who
    /// cannot answer one of four should not lose the three they can.
    #[test]
    fn skipping_a_question_moves_on_rather_than_abandoning_the_series() {
        let given = answers(&a_series(), &[KeyCode::Esc, KeyCode::Enter, KeyCode::Enter])
            .expect("finished");
        assert_eq!(given, vec![Answer::Declined, Answer::Chosen(vec![0])]);
    }

    /// And the last question is the one that ends it, however it was settled.
    #[test]
    fn skipping_the_last_question_still_ends_the_series() {
        let given = answers(&a_series(), &[KeyCode::Enter, KeyCode::Esc, KeyCode::Enter])
            .expect("finished");
        assert_eq!(given, vec![Answer::Chosen(vec![0]), Answer::Declined]);
    }

    /// Marks belong to the question they were made against. Carrying them forward would report
    /// the person as having said something about a question they had not reached.
    #[test]
    fn marks_from_one_question_do_not_carry_into_the_next() {
        let given = answers(
            &a_series(),
            &[
                KeyCode::Down,
                KeyCode::Down,
                KeyCode::Char(' '),
                KeyCode::Enter,
                KeyCode::Enter,
                KeyCode::Enter,
            ],
        )
        .expect("finished");
        assert_eq!(
            given,
            vec![Answer::Chosen(vec![2]), Answer::Chosen(vec![0])],
            "the second question inherited the first question's cursor or marks"
        );
    }

    /// The series as the review tests put it: three questions, the first answered with the second
    /// option, so that a change to it is visible in what comes back.
    fn three_asked() -> Asking {
        Asking {
            prompts: vec![prompt(false), platforms(), prompt(false)],
            ..Default::default()
        }
    }

    /// The issue's own scenario: answer the first question, go back, change it, confirm. The
    /// regression it rejects is a picker that sends at the last answer (the old behaviour), or one
    /// that keeps the first pick beside the new one, or that drops the answers after it.
    #[test]
    fn an_answer_changed_from_the_review_is_the_one_sent() {
        use KeyCode::*;
        let given = answers(
            &a_series(),
            &[Enter, Enter, Char('1'), Down, Char(' '), Enter, Enter],
        )
        .expect("confirmed");
        assert_eq!(
            given,
            vec![Answer::Chosen(vec![1]), Answer::Chosen(vec![0])]
        );
    }

    /// Nothing leaves until enter on the review, whatever was pressed to reach it.
    #[test]
    fn nothing_is_sent_before_the_review_is_confirmed() {
        use KeyCode::*;
        for keys in [
            vec![Enter, Enter],
            vec![Esc, Esc],
            vec![Enter, Enter, Esc, Enter],
            vec![Enter, Enter, Char('2'), Enter],
        ] {
            assert_eq!(
                answers(&a_series(), &keys),
                None,
                "{keys:?} sent the answers"
            );
        }
    }

    /// A question left for the one before it comes back with what was answered there, in both
    /// kinds, so a person fixing one word does not retype the rest.
    #[test]
    fn going_back_reopens_a_question_with_its_earlier_answer() {
        use KeyCode::*;
        let given = answers(
            &a_series(),
            &[
                Char('o'),
                Char('h'),
                Char('i'),
                Enter,
                Left,
                Enter,
                Enter,
                Enter,
            ],
        )
        .expect("confirmed");
        assert_eq!(given[0], Answer::Typed("hi".into()), "the words were lost");

        let given = answers(
            &a_series(),
            &[Down, Down, Enter, Char('h'), Enter, Enter, Enter],
        )
        .expect("confirmed");
        assert_eq!(given[0], Answer::Chosen(vec![2]), "the mark was lost");
    }

    /// The first question has nothing before it, and what was half done on a question is not
    /// carried back to the one before.
    #[test]
    fn going_back_from_the_first_question_does_nothing() {
        use KeyCode::*;
        assert_eq!(answers(&a_series(), &[Left, Char('h'), Enter, Enter]), None);
        let given = answers(
            &a_series(),
            &[Enter, Down, Char(' '), Left, Enter, Enter, Enter],
        )
        .expect("confirmed");
        assert_eq!(
            given[1],
            Answer::Chosen(vec![0]),
            "a mark left on the question the person went back from was kept"
        );
    }

    /// Changing the middle of three returns to the review and not through the third question.
    #[test]
    fn a_question_reopened_from_the_review_returns_to_the_review() {
        use KeyCode::*;
        let given = answers(
            &three_asked(),
            &[
                Enter,
                Enter,
                Enter,
                Char('2'),
                Down,
                Char(' '),
                Enter,
                Enter,
            ],
        )
        .expect("confirmed");
        assert_eq!(
            given,
            vec![
                Answer::Chosen(vec![0]),
                Answer::Chosen(vec![0, 1]),
                Answer::Chosen(vec![0])
            ]
        );
    }

    /// Escape on the review reopens the last question and declines nothing already answered.
    #[test]
    fn escape_on_the_review_declines_nothing() {
        use KeyCode::*;
        let given = answers(&a_series(), &[Enter, Enter, Esc, Enter, Enter]).expect("confirmed");
        assert_eq!(
            given,
            vec![Answer::Chosen(vec![0]), Answer::Chosen(vec![0])]
        );
    }

    /// A key that names no question changes nothing.
    #[test]
    fn a_digit_past_the_last_question_does_nothing_on_the_review() {
        use KeyCode::*;
        let given =
            answers(&a_series(), &[Enter, Enter, Char('3'), Char('0'), Enter]).expect("confirmed");
        assert_eq!(given.len(), 2);
    }

    /// A lone question is sent as it always was, with no review in between.
    #[test]
    fn a_lone_question_has_no_review() {
        assert_eq!(
            answer(&prompt(false), &[KeyCode::Enter]),
            Some(Answer::Chosen(vec![0]))
        );
    }

    /// The screen: each question's tag beside its answer, a skip said as one, and the keys.
    #[test]
    fn the_review_lists_every_answer_beside_its_question() {
        use KeyCode::*;
        let rows = rows_after(&a_series(), &[Down, Enter, Esc], 80, 24);
        let screen = rows.join("\n");
        assert!(screen.contains(t!(ask_review_title)), "{screen}");
        let first = row_holding(&rows, "Cache layer");
        assert!(rows[first + 1].contains("Query"), "{screen}");
        let second = row_holding(&rows, "Platforms");
        assert!(
            rows[second + 1].contains(t!(ask_review_skipped)),
            "{screen}"
        );
        assert!(
            screen.contains("1-2") && screen.contains(t!(ask_key_send)),
            "{screen}"
        );
    }

    /// An answer as long as the field allows is cut, so the keys that send stay on screen.
    #[test]
    fn a_long_answer_does_not_push_the_keys_off_the_review() {
        use KeyCode::*;
        let mut keys = vec![Char('o')];
        keys.extend(std::iter::repeat_n(Char('x'), 400));
        keys.extend([Enter, Enter]);
        let rows = rows_after(&a_series(), &keys, 60, 20);
        assert!(
            rows.iter().any(|row| row.contains(t!(ask_key_send))),
            "{}",
            rows.join("\n")
        );
    }

    /// The question after the first says how to go back, and the first does not.
    #[test]
    fn the_way_back_is_offered_only_where_there_is_one() {
        let first = rows_after(&a_series(), &[], 80, 24).join("\n");
        let second = rows_after(&a_series(), &[KeyCode::Enter], 80, 24).join("\n");
        assert!(!first.contains(t!(ask_key_back)), "{first}");
        assert!(second.contains(t!(ask_key_back)), "{second}");
    }

    /// Nobody confirmed the review, so a clock that runs out there is ASK-9's case and keeps what
    /// was answered, as it does anywhere else.
    #[test]
    fn a_review_left_alone_keeps_the_answers_given() {
        use KeyCode::*;
        let asking = a_series();
        let mut picker = Picker::new(&asking);
        for key in [Enter, Enter] {
            picker.press(key);
        }
        assert_eq!(
            picker.time_out(),
            vec![Answer::Chosen(vec![0]), Answer::Chosen(vec![0])]
        );
    }

    /// A person who reopens the first of three to fix it and walks away has still given the other
    /// two, and the one they were fixing keeps what it had. The regression it rejects is the clock
    /// reporting confirmed answers as no answer.
    #[test]
    fn a_question_gone_back_to_keeps_its_answer_and_so_do_those_after_it() {
        use KeyCode::*;
        let asking = three_asked();
        let mut picker = Picker::new(&asking);
        for key in [Enter, Enter, Enter, Char('1'), Down, Char(' ')] {
            picker.press(key);
        }
        assert_eq!(
            picker.time_out(),
            vec![
                Answer::Chosen(vec![0]),
                Answer::Chosen(vec![0]),
                Answer::Chosen(vec![0])
            ]
        );
        let asking = a_series();
        let mut picker = Picker::new(&asking);
        for key in [Enter, Left] {
            picker.press(key);
        }
        assert_eq!(
            picker.time_out(),
            vec![Answer::Chosen(vec![0]), Answer::TimedOut]
        );
    }

    /// Going back by hand from a question reopened from the review walks the series on, through
    /// the question that was reopened, and does not jump to the review past it.
    #[test]
    fn going_back_by_hand_from_a_reopened_question_walks_the_series_on() {
        use KeyCode::*;
        let asking = three_asked();
        let mut picker = Picker::new(&asking);
        for key in [Enter, Enter, Enter, Char('3'), Left, Enter] {
            picker.press(key);
        }
        assert!(
            !picker.reviewing,
            "the review came up past the third question"
        );
        assert_eq!(picker.at, 2);
    }

    /// Wide characters and a header as long as the planner likes both take one row each.
    #[test]
    fn a_wide_answer_and_a_long_tag_leave_the_keys_on_the_review() {
        use KeyCode::*;
        let mut long = prompt(false);
        long.header = "h".repeat(200);
        let asking = Asking {
            prompts: vec![long.clone(), long.clone(), long.clone(), long],
            ..Default::default()
        };
        let mut keys = vec![Char('o')];
        keys.extend(std::iter::repeat_n(Char('語'), 300));
        keys.push(Enter);
        keys.extend([Enter, Enter, Enter]);
        let rows = rows_after(&asking, &keys, 60, 20);
        assert!(
            rows.iter().any(|row| row.contains(t!(ask_key_send))),
            "{}",
            rows.join("\n")
        );
    }

    /// The memory is written from what `ask` returns, so an answer corrected on the review is the
    /// one remembered and the first pick is not (ASK-8).
    #[test]
    fn the_answer_remembered_is_the_one_confirmed() {
        use KeyCode::*;
        let asking = a_series();
        let mut session = Session::new(".");
        let given = put(&mut session, &asking, |outstanding| {
            answers(
                outstanding,
                &[Enter, Enter, Char('1'), Down, Char(' '), Enter, Enter],
            )
            .expect("sent")
        });
        assert_eq!(given[0], Answer::Chosen(vec![1]));
        let again = put(&mut session, &asking, |shown| {
            assert!(
                shown.prompts.is_empty(),
                "a confirmed question was put again"
            );
            Vec::new()
        });
        assert_eq!(again, given);
    }

    /// Half-typed text is no different: it was written in answer to the question on screen.
    #[test]
    fn a_field_left_open_does_not_follow_the_person_to_the_next_question() {
        let given = answers(
            &a_series(),
            &[
                KeyCode::Char('o'),
                KeyCode::Char('h'),
                KeyCode::Char('i'),
                KeyCode::Enter,
                KeyCode::Enter,
                KeyCode::Enter,
            ],
        )
        .expect("finished");
        assert_eq!(
            given,
            vec![Answer::Typed("hi".into()), Answer::Chosen(vec![0])]
        );
    }

    /// Someone partway through a series needs to know how much of it is left, or a second
    /// question arriving where they expected to be finished reads as the agent looping.
    #[test]
    fn the_position_in_the_series_is_shown() {
        let rows = rows_after(&a_series(), &[], 80, 24);
        assert!(
            rows.iter().any(|row| row.contains("(1 of 2)")),
            "the first question did not say where it sat: {rows:?}"
        );

        let later = rows_after(&a_series(), &[KeyCode::Enter], 80, 24);
        assert!(
            later.iter().any(|row| row.contains("(2 of 2)")),
            "the second question did not say where it sat: {later:?}"
        );
    }

    /// A lone question cannot be anywhere else in anything, so counting it is noise.
    #[test]
    fn a_lone_question_is_not_labelled_one_of_one() {
        let rows = screen_rows(&prompt(false), 80, 24);
        assert!(
            !rows.iter().any(|row| row.contains("1 of 1")),
            "a single question counted itself: {rows:?}"
        );
    }

    /// The next question has to be the one drawn. Leaving the first on screen would take the
    /// answer to one question against the text of another.
    #[test]
    fn the_next_question_is_the_one_drawn() {
        let later = rows_after(&a_series(), &[KeyCode::Enter], 80, 24);
        let screen = later.join("\n");
        assert!(screen.contains("Which platforms?"), "{screen}");
        assert!(!screen.contains("Which cache layer?"), "{screen}");
    }

    /// A way in that nobody can see is one most people never find, and answering in your own
    /// words is the way out of a set of options that does not contain the answer.
    #[test]
    fn answering_in_your_own_words_is_offered_in_the_list() {
        let rows = screen_rows(&prompt(false), 80, 24);
        assert!(
            rows.iter().any(|row| row.contains(own_words())),
            "the free-text row is not on screen: {rows:?}"
        );
    }

    /// It sits after the options rather than among them, because it is a way in rather than a
    /// choice and putting it in the middle would read as one of the answers.
    #[test]
    fn the_free_text_row_sits_below_every_option() {
        let rows = screen_rows(&prompt(false), 80, 24);
        assert!(
            row_holding(&rows, own_words()) > row_holding(&rows, "Neither"),
            "the free-text row was drawn among the options: {rows:?}"
        );
    }

    /// The cursor has to reach it, or it is a row the person can read and not use.
    #[test]
    fn the_cursor_reaches_the_free_text_row_past_the_last_option() {
        let keys = [
            KeyCode::Down,
            KeyCode::Down,
            KeyCode::Down,
            KeyCode::Enter,
            KeyCode::Char('h'),
            KeyCode::Char('i'),
            KeyCode::Enter,
        ];
        assert_eq!(
            answer(&prompt(false), &keys),
            Some(Answer::Typed("hi".into()))
        );
    }

    /// Space on that row means what enter means. There is nothing there to mark, so a key that
    /// marks elsewhere and does nothing here would read as broken.
    #[test]
    fn space_on_the_free_text_row_opens_the_field_rather_than_marking() {
        let keys = [
            KeyCode::Down,
            KeyCode::Down,
            KeyCode::Down,
            KeyCode::Char(' '),
            KeyCode::Char('n'),
            KeyCode::Char('o'),
            KeyCode::Enter,
        ];
        assert_eq!(
            answer(&prompt(true), &keys),
            Some(Answer::Typed("no".into()))
        );
    }

    /// With the field open the person is already answering in their own words, so a row inviting
    /// them to do what they are doing is one more thing to read.
    #[test]
    fn the_free_text_row_makes_way_for_the_field_it_opens() {
        let rows = rows_after(&one(&prompt(false)), &[KeyCode::Char('o')], 80, 24);
        assert!(
            !rows.iter().any(|row| row.contains(own_words())),
            "the row was still drawn under the open field: {rows:?}"
        );
    }

    /// Marks survive a look at the field: escape comes back to the list, and what was picked
    /// before opening it is still picked.
    #[test]
    fn opening_the_field_and_leaving_it_keeps_what_was_marked() {
        let keys = [
            KeyCode::Char(' '),
            KeyCode::Char('o'),
            KeyCode::Esc,
            KeyCode::Enter,
        ];
        assert_eq!(answer(&prompt(true), &keys), Some(Answer::Chosen(vec![0])));
    }

    /// A picker shown nothing draws nothing and answers nothing, which is what lets the main
    /// loop call it unconditionally.
    #[test]
    fn a_series_with_no_questions_in_it_is_not_drawn() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        assert!(ask(&mut terminal, &Asking::default()).is_empty());
    }

    /// Pressing space has to show, or it reads as a key that does nothing.
    #[test]
    fn a_marked_option_is_drawn_as_marked() {
        let question = one(&prompt(false));
        let mut picker = Picker::new(&question);
        picker.press(KeyCode::Char(' '));

        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        terminal.draw(|frame| picker.draw(frame)).expect("drawn");
        let screen: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(
            screen.contains("◉"),
            "a marked option is not shown as marked"
        );
    }

    #[test]
    fn the_user_can_answer_in_their_own_words() {
        let keys = [
            KeyCode::Char('o'),
            KeyCode::Char('n'),
            KeyCode::Char('o'),
            KeyCode::Enter,
        ];
        assert_eq!(
            answer(&prompt(false), &keys),
            Some(Answer::Typed("no".into()))
        );
    }

    /// Backspace has to work, or a typo can only be fixed by abandoning the answer.
    #[test]
    fn typing_can_be_corrected() {
        let keys = [
            KeyCode::Char('o'),
            KeyCode::Char('n'),
            KeyCode::Char('x'),
            KeyCode::Backspace,
            KeyCode::Char('o'),
            KeyCode::Enter,
        ];
        assert_eq!(
            answer(&prompt(false), &keys),
            Some(Answer::Typed("no".into()))
        );
    }

    /// An empty field is not an answer, for the same reason Enter does not approve a write: it
    /// is the key most likely to be pressed out of habit.
    #[test]
    fn an_empty_field_is_not_an_answer() {
        let keys = [KeyCode::Char('o'), KeyCode::Enter, KeyCode::Enter];
        assert_eq!(answer(&prompt(false), &keys), None);
    }

    /// Escape from the field goes back to the list rather than out of the question, so a person
    /// who opened it by mistake has not lost the options.
    #[test]
    fn leaving_the_field_returns_to_the_options() {
        let keys = [
            KeyCode::Char('o'),
            KeyCode::Char('x'),
            KeyCode::Esc,
            KeyCode::Down,
            KeyCode::Enter,
        ];
        assert_eq!(answer(&prompt(false), &keys), Some(Answer::Chosen(vec![1])));
    }

    /// Declining must always be reachable, or a question could hold the session open.
    #[test]
    fn escape_declines() {
        assert_eq!(
            answer(&prompt(false), &[KeyCode::Esc]),
            Some(Answer::Declined)
        );
        assert_eq!(
            answer(&prompt(true), &[KeyCode::Esc]),
            Some(Answer::Declined)
        );
    }

    #[test]
    fn a_question_with_no_options_opens_the_field_at_once() {
        let bare = bravebot_core::ask::prompt(&Question::new(
            "Branch",
            "Which branch?",
            Vec::new(),
            false,
        ));
        assert_eq!(
            answer(
                &bare,
                &[KeyCode::Char('m'), KeyCode::Char('e'), KeyCode::Enter]
            ),
            Some(Answer::Typed("me".into()))
        );
    }

    /// With no list to fall back to, escape from the field is a decline rather than a dead end.
    #[test]
    fn escaping_a_question_with_no_options_declines() {
        let bare = bravebot_core::ask::prompt(&Question::new(
            "Branch",
            "Which branch?",
            Vec::new(),
            false,
        ));
        assert_eq!(answer(&bare, &[KeyCode::Esc]), Some(Answer::Declined));
    }

    /// A typed answer is bounded so the field cannot be pushed off the screen. The cap drops
    /// further keystrokes rather than truncating what is already there.
    #[test]
    fn a_typed_answer_is_bounded() {
        let question = one(&prompt(false));
        let mut picker = Picker::new(&question);
        picker.press(KeyCode::Char('o'));
        for _ in 0..(MAX_TYPED + 50) {
            picker.press(KeyCode::Char('a'));
        }
        match picker.press(KeyCode::Enter) {
            Step::Answered(given) => match given.first() {
                Some(Answer::Typed(text)) => assert_eq!(text.chars().count(), MAX_TYPED),
                other => panic!("the field did not answer: {other:?}"),
            },
            _ => panic!("the field did not answer"),
        }
    }

    #[test]
    fn the_question_and_every_option_are_drawn() {
        let screen = rendered(&prompt(false));
        for expected in [
            "Which cache layer?",
            "HTTP",
            "in front of the handler",
            "Query",
            "Neither",
        ] {
            assert!(screen.contains(expected), "{expected} is not on screen");
        }
    }

    /// The way out has to be visible. A person who cannot see how to skip will answer something
    /// rather than nothing, and the planner will take it as their view.
    #[test]
    fn the_keys_say_how_to_skip() {
        assert!(rendered(&prompt(false)).contains("esc skip"));
    }

    /// The way out must survive a narrow terminal. It may wrap, but it may not fall off the
    /// screen: a person who cannot see how to skip will answer something rather than nothing, and
    /// the planner will take it as their view.
    #[test]
    fn the_way_out_survives_a_narrow_terminal() {
        for multiple in [false, true] {
            let mut terminal = Terminal::new(TestBackend::new(48, 20)).expect("terminal");
            let question = one(&prompt(multiple));
            let mut picker = Picker::new(&question);
            terminal.draw(|frame| picker.draw(frame)).expect("drawn");

            let screen: String = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect();
            assert!(screen.contains("esc"), "the escape hint is off screen");
            assert!(screen.contains("skip"), "the escape hint is off screen");
        }
    }

    /// A question that takes several answers has to say so, or the person picks one and the
    /// others are never considered.
    #[test]
    fn a_multiple_choice_question_shows_how_to_pick_several() {
        assert!(rendered(&prompt(true)).contains("pick any"));
        assert!(!rendered(&prompt(false)).contains("pick any"));
    }

    /// Space is offered on both kinds of question, since it now works on both.
    #[test]
    fn the_keys_offer_space_whatever_the_question() {
        assert!(rendered(&prompt(false)).contains("space"));
        assert!(rendered(&prompt(true)).contains("space"));
    }

    /// The tag is what tells one question from another when several arrive together, so it has
    /// to be on screen and above the sentence it labels.
    #[test]
    fn the_tag_is_drawn_above_the_question() {
        let rows = screen_rows(&prompt(false), 80, 24);
        let top = row_holding(&rows, "the agent is asking");
        assert_eq!(row_holding(&rows, "Cache layer"), top + 1, "{rows:?}");
        assert_eq!(
            row_holding(&rows, "Which cache layer?"),
            top + 2,
            "{rows:?}"
        );
    }

    /// A tag the model wrote long must not cost the person the sentence they have to answer.
    #[test]
    fn a_long_tag_is_shortened_rather_than_pushing_the_question_off_the_line() {
        let wordy = bravebot_core::ask::prompt(&Question::new(
            "An extremely long tag nobody could read at a glance",
            "Which cache layer?",
            vec![Choice::new("HTTP", None)],
            false,
        ));
        let rows = screen_rows(&wordy, 80, 24);
        assert!(
            rows.iter().any(|row| row.contains('\u{2026}')),
            "a long tag was not shortened: {rows:?}"
        );
        assert!(
            row_holding(&rows, "Which cache layer?") > 0,
            "the question was pushed off the box"
        );
    }

    /// Nothing is drawn for a question with no tag. An empty chip is a smear of colour the
    /// person has to work out the meaning of.
    #[test]
    fn a_question_with_no_tag_draws_no_chip() {
        let untagged = bravebot_core::ask::prompt(&Question::new(
            "",
            "Which cache layer?",
            vec![Choice::new("HTTP", None)],
            false,
        ));
        let rows = screen_rows(&untagged, 80, 24);
        assert_eq!(
            row_holding(&rows, "Which cache layer?"),
            row_holding(&rows, "the agent is asking") + 1,
            "an empty tag still took a line: {rows:?}"
        );
    }

    /// A list too long for the box must say what is missing. Silence would read as an option
    /// that was never offered.
    #[test]
    fn a_list_cut_short_says_so() {
        let many = bravebot_core::ask::prompt(&Question::new(
            "File",
            "Which file?",
            (0..60)
                .map(|i| Choice::new(format!("file-{i}.rs"), None))
                .collect(),
            false,
        ));
        assert!(rendered(&many).contains("more, use the arrow keys"));
    }

    /// A detail belongs under its label, not beside it. Beside it, a long explanation is pushed
    /// off the edge of the box or wraps into the next option and reads as part of it.
    #[test]
    fn an_option_detail_sits_on_its_own_line_beneath_the_label() {
        let rows = screen_rows(&prompt(false), 80, 24);
        let label = row_holding(&rows, "HTTP");
        let detail = row_holding(&rows, "in front of the handler");

        assert_eq!(
            detail,
            label + 1,
            "the detail is not on the line directly beneath its label"
        );
        assert!(
            !rows[label].contains("in front of the handler"),
            "the detail is still beside the label: {}",
            rows[label]
        );
    }

    /// Aligned under the label rather than at the margin, so the pair reads as one option.
    #[test]
    fn a_detail_is_indented_to_line_up_with_its_label() {
        let rows = screen_rows(&prompt(false), 80, 24);
        let label = &rows[row_holding(&rows, "HTTP")];
        let detail = &rows[row_holding(&rows, "in front of the handler")];

        assert_eq!(
            column_of(label, "HTTP"),
            column_of(detail, "in front of the handler"),
            "the detail does not line up with its label"
        );
    }

    /// With a second line under each option, unspaced options read as twice as many options.
    #[test]
    fn options_carrying_details_are_spaced_apart() {
        let rows = screen_rows(&prompt(false), 80, 24);
        let detail = row_holding(&rows, "in front of the handler");
        assert_eq!(
            row_holding(&rows, "Query"),
            detail + 2,
            "the next option runs straight into the detail above it"
        );
    }

    /// And a list with nothing to explain stays compact: blank lines between bare options would
    /// waste the height a long list needs.
    #[test]
    fn options_without_details_are_not_spaced_apart() {
        let bare = bravebot_core::ask::prompt(&Question::new(
            "Which",
            "Which?",
            vec![Choice::new("One", None), Choice::new("Two", None)],
            false,
        ));
        let rows = screen_rows(&bare, 80, 24);
        assert_eq!(
            row_holding(&rows, "Two"),
            row_holding(&rows, "One") + 1,
            "bare options were spaced apart"
        );
    }

    /// The detail is drawn dimmed, so the eye lands on the options rather than the explanations.
    #[test]
    fn a_detail_is_drawn_dimmer_than_its_label() {
        let _held = theme::exclusive();
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        let question = prompt(false);
        let asking = one(&question);
        let mut picker = Picker::new(&asking);
        terminal.draw(|frame| picker.draw(frame)).expect("drawn");
        let buffer = terminal.backend().buffer().clone();

        let rows = screen_rows(&question, 80, 24);
        let y = row_holding(&rows, "in front of the handler") as u16;
        let x = column_of(&rows[y as usize], "in front of the handler") as u16;
        assert_eq!(buffer[(x, y)].symbol(), "i", "not the start of the detail");
        assert_eq!(buffer[(x, y)].fg, theme::muted());
    }

    /// Eight options whose labels are each long enough to wrap at 80 columns, which is an
    /// ordinary sentence a planner writes rather than a pathological string.
    fn wrapping_labels() -> Prompt {
        bravebot_core::ask::prompt(&Question::new(
            "Cache layer",
            "Which approach should I take?",
            (0..8)
                .map(|i| Choice::new(format!("Option {i}: refactor the cache layer to use a bounded LRU, keep the existing interface, and add a metrics hook so the hit rate is visible"), None))
                .collect(),
            false,
        ))
    }

    /// The question sentence is one line and several rows, and the room kept for what sits below
    /// the list has to allow for the rows. Counted as one line, the list is given the two rows
    /// the sentence took, and the line saying how to answer is drawn past the bottom border.
    #[test]
    fn a_question_sentence_that_wraps_keeps_the_room_it_takes() {
        let long = bravebot_core::ask::prompt(&Question::new(
            "Cache layer",
            "Which of these approaches should I take, bearing in mind that the interface is public and the hit rate is something we would like to be able to watch afterwards?",
            (0..60)
                .map(|i| Choice::new(format!("file-{i}.rs"), None))
                .collect(),
            false,
        ));
        let rows = screen_rows(&long, 80, 24);
        assert!(
            rows.iter().any(|row| row.contains("esc")),
            "the keys were pushed off the box by the wrapped question:\n{}",
            rows.join("\n")
        );
        assert!(
            rows.iter()
                .any(|row| row.contains("more, use the arrow keys")),
            "the list was cut short without saying so:\n{}",
            rows.join("\n")
        );
    }

    /// A list the box has room for is drawn whole, wherever the cursor is in it. Scrolled
    /// against an estimate of how many options fit, one wrapping label makes the estimate stand
    /// for every row, and moving to the end of a list that fitted comfortably hides the start of
    /// it behind a count of options that were never cut.
    #[test]
    fn a_list_the_box_has_room_for_is_not_scrolled() {
        let mixed = bravebot_core::ask::prompt(&Question::new(
            "Cache layer",
            "Which approach should I take?",
            vec![
                Choice::new("alpha.rs", None),
                Choice::new("beta.rs", None),
                Choice::new(
                    "a label long enough to wrap onto a second row of the box, which is an ordinary sentence",
                    None,
                ),
                Choice::new("delta.rs", None),
                Choice::new("epsilon.rs", None),
            ],
            false,
        ));
        let rows = rows_after(&one(&mixed), &[KeyCode::Down; 4], 80, 24);
        assert!(
            rows.iter().any(|row| row.contains("alpha.rs")),
            "the list scrolled although the box had room for all of it:\n{}",
            rows.join("\n")
        );
        assert!(
            !rows
                .iter()
                .any(|row| row.contains("more, use the arrow keys")),
            "options were reported hidden that the box had room for:\n{}",
            rows.join("\n")
        );
    }

    /// The field is one line and several rows once a sentence has been typed into it, and the
    /// room kept below the list has to allow for the rows. Counted as one line, what a person
    /// types pushes the only hint saying how to leave the field off the bottom of the box.
    #[test]
    fn a_field_that_wraps_keeps_its_own_keys_on_screen() {
        let many = bravebot_core::ask::prompt(&Question::new(
            "File",
            "Which file?",
            (0..20)
                .map(|i| Choice::new(format!("file-{i}.rs"), None))
                .collect(),
            false,
        ));
        let mut keys = vec![KeyCode::Char('o')];
        keys.extend([KeyCode::Char('x'); 140]);
        let rows = rows_after(&one(&many), &keys, 80, 24);
        assert!(
            rows.iter().any(|row| row.contains("back to the options")),
            "the way out of the field was pushed off the box by what was typed into it:\n{}",
            rows.join("\n")
        );
    }

    /// The option under the cursor has to be on screen, or Enter answers with something the
    /// person cannot read. The list is fitted a line shorter once anything is hidden, to make
    /// room for the line saying so, and an estimate that does not allow for that line scrolls
    /// the list one option short of the one being moved onto.
    #[test]
    fn the_option_under_the_cursor_is_drawn_at_the_end_of_a_long_list() {
        let many = bravebot_core::ask::prompt(&Question::new(
            "File",
            "Which file?",
            (0..60)
                .map(|i| Choice::new(format!("file-{i}.rs"), None))
                .collect(),
            false,
        ));
        let rows = rows_after(&one(&many), &[KeyCode::Down; 59], 80, 24);
        assert!(
            rows.iter().any(|row| row.contains("file-59.rs")),
            "the last option is off the box after moving onto it:\n{}",
            rows.join("\n")
        );
    }

    /// A label long enough to wrap costs two rows of the box, and the budget has to count them.
    /// Counted as one apiece, eight options are fitted into a box with room for six and the last
    /// two are drawn past the bottom border: options the routing gate approved, which the planner
    /// is told the person was shown, and which nothing on screen accounts for.
    #[test]
    fn options_whose_labels_wrap_are_fitted_by_the_rows_they_draw() {
        let rows = screen_rows(&wrapping_labels(), 80, 24);
        assert!(
            rows.iter()
                .any(|row| row.contains("more, use the arrow keys")),
            "the list was cut short without saying so:\n{}",
            rows.join("\n")
        );
        assert!(
            rows.iter().any(|row| row.contains(own_words())),
            "the free-text row was pushed off the box:\n{}",
            rows.join("\n")
        );
        assert!(
            rows.iter().any(|row| row.contains("esc")),
            "the keys were pushed off the box:\n{}",
            rows.join("\n")
        );
    }

    /// Scrolling is driven by the same count, so a cursor moved past the last drawn option has to
    /// bring it into view. Estimated from unwrapped labels the estimate covers the whole list, the
    /// offset never moves, and the cursor leaves the screen with the option it is on.
    #[test]
    fn a_wrapping_list_scrolls_to_the_option_under_the_cursor() {
        let rows = rows_after(&one(&wrapping_labels()), &[KeyCode::Down; 7], 80, 24);
        assert!(
            rows.iter().any(|row| row.contains("Option 7:")),
            "the option under the cursor is off the box:\n{}",
            rows.join("\n")
        );
        assert!(
            rows.iter().any(|row| row.contains('\u{203a}')),
            "the cursor is off the box:\n{}",
            rows.join("\n")
        );
    }

    /// A tall option list still has to leave room for the keys. If the height of the details were
    /// not counted, the last options would be drawn over the line that says how to answer.
    #[test]
    fn a_detailed_list_still_leaves_room_for_the_keys() {
        let many = bravebot_core::ask::prompt(&Question::new(
            "File",
            "Which file?",
            (0..40)
                .map(|i| Choice::new(format!("file-{i}.rs"), Some(format!("the {i}th one"))))
                .collect(),
            false,
        ));
        let rows = screen_rows(&many, 80, 24);
        assert!(
            rows.iter().any(|row| row.contains("esc")),
            "the keys were pushed off the box by the option list"
        );
        assert!(
            rows.iter()
                .any(|row| row.contains("more, use the arrow keys")),
            "the list was cut short without saying so"
        );
    }
    /// The hint line must fit the box it is drawn in. Wrapped onto a second line it reads as two
    /// half-hints, and the keys are what a person scans when they do not know what to press.
    #[test]
    fn the_keys_fit_on_one_line() {
        for multiple in [false, true] {
            let rows = screen_rows(&prompt(multiple), 72, 20);
            let line = row_holding(&rows, "esc");
            assert!(
                rows[line].contains("skip"),
                "the hints wrapped onto a second line: {}",
                rows[line]
            );
        }
    }
    /// A person who opens the field must be able to see how to get out of it. Drawing the field
    /// with no keys at all leaves them with a cursor and no idea what escape does.
    #[test]
    fn the_field_shows_its_own_keys() {
        let question = one(&prompt(false));
        let mut picker = Picker::new(&question);
        picker.press(KeyCode::Char('o'));

        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        terminal.draw(|frame| picker.draw(frame)).expect("drawn");
        let screen: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();

        assert!(
            screen.contains("enter"),
            "the field does not say how to answer"
        );
        assert!(
            screen.contains("back to the options"),
            "no way back is offered"
        );
    }

    /// And where there were no options to go back to, escape leaves the question, so the hint has
    /// to say that instead of offering a list that is not there.
    #[test]
    fn a_field_with_no_options_behind_it_offers_to_skip() {
        let bare = one(&bravebot_core::ask::prompt(&Question::new(
            "Branch",
            "Which branch?",
            Vec::new(),
            false,
        )));
        let mut picker = Picker::new(&bare);

        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        terminal.draw(|frame| picker.draw(frame)).expect("drawn");
        let screen: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();

        assert!(screen.contains("skip the question"));
        assert!(!screen.contains("back to the options"));
    }

    /// What the session said beside the work, oldest first.
    fn said(session: &Session) -> Vec<String> {
        session
            .transcript
            .iter()
            .filter(|entry| entry.speaker == crate::state::Speaker::System)
            .map(|entry| entry.text.clone())
            .collect()
    }

    /// ASK-8 as the main loop reaches it. The picker is drawn by `ask`, so a stand-in that records
    /// what it was shown is what tells a series put whole from one put only for what is left.
    #[test]
    fn a_question_the_person_answered_is_not_drawn_again() {
        let mut session = Session::new(".");
        let asking = one(&prompt(false));

        let first = put(&mut session, &asking, |shown| {
            assert_eq!(shown, &asking, "the first time the question is drawn whole");
            vec![Answer::Chosen(vec![1])]
        });
        let again = put(&mut session, &asking, |shown| {
            assert!(
                shown.prompts.is_empty(),
                "a question answered before was drawn again: {shown:?}"
            );
            Vec::new()
        });

        assert_eq!(first, vec![Answer::Chosen(vec![1])]);
        assert_eq!(again, first, "the remembered answer was not handed back");
    }

    /// Said once per question answered from memory, so a reused answer is not mistaken for a
    /// question that was never put.
    #[test]
    fn an_answer_reused_from_memory_is_said_to_have_been_given_already() {
        let mut session = Session::new(".");
        let asking = one(&prompt(false));
        put(&mut session, &asking, |_| vec![Answer::Declined]);
        assert!(said(&session).is_empty(), "the first asking said something");

        put(&mut session, &asking, |_| Vec::new());

        assert_eq!(
            said(&session),
            vec![t!(session_answered_already, question = "Which cache layer?").to_string()]
        );
    }

    /// The questions still open are the only ones drawn, and every answer keeps the place of the
    /// question it belongs to, whichever of them were remembered.
    #[test]
    fn a_series_with_some_questions_settled_draws_only_the_rest() {
        let mut session = Session::new(".");
        put(&mut session, &one(&platforms()), |_| {
            vec![Answer::Chosen(vec![0, 1])]
        });

        let both = Asking {
            prompts: vec![prompt(false), platforms()],
            ..Default::default()
        };
        let given = put(&mut session, &both, |shown| {
            assert_eq!(
                shown.prompts,
                vec![prompt(false)],
                "the settled one was drawn"
            );
            vec![Answer::Typed("neither".into())]
        });

        assert_eq!(
            given,
            vec![Answer::Typed("neither".into()), Answer::Chosen(vec![0, 1])]
        );
    }

    /// A picker that could not draw, or lost the keyboard, answers nothing. The question was not
    /// put to the person, so it is not remembered as skipped and is drawn when asked again.
    #[test]
    fn a_question_the_picker_left_unanswered_is_drawn_again() {
        let mut session = Session::new(".");
        let asking = one(&prompt(false));

        let first = put(&mut session, &asking, |_| Vec::new());
        let mut drawn = 0;
        put(&mut session, &asking, |shown| {
            drawn = shown.prompts.len();
            vec![Answer::Chosen(vec![0])]
        });

        assert_eq!(first, vec![Answer::Declined]);
        assert_eq!(drawn, 1, "a question nobody was asked was not drawn again");
    }

    /// ASK-8, ASK-9: a question closed on the clock is not remembered. A person who was away said
    /// nothing, so a planner asking it again is asking for the first time, and a remembered "no
    /// answer" would be given back in their place. What was answered beside it is remembered, and
    /// the transcript says a question was closed so the person who returns can see it.
    #[test]
    fn a_question_closed_on_the_clock_is_asked_again_and_the_one_beside_it_is_remembered() {
        let both = Asking {
            prompts: vec![prompt(false), platforms()],
            ..Default::default()
        };

        let mut session = Session::new(".");
        session.adopt_ask_idle(Some(90));
        let given = put(&mut session, &both, |_| {
            vec![Answer::Chosen(vec![0]), Answer::TimedOut]
        });
        assert_eq!(given, vec![Answer::Chosen(vec![0]), Answer::TimedOut]);
        assert!(
            said(&session).contains(&t!(session_question_timed_out, seconds = 90).to_string()),
            "{:?}",
            said(&session)
        );

        let mut drawn = Vec::new();
        put(&mut session, &both, |shown| {
            drawn = shown.prompts.clone();
            vec![Answer::Declined]
        });
        assert_eq!(
            drawn,
            vec![platforms()],
            "the question nobody answered was not put again, or the one answered was"
        );

        let mut brief = Session::new(".");
        brief.adopt_ask_idle(Some(1));
        put(&mut brief, &both, |_| {
            vec![Answer::TimedOut, Answer::TimedOut]
        });
        let said_once = said(&brief).join("\n");
        assert!(
            said_once.contains("after 1 second with"),
            "a single second was counted as seconds: {said_once}"
        );

        let mut settled = Session::new(".");
        settled.adopt_ask_idle(Some(90));
        put(&mut settled, &both, |_| {
            vec![Answer::Chosen(vec![0]), Answer::Declined]
        });
        assert!(
            said(&settled).is_empty(),
            "a series nobody was late for said it had been closed"
        );
    }

    /// A question that differs in its tag is another question, and is drawn.
    #[test]
    fn a_question_that_differs_in_its_tag_is_drawn_though_the_sentence_was_answered() {
        let mut session = Session::new(".");
        put(&mut session, &one(&prompt(false)), |_| {
            vec![Answer::Chosen(vec![0])]
        });
        let retagged = bravebot_core::ask::prompt(&Question::new(
            "Caching",
            "Which cache layer?",
            vec![
                Choice::new("HTTP", Some("in front of the handler".into())),
                Choice::new("Query", None),
                Choice::new("Neither", None),
            ],
            false,
        ));

        let mut drawn = 0;
        put(&mut session, &one(&retagged), |shown| {
            drawn = shown.prompts.len();
            vec![Answer::Declined]
        });

        assert_eq!(drawn, 1);
    }

    /// A terminal and a clock that run on a script instead of a person: what arrives, and at what
    /// time since the questions were put. Waiting past the last thing scripted with no limit would
    /// never end, which is reported as a failure rather than a hang.
    struct Scripted {
        started: Instant,
        elapsed: Duration,
        arriving: std::collections::VecDeque<(u64, Event)>,
        /// How long the loop is held up between the time it starts and the first time it looks
        /// at the clock again, as a process that was stopped or starved would be.
        stall: Duration,
        readings: usize,
    }

    impl Scripted {
        fn new(arriving: Vec<(u64, Event)>) -> Self {
            Self {
                started: Instant::now(),
                elapsed: Duration::ZERO,
                arriving: arriving.into(),
                stall: Duration::ZERO,
                readings: 0,
            }
        }

        fn stalled_for(mut self, stall: Duration) -> Self {
            self.stall = stall;
            self
        }
    }

    impl Events for Scripted {
        fn now(&mut self) -> Instant {
            self.readings += 1;
            if self.readings == 2 {
                self.elapsed += self.stall;
            }
            self.started + self.elapsed
        }

        fn next(&mut self, within: Option<Duration>) -> io::Result<Option<Event>> {
            let limit = within.map(|within| self.elapsed + within);
            let due = self
                .arriving
                .front()
                .map(|(at, _)| Duration::from_secs(*at));
            match (due, limit) {
                (Some(at), limit) if limit.is_none_or(|limit| at <= limit) => {
                    self.elapsed = self.elapsed.max(at);
                    Ok(self.arriving.pop_front().map(|(_, event)| event))
                }
                (_, Some(limit)) => {
                    self.elapsed = limit;
                    Ok(None)
                }
                (_, None) => Err(io::Error::other("waiting with nothing left to arrive")),
            }
        }
    }

    fn pressed(code: KeyCode) -> Event {
        Event::Key(event::KeyEvent::new(code, event::KeyModifiers::NONE))
    }

    /// The answers a series ends with when `arriving` happens and the picker waits `idle`, and how
    /// long it had been going when it ended.
    fn waited(
        asking: &Asking,
        idle: Option<u64>,
        arriving: Vec<(u64, Event)>,
    ) -> (Vec<Answer>, Duration) {
        waited_after_a_stall(asking, idle, arriving, Duration::ZERO)
    }

    fn waited_after_a_stall(
        asking: &Asking,
        idle: Option<u64>,
        arriving: Vec<(u64, Event)>,
        stall: Duration,
    ) -> (Vec<Answer>, Duration) {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        let mut picker = Picker::new(asking);
        let mut events = Scripted::new(arriving).stalled_for(stall);
        let given = converse(
            &mut terminal,
            &mut picker,
            idle.map(Duration::from_secs),
            &mut events,
        );
        (given, events.elapsed)
    }

    fn three_questions() -> Asking {
        Asking {
            prompts: vec![prompt(false), platforms(), prompt(false)],
            ..Default::default()
        }
    }

    /// ASK-9: the question on screen and the ones behind it close with no answer, and the answer
    /// already given stays. The time runs from the last key, so it ends a whole `idle` after the
    /// answer to the first question and not after the questions were put.
    #[test]
    fn a_series_left_alone_closes_from_the_question_on_screen_and_keeps_what_was_answered() {
        let (given, ended) = waited(
            &three_questions(),
            Some(60),
            vec![(10, pressed(KeyCode::Enter))],
        );
        assert_eq!(
            given,
            vec![Answer::Chosen(vec![0]), Answer::TimedOut, Answer::TimedOut]
        );
        assert_eq!(ended, Duration::from_secs(70));
    }

    /// Any key press restarts the time, not only the one that answers. A person who is scrolling
    /// the options is there, and closing the question under them is what the setting is not for.
    #[test]
    fn a_key_that_answers_nothing_still_restarts_the_time() {
        let (given, ended) = waited(
            &one(&prompt(false)),
            Some(60),
            vec![
                (50, pressed(KeyCode::Down)),
                (100, pressed(KeyCode::Down)),
                (150, pressed(KeyCode::Enter)),
            ],
        );
        assert_eq!(given, vec![Answer::Chosen(vec![2])]);
        assert_eq!(ended, Duration::from_secs(150));
    }

    /// Nothing but a key press is somebody at the keyboard. A change of size or the terminal being
    /// left is reported to the picker and leaves the time running.
    #[test]
    fn an_event_that_is_not_a_key_press_does_not_restart_the_time() {
        let (given, ended) = waited(
            &one(&prompt(false)),
            Some(60),
            vec![(30, Event::Resize(80, 24)), (50, Event::FocusLost)],
        );
        assert_eq!(given, vec![Answer::TimedOut]);
        assert_eq!(ended, Duration::from_secs(60));
    }

    /// With no time set a question waits however long it takes, so the setting is the only thing
    /// that closes one. A picker that carried a time of its own would close this one.
    #[test]
    fn with_no_time_set_a_question_waits_for_as_long_as_it_takes() {
        let (given, ended) = waited(
            &one(&prompt(false)),
            None,
            vec![(3 * 24 * 60 * 60, pressed(KeyCode::Enter))],
        );
        assert_eq!(given, vec![Answer::Chosen(vec![0])]);
        assert_eq!(ended, Duration::from_secs(3 * 24 * 60 * 60));
    }

    /// What was marked or typed and never confirmed is not an answer. Reporting it would hand the
    /// planner a reply the person did not give, which is the thing the time out exists to avoid.
    #[test]
    fn marks_and_half_typed_words_are_not_an_answer_when_the_time_runs_out() {
        let (marked, _) = waited(
            &one(&prompt(true)),
            Some(60),
            vec![
                (1, pressed(KeyCode::Char(' '))),
                (2, pressed(KeyCode::Down)),
                (3, pressed(KeyCode::Char(' '))),
            ],
        );
        assert_eq!(marked, vec![Answer::TimedOut]);

        let (typed, _) = waited(
            &one(&prompt(false)),
            Some(60),
            vec![
                (1, pressed(KeyCode::Char('o'))),
                (2, pressed(KeyCode::Char('h'))),
                (3, pressed(KeyCode::Char('i'))),
            ],
        );
        assert_eq!(typed, vec![Answer::TimedOut]);
    }

    /// A key pressed inside the time is an answer even when the loop reached it late, as a process
    /// that was stopped and resumed does. Without one waiting, the same stall is the time running
    /// out. Rejects: a deadline honoured before the keys already queued are read.
    #[test]
    fn a_key_waiting_when_the_loop_runs_late_is_still_an_answer() {
        let stall = Duration::from_secs(100);
        let (given, _) = waited_after_a_stall(
            &one(&prompt(false)),
            Some(60),
            vec![(50, pressed(KeyCode::Enter))],
            stall,
        );
        assert_eq!(given, vec![Answer::Chosen(vec![0])]);

        let (given, _) = waited_after_a_stall(&one(&prompt(false)), Some(60), Vec::new(), stall);
        assert_eq!(given, vec![Answer::TimedOut]);
    }

    /// A key that arrives in the last moment is an answer, so a person who presses it as the time
    /// is up is not told they were away.
    #[test]
    fn a_key_arriving_as_the_time_is_up_is_still_an_answer() {
        let (given, _) = waited(
            &one(&prompt(false)),
            Some(60),
            vec![(60, pressed(KeyCode::Enter))],
        );
        assert_eq!(given, vec![Answer::Chosen(vec![0])]);
    }
}
