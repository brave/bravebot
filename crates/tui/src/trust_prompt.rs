//! Asking, at startup, whether to trust the working directory.
//!
//! The answer decides how the session behaves. Trusting the directory means work inside it
//! proceeds without a prompt for every write, because reads from it return trusted data.
//! Declining means everything is untrusted, so every write is shown, which is the correct
//! behaviour for a directory whose contents came from somewhere else.
//!
//! Nothing is trusted by default. An unreadable terminal, an unexpected key, or a lost event
//! stream all resolve to declining, because the failure mode of guessing wrong here is that a
//! session silently writes to files nobody vouched for.
//!
//! The one session that is not asked is the one bypassing every permission, which answers this
//! question along with the rest. [`answered_by`] is where that is decided.
//!
//! A settings file may name directories to open beside the working directory, and a name in one is
//! a request rather than a grant: each is put as its own question and only an accepted one is
//! opened. A file arriving with a checkout is the easiest thing on the machine to write to, so a
//! name in one deciding what is reachable and trusted would be reach granted by whatever last
//! edited it.
//!
//! The `allow` rules such a file carries are the same kind of request, for the same reason, and are
//! put as a third question after the other two. Each one answers an approval prompt, so a rule read
//! out of a checkout would be a prompt answered by whatever last edited it. What makes the question
//! worth asking is that it lists the rules it would grant and the file each came from: an answer
//! about a tree's content spent on capability would be the defect rather than consent to it.

use bravebot_agent::PermissionMode;
use bravebot_agent::granted::Proposed;
use bravebot_agent::workspace::key_of;
use bravebot_core::trust::TrustStore;
use bravebot_i18n::t;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::crossterm::event::{self, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use std::path::Path;

use crate::input;
use crate::theme;

/// What the user decided about the working directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Trust,
    /// Trust, and keep the answer for later sessions started in exactly this directory (TRUST-23).
    ///
    /// Grants this session what [`Answer::Trust`] grants and nothing more; what differs is that the
    /// caller writes it down. Only ever the answer at the working directory's own question, and only
    /// where the caller offered it: a directory a settings file named and the rules a checkout
    /// proposed are requests from the tree, and a standing answer to one would be the tree's.
    Remember,
    Decline,
    /// Leave without starting a session at all.
    Leave,
}

/// Ask about `directory`, returning the trust map the session should start with and the answer that
/// made it.
///
/// Trusting records the workspace root, which covers everything beneath it. Declining records
/// nothing, leaving an empty map in which no path is trusted.
///
/// Asked afresh every time a session begins unless an earlier one here was told to remember
/// (TRUST-23), in which case this is not called. `keeping` is where that answer would be written, or
/// `None` where it may not be: the answer is offered only with somewhere to put it, since a key that
/// said it remembered and wrote nothing would be the next session asking somebody who was told it
/// would not. Writing it is the caller's, which holds the session it is recorded against.
///
/// `None` is the third answer: the user pressed Ctrl-C, which is neither trusting nor declining
/// but a request to leave, so no session begins at all.
pub fn ask<B: Backend>(
    terminal: &mut Terminal<B>,
    directory: &Path,
    keeping: Option<&Path>,
    carried: &mut String,
) -> Option<(TrustStore, Answer)> {
    let answer = ask_one(terminal, carried, keeping.is_some(), |frame, offered| {
        draw(frame, directory, keeping, offered)
    });

    trust_for(answer, directory).map(|trust| (trust, answer))
}

/// Ask about each directory a settings file named, returning the ones to open.
///
/// One question per directory, because each grants reach and trust over a tree of its own and one
/// key standing in for all of them would be a grant nobody read. What an answer of yes grants is
/// what `/add-dir` grants, so a directory a file named and a directory a person typed are open on
/// identical terms.
///
/// `None` is the request to leave, for the reason it is at the working directory's own question: a
/// session that began behind it is one nobody agreed to have.
pub fn ask_named<B: Backend>(
    terminal: &mut Terminal<B>,
    directories: &[String],
    carried: &mut String,
) -> Option<Vec<String>> {
    accepted(directories, |directory| {
        ask_one(terminal, carried, false, |frame, offered| {
            draw_named(frame, directory, offered)
        })
    })
}

/// Ask whether to grant the `allow` rules a checkout proposed, returning whether they were granted.
///
/// One question for the whole list rather than one per rule. A directory grants reach over a tree of
/// its own, which is why [`ask_named`] asks about each; rules are a list a person reads at once, and
/// thirty boxes would be thirty answers nobody reads, the cost `permissions.md` already records
/// against `additionalDirectories`. Accepting grants every rule listed and declining grants none,
/// and the session begins either way: a rule nobody granted is a prompt the person still gets.
///
/// Asked after the working directory's own question and separately from it, because trusting a
/// tree's content and granting a rule that suppresses a prompt are different claims. One box
/// answering both would be collecting an answer about content and spending it on capability, which
/// is what [PERM-14] is about.
///
/// `None` is the request to leave, for the reason it is at the other two questions: a session that
/// began behind it is one nobody agreed to have.
///
/// [PERM-14]: ../../docs/specs/permissions.md
pub fn ask_granted<B: Backend>(
    terminal: &mut Terminal<B>,
    rules: &[Proposed],
    carried: &mut String,
) -> Option<bool> {
    granting(rules, || {
        ask_one(terminal, carried, false, |frame, offered| {
            draw_granted(frame, rules, offered)
        })
    })
}

/// Whether the rules are granted, given how the one question about them was answered.
///
/// Separated from the terminal so the decision can be tested without one, the way [`accepted`] is.
/// An empty list is not asked about and grants nothing: no checkout `allow` entries means no box,
/// which is the common case and is what [PERM-12] requires of a session nobody configured.
///
/// [PERM-12]: ../../docs/specs/permissions.md
fn granting(rules: &[Proposed], answer: impl FnOnce() -> Answer) -> Option<bool> {
    if rules.is_empty() {
        return Some(false);
    }
    match answer() {
        // Never put here, since this question does not offer it; a yes is what it would have been.
        Answer::Trust | Answer::Remember => Some(true),
        Answer::Decline => Some(false),
        Answer::Leave => None,
    }
}

