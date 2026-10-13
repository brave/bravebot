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
use bravebot_core::trust::TrustStore;
use bravebot_i18n::t;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::crossterm::event::{self, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
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
    /// Trust, and keep the answer for later sessions started in this directory or below its git root (TRUST-23).
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
    // Kept across draws for the reason the rules question keeps its own: what `r` needs is that what
    // it writes was on the screen, whichever draw put it there.
    let mut recorded = false;
    let answer = ask_one(terminal, carried, |frame, offered, scroll| {
        draw(frame, directory, keeping, offered, scroll, &mut recorded)
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
        ask_one(terminal, carried, |frame, offered, scroll| {
            draw_named(frame, directory, offered, scroll)
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
/// A yes is taken only once every rule has been on the screen with its file (PERM-15). A list longer
/// than the box is read a part at a time and one `y` grants all of it, so a yes taken with part of
/// the list below the bottom edge would grant rules nobody was shown (#954).
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
    // Kept across draws rather than read off the last one, because a list longer than the box is
    // read a part at a time: what a yes needs is that each rule was on the screen, not all at once.
    let mut seen = vec![false; rules.len()];
    granting(rules, || {
        ask_one(terminal, carried, |frame, offered, scroll| {
            draw_granted(frame, rules, offered, scroll, &mut seen)
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
        Answer::Decline => Some(bravebot_agent::workspace::trust_store(directory)),
    }
}

/// The rule trusting the workspace records: the root, which covers everything beneath it.
///
/// One place, so the map reached without the question is the map a yes would have written.
pub fn trusting_the_workspace(directory: &Path) -> TrustStore {
    let mut trust = bravebot_agent::workspace::trust_store(directory);
    trust.trust(".");
    trust
}

/// Block until the user answers.
///
/// `draw_it` draws the question with what scrolls moved down by the rows it is given, and says how
/// far that can go and which yeses may be taken yet.
fn ask_one<B: Backend>(
    terminal: &mut Terminal<B>,
    carried: &mut String,
    mut draw_it: impl FnMut(&mut ratatui::Frame, bool, u16) -> Drawn,
) -> Answer {
    let mut offered_to_leave = false;
    let mut scroll = 0u16;

    loop {
        let mut drawn = Drawn {
            furthest: 0,
            page: 1,
            answerable: false,
            remembering: false,
        };
        // Drawn inside the loop rather than once before it, because the offer to leave is part of
        // what the question says: a panel drawn once would take the first interrupt and then look as
        // though nothing had happened.
        //
        // A terminal that cannot be drawn to cannot carry the question.
        if terminal
            .draw(|frame| drawn = draw_it(frame, offered_to_leave, scroll))
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
                    Some(key) => match answer_for(
                        key,
                        offered_to_leave,
                        arrived_alone,
                        drawn.remembering,
                        drawn.answerable,
                    ) {
                        Response::Answer(answer) => return answer,
                        Response::Scroll(by) => {
                            scroll = drawn.moved(scroll, by);
                            continue;
                        }
                        Response::Page(by) => {
                            scroll = drawn.moved(scroll, by.saturating_mul(i32::from(drawn.page)));
                            continue;
                        }
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
/// `keeping` is whether `r` is taken: remembering is on offer and what it writes has been on the
/// screen. Where it is not, `r` is a key like any other. `answerable` is whether a yes may be taken
/// yet, which at the rules question is not until every rule has been on the screen.
///
/// Separated from the loop so it can be tested without a terminal.
fn answer_for(
    key: KeyEvent,
    offered_to_leave: bool,
    arrived_alone: bool,
    keeping: bool,
    answerable: bool,
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
        // Both yeses wait for everything they grant to have been shown, and no waits for nothing:
        // declining grants nothing, so there is nothing it has to have been told first.
        KeyCode::Char('y' | 'Y') if answerable => Response::Answer(Answer::Trust),
        KeyCode::Char('r' | 'R') if keeping && answerable => Response::Answer(Answer::Remember),
        KeyCode::Char('n' | 'N') | KeyCode::Esc => Response::Answer(Answer::Decline),
        // The keys the write and run prompts scroll with, for the same reason (PROMPT-4).
        KeyCode::Up | KeyCode::Char('k') => Response::Scroll(-1),
        KeyCode::Down | KeyCode::Char('j') => Response::Scroll(1),
        KeyCode::PageUp => Response::Page(-1),
        KeyCode::PageDown => Response::Page(1),
        KeyCode::Home => Response::Scroll(i32::MIN),
        KeyCode::End => Response::Scroll(i32::MAX),
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
    /// Move what scrolls by this many rows, which answers nothing.
    Scroll(i32),
    /// Move what scrolls by this many of the last draw's pages, which answers nothing.
    Page(i32),
    /// The interrupt was pressed with nothing offered yet, so the way out is offered and the next
    /// press of it takes it.
    Offer,
    /// Nothing, and the question stays on the screen.
    Nothing,
}

/// What one draw of a question decided for the keys that answer it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Drawn {
    /// How far what scrolls can be moved, which is nothing where it all fits.
    furthest: u16,
    /// The rows a page moves: one fewer than the box shows, so a rule of two rows split by the
    /// bottom edge is whole on the next page. A fixed ten passed over rows of a shorter box, and a
    /// rule nobody can page to is one `y` never grants.
    page: u16,
    /// Whether a yes may be taken.
    answerable: bool,
    /// Whether `r` may be taken, which is never where remembering is not on offer.
    remembering: bool,
}

impl Drawn {
    /// Where what scrolls starts once moved `by` rows from `scroll`.
    ///
    /// Moved from where this draw put it rather than from `scroll`, which a terminal made taller
    /// since can leave past the bottom, where a press of Up would have moved nothing.
    fn moved(&self, scroll: u16, by: i32) -> u16 {
        let from = i32::from(scroll.min(self.furthest));
        let to = from.saturating_add(by).clamp(0, i32::from(self.furthest));
        u16::try_from(to).unwrap_or(self.furthest)
    }
}

/// Draw the question about the working directory, setting `recorded` once a draw has put on the
/// screen together what `r` does and the record it writes.
///
/// `keeping` is where remembering would write the answer, or `None` where it is not offered. What a
/// yes grants is named in the question line, which stays above what scrolls, so a yes is taken from
/// the first draw with the question and the keys whole. What `r` grants beyond it is in the lines
/// that scroll, so `r` is not taken until they have been shown.
fn draw(
    frame: &mut ratatui::Frame,
    directory: &Path,
    keeping: Option<&Path>,
    offered_to_leave: bool,
    scroll: u16,
    recorded: &mut bool,
) -> Drawn {
    let question = vec![
        asking(
            t!(trust_directory_question),
            &directory.display().to_string(),
        ),
        Line::raw(""),
    ];
    let mut lines = vec![
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
    let width = Laid::width(frame.area());
    let mut remembering = 0..0;
    if let Some(path) = keeping {
        lines.push(Line::raw(""));
        let from = u32::from(rows_of(&lines, width));
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
        remembering = from..u32::from(rows_of(&lines, width));
    }

    let answers = |answerable, taken| {
        keys(
            t!(trust_directory_yes),
            t!(trust_directory_no),
            keeping.map(|_| (t!(trust_directory_remember), taken)),
            offered_to_leave,
            answerable,
        )
    };
    let laid = Laid::new(
        frame.area(),
        &question,
        &lines,
        &answers(true, true),
        scroll,
    );
    *recorded |= laid.shows(&remembering);
    // Where nothing is offered there are no rows to have shown, and an empty run of them counts as
    // on the screen.
    let drawn = laid.drawn(true, keeping.is_some() && *recorded);

    let hint = match keeping {
        Some(_) if !*recorded && !laid.could_show(&remembering) => {
            withheld(t!(trust_directory_remember_too_small))
        }
        Some(_) if !*recorded => withheld(t!(trust_directory_remember_unseen)),
        _ => laid.rows_hint(),
    };
    panel(
        frame,
        t!(trust_directory_title),
        &laid,
        question,
        lines,
        hint,
        answers(drawn.answerable, drawn.remembering),
    );
    drawn
}

/// Draw the question about one directory a settings file named.
///
/// What a yes grants is the path in the question line, which stays above what scrolls, so a yes is
/// taken from the first draw with the question and the keys whole.
fn draw_named(
    frame: &mut ratatui::Frame,
    directory: &str,
    offered_to_leave: bool,
    scroll: u16,
) -> Drawn {
    let question = vec![
        asking(t!(named_directory_question), directory),
        Line::raw(""),
    ];
    let lines = vec![
        Line::from(Span::raw(t!(named_directory_explained))),
        Line::raw(""),
        Line::from(Span::styled(
            t!(named_directory_regardless),
            Style::default().fg(theme::muted()),
        )),
    ];
    let answers = |answerable| {
        keys(
            t!(named_directory_yes),
            t!(named_directory_no),
            None,
            offered_to_leave,
            answerable,
        )
    };
    let laid = Laid::new(frame.area(), &question, &lines, &answers(true), scroll);
    let drawn = laid.drawn(true, false);
    let hint = laid.rows_hint();
    panel(
        frame,
        t!(named_directory_title),
        &laid,
        question,
        lines,
        hint,
        answers(drawn.answerable),
    );
    drawn
}

/// Draw the question about the `allow` rules a checkout proposed, marking in `seen` each rule that
/// this draw put on the screen with its file.
///
/// Every rule is listed, in the order the files wrote them, with the file each came from. That is
/// what makes this an acceptable gate at all: the answer is informed consent for specific grants
/// rather than a general feeling about the tree, and a question that named none of them would be the
/// defect wearing a consent story.
///
/// Listed is not enough where the list is longer than the box, since one `y` grants all of it: a
/// rule below the bottom edge is one the person was not shown. So a yes is taken only once every
/// entry of `seen` is set, and the row above the keys says how many are not.
fn draw_granted(
    frame: &mut ratatui::Frame,
    rules: &[Proposed],
    offered_to_leave: bool,
    scroll: u16,
    seen: &mut [bool],
) -> Drawn {
    let question = vec![
        Line::from(Span::styled(
            t!(granted_rules_question),
            Style::default()
                .fg(theme::brand_primary())
                .add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
    ];
    let width = Laid::width(frame.area());
    let mut lines = Vec::with_capacity(rules.len() * 2 + 5);
    // The rows each rule takes, its file's included. Counted a line at a time, which is what the
    // whole comes to: the paragraph wraps each line on its own. Wider than the rows a scroll
    // reaches, so a rule past the last of them is counted where it is rather than at the last.
    let mut taking = Vec::with_capacity(rules.len());
    let mut row = 0u32;
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
        let rows = u32::from(rows_of(&lines[lines.len() - 2..], width));
        taking.push(row..row.saturating_add(rows));
        row = row.saturating_add(rows);
    }
    lines.extend([
        Line::raw(""),
        Line::from(Span::raw(t!(granted_rules_explained))),
        Line::raw(""),
        Line::from(Span::styled(
            t!(granted_rules_regardless),
            Style::default().fg(theme::muted()),
        )),
    ]);

    // Measured with the key live, which is the same text: whether a yes is taken changes how the
    // key is coloured and not how many rows the line takes, so it can be decided after the layout.
    let answers = |answerable| {
        keys(
            t!(granted_rules_yes),
            t!(granted_rules_no),
            None,
            offered_to_leave,
            answerable,
        )
    };
    let laid = Laid::new(frame.area(), &question, &lines, &answers(true), scroll);
    let mut unseen = 0;
    let mut unshowable = false;
    for (seen, rows) in seen.iter_mut().zip(&taking) {
        *seen |= laid.shows(rows);
        if !*seen {
            unseen += 1;
            unshowable |= !laid.could_show(rows);
        }
    }
    let drawn = laid.drawn(unseen == 0, false);

    let hint = if unshowable {
        withheld(t!(granted_rules_too_small))
    } else if unseen > 0 {
        withheld(&t!(granted_rules_unseen, count = unseen))
    } else {
        laid.rows_hint()
    };
    panel(
        frame,
        t!(granted_rules_title),
        &laid,
        question,
        lines,
        hint,
        answers(drawn.answerable),
    );
    drawn
}

/// The row above the keys where a key is not taken yet, saying which and why: a key that did nothing
/// and said nothing would read as a question that had stopped answering.
fn withheld(why: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("   {why}"),
        Style::default().fg(theme::running()),
    ))
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
/// them where `remember` names it, with whether it is taken yet.
///
/// `answerable` is whether a yes is taken yet. A key not taken yet is drawn muted rather than
/// dropped, so the line keeps its shape and the row above it says why.
fn keys(
    yes: &str,
    no: &str,
    remember: Option<(&str, bool)>,
    offered_to_leave: bool,
    answerable: bool,
) -> Line<'static> {
    let mut spans = vec![
        Span::styled(
            "  y",
            Style::default()
                .fg(if answerable {
                    theme::ok()
                } else {
                    theme::muted()
                })
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {yes}    ")),
    ];
    if let Some((remember, taken)) = remember {
        spans.push(Span::styled(
            "r",
            Style::default()
                .fg(if taken {
                    theme::running()
                } else {
                    theme::muted()
                })
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

/// Where one question's parts go on the screen it is drawn on.
///
/// The question is at the top and the keys at the bottom whatever the box's height, and what lies
/// between them scrolls (PROMPT-4). The keys used to be the last line of one paragraph, so a box
/// taller than the screen cut them off, along with everything else past the bottom edge.
struct Laid {
    /// The box, border included.
    area: Rect,
    question: Rect,
    body: Rect,
    /// The row above the keys, saying there is more where what scrolls does not fit.
    hint: Rect,
    keys: Rect,
    /// The first row of what scrolls that is on the screen.
    offset: u16,
    /// How far what scrolls can be moved, which is nothing where it all fits.
    furthest: u16,
    /// Whether the question and the keys are both on the screen whole, in a box with a column to
    /// draw in. One with none draws nothing, and yet every line in it takes no rows, so every run
    /// of rows fits.
    whole: bool,
}

impl Laid {
    fn new(
        screen: Rect,
        question: &[Line<'static>],
        lines: &[Line<'static>],
        answers: &Line<'static>,
        scroll: u16,
    ) -> Self {
        let share = centred(screen);
        let width = share.width.saturating_sub(2);
        let asking = rows_of(question, width);
        let rest = rows_of(lines, width);
        let answering = rows_of(std::slice::from_ref(answers), width);

        // Made taller where what it says does not fit, up to the whole screen: the border's two rows
        // and the one above the keys.
        let needed = asking
            .saturating_add(rest)
            .saturating_add(answering)
            .saturating_add(3);
        let height = needed.clamp(share.height, screen.height);
        let area = Rect {
            y: screen.y + (screen.height - height) / 2,
            height,
            ..share
        };
        let inside = area.inner(Margin::new(1, 1));

        // The question claims its rows first and the keys next, so a box too short for both keeps
        // what is being asked: a `y` pressed at a panel that never said what it was about is the
        // worse of the two to lose.
        let question_rows = asking.min(inside.height);
        let keys_rows = answering.min(inside.height - question_rows);
        let between = inside.height - question_rows - keys_rows;
        // Where what scrolls does not fit, a row saying there is more, only where it leaves a row of
        // what it is about. Where it fits, a blank row to spare it, and none where it fits exactly: a
        // blank that pushed the last row out of view would make a box that fitted scroll.
        let hint_rows = u16::from(if rest > between {
            between > 1
        } else {
            between > rest
        });
        let body_rows = between - hint_rows;
        let furthest = rest.saturating_sub(body_rows);

        let row = |y: u16, height: u16| Rect {
            y,
            height,
            ..inside
        };
        let question = row(inside.y, question_rows);
        let body = row(question.bottom(), body_rows);
        let hint = row(body.bottom(), hint_rows);
        let keys = row(hint.bottom(), keys_rows);
        Laid {
            area,
            question,
            body,
            hint,
            keys,
            offset: scroll.min(furthest),
            furthest,
            // The question claims its rows first, so keys that are whole have it whole above them.
            whole: width > 0 && keys_rows == answering,
        }
    }

    /// What this draw decided, given whether a yes and `r` are taken on what has been shown.
    ///
    /// Neither is where the question or the keys are cut off: a key pressed at a box that no longer
    /// says what it answers, or that has lost the keys that say how to, is not an answer to it.
    fn drawn(&self, answerable: bool, remembering: bool) -> Drawn {
        Drawn {
            furthest: self.furthest,
            page: self.body.height.saturating_sub(1).max(1),
            answerable: self.whole && answerable,
            remembering: self.whole && remembering,
        }
    }

    /// The columns a question's lines wrap to on this screen.
    fn width(screen: Rect) -> u16 {
        centred(screen).width.saturating_sub(2)
    }

    /// Whether every one of these rows of what scrolls is on the screen, with the question whole
    /// above them and the keys whole below.
    fn shows(&self, rows: &std::ops::Range<u32>) -> bool {
        let offset = u32::from(self.offset);
        self.whole && offset <= rows.start && rows.end <= offset + u32::from(self.body.height)
    }

    /// Whether some scroll of this box could show every one of these rows at once. A run taller
    /// than the box is one no key puts on the screen, and a hint pointing at the arrows would send
    /// somebody scrolling for it.
    fn could_show(&self, rows: &std::ops::Range<u32>) -> bool {
        rows.end - rows.start <= u32::from(self.body.height)
    }

    /// How much further what scrolls goes, or that there is nothing below, and nothing where it all
    /// fits.
    fn rows_hint(&self) -> Line<'static> {
        if self.furthest == 0 {
            return Line::raw("");
        }
        Line::from(Span::styled(
            crate::confirm::scroll_hint(self.furthest - self.offset),
            Style::default().fg(theme::brand_primary()),
        ))
    }
}

/// The rows these lines take when wrapped to `width`.
fn rows_of(lines: &[Line<'static>], width: u16) -> u16 {
    let wrapped = Paragraph::new(lines.to_vec()).wrap(Wrap { trim: false });
    u16::try_from(wrapped.line_count(width)).unwrap_or(u16::MAX)
}

/// Draw one question where [`Laid`] put its parts, in a box whose every cell the theme paints.
///
/// One implementation for every question here, because the chrome is what says the question is the
/// system's own: `Clear` empties the cells under the panel without colouring them, so the
/// background and text colour are set for the block rather than for the border alone.
fn panel(
    frame: &mut ratatui::Frame,
    title: &str,
    laid: &Laid,
    question: Vec<Line<'static>>,
    lines: Vec<Line<'static>>,
    hint: Line<'static>,
    answers: Line<'static>,
) {
    frame.render_widget(Clear, laid.area);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme::brand_primary()))
            .title(format!(" {title} "))
            .style(Style::default().bg(theme::background()).fg(theme::text())),
        laid.area,
    );
    let wrapped = |lines: Vec<Line<'static>>| Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(wrapped(question), laid.question);
    frame.render_widget(wrapped(lines).scroll((laid.offset, 0)), laid.body);
    frame.render_widget(Paragraph::new(hint), laid.hint);
    frame.render_widget(wrapped(vec![answers]), laid.keys);
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
                answer_for(key, false, false, false, true),
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
                false,
                true
            ),
            Response::Answer(Answer::Trust)
        );
        assert_eq!(
            answer_for(
                KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE),
                false,
                true,
                false,
                true
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
            assert_eq!(
                answer_for(key, false, false, false, true),
                Response::Nothing
            );
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
        answer_for(
            KeyEvent::new(code, KeyModifiers::NONE),
            false,
            true,
            false,
            true,
        )
    }

    /// The rules question as it is first drawn, with no interrupt pressed yet.
    fn draw_granted_at_rest(frame: &mut ratatui::Frame, rules: &[Proposed]) {
        draw_granted(frame, rules, false, 0, &mut vec![false; rules.len()]);
    }

    /// The question as it is first drawn, with no interrupt pressed yet.
    fn draw_at_rest(frame: &mut ratatui::Frame, directory: &Path) {
        draw(frame, directory, None, false, 0, &mut false);
    }

    /// The same for a directory a settings file named.
    fn draw_named_at_rest(frame: &mut ratatui::Frame, directory: &str) {
        draw_named(frame, directory, false, 0);
    }

    /// A press that appeared to do nothing and said nothing reads as a question that has stopped
    /// answering, so the keys line says which press leaves once the first has been made.
    #[test]
    fn the_question_says_which_press_leaves_once_one_has_been_made() {
        let at_rest = rendered(|frame| {
            draw(frame, Path::new("/tmp/x"), None, false, 0, &mut false);
        });
        assert!(
            at_rest.contains("ctrl-c quit"),
            "the way out was not named at all: {at_rest}"
        );

        // Short enough to sit on the keys line at the narrow width this renders at, since a hint
        // that wrapped across the border would say it worse than not saying it.
        let offered = rendered(|frame| {
            draw(frame, Path::new("/tmp/x"), None, true, 0, &mut false);
        });
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

    /// `count` rules from one file, the last of them the one the report saw answer the run prompt.
    fn a_long_list(count: usize) -> Vec<Proposed> {
        let path = Path::new("/home/ubuntu/cpoc/.bravebot/settings.json");
        let mut rules: Vec<Proposed> = (1..count)
            .map(|i| Proposed::new(path, &format!("Bash(cargo test --lib part{i})")))
            .collect();
        rules.push(Proposed::new(path, "Bash(bash scripts/check.sh)"));
        rules
    }

    /// The rules question drawn once on a terminal of this size, scrolled down by `scroll` rows,
    /// with what it showed marked in `seen`.
    fn granted_on(
        (width, height): (u16, u16),
        rules: &[Proposed],
        scroll: u16,
        seen: &mut [bool],
    ) -> (ratatui::buffer::Buffer, Drawn) {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        let mut drawn = None;
        terminal
            .draw(|frame| drawn = Some(draw_granted(frame, rules, false, scroll, seen)))
            .expect("draw");
        (
            terminal.backend().buffer().clone(),
            drawn.expect("the question was drawn"),
        )
    }

    /// PERM-15, PROMPT-4: a list longer than the box keeps the question and the keys on the screen,
    /// and says how many rules it has not shown. The keys were the last line of the list's own
    /// paragraph, so the thirteen rules of the report lost the keys and their last three rules at
    /// 80x24, and the last of those went on to answer the run prompt (#954).
    #[test]
    fn a_list_longer_than_the_box_keeps_the_question_and_the_keys_and_says_how_many_rules_are_below()
     {
        let rules = a_long_list(13);
        let (buffer, _) = granted_on((80, 24), &rules, 0, &mut [false; 13]);
        let shown = inside(&buffer);

        assert!(
            shown.starts_with("This project's settings ask to stop you"),
            "the question was not at the top: {shown}"
        );
        assert!(shown.contains("ctrl-c"), "the keys were cut off: {shown}");
        // Nine of the thirteen fit whole above the row saying so, and it counts rules rather than
        // rows, since a rule is what a yes grants.
        assert!(
            shown.contains("y grants nothing: 4 rules not shown yet"),
            "nothing said how much of the list is below the edge: {shown}"
        );
    }

    /// PROMPT-4: and the rest of the list can be scrolled to, the last rule and what granting does
    /// included, with the keys still under it.
    #[test]
    fn the_rest_of_a_long_list_can_be_scrolled_to() {
        let rules = a_long_list(13);
        let mut seen = [false; 13];
        let (_, first) = granted_on((80, 24), &rules, 0, &mut seen);
        let (buffer, _) = granted_on((80, 24), &rules, first.furthest, &mut seen);
        let shown = inside(&buffer);

        assert!(
            shown.contains("Bash(bash scripts/check.sh)"),
            "the last rule could not be reached: {shown}"
        );
        assert!(
            shown.contains("not by you"),
            "what granting does could not be reached: {shown}"
        );
        assert!(shown.contains("ctrl-c"), "the keys scrolled away: {shown}");
    }

    /// PERM-15: `y` grants nothing while a rule has not been on the screen. One key grants the whole
    /// list, so a yes at the first draw of a long one granted rules nobody was shown (#954). `n`
    /// answers from the first draw, since declining grants nothing and needs nothing read first.
    #[test]
    fn y_grants_nothing_while_a_rule_has_not_been_on_the_screen() {
        let y = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
        let n = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        let rules = a_long_list(13);
        let mut seen = [false; 13];

        let (_, mut drawn) = granted_on((80, 24), &rules, 0, &mut seen);
        assert!(
            !drawn.answerable,
            "a yes was on offer with rules below the edge"
        );
        assert_eq!(
            answer_for(y, false, true, false, drawn.answerable),
            Response::Nothing
        );
        assert_eq!(
            answer_for(n, false, true, false, drawn.answerable),
            Response::Answer(Answer::Decline)
        );

        // A page at a time to the end, which puts every rule on the screen whole at some point.
        let mut scroll = 0u16;
        let mut buffer;
        loop {
            scroll = drawn.moved(scroll, i32::from(drawn.page));
            (buffer, drawn) = granted_on((80, 24), &rules, scroll, &mut seen);
            if scroll == drawn.furthest {
                break;
            }
        }
        assert!(drawn.answerable, "reading every rule left the yes withheld");
        assert_eq!(
            answer_for(y, false, true, false, drawn.answerable),
            Response::Answer(Answer::Trust)
        );
        assert!(
            !inside(&buffer).contains("not shown yet"),
            "the box still said a rule was unread: {}",
            inside(&buffer)
        );
    }

    /// PERM-15: jumping over part of the list is not reading it. End puts the last rule on the
    /// screen without the ones between, so an answer taken on having reached the bottom would grant
    /// the middle of the list unseen.
    #[test]
    fn jumping_to_the_end_of_a_list_leaves_the_rules_it_skipped_unread() {
        let rules = a_long_list(30);
        let mut seen = [false; 30];
        let (_, first) = granted_on((80, 24), &rules, 0, &mut seen);
        let (buffer, drawn) = granted_on((80, 24), &rules, first.furthest, &mut seen);

        assert!(
            !drawn.answerable,
            "reaching the bottom was taken for reading the list"
        );
        assert!(
            inside(&buffer).contains("rules not shown yet"),
            "{}",
            inside(&buffer)
        );
    }

    /// PERM-15: a rule longer than the box can show at once is never granted. The box shows a part
    /// of it at any one time, so no draw showed the rule a yes would grant, and a rule counted as
    /// shown on any one of its rows would be granted on a part nobody saw the rest of.
    #[test]
    fn a_rule_that_wraps_past_the_box_is_never_granted() {
        let long = format!("Bash(true {})", "--flag ".repeat(400));
        let rules = proposed(&[("/work/.bravebot/settings.json", &long)]);
        let mut seen = [false];
        let (_, first) = granted_on((80, 24), &rules, 0, &mut seen);
        assert!(first.furthest > 0, "the rule fit, so this tests nothing");

        for scroll in 0..=first.furthest {
            let (_, drawn) = granted_on((80, 24), &rules, scroll, &mut seen);
            assert!(
                !drawn.answerable,
                "a yes was on offer at row {scroll} of a rule never shown whole"
            );
        }
    }

    /// PERM-15: where the terminal cannot show the question, one rule with its file, and the keys at
    /// once, no answer grants anything. Each size loses a different one of the three: the keys, the
    /// file under the rule, or every column.
    #[test]
    fn a_terminal_too_small_for_a_rule_and_the_keys_grants_nothing() {
        let rules = proposed(&[(
            "/work/.bravebot/settings.json",
            "Bash(bash scripts/check.sh)",
        )]);
        for size in [(24, 8), (80, 4), (80, 7), (2, 40), (1, 1)] {
            let mut seen = [false];
            let (_, first) = granted_on(size, &rules, 0, &mut seen);
            for scroll in 0..=first.furthest {
                let (_, drawn) = granted_on(size, &rules, scroll, &mut seen);
                assert!(
                    !drawn.answerable,
                    "a yes was on offer at {size:?}, scrolled {scroll}"
                );
            }
        }
    }

    /// The keys the write and run prompts scroll with scroll the startup questions, and none of them
    /// answers: without them the rest of a long list could not be reached, and so not granted.
    #[test]
    fn the_arrows_scroll_the_question_and_answer_nothing() {
        for (code, moving) in [
            (KeyCode::Up, Response::Scroll(-1)),
            (KeyCode::Down, Response::Scroll(1)),
            (KeyCode::PageUp, Response::Page(-1)),
            (KeyCode::PageDown, Response::Page(1)),
            (KeyCode::Home, Response::Scroll(i32::MIN)),
            (KeyCode::End, Response::Scroll(i32::MAX)),
        ] {
            assert_eq!(pressing(code), moving, "{code:?}");
        }
    }

    /// `j` and `k` scroll the startup questions a line as Down and Up do, wherever the question
    /// offers to remember, and are not answers: a person scrolling a long list with them must not
    /// grant it, and one pressed with Ctrl is not a scroll.
    #[test]
    fn j_and_k_scroll_the_question_a_line_and_answer_nothing() {
        for keeping in [false, true] {
            let press = |code, modifiers| {
                answer_for(KeyEvent::new(code, modifiers), false, true, keeping, true)
            };
            assert_eq!(
                press(KeyCode::Char('j'), KeyModifiers::NONE),
                Response::Scroll(1)
            );
            assert_eq!(
                press(KeyCode::Char('k'), KeyModifiers::NONE),
                Response::Scroll(-1)
            );
            for letter in ['j', 'k'] {
                assert_eq!(
                    press(KeyCode::Char(letter), KeyModifiers::CONTROL),
                    Response::Nothing,
                    "ctrl-{letter}"
                );
            }
        }
    }

    /// A scroll moves from where the last draw put it. A terminal made taller since can leave the
    /// scroll past the bottom, and moving from there took a press of Up to do nothing.
    #[test]
    fn a_scroll_moves_from_where_the_box_was_drawn() {
        let drawn = Drawn {
            furthest: 5,
            page: 3,
            answerable: true,
            remembering: false,
        };
        assert_eq!(drawn.moved(20, -1), 4);
        assert_eq!(drawn.moved(4, 3), 5);
        assert_eq!(drawn.moved(60_000, i32::MIN), 0);
        assert_eq!(drawn.moved(0, i32::MAX), 5);
    }

    /// PROMPT-4, PERM-15: a page moves one row fewer than the box shows, so paging from the top of a
    /// list puts every rule on the screen whole on a terminal whose box shows fewer than ten rows. A
    /// page of ten passed over the rows between, and a page as tall as a box of seven rows leaves
    /// each rule of two rows the bottom edge splits cut off on both pages.
    #[test]
    fn paging_through_a_list_on_a_short_terminal_reads_every_rule() {
        let rules = a_long_list(13);
        let mut seen = [false; 13];
        let (buffer, mut drawn) = granted_on((80, 13), &rules, 0, &mut seen);
        assert!(
            inside(&buffer).matches("Bash(").count() < 5,
            "the box showed ten rows, so this tests nothing: {}",
            inside(&buffer)
        );
        let mut scroll = 0;
        while scroll < drawn.furthest {
            scroll = drawn.moved(scroll, i32::from(drawn.page));
            (_, drawn) = granted_on((80, 13), &rules, scroll, &mut seen);
        }
        assert!(
            drawn.answerable,
            "paging to the end left a rule unread: {seen:?}"
        );
    }

    /// PERM-15: a rule past the last row a scroll reaches is never granted. A scroll stops at the
    /// 65535th row, and a rule whose rows were counted as stopping there too was counted as shown
    /// by a draw that never reached it.
    #[test]
    fn a_rule_past_the_last_row_a_scroll_reaches_is_never_granted() {
        let rules = a_long_list(33_000);
        let mut seen = vec![true; rules.len()];
        seen[rules.len() - 1] = false;
        let (_, drawn) = granted_on((80, 24), &rules, u16::MAX, &mut seen);
        assert!(!seen[rules.len() - 1], "the last rule was counted shown");
        assert!(!drawn.answerable);
    }

    /// PROMPT-4: no question takes a yes, or `r`, where the terminal cuts off its question or its
    /// keys, one that took them at a larger size included. A key pressed at a box that no longer
    /// says what it answers is not an answer to it.
    #[test]
    fn a_question_a_small_terminal_cuts_off_takes_no_answer() {
        let record = Path::new("/home/me/.bravebot/trusted/work-1a2b.jsonl");
        let rules = a_long_list(1);
        // Drawn with the record and the rule already shown, so what is taken is decided by the size.
        let answers = |(width, height): (u16, u16)| {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
            let mut drawn = Vec::new();
            for question in 0..3 {
                terminal
                    .draw(|frame| {
                        drawn.push(match question {
                            0 => draw(frame, here(), Some(record), false, 0, &mut true),
                            1 => draw_named(frame, "/elsewhere", false, 0),
                            _ => draw_granted(frame, &rules, false, 0, &mut [true]),
                        });
                    })
                    .expect("draw");
            }
            drawn
        };

        let whole = answers((80, 24));
        assert!(whole.iter().all(|drawn| drawn.answerable), "{whole:?}");
        assert!(whole[0].remembering, "{whole:?}");
        for size in [(80, 3), (80, 4), (24, 6), (2, 40), (1, 1)] {
            for drawn in answers(size) {
                assert!(
                    !drawn.answerable && !drawn.remembering,
                    "an answer was taken at {size:?}: {drawn:?}"
                );
            }
        }
    }

    /// PROMPT-4: a question that fits its box says nothing of scrolling and does not scroll, one
    /// that fits it exactly included: the blank above the keys is dropped there rather than pushing
    /// the record `r` writes below the edge.
    #[test]
    fn a_question_that_fits_its_box_says_nothing_of_scrolling() {
        let record = Path::new("/home/me/.bravebot/trusted/work-1a2b.jsonl");
        let rules = a_long_list(3);
        let mut furthest = Vec::new();
        let shown = [
            rendered(|frame| {
                furthest.push(draw(frame, here(), Some(record), false, 0, &mut false).furthest);
            }),
            rendered(|frame| furthest.push(draw_named(frame, "/elsewhere", false, 0).furthest)),
            rendered(|frame| {
                furthest.push(draw_granted(frame, &rules, false, 0, &mut [false; 3]).furthest);
            }),
        ];
        assert_eq!(furthest, [0, 0, 0]);
        for shown in shown {
            assert!(!shown.contains("↑↓"), "{shown}");
        }
    }

    /// PERM-15, TRUST-23: where what a key waits for is taller than the box, the row above the keys
    /// says so rather than pointing at the arrows, since no scroll shows it.
    #[test]
    fn the_hint_says_when_no_scroll_can_show_what_a_key_waits_for() {
        let long = format!("Bash(true {})", "--flag ".repeat(400));
        let rules = proposed(&[("/work/.bravebot/settings.json", &long)]);
        let (buffer, _) = granted_on((80, 24), &rules, 0, &mut [false]);
        assert!(
            inside(&buffer).contains("y grants nothing: a rule is taller than this box"),
            "{}",
            inside(&buffer)
        );

        let record = Path::new("/home/me/.bravebot/trusted/work-1a2b.jsonl");
        let mut terminal = Terminal::new(TestBackend::new(80, 10)).expect("terminal");
        let mut drawn = None;
        terminal
            .draw(|frame| drawn = Some(draw(frame, here(), Some(record), false, 0, &mut false)))
            .expect("draw");
        let shown = inside(terminal.backend().buffer());
        assert!(
            shown.contains("r remembers nothing: what it writes is taller than this box"),
            "{shown}"
        );
        assert!(!drawn.expect("the question was drawn").remembering);
    }

    /// PROMPT-4: the question about the working directory goes through the same box, so on a short
    /// terminal it keeps its question and keys too, and the record `r` would write can be scrolled to.
    #[test]
    fn a_directory_question_longer_than_the_box_keeps_its_keys_and_scrolls_to_the_rest() {
        let record = Path::new("/home/me/.bravebot/trusted/work-1a2b.jsonl");
        let size = (80, 13);
        let at = |scroll| {
            let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).expect("terminal");
            let mut drawn = None;
            terminal
                .draw(|frame| {
                    drawn = Some(draw(frame, here(), Some(record), false, scroll, &mut false))
                })
                .expect("draw");
            (
                inside(terminal.backend().buffer()),
                drawn.expect("the question was drawn"),
            )
        };

        let (shown, drawn) = at(0);
        assert!(shown.starts_with("Trust /work?"), "{shown}");
        assert!(shown.contains("ctrl-c"), "the keys were cut off: {shown}");
        assert!(shown.contains("↑↓"), "nothing said there is more: {shown}");
        assert!(drawn.answerable);

        let (shown, _) = at(drawn.furthest);
        assert!(
            shown.contains("work-1a2b.jsonl"),
            "the record could not be scrolled to: {shown}"
        );
        assert!(shown.contains("ctrl-c"), "the keys scrolled away: {shown}");
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
                true,
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
        assert_eq!(answer_for(r, false, false, true, true), Response::Nothing);
        let chord = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL);
        assert_eq!(
            answer_for(chord, false, true, true, true),
            Response::Nothing
        );
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
        let output = rendered(|frame| {
            draw(frame, here(), Some(record), false, 0, &mut false);
        });

        assert!(
            output.contains("trust and remember"),
            "the key was not named: {output}"
        );
        assert!(
            output.contains("it is a git root"),
            "nothing said how far the answer reaches: {output}"
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

    /// TRUST-23: `r` remembers nothing until what it writes has been on the screen, and `y` answers
    /// from the first draw. The record is part of what `r` grants, so an `r` taken with it below the
    /// bottom edge endorses a record nobody was shown; what `y` grants is the directory in the
    /// question line, which is always on the screen.
    #[test]
    fn r_remembers_nothing_until_the_record_it_writes_has_been_on_the_screen() {
        let r = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE);
        let y = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
        let record = Path::new("/home/me/.bravebot/trusted/work-1a2b.jsonl");
        let mut recorded = false;
        let at = |scroll, recorded: &mut bool| {
            let mut terminal = Terminal::new(TestBackend::new(72, 16)).expect("terminal");
            let mut drawn = None;
            terminal
                .draw(|frame| {
                    drawn = Some(draw(frame, here(), Some(record), false, scroll, recorded));
                })
                .expect("draw");
            (
                inside(terminal.backend().buffer()),
                drawn.expect("the question was drawn"),
            )
        };

        let (shown, first) = at(0, &mut recorded);
        assert!(
            !shown.contains("work-1a2b.jsonl"),
            "the record fit: {shown}"
        );
        assert!(!first.remembering, "r was on offer with its record unseen");
        assert_eq!(
            answer_for(r, false, true, first.remembering, first.answerable),
            Response::Nothing
        );
        assert_eq!(
            answer_for(y, false, true, first.remembering, first.answerable),
            Response::Answer(Answer::Trust)
        );
        assert!(
            shown.contains("r remembers nothing: what it writes is not shown yet"),
            "nothing said why r does nothing: {shown}"
        );

        let (_, scrolled) = at(first.furthest, &mut recorded);
        assert!(
            scrolled.remembering,
            "r was withheld once the record was read"
        );
        // And it stays taken once scrolled back, since the record was shown.
        let (shown, back) = at(0, &mut recorded);
        assert_eq!(
            answer_for(r, false, true, back.remembering, back.answerable),
            Response::Answer(Answer::Remember)
        );
        assert!(!shown.contains("not shown yet"), "{shown}");
    }

    /// TRUST-23: and a question that does not offer `r` never takes it, though it has no record to
    /// have left below the edge: an empty run of rows is on the screen at every scroll, so a draw
    /// that took `r` on its record having been shown would take it at every question.
    #[test]
    fn a_question_not_offering_to_remember_never_takes_r() {
        let drawn = |draw_it: &mut dyn FnMut(&mut ratatui::Frame) -> Drawn| {
            let mut terminal = Terminal::new(TestBackend::new(72, 20)).expect("terminal");
            let mut drawn = None;
            terminal
                .draw(|frame| drawn = Some(draw_it(frame)))
                .expect("draw");
            drawn.expect("the question was drawn")
        };
        let rules = a_long_list(1);
        for scroll in [0, u16::MAX] {
            assert!(
                !drawn(&mut |frame| draw(frame, here(), None, false, scroll, &mut false))
                    .remembering
            );
            assert!(
                !drawn(&mut |frame| draw_named(frame, "/elsewhere", false, scroll)).remembering
            );
            assert!(
                !drawn(&mut |frame| draw_granted(frame, &rules, false, scroll, &mut [false]))
                    .remembering
            );
        }
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
            .draw(|frame| {
                draw(frame, directory, Some(record), false, 0, &mut false);
            })
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
        paints_the_themes_chrome(|frame| {
            draw(frame, directory, Some(record), false, 0, &mut false);
        });
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
            .draw(|frame| {
                draw(
                    frame,
                    Path::new("/tmp/x"),
                    Some(record),
                    false,
                    0,
                    &mut false,
                );
            })
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
        assert_eq!(answer_for(key, false, true, false, true), Response::Offer);
        assert_eq!(
            answer_for(key, true, true, false, true),
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
            answer_for(key, false, true, false, true),
            Response::Answer(Answer::Leave),
            "one byte ended the session before it began"
        );
    }

    /// And two of them in one write are two key events rather than two presses, so neither half of
    /// the gesture comes from a key that arrived with others.
    #[test]
    fn two_interrupts_that_arrived_together_close_nothing() {
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(
            answer_for(key, false, false, false, true),
            Response::Nothing
        );
        assert_eq!(answer_for(key, true, false, false, true), Response::Nothing);
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
                false,
                true
            ),
            Response::Nothing
        );
        assert_eq!(
            answer_for(
                KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL),
                false,
                true,
                false,
                true
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