/// Which of the directories to open, given how the question about each was answered.
///
/// Separated from the terminal so the decision can be tested without one, the way [`trust_for`]
/// is. Leaving opens none of them, including any accepted before it: a person on their way out is
/// not somebody who granted something.
fn accepted(directories: &[String], mut answer: impl FnMut(&str) -> Answer) -> Option<Vec<String>> {
    let mut opening = Vec::new();
    for directory in directories {
        match answer(directory) {
            Answer::Trust | Answer::Remember => opening.push(directory.clone()),
            Answer::Decline => continue,
            Answer::Leave => return None,
        }
    }
    Some(opening)
}

/// The map for a session whose mode has already answered, or `None` where a person must answer.
///
/// Bypassing answers this question along with every other one, and answers it yes. That mode
/// already approves vouching for each quarantined file the planner asks to read, so putting the
/// question would be asking for a grant the session is about to make anyway, one file at a time,
/// and a modal box before the first prompt is the most conspicuous thing there is to put to a
/// person who asked to be asked about nothing.
///
/// Separated from the loop so it can be tested without a terminal.
pub fn answered_by(mode: PermissionMode, directory: &Path) -> Option<TrustStore> {
    match mode {
        PermissionMode::Bypass => Some(trusting_the_workspace(directory)),
        PermissionMode::Ask | PermissionMode::AcceptEdits | PermissionMode::Plan => None,
    }
}

/// The map an answer starts the session with, or `None` for leaving.
///
/// Every map is made against the working directory, declining included: a map is asked about
/// relative names whatever it holds, and one made against somewhere else would answer about
/// another project's files.
///
/// Public because the question is put on two surfaces and answered in one place. A session in
/// lines (CLI-14) asks it as a line rather than as a panel, and what a yes grants there has to be
/// what a yes grants here: two functions writing the map would be two readings of TRUST-7, and the
/// one the tests pin is this one.
///
/// Remembering grants what trusting does, so what a later session is spared asking is what this one
/// was granted, and nothing the record could come to mean is a rule a yes never writes.
pub fn trust_for(answer: Answer, directory: &Path) -> Option<TrustStore> {
    match answer {
        Answer::Leave => None,
        Answer::Trust | Answer::Remember => Some(trusting_the_workspace(directory)),
        Answer::Decline => Some(TrustStore::new(key_of(directory))),
    }
}

/// The rule trusting the workspace records: the root, which covers everything beneath it.
///
/// One place, so the map reached without the question is the map a yes would have written.
pub fn trusting_the_workspace(directory: &Path) -> TrustStore {
    let mut trust = TrustStore::new(key_of(directory));
    trust.trust(".");
    trust
}

/// Block until the user answers.
///
/// `keeping` is whether remembering is on offer at this question.
fn ask_one<B: Backend>(
    terminal: &mut Terminal<B>,
    carried: &mut String,
    keeping: bool,
    mut draw_it: impl FnMut(&mut ratatui::Frame, bool),
) -> Answer {
    let mut offered_to_leave = false;

    loop {
        // Drawn inside the loop rather than once before it, because the offer to leave is part of
        // what the question says: a panel drawn once would take the first interrupt and then look as
        // though nothing had happened.
        //
        // A terminal that cannot be drawn to cannot carry the question.
        if terminal
            .draw(|frame| draw_it(frame, offered_to_leave))
            .is_err()
        {
            return Answer::Decline;
        }

        match input::read() {
            // A paste answers nothing, and neither do words another program typed: both arrive as
            // one event whatever they carry, and nothing in either is a key somebody pressed.
            Ok(taken) => {
                // Asked of the event just handed out, so it is read before the next one replaces it.
                let arrived_alone = input::the_last_event_arrived_alone();
                if withdraws_the_offer(&taken) {
                    offered_to_leave = false;
                }
                match input::key_of(&taken) {
                    // Presses only: the interface asks for disambiguated keys, so a release arrives
                    // too, and answering twice grants standing permission on one keystroke.
                    Some(key) if key.kind != event::KeyEventKind::Press => continue,
                    Some(key) => match answer_for(key, offered_to_leave, arrived_alone, keeping) {
                        Response::Answer(answer) => return answer,
                        // Kept rather than dropped. What this refused to answer on was a run of keys
                        // another program wrote, and words that vanish leave a person with no account
                        // of what just happened: the question stayed up and their virtualenv
                        // activated, with nothing on the screen joining the two. Carried to the box,
                        // where they can read it and decide (#403).
                        Response::Nothing if !arrived_alone => {
                            carried.extend(input::text_of(&key));
                            continue;
                        }
                        Response::Offer => {
                            offered_to_leave = true;
                            continue;
                        }
                        Response::Nothing => continue,
                    },
                    None => continue,
                }
            }
            Err(_) => return Answer::Decline,
        }
    }
}

/// Whether an event withdraws an offer to leave that is standing.
///
/// Anything that is not the key that leaves. The offer answers the press just made, so a mouse
/// report, a resize, and words another program typed all mean somebody is still here and has moved
/// on. An offer left standing through one of those would let a byte written much later take it,
/// which turns two presses back into one: the interrupt an editor writes ahead of a virtualenv
/// activation arrives on its own (#403), so it arms the offer, and without this the offer was still
/// armed whenever the next one arrived. The session's own ladder keeps the same rule for the same
/// reason ([INPUT-4](../../../docs/specs/terminal-input.md)).
///
/// Separated from the loop so it can be tested without a terminal.
fn withdraws_the_offer(taken: &event::Event) -> bool {
    match input::key_of(taken) {
        // A release is the tail of a press already answered rather than something somebody did next.
        // Windows sends one for every keystroke, carrying the modifiers still held, so letting go of
        // Ctrl before C arrives as a bare `c`; withdrawing on that took the way out of this question
        // away from whoever lets go in that order, and this question is the first screen of a session.
        Some(key) if key.kind == event::KeyEventKind::Release => false,
        Some(key) => {
            !(key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c'))
        }
        None => true,
    }
}

/// Interpret one key press, or `None` for a key that answers nothing.
///
/// `keeping` is whether remembering is on offer; where it is not, `r` is a key like any other.
///
/// Separated from the loop so it can be tested without a terminal.
fn answer_for(
    key: KeyEvent,
    offered_to_leave: bool,
    arrived_alone: bool,
    keeping: bool,
) -> Response {
    // Raw mode delivers Ctrl-C as a key rather than as a signal, so a prompt that ignored it would
    // be a screen with no way out: the interrupt everyone reaches for would do nothing. It is not an
    // answer to the question, so it starts nothing rather than declining.
    //
    // **Twice, and neither press from a key that arrived with others.** Leaving here ends the
    // session before it begins, which nothing undoes, and an interrupt is one byte another program
    // can write into the terminal: the editor that activates a virtualenv writes one ahead of the
    // line it types (#403), and on one press that byte closed a question nobody had read. This is
    // the rule the session's own ladder keeps, kept here for the same reason (INPUT-4).
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('c') if !arrived_alone => Response::Nothing,
            KeyCode::Char('c') if offered_to_leave => Response::Answer(Answer::Leave),
            KeyCode::Char('c') => Response::Offer,
            _ => Response::Nothing,
        };
    }

    // **No answer at all from a key that arrived with others**, which is the whole of what stops a
    // program answering this question. A terminal cannot say who wrote a byte, but it can say what
    // was waiting together, and a person cannot fill the buffer between one read and the next: one
    // event waiting is a keystroke, and several are a write. The line an editor types to activate a
    // virtualenv spells an `n` on its way past (#403), and the question used to take it.
    //
    // This is asked here rather than by the reader, so nothing is withheld from anybody: a person's
    // own typing and a person's own paste arrive untouched and are answered by whatever reads them.
    // What the reader supplies is the fact, and what this does is decline to grant on it.
    if !arrived_alone {
        return Response::Nothing;
    }

    match key.code {
        KeyCode::Char('y' | 'Y') => Response::Answer(Answer::Trust),
        KeyCode::Char('r' | 'R') if keeping => Response::Answer(Answer::Remember),
        KeyCode::Char('n' | 'N') | KeyCode::Esc => Response::Answer(Answer::Decline),
        // Enter is deliberately not a yes: it is the key most likely to be pressed
        // out of habit, and this question grants standing permission.
        _ => Response::Nothing,
    }
}

/// What one key press did to the question.
#[derive(Debug, PartialEq, Eq)]
enum Response {
    /// The question is answered.
    Answer(Answer),
    /// The interrupt was pressed with nothing offered yet, so the way out is offered and the next
    /// press of it takes it.
    Offer,
    /// Nothing, and the question stays on the screen.
    Nothing,
}

/// Draw the question about the working directory.
///
/// `keeping` is where remembering would write the answer, or `None` where it is not offered.
fn draw(
    frame: &mut ratatui::Frame,
    directory: &Path,
    keeping: Option<&Path>,
    offered_to_leave: bool,
) {
    let mut lines = vec![
        asking(
            t!(trust_directory_question),
            &directory.display().to_string(),
        ),
        Line::raw(""),
        // One line each, wrapped by the paragraph rather than broken here: a translation does
        // not break where the English did, and a sentence split into two spans cannot be rewrapped.
        Line::from(Span::raw(t!(trust_directory_explained))),
        Line::raw(""),
        Line::from(Span::styled(
            t!(trust_directory_regardless),
            Style::default().fg(theme::muted()),
        )),
    ];

    // What `r` would do, where it is offered (RUN-19). What its one word cannot say: that later
    // sessions read it, that the directory has to be this one exactly, and where it is written,
    // which is part of the grant rather than a footnote because nobody can endorse a record they
    // were not shown. Not indented, as the command prompt's are: these lines are long enough to
    // wrap, and a wrapped line starts again at the left edge, under nothing.
    if let Some(path) = keeping {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            t!(trust_directory_remember_explained),
            Style::default().fg(theme::muted()),
        )));
        // The half a person is most likely to assume the other way, since a remembered answer
        // elsewhere in this program is kept per directory name, so it is the half that is coloured.
        lines.push(Line::from(Span::styled(
            t!(trust_directory_remember_exact),
            Style::default().fg(theme::running()),
        )));
        lines.push(Line::from(Span::styled(
            t!(trust_directory_remember_where),
            Style::default().fg(theme::muted()),
        )));
        lines.push(Line::from(Span::styled(
            path.display().to_string(),
            Style::default().add_modifier(Modifier::BOLD),
        )));
    }

    lines.extend([
        Line::raw(""),
        keys(
            t!(trust_directory_yes),
            t!(trust_directory_no),
            keeping.map(|_| t!(trust_directory_remember)),
            offered_to_leave,
        ),
    ]);

    panel(frame, t!(trust_directory_title), lines);
}

/// Draw the question about one directory a settings file named.
fn draw_named(frame: &mut ratatui::Frame, directory: &str, offered_to_leave: bool) {
    let lines = vec![
        asking(t!(named_directory_question), directory),
        Line::raw(""),
        Line::from(Span::raw(t!(named_directory_explained))),
        Line::raw(""),
        Line::from(Span::styled(
            t!(named_directory_regardless),
            Style::default().fg(theme::muted()),
        )),
        Line::raw(""),
        keys(
            t!(named_directory_yes),
            t!(named_directory_no),
            None,
            offered_to_leave,
        ),
    ];

    panel(frame, t!(named_directory_title), lines);
}

/// Draw the question about the `allow` rules a checkout proposed.
///
/// Every rule is listed, in the order the files wrote them, with the file each came from. That is
/// what makes this an acceptable gate at all: the answer is informed consent for specific grants
/// rather than a general feeling about the tree, and a question that named none of them would be the
/// defect wearing a consent story.
fn draw_granted(frame: &mut ratatui::Frame, rules: &[Proposed], offered_to_leave: bool) {
    let mut lines = vec![
        Line::from(Span::styled(
            t!(granted_rules_question),
            Style::default()
                .fg(theme::brand_primary())
                .add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
    ];
    // The rule first and the file under it, because the rule is what an answer is about and the file
    // is what explains where a line the person never wrote came from.
    for rule in rules {
        lines.push(Line::from(Span::styled(
            format!("  {}", rule.rule),
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(Span::styled(
            format!("    {}", rule.path.display()),
            Style::default().fg(theme::muted()),
        )));
    }
    lines.extend([
        Line::raw(""),
        Line::from(Span::raw(t!(granted_rules_explained))),
        Line::raw(""),
        Line::from(Span::styled(
            t!(granted_rules_regardless),
            Style::default().fg(theme::muted()),
        )),
        Line::raw(""),
        keys(
            t!(granted_rules_yes),
            t!(granted_rules_no),
            None,
            offered_to_leave,
        ),
    ]);

    panel(frame, t!(granted_rules_title), lines);
}

/// What is being asked, and the path it is being asked about.
///
/// The path is the whole of what an answer is about, so it is drawn rather than summarised.
fn asking(question: &str, directory: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{question} "),
            Style::default()
                .fg(theme::brand_primary())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            directory.to_string(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw("?"),
    ])
}

/// The answers on offer: the same two keys and the same way out at every question, and `r` between
/// them where `remember` names it.
fn keys(yes: &str, no: &str, remember: Option<&str>, offered_to_leave: bool) -> Line<'static> {
    let mut spans = vec![
        Span::styled(
            "  y",
            Style::default()
                .fg(theme::ok())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {yes}    ")),
    ];
    if let Some(remember) = remember {
        spans.push(Span::styled(
            "r",
            Style::default()
                .fg(theme::running())
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(format!(" {remember}    ")));
    }
    spans.extend([
        Span::styled(
            "n",
            Style::default()
                .fg(theme::fail())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {no}    ")),
        Span::styled(
            "ctrl-c",
            Style::default()
                .fg(theme::muted())
                .add_modifier(Modifier::BOLD),
        ),
        // Says which press leaves once the first has been made, because a press that appeared to do
        // nothing and said nothing reads as a question that has stopped answering.
        Span::styled(
            format!(
                " {}",
                if offered_to_leave {
                    t!(trust_quit_again)
                } else {
                    t!(quit)
                }
            ),
            Style::default().fg(theme::muted()),
        ),
    ]);
    Line::from(spans)
}

/// Draw one question, in a box whose every cell the theme paints.
///
/// One implementation for both questions here, because the chrome is what says the question is the
/// system's own: `Clear` empties the cells under the panel without colouring them, so the
/// background and text colour are set for the block rather than for the border alone.
fn panel(frame: &mut ratatui::Frame, title: &str, lines: Vec<Line<'static>>) {
    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme::brand_primary()))
                .title(format!(" {title} "))
                .style(Style::default().bg(theme::background()).fg(theme::text())),
        )
        .wrap(Wrap { trim: false });
    let area = fitted(frame.area(), &paragraph);
    frame.render_widget(Clear, area);
    frame.render_widget(paragraph, area);
}

/// The centred box, made taller where what it says does not fit, up to the whole screen.
///
/// The keys are the last line, so a box that clipped its content would lose them first and leave a
/// question with no way to answer it on the screen.
fn fitted(screen: Rect, paragraph: &Paragraph) -> Rect {
    let share = centred(screen);
    let needed = paragraph.line_count(share.width.saturating_sub(2));
    let height = u16::try_from(needed)
        .unwrap_or(u16::MAX)
        .clamp(share.height, screen.height);
    Rect {
        y: screen.y + (screen.height - height) / 2,
        height,
        ..share
    }
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
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;

    /// The reported failure, at the question it reaches first. An editor activating a virtualenv
    /// types `source .../env/bin/activate` into the terminal it opened, and every character of it
    /// arrives in one read: the `n` in the path used to answer this question, so the directory was
    /// settled by a program and the rest of the path went into the box (#403).
    #[test]
    fn a_line_another_program_typed_answers_nothing() {
        for c in " source /Users/me/project/env/bin/activate\r".chars() {
            let key = match c {
                '\r' => KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                c => KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE),
            };
            // Arriving with the rest of the line, which is what one read of a write looks like.
            assert_eq!(
                answer_for(key, false, false, false),
                Response::Nothing,
                "{c:?} out of a written line answered the question"
            );
        }
    }

    /// And a person's own press still answers it, which is the half that has to keep working.
    #[test]
    fn a_press_of_its_own_still_answers() {
        assert_eq!(
            answer_for(
                KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
                false,
                true,
                false
            ),
            Response::Answer(Answer::Trust)
        );
        assert_eq!(
            answer_for(
                KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE),
                false,
                true,
                false
            ),
            Response::Answer(Answer::Decline)
        );
    }

    /// What the question refuses is kept, not dropped. A line that vanished left a person with a
    /// question still waiting and a virtualenv activated, and nothing on the screen joining the two.
    #[test]
    fn what_the_question_refuses_is_carried_for_the_box() {
        let mut carried = String::new();
        for c in " source /tmp/x/env/bin/activate".chars() {
            let key = KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
            // Arriving with the rest of the line, so the question answers nothing on it.
            assert_eq!(answer_for(key, false, false, false), Response::Nothing);
            carried.extend(crate::input::text_of(&key));
        }
        assert_eq!(carried, " source /tmp/x/env/bin/activate");
    }

    /// A chord inside those words is not carried, since what a program wrote is words and a chord in
    /// them was pressed by nobody.
    #[test]
    fn a_chord_among_the_carried_words_is_left_out() {
        let interrupt = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(crate::input::text_of(&interrupt), None);
    }

    /// One key pressed at a question with nothing offered, arriving on its own.
    fn pressing(code: KeyCode) -> Response {
        answer_for(KeyEvent::new(code, KeyModifiers::NONE), false, true, false)
    }

    /// The rules question as it is first drawn, with no interrupt pressed yet.
    fn draw_granted_at_rest(frame: &mut ratatui::Frame, rules: &[Proposed]) {
        draw_granted(frame, rules, false);
    }

    /// The question as it is first drawn, with no interrupt pressed yet.
    fn draw_at_rest(frame: &mut ratatui::Frame, directory: &Path) {
        draw(frame, directory, None, false);
    }

    /// The same for a directory a settings file named.
    fn draw_named_at_rest(frame: &mut ratatui::Frame, directory: &str) {
        draw_named(frame, directory, false);
    }

    /// A press that appeared to do nothing and said nothing reads as a question that has stopped
    /// answering, so the keys line says which press leaves once the first has been made.
    #[test]
    fn the_question_says_which_press_leaves_once_one_has_been_made() {
        let at_rest = rendered(|frame| draw(frame, Path::new("/tmp/x"), None, false));
        assert!(
            at_rest.contains("ctrl-c quit"),
            "the way out was not named at all: {at_rest}"
        );

        // Short enough to sit on the keys line at the narrow width this renders at, since a hint
        // that wrapped across the border would say it worse than not saying it.
        let offered = rendered(|frame| draw(frame, Path::new("/tmp/x"), None, true));
        assert!(
            offered.contains("ctrl-c again"),
            "the press that offered the way out said nothing: {offered}"
        );
    }

    /// The working directory these answers are about.
    fn here() -> &'static Path {
        Path::new("/work")
    }

    /// The characters one question puts on a terminal, in reading order.
    ///
    /// Takes the drawing rather than a path, so both questions are measured by one helper and an
    /// assertion about what a prompt says cannot be made of one and forgotten on the other.
    fn rendered(draw_it: impl FnOnce(&mut ratatui::Frame)) -> String {
        let mut terminal = Terminal::new(TestBackend::new(72, 20)).expect("terminal");
        terminal.draw(draw_it).expect("draw");
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    /// Every cell the border encloses carries the theme's own colours, for one question.
    ///
    /// `Clear` empties the cells under a panel without colouring them, so a prompt that styled
    /// only its border is a hole in the palette: themed border, terminal-default everything else.
    fn paints_the_themes_chrome(draw_it: impl FnOnce(&mut ratatui::Frame)) {
        let _held = theme::exclusive();
        let theme = theme::find("nord").expect("nord is built in");
        theme::apply(&theme);
        let painted = theme::background();

        let mut terminal = Terminal::new(TestBackend::new(72, 20)).expect("terminal");
        terminal.draw(draw_it).expect("draw");
        let buffer = terminal.backend().buffer().clone();
        theme::apply_brave();

        let inside = drawn_box(&buffer);
        // Every cell the border encloses, including the rows the prose did not reach: an unpainted
        // row below the keys is the same hole as an unpainted one beside them.
        for y in 1..inside.height - 1 {
            for x in 1..inside.width - 1 {
                let cell = &buffer[(inside.x + x, inside.y + y)];
                assert_eq!(
                    cell.bg, painted,
                    "the cell at {x},{y} kept the terminal's own background"
                );
                assert_ne!(
                    cell.fg,
                    Color::Reset,
                    "the cell at {x},{y} kept the terminal's own text colour"
                );
            }
        }
    }

    /// Where the panel's rounded border was drawn, since a box that grew to fit is not the share
    /// [`centred`] gives it.
    fn drawn_box(buffer: &ratatui::buffer::Buffer) -> Rect {
        let area = buffer.area;
        let at = |symbol: &str| {
            (area.top()..area.bottom())
                .flat_map(|y| (area.left()..area.right()).map(move |x| (x, y)))
                .find(|&(x, y)| buffer[(x, y)].symbol() == symbol)
                .expect("the panel's border")
        };
        let (left, top) = at("╭");
        let (right, bottom) = at("╯");
        Rect::new(left, top, right - left + 1, bottom - top + 1)
    }

    fn names(directories: &[&str]) -> Vec<String> {
        directories.iter().map(|d| d.to_string()).collect()
    }

    /// The flag says nothing is put to the person, and a modal box before the first prompt is the
    /// most conspicuous thing there is to put. The map is the one a yes would have written, which
    /// is what the mode is about to grant anyway: it approves vouching for every quarantined file
    /// the planner reads, so the workspace ends up trusted a file at a time regardless.
    #[test]
    fn bypassing_trusts_the_workspace_instead_of_asking() {
        let trust =
            answered_by(PermissionMode::Bypass, here()).expect("bypassing answers the question");

        assert!(trust.is_trusted("."));
        assert!(trust.is_trusted("src/main.rs"), "the rule covers the tree");
    }

    /// Every other mode answers fewer questions than this one, so none of them may answer the one
    /// that grants standing permission over a whole tree.
    #[test]
    fn every_other_mode_leaves_the_question_to_the_person() {
        for mode in [
            PermissionMode::Ask,
            PermissionMode::AcceptEdits,
            PermissionMode::Plan,
        ] {
            assert!(answered_by(mode, here()).is_none(), "{mode:?} answered it");
        }
    }

    #[test]
    fn the_prompt_names_the_directory_and_both_answers() {
        let output = rendered(|frame| draw_at_rest(frame, Path::new("/home/me/project")));
        assert!(output.contains("/home/me/project"));
        assert!(output.contains("trust it"));
        assert!(output.contains("every write"));
    }

    /// The question must say what saying yes actually does, since it grants standing
    /// permission rather than approving one action.
    #[test]
    fn the_prompt_explains_the_consequence() {
        let output = rendered(|frame| draw_at_rest(frame, Path::new("/tmp/x")));
        assert!(output.contains("trusted"), "no mention of trust: {output}");
        // Wrapping can split a phrase across lines, so assert on a short fragment.
        assert!(
            output.contains("Say no if you"),
            "no guidance on when to decline: {output}"
        );
    }

    /// This is the first screen of a session and it grants standing permission over a whole tree,
    /// so its chrome is the first thing that says the question is the system's own.
    #[test]
    fn the_prompt_paints_the_themes_background_inside_its_border() {
        paints_the_themes_chrome(|frame| draw_at_rest(frame, Path::new("/home/me/project")));
    }

    /// A settings file arrives with whatever produced the checkout, so a directory named in one is
    /// a request. Only the answer opens it, and a name nobody accepted leaves the path as
    /// unreachable and unvouched for as any other outside the workspace.
    #[test]
    fn a_directory_a_file_named_is_opened_only_where_the_person_accepts_it() {
        let named = names(&["/home/me/notes", "/home/me/.ssh", "/srv/shared"]);

        let accepted_one = accepted(&named, |directory| match directory {
            "/home/me/notes" => Answer::Trust,
            _ => Answer::Decline,
        })
        .expect("answering still starts a session");
        assert_eq!(accepted_one, names(&["/home/me/notes"]));

        let accepted_none =
            accepted(&named, |_| Answer::Decline).expect("declining still starts a session");
        assert!(
            accepted_none.is_empty(),
            "a declined name was opened anyway: {accepted_none:?}"
        );
    }

    /// Leaving is the answer to no question, so a name accepted before it is not a grant either:
    /// a session that opened a directory on the way out would be acting on an answer nobody
    /// finished giving.
    #[test]
    fn leaving_at_one_of_the_questions_opens_nothing() {
        let named = names(&["/home/me/notes", "/home/me/.ssh"]);

        let answered = accepted(&named, |directory| match directory {
            "/home/me/notes" => Answer::Trust,
            _ => Answer::Leave,
        });

        assert!(answered.is_none(), "leaving started a session anyway");
    }

    /// The rules a checkout proposes in the tests below.
    fn proposed(rules: &[(&str, &str)]) -> Vec<Proposed> {
        rules
            .iter()
            .map(|(path, rule)| Proposed::new(Path::new(path), rule))
            .collect()
    }

    /// PERM-15: accepting grants the rules that were listed and declining grants none, and either
    /// way the session begins. Both directions, because a fix that granted on any answer would pass
    /// a test that only checked the accepting one, and that is the defect this route closes.
    #[test]
    fn the_rules_are_granted_only_where_the_person_accepts_them() {
        let rules = proposed(&[(
            "/work/.bravebot/settings.json",
            "Bash(bash scripts/check.sh)",
        )]);

        assert!(
            granting(&rules, || Answer::Trust).expect("answering still starts a session"),
            "an accepted rule was not granted"
        );
        assert!(
            !granting(&rules, || Answer::Decline).expect("declining still starts a session"),
            "a declined rule was granted anyway"
        );
    }

    /// PERM-12: no rules means no change. A session with nothing proposed is asked nothing extra,
    /// which is the common case: a box that appeared with an empty list would be a question about
    /// nothing, and a question that changes nothing either way trains answering without reading.
    #[test]
    fn nothing_is_asked_where_there_is_nothing_to_grant() {
        let mut asked = false;
        let granted = granting(&[], || {
            asked = true;
            Answer::Trust
        })
        .expect("a session with nothing proposed still starts");

        assert!(!asked, "a question was put about an empty list");
        assert!(!granted, "an empty list granted something");
    }

    /// Leaving is the answer to no question, so nothing is granted on the way out: a session that
    /// granted a rule while its user was leaving would be acting on an answer nobody gave.
    #[test]
    fn leaving_at_the_rules_question_grants_nothing_and_starts_no_session() {
        let rules = proposed(&[(
            "/work/.bravebot/settings.json",
            "Bash(bash scripts/check.sh)",
        )]);

        assert!(
            granting(&rules, || Answer::Leave).is_none(),
            "leaving started a session anyway"
        );
    }

    /// PERM-15: the box names every rule it would grant and the file each came from. This is what
    /// makes trust an acceptable gate here at all: a question that collected an answer about the
    /// tree and spent it on capability would be the defect wearing a consent story, so a rule the
    /// person was not shown is a rule the box may not grant.
    #[test]
    fn the_rules_prompt_names_every_rule_and_the_file_it_came_from() {
        let rules = proposed(&[
            (
                "/work/.bravebot/settings.json",
                "Bash(bash scripts/check.sh)",
            ),
            ("/work/.bravebot/settings.local.json", "Edit(src/**)"),
        ]);
        let output = rendered(|frame| draw_granted_at_rest(frame, &rules));

        assert!(
            output.contains("Bash(bash scripts/check.sh)"),
            "the first rule was not shown: {output}"
        );
        assert!(
            output.contains("Edit(src/**)"),
            "the second rule was not shown: {output}"
        );
        assert!(
            output.contains("settings.json"),
            "no file was named: {output}"
        );
        assert!(
            output.contains("settings.local.json"),
            "the second rule's file was not named: {output}"
        );
    }

    /// The box has to say what accepting does, since a rule answers a prompt the person would
    /// otherwise have seen and that is the one thing they cannot read off a transcript afterwards.
    #[test]
    fn the_rules_prompt_explains_what_granting_does() {
        let rules = proposed(&[(
            "/work/.bravebot/settings.json",
            "Bash(bash scripts/check.sh)",
        )]);
        let output = rendered(|frame| draw_granted_at_rest(frame, &rules));

        // Wrapping can split a phrase across lines, so assert on short fragments.
        assert!(
            output.contains("approval prompt"),
            "no mention of what a rule answers: {output}"
        );
        assert!(
            output.contains("not by you"),
            "no mention of who wrote the rules: {output}"
        );
    }

    /// This question needs the theme's own chrome for the reason the other two do: the frame is what
    /// says the question is the system's and not something a file being read is asking.
    #[test]
    fn the_rules_prompt_paints_the_themes_background_inside_its_border() {
        let rules = proposed(&[(
            "/work/.bravebot/settings.json",
            "Bash(bash scripts/check.sh)",
        )]);
        paints_the_themes_chrome(|frame| draw_granted_at_rest(frame, &rules));
    }

    /// The path is the whole of what the answer is about, and it is the one thing a settings file
    /// chose rather than the person reading the box.
    #[test]
    fn the_named_prompt_shows_the_directory_it_would_open() {
        let output = rendered(|frame| draw_named_at_rest(frame, "/home/me/.ssh"));
        assert!(output.contains("/home/me/.ssh"), "no path: {output}");
        assert!(output.contains("open it"));
        assert!(output.contains("leave it closed"));
    }

    /// Opening a directory grants two things at once, reach and trust, and neither is on the
    /// screen unless the question says so. The question also has to say where it came from: a box
    /// naming a directory the person has never typed is otherwise unexplained.
    #[test]
    fn the_named_prompt_explains_what_opening_does() {
        let output = rendered(|frame| draw_named_at_rest(frame, "/srv/shared"));
        // Wrapping can split a phrase across lines, so assert on short fragments.
        assert!(
            output.contains("settings file"),
            "no mention of where the name came from: {output}"
        );
        assert!(output.contains("trusted"), "no mention of trust: {output}");
    }

    /// This question needs the theme's own chrome for the reason the working directory's does: the
    /// frame is what says the question is the system's and not something a file being read is
    /// asking.
    #[test]
    fn the_named_prompt_paints_the_themes_background_inside_its_border() {
        paints_the_themes_chrome(|frame| draw_named_at_rest(frame, "/home/me/.ssh"));
    }

    /// All three questions, since any can be the first thing a session draws on a small terminal.
    ///
    /// Surviving the draw is half of it. These are asked before a session exists, and nothing
    /// else is on the screen to say what the keys mean, so a small terminal that drew the border
    /// and lost the question would leave somebody pressing `y` at a panel that never said what it
    /// was about. The rules box is the one that can be arbitrarily long, so it is also the one where
    /// the question could be pushed out of view by its own content.
    #[test]
    fn a_tiny_terminal_still_renders() {
        let mut terminal = Terminal::new(TestBackend::new(24, 8)).expect("terminal");
        terminal
            .draw(|frame| draw_at_rest(frame, Path::new("/tmp/x")))
            .expect("must not panic on a small area");
        assert!(
            drawn_on(&terminal).contains("Trust /tmp/x?"),
            "the question was drawn out of view: {}",
            drawn_on(&terminal)
        );

        terminal
            .draw(|frame| draw_named_at_rest(frame, "/tmp/x"))
            .expect("must not panic on a small area");
        assert!(
            drawn_on(&terminal).contains("Open /tmp/x?"),
            "the question was drawn out of view: {}",
            drawn_on(&terminal)
        );

        let rules = proposed(&[(
            "/work/.bravebot/settings.json",
            "Bash(bash scripts/check.sh)",
        )]);
        terminal
            .draw(|frame| draw_granted_at_rest(frame, &rules))
            .expect("must not panic on a small area");
        assert!(
            drawn_on(&terminal).contains("This project"),
            "the question was drawn out of view: {}",
            drawn_on(&terminal)
        );
    }

    /// The characters on a terminal that has already been drawn to.
    fn drawn_on(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    /// Trusting records the root, which covers the whole tree. Asked of the answer rather than of
    /// a store built here, for the reason [`declining_trusts_nothing`] is: a `trust_for` that
    /// returned an empty map on a yes would satisfy a test that trusted `.` itself, and every
    /// write would be shown to somebody who said yes.
    #[test]
    fn trusting_covers_the_whole_workspace() {
        let trust = trust_for(Answer::Trust, here()).expect("trusting starts a session");
        assert!(trust.is_trusted("."));
        assert!(trust.is_trusted("src/main.rs"));
        assert!(trust.is_trusted("deep/nested/file.txt"));
    }

    /// The key that grants standing permission over a whole tree is the deliberate one and no
    /// other. Enter is the key most likely to be pressed out of habit, so it answers nothing: a
    /// keystroke made without reading must not be the answer that costs the most to get wrong.
    #[test]
    fn only_y_trusts_and_enter_answers_nothing() {
        assert_eq!(
            pressing(KeyCode::Char('y')),
            Response::Answer(Answer::Trust)
        );
        assert_eq!(
            pressing(KeyCode::Char('Y')),
            Response::Answer(Answer::Trust)
        );
        assert_eq!(
            pressing(KeyCode::Char('n')),
            Response::Answer(Answer::Decline)
        );
        assert_eq!(
            pressing(KeyCode::Char('N')),
            Response::Answer(Answer::Decline)
        );
        assert_eq!(pressing(KeyCode::Esc), Response::Answer(Answer::Decline));
        assert_eq!(pressing(KeyCode::Enter), Response::Nothing);
    }

    /// `r` answers only where the question offered it (TRUST-23): at a question that did not, it
    /// is a key nobody was told about, and a yes that outlives the session is not one to take from
    /// a key the person was never shown.
    #[test]
    fn r_remembers_only_at_a_question_that_offers_it() {
        let remember = |code, keeping| {
            answer_for(
                KeyEvent::new(code, KeyModifiers::NONE),
                false,
                true,
                keeping,
            )
        };
        assert_eq!(
            remember(KeyCode::Char('r'), true),
            Response::Answer(Answer::Remember)
        );
        assert_eq!(
            remember(KeyCode::Char('R'), true),
            Response::Answer(Answer::Remember)
        );
        assert_eq!(remember(KeyCode::Char('r'), false), Response::Nothing);
        assert_eq!(remember(KeyCode::Char('R'), false), Response::Nothing);
    }

    /// The answer that lasts longest is held to the rule the others are: an `r` in a line another
    /// program wrote answers nothing, and neither does a chord.
    #[test]
    fn an_r_nobody_pressed_on_its_own_remembers_nothing() {
        let r = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE);
        assert_eq!(answer_for(r, false, false, true), Response::Nothing);
        let chord = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL);
        assert_eq!(answer_for(chord, false, true, true), Response::Nothing);
    }

    /// Remembering grants what yes grants and nothing more (TRUST-23): the record says the question
    /// need not be put again, not that the answer grew.
    #[test]
    fn remembering_trusts_exactly_what_yes_trusts() {
        let remembered = trust_for(Answer::Remember, here()).expect("remembering starts a session");
        assert_eq!(
            Some(remembered),
            trust_for(Answer::Trust, here()),
            "remembering granted a different map from yes"
        );
    }

    /// The record is written where the person can see it and read it before agreeing, so the
    /// question says what `r` does, that the directory has to be this one, and names the file.
    #[test]
    fn the_prompt_offering_to_remember_names_the_record_it_writes() {
        let record = Path::new("/home/me/.bravebot/trusted/work-1a2b.jsonl");
        let output = rendered(|frame| draw(frame, here(), Some(record), false));

        assert!(
            output.contains("trust and remember"),
            "the key was not named: {output}"
        );
        assert!(
            output.contains("exactly this directory"),
            "nothing said the answer is about this directory alone: {output}"
        );
        assert!(
            output.contains("work-1a2b.jsonl"),
            "the record was not named: {output}"
        );
        assert!(
            output.contains("/forget-trust"),
            "nothing said how to take it back: {output}"
        );
    }

    /// And where it is not offered, nothing on the screen says it is.
    #[test]
    fn the_prompt_not_offering_to_remember_says_nothing_of_it() {
        let output = rendered(|frame| draw_at_rest(frame, here()));
        assert!(
            !output.contains("remember"),
            "a key that answers nothing was shown: {output}"
        );
        assert!(!output.contains("/forget-trust"), "{output}");
    }

    /// The offer makes the question longer than the share of the screen the others take, and the
    /// keys are its last line, so at a common terminal size the box grows to hold them rather than
    /// clipping the one line that says how to answer.
    #[test]
    fn the_keys_stay_on_screen_when_the_offer_lengthens_the_question() {
        let directory = Path::new("/Users/somebody/projects/a-long-project-name/checkout");
        let store =
            bravebot_agent::trusted::Store::new(Path::new("/Users/somebody/.bravebot"), directory);
        let record = store.path();
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        terminal
            .draw(|frame| draw(frame, directory, Some(record), false))
            .expect("draw");
        let output = inside(terminal.backend().buffer());

        assert!(output.contains("ctrl-c"), "the keys were clipped: {output}");
        assert!(
            output.contains("trust and remember"),
            "the remember key was clipped: {output}"
        );
        assert!(
            output.contains(&record.display().to_string()),
            "the record was clipped: {output}"
        );
        paints_the_themes_chrome(|frame| draw(frame, directory, Some(record), false));
    }

    /// What the panel says, its rows run together, so a path wrapped across two of them reads as
    /// one.
    fn inside(buffer: &ratatui::buffer::Buffer) -> String {
        let panel = drawn_box(buffer);
        (panel.top() + 1..panel.bottom() - 1)
            .map(|y| {
                let row: String = (panel.left() + 1..panel.right() - 1)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect();
                row.trim_end().to_string()
            })
            .collect()
    }

    /// A terminal too small for all of it still shows what is being asked.
    #[test]
    fn a_tiny_terminal_offering_to_remember_still_renders() {
        let record = Path::new("/home/me/.bravebot/trusted/x-1.jsonl");
        let mut terminal = Terminal::new(TestBackend::new(24, 8)).expect("terminal");
        terminal
            .draw(|frame| draw(frame, Path::new("/tmp/x"), Some(record), false))
            .expect("must not panic on a small area");
        assert!(
            drawn_on(&terminal).contains("Trust /tmp/x?"),
            "the question was drawn out of view: {}",
            drawn_on(&terminal)
        );
    }

    /// Ctrl-C is the interrupt everyone reaches for, and raw mode turns it into an ordinary key
    /// press. A prompt that ignored it would be a screen with no way out, so it still leaves; what
    /// it takes is a second press.
    #[test]
    fn ctrl_c_leaves_on_the_second_press() {
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(answer_for(key, false, true, false), Response::Offer);
        assert_eq!(
            answer_for(key, true, true, false),
            Response::Answer(Answer::Leave)
        );
    }

    /// The reported case at the question nobody had answered yet. VS Code writes one interrupt ahead
    /// of the virtualenv line, and on one press that byte ended the session before it began: the
    /// person saw bravebot vanish and their shell run the activation (#403).
    #[test]
    fn one_interrupt_another_program_wrote_closes_nothing() {
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_ne!(
            answer_for(key, false, true, false),
            Response::Answer(Answer::Leave),
            "one byte ended the session before it began"
        );
    }

    /// And two of them in one write are two key events rather than two presses, so neither half of
    /// the gesture comes from a key that arrived with others.
    #[test]
    fn two_interrupts_that_arrived_together_close_nothing() {
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(answer_for(key, false, false, false), Response::Nothing);
        assert_eq!(answer_for(key, true, false, false), Response::Nothing);
    }

    /// And an offer does not stand about waiting to be taken. The interrupt an editor writes arrives
    /// on its own, so it arms the offer; if anything happening afterwards left it armed, the next
    /// such byte would leave, and the two presses would be one again.
    #[test]
    fn anything_but_the_key_that_leaves_withdraws_the_offer() {
        let interrupt = event::Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(
            !withdraws_the_offer(&interrupt),
            "the second press of the gesture withdrew the offer it was answering"
        );

        for taken in [
            event::Event::Key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE)),
            event::Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            event::Event::Resize(80, 24),
            event::Event::FocusGained,
            event::Event::Paste("source /tmp/x/env/bin/activate".to_owned()),
        ] {
            assert!(
                withdraws_the_offer(&taken),
                "{taken:?} left the offer standing"
            );
        }
    }

    /// Except a release, which is the tail of the press being answered. A terminal may report one for
    /// every keystroke, and which modifiers it carries depends on the order somebody lets go of the
    /// keys: letting go of Ctrl before C sends a bare `c`. Withdrawing on that meant the way out of
    /// this question depended on how a person happened to release two keys, and for one of the two
    /// orders there was no way out at all.
    #[test]
    fn a_key_release_does_not_withdraw_the_offer() {
        for code in [KeyCode::Char('c'), KeyCode::Char('n')] {
            let released = event::Event::Key(KeyEvent::new_with_kind(
                code,
                KeyModifiers::NONE,
                event::KeyEventKind::Release,
            ));
            assert!(
                !withdraws_the_offer(&released),
                "letting go of {code:?} withdrew the offer"
            );
        }
    }

    /// Leaving is not a quiet decline: a session that started anyway would be one the user
    /// never agreed to have.
    #[test]
    fn leaving_starts_no_session() {
        assert!(trust_for(Answer::Leave, here()).is_none());
        assert!(trust_for(Answer::Decline, here()).is_some());
    }

    /// A plain `c` is not an interrupt, and neither is any other control chord.
    #[test]
    fn only_ctrl_c_leaves() {
        assert_eq!(
            answer_for(
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE),
                false,
                true,
                false
            ),
            Response::Nothing
        );
        assert_eq!(
            answer_for(
                KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL),
                false,
                true,
                false
            ),
            Response::Nothing
        );
    }

    /// Declining leaves nothing trusted, so every write is shown. Asked of the answer rather than
    /// of a store built here, which is what the clause is about: a `trust_for` that trusted `.`
    /// on a decline would have satisfied a test that only built its own empty store.
    #[test]
    fn declining_trusts_nothing() {
        let trust = trust_for(Answer::Decline, here()).expect("declining still starts a session");
        assert!(trust.is_empty());
        assert!(!trust.is_trusted("src/main.rs"));
        assert!(!trust.is_trusted("."));
    }
}
