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
//! The answers are rows rather than keys, and Enter takes the row under the cursor (PROMPT-11). No
//! single key press answers either question: the cursor opens on the row that declines, and a bare
//! letter, Ctrl-C and Escape all move nothing. The way out is the row that says so. A terminal says
//! nothing about who wrote a byte, so a question granting reach and trust over a whole tree cannot
//! rest on one keystroke having come from a person, nor on two that a program writes as easily.
//!
//! The one session that is not asked is the one bypassing every permission, which answers this
//! question along with the rest. [`answered_by`] is where that is decided.
//!
//! A settings file may name directories to open beside the working directory, and a name in one is
//! a request rather than a grant: each is put as its own question and only an accepted one is
//! opened. A file arriving with a checkout is the easiest thing on the machine to write to, so a
//! name in one deciding what is reachable and trusted would be reach granted by whatever last
//! edited it.

use bravebot_agent::PermissionMode;
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
    Decline,
    /// Leave without starting a session at all.
    Leave,
}

/// The answers as rows, in the order they are drawn.
///
/// The rows are the answers themselves rather than a parallel list, so a row can only ever mean the
/// answer it returns: a second enum would be two places to keep the order and the meanings in step.
const ROWS: [Answer; 3] = [Answer::Trust, Answer::Decline, Answer::Leave];

/// Which row the cursor is on.
#[derive(Debug)]
struct Choosing {
    selected: usize,
}

impl Choosing {
    /// Open on the row that declines.
    ///
    /// This is the safety property rather than a preference: Enter is the key most likely to be
    /// pressed without reading, and a program writing at the terminal spells one sooner or later, so
    /// the row it lands on has to be the one that grants nothing. Found by searching [`ROWS`] rather
    /// than written as an index, so reordering the rows cannot quietly move the opening cursor onto
    /// the one that trusts.
    fn new() -> Self {
        let selected = ROWS
            .iter()
            .position(|row| *row == Answer::Decline)
            .expect("declining is one of the rows");
        Self { selected }
    }

    /// The row Enter would take.
    fn chosen(&self) -> Answer {
        ROWS[self.selected]
    }

    fn down(&mut self) {
        self.selected = (self.selected + 1).min(ROWS.len() - 1);
    }

    fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }
}

/// What a key press did to the question.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    /// Still choosing.
    Continue,
    /// Take the row under the cursor.
    Confirm,
}

/// Interpret one key press.
///
/// Separated from the loop so the decision can be tested without a terminal, the way [`trust_for`]
/// is. There is no arm that answers on a letter: `y` and `n` are gone rather than kept as
/// shortcuts, because a shortcut is exactly the single keystroke this question must not accept.
/// The arrows are the only movement for the same reason, since `j` and `k` are letters and a burst
/// of prose is full of them.
fn handle_key(choosing: &mut Choosing, key: KeyEvent) -> Outcome {
    // Raw mode delivers Ctrl-C as a key rather than as a signal. It moves nothing, for the reason
    // no letter does: a key that put the cursor on the row that leaves would be half of the quit
    // gesture, and the other half is an Enter, so a program able to write two bytes would have
    // spelled the whole of it. The way out is the row that says so, reached with the arrows.
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return Outcome::Continue;
    }

    match key.code {
        KeyCode::Enter => Outcome::Confirm,
        KeyCode::Up => {
            choosing.up();
            Outcome::Continue
        }
        KeyCode::Down => {
            choosing.down();
            Outcome::Continue
        }
        _ => Outcome::Continue,
    }
}

/// Ask about `directory`, returning the trust map the session should start with.
///
/// Trusting records the workspace root, which covers everything beneath it. Declining records
/// nothing, leaving an empty map in which no path is trusted.
///
/// Asked afresh every time a session begins. The answer is standing permission for as long as
/// that session lasts and no longer: it is not written down anywhere a later launch will read it,
/// so nothing this grants can be inherited by a session whose user was never asked. A resumed
/// session is the one exception, and it inherits the answer its own user gave rather than
/// skipping the question, which is why this is not called at all in that case.
///
/// `None` is the third answer: the user confirmed the row that leaves, which is neither trusting nor
/// declining but a request to leave, so no session begins at all.
pub fn ask<B: Backend>(terminal: &mut Terminal<B>, directory: &Path) -> Option<TrustStore> {
    let answer = ask_one(terminal, |frame, choosing| draw(frame, directory, choosing));

    trust_for(answer, directory)
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
) -> Option<Vec<String>> {
    accepted(directories, |directory| {
        ask_one(terminal, |frame, choosing| {
            draw_named(frame, directory, choosing)
        })
    })
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
            Answer::Trust => opening.push(directory.clone()),
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
pub fn trust_for(answer: Answer, directory: &Path) -> Option<TrustStore> {
    match answer {
        Answer::Leave => None,
        Answer::Trust => Some(trusting_the_workspace(directory)),
        Answer::Decline => Some(TrustStore::new(directory)),
    }
}

/// The rule trusting the workspace records: the root, which covers everything beneath it.
///
/// One place, so the map reached without the question is the map a yes would have written.
fn trusting_the_workspace(directory: &Path) -> TrustStore {
    let mut trust = TrustStore::new(directory);
    trust.trust(".");
    trust
}

/// Put one question and block until a row is confirmed.
///
/// Drawn inside the loop rather than once before it, because the cursor is part of what the question
/// says: a panel drawn once would show the person a row they had moved off. One function for both
/// questions, so the keys that answer them cannot drift apart.
fn ask_one<B: Backend>(
    terminal: &mut Terminal<B>,
    mut draw_it: impl FnMut(&mut ratatui::Frame, &Choosing),
) -> Answer {
    let mut choosing = Choosing::new();

    loop {
        // A terminal that cannot be drawn to cannot carry the question.
        if terminal.draw(|frame| draw_it(frame, &choosing)).is_err() {
            return Answer::Decline;
        }

        match input::read() {
            // A paste answers nothing, and neither do words another program typed: both arrive as
            // one event however many characters they carry, and nothing in either is a key somebody
            // pressed. Presses only for the same reason a release is dropped below.
            Ok(taken) => match taken.key() {
                // Presses only: the interface asks for disambiguated keys, so a release arrives too,
                // and a release taken for a press moves the cursor twice for one keystroke.
                Some(key) if key.kind != event::KeyEventKind::Press => continue,
                Some(key) => match handle_key(&mut choosing, key) {
                    Outcome::Confirm => return choosing.chosen(),
                    Outcome::Continue => continue,
                },
                None => continue,
            },
            Err(_) => return Answer::Decline,
        }
    }
}

/// Draw the question about the working directory.
fn draw(frame: &mut ratatui::Frame, directory: &Path, choosing: &Choosing) {
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
        Line::raw(""),
    ];
    lines.extend(rows(
        choosing,
        t!(trust_directory_yes),
        t!(trust_directory_no),
    ));

    panel(frame, t!(trust_directory_title), lines);
}

/// Draw the question about one directory a settings file named.
fn draw_named(frame: &mut ratatui::Frame, directory: &str, choosing: &Choosing) {
    let mut lines = vec![
        asking(t!(named_directory_question), directory),
        Line::raw(""),
        Line::from(Span::raw(t!(named_directory_explained))),
        Line::raw(""),
        Line::from(Span::styled(
            t!(named_directory_regardless),
            Style::default().fg(theme::muted()),
        )),
        Line::raw(""),
    ];
    lines.extend(rows(
        choosing,
        t!(named_directory_yes),
        t!(named_directory_no),
    ));

    panel(frame, t!(named_directory_title), lines);
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

/// The answers on offer, one per row, with the cursor on the one Enter would take.
///
/// The same rows and the same keys at either question, so what a person learns answering the first
/// is what answers the rest. The row that leaves is drawn with the other two rather than left to a
/// key mentioned in a footnote: a way out nobody can see is one they cannot take.
fn rows(choosing: &Choosing, yes: &str, no: &str) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = ROWS
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let label = match row {
                Answer::Trust => yes.to_string(),
                Answer::Decline => no.to_string(),
                Answer::Leave => t!(quit).to_string(),
            };

            let chosen = index == choosing.selected;
            let marker = if chosen { "❯ " } else { "  " };
            let style = if chosen {
                Style::default()
                    .fg(theme::brand_primary())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::text())
            };

            Line::from(vec![
                Span::styled(
                    format!("  {marker}"),
                    Style::default().fg(theme::brand_primary()),
                ),
                Span::styled(label, style),
            ])
        })
        .collect();

    lines.push(Line::from(Span::styled(
        format!("  {}", t!(trust_question_keys)),
        Style::default().fg(theme::muted()),
    )));
    lines
}

/// Draw one question, in a box whose every cell the theme paints.
///
/// One implementation for both questions here, because the chrome is what says the question is the
/// system's own: `Clear` empties the cells under the panel without colouring them, so the
/// background and text colour are set for the block rather than for the border alone.
fn panel(frame: &mut ratatui::Frame, title: &str, lines: Vec<Line<'static>>) {
    let area = centred(frame.area());
    frame.render_widget(Clear, area);

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(theme::brand_primary()))
                    .title(format!(" {title} "))
                    .style(Style::default().bg(theme::background()).fg(theme::text())),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

/// A centred box, sized to the terminal but never larger than it.
///
/// Eighty percent of the height rather than seventy, because the answers are three rows and a line
/// saying which keys take them: at seventy a twenty-row terminal clipped the key line, which is the
/// one line on the screen that says how to answer at all.
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
    use ratatui::style::Color;

    /// The working directory these answers are about.
    fn here() -> &'static Path {
        Path::new("/work")
    }

    /// One key press, with nothing held down.
    fn pressed(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// A command line another program wrote into the terminal, spelled so that the first letter
    /// carrying a meaning under the old bare-key answers is the one that granted the most: `you`
    /// puts a `y` in front of every `n`, so a question answered by letters trusted the whole tree.
    ///
    /// Both shapes the line can arrive in, because they fail differently. Folded into a paste it
    /// reaches the question as no keys at all; arriving a key at a time, which is what no classifier
    /// can tell from typing, every letter has to move nothing so the Enter at the end lands on the
    /// row that grants nothing.
    const A_TYPED_IN_COMMAND_LINE: &str = "source /home/you/app/.venv/bin/activate\r";

    /// The answer a run of key presses reaches, or `None` where none of them confirmed a row.
    fn answered_by_keys(keys: impl IntoIterator<Item = KeyEvent>) -> Option<Answer> {
        let mut choosing = Choosing::new();
        for key in keys {
            if handle_key(&mut choosing, key) == Outcome::Confirm {
                return Some(choosing.chosen());
            }
        }
        None
    }

    #[test]
    fn a_command_line_another_program_typed_in_answers_nothing() {
        let as_keys: Vec<KeyEvent> =
            crate::input::resolve(crate::input::run_spelling(A_TYPED_IN_COMMAND_LINE))
                .into_iter()
                .filter_map(|taken| taken.key())
                .collect();
        assert!(
            as_keys.is_empty(),
            "a burst reached the question as key presses"
        );

        let reached = answered_by_keys(crate::input::run_spelling(A_TYPED_IN_COMMAND_LINE));
        assert_ne!(
            reached,
            Some(Answer::Trust),
            "a line another program typed in trusted the workspace"
        );
        assert_ne!(
            reached,
            Some(Answer::Leave),
            "a line another program typed in ended the session"
        );

        // What it would have written, rather than which answer it reached: a `trust_for` that
        // trusted the tree on a decline would satisfy an assertion about the answer alone.
        let trust = trust_for(reached.unwrap_or(Answer::Decline), here())
            .expect("the session still starts");
        assert!(
            trust.is_empty(),
            "a line another program typed in granted trust"
        );
    }

    /// The second surface, driven through the function that decides what gets opened rather than
    /// through a key, because opening the directory is the grant. A settings file naming a path and
    /// a program typing at the terminal are two things neither of which is a person.
    #[test]
    fn a_command_line_another_program_typed_in_opens_no_named_directory() {
        let named = names(&["/home/me/.ssh"]);

        let opened = accepted(&named, |_| {
            answered_by_keys(crate::input::run_spelling(A_TYPED_IN_COMMAND_LINE))
                .unwrap_or(Answer::Decline)
        })
        .expect("declining still starts a session");

        assert!(
            opened.is_empty(),
            "a line another program typed in opened a directory: {opened:?}"
        );
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

        let inside = centred(*buffer.area());
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
        let output = rendered(|frame| draw(frame, Path::new("/home/me/project"), &Choosing::new()));
        assert!(output.contains("/home/me/project"));
        assert!(output.contains("trust it"));
        assert!(output.contains("every write"));
        // The way out is a row like the others, not a key named in a footnote.
        assert!(
            output.contains("quit"),
            "no way out on the screen: {output}"
        );
    }

    /// The line saying which keys take a row is the only thing on the screen that says how to
    /// answer, so it has to survive the sizes a terminal actually comes in. The rows and that line
    /// are four more than the single key line they replaced, and at the panel's old height a
    /// twenty-row terminal drew the border over it.
    #[test]
    fn the_keys_that_answer_stay_on_screen_at_ordinary_sizes() {
        for (width, height) in [(72, 20), (80, 24), (120, 40)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
            terminal
                .draw(|frame| draw(frame, here(), &Choosing::new()))
                .expect("draw");
            let output = drawn_on(&terminal);

            assert!(
                output.contains("Enter confirm"),
                "the keys were clipped at {width}x{height}: {output}"
            );
            assert!(
                output.contains("quit"),
                "the way out was clipped at {width}x{height}: {output}"
            );
        }
    }

    /// Which row Enter would take has to be on the screen, at both questions. The cursor is the
    /// whole of what says so, and a panel that listed three answers without marking one would be a
    /// question whose answer a person cannot predict before pressing the key.
    #[test]
    fn the_cursor_opens_on_declining_where_a_person_can_see_it() {
        let marked = |output: &str, label: &str| {
            output
                .split('❯')
                .nth(1)
                .is_some_and(|after| after.starts_with(&format!(" {label}")))
        };

        let workspace = rendered(|frame| draw(frame, here(), &Choosing::new()));
        assert!(
            marked(&workspace, "ask me about every write"),
            "the cursor did not open on declining: {workspace}"
        );

        let named = rendered(|frame| draw_named(frame, "/home/me/.ssh", &Choosing::new()));
        assert!(
            marked(&named, "leave it closed"),
            "the cursor did not open on declining: {named}"
        );
    }

    /// The question must say what saying yes actually does, since it grants standing
    /// permission rather than approving one action.
    #[test]
    fn the_prompt_explains_the_consequence() {
        let output = rendered(|frame| draw(frame, Path::new("/tmp/x"), &Choosing::new()));
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
        paints_the_themes_chrome(|frame| {
            draw(frame, Path::new("/home/me/project"), &Choosing::new())
        });
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

    /// The path is the whole of what the answer is about, and it is the one thing a settings file
    /// chose rather than the person reading the box.
    #[test]
    fn the_named_prompt_shows_the_directory_it_would_open() {
        let output = rendered(|frame| draw_named(frame, "/home/me/.ssh", &Choosing::new()));
        assert!(output.contains("/home/me/.ssh"), "no path: {output}");
        assert!(output.contains("open it"));
        assert!(output.contains("leave it closed"));
    }

    /// Opening a directory grants two things at once, reach and trust, and neither is on the
    /// screen unless the question says so. The question also has to say where it came from: a box
    /// naming a directory the person has never typed is otherwise unexplained.
    #[test]
    fn the_named_prompt_explains_what_opening_does() {
        let output = rendered(|frame| draw_named(frame, "/srv/shared", &Choosing::new()));
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
        paints_the_themes_chrome(|frame| draw_named(frame, "/home/me/.ssh", &Choosing::new()));
    }

    /// Both questions, since either can be the first thing a session draws on a small terminal.
    ///
    /// Surviving the draw is half of it. These two are asked before a session exists, and nothing
    /// else is on the screen to say what the keys mean, so a small terminal that drew the border
    /// and lost the question would leave somebody pressing `y` at a panel that never said what it
    /// was about.
    #[test]
    fn a_tiny_terminal_still_renders() {
        let mut terminal = Terminal::new(TestBackend::new(24, 8)).expect("terminal");
        terminal
            .draw(|frame| draw(frame, Path::new("/tmp/x"), &Choosing::new()))
            .expect("must not panic on a small area");
        assert!(
            drawn_on(&terminal).contains("Trust /tmp/x?"),
            "the question was drawn out of view: {}",
            drawn_on(&terminal)
        );

        terminal
            .draw(|frame| draw_named(frame, "/tmp/x", &Choosing::new()))
            .expect("must not panic on a small area");
        assert!(
            drawn_on(&terminal).contains("Open /tmp/x?"),
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

    /// Enter is the key most likely to be pressed without reading, and the likeliest single byte to
    /// arrive from a program writing at the terminal, so the row it lands on before anything has
    /// moved the cursor has to be the one that grants nothing.
    #[test]
    fn the_question_opens_on_declining_so_a_stray_enter_grants_nothing() {
        assert_eq!(Choosing::new().chosen(), Answer::Decline);
        assert_eq!(
            answered_by_keys([pressed(KeyCode::Enter)]),
            Some(Answer::Decline)
        );

        let trust = trust_for(Answer::Decline, here()).expect("declining still starts a session");
        assert!(trust.is_empty(), "a stray Enter granted trust");
    }

    /// No letter is an answer and no letter is a movement. `y` and `n` are gone rather than kept as
    /// shortcuts, since a shortcut is the single keystroke this question must not take, and `j` and
    /// `k` are excluded with them: a burst of prose is full of both, and a cursor a program can move
    /// is a row a program can reach.
    #[test]
    fn no_bare_letter_moves_the_cursor_or_answers() {
        for letter in ['y', 'Y', 'n', 'N', 'c', 'j', 'k', 'q'] {
            let mut choosing = Choosing::new();
            assert_eq!(
                handle_key(&mut choosing, pressed(KeyCode::Char(letter))),
                Outcome::Continue,
                "`{letter}` answered the question"
            );
            assert_eq!(
                choosing.chosen(),
                Answer::Decline,
                "`{letter}` moved the cursor"
            );
        }
    }

    /// Ctrl-C is the interrupt everyone reaches for, and raw mode turns it into an ordinary key
    /// press, so a prompt that ignored it would be a screen with no way out. It points at the way
    /// out instead of taking it: one byte written by another program must not end a session, and the
    /// press after it is what does.
    #[test]
    fn ctrl_c_moves_nothing_and_decides_nothing() {
        let mut choosing = Choosing::new();
        let interrupt = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);

        assert_eq!(
            handle_key(&mut choosing, interrupt),
            Outcome::Continue,
            "Ctrl-C answered the question on its own"
        );
        assert_eq!(
            choosing.chosen(),
            Answer::Decline,
            "Ctrl-C moved the cursor off the row that grants nothing"
        );
    }

    /// Two bytes, which is what a shell integration writes when it clears the line before typing:
    /// an interrupt and then a return. While Ctrl-C put the cursor on the row that leaves, those
    /// two spelled the whole quit gesture between them and the session ended without a person,
    /// which is the same silent exit the bare interrupt used to produce.
    #[test]
    fn an_interrupt_then_a_return_does_not_leave() {
        let mut choosing = Choosing::new();
        handle_key(
            &mut choosing,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        );

        assert_eq!(
            handle_key(&mut choosing, pressed(KeyCode::Enter)),
            Outcome::Confirm
        );
        assert_ne!(
            choosing.chosen(),
            Answer::Leave,
            "an interrupt and a return spelled leaving between them"
        );
    }

    /// The same two bytes with the words in between, which is the whole of what was reported: the
    /// clear, the command, then the return.
    #[test]
    fn an_interrupt_a_run_and_a_return_does_not_leave() {
        let mut choosing = Choosing::new();
        handle_key(
            &mut choosing,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        );
        for key in crate::input::run_spelling("source .venv/bin/activate") {
            handle_key(&mut choosing, key);
        }

        assert_eq!(
            handle_key(&mut choosing, pressed(KeyCode::Enter)),
            Outcome::Confirm
        );
        assert_ne!(
            choosing.chosen(),
            Answer::Leave,
            "a cleared line and a command spelled leaving between them"
        );
    }

    /// Escape is the other key a person reaches for to get out, so it points at the same row. It
    /// used to decline, which made a stray one an answer.
    #[test]
    fn escape_moves_nothing_and_decides_nothing() {
        let mut choosing = Choosing::new();

        assert_eq!(
            handle_key(&mut choosing, pressed(KeyCode::Esc)),
            Outcome::Continue,
            "Escape answered the question on its own"
        );
        assert_eq!(
            choosing.chosen(),
            Answer::Decline,
            "Escape moved the cursor off the row that grants nothing"
        );
    }

    /// Every row is reachable and Enter takes the one under the cursor, since a picker that could
    /// not reach one of its answers would be a question with an answer nobody can give.
    #[test]
    fn enter_takes_the_row_under_the_cursor() {
        assert_eq!(
            answered_by_keys([pressed(KeyCode::Up), pressed(KeyCode::Enter)]),
            Some(Answer::Trust)
        );
        assert_eq!(
            answered_by_keys([pressed(KeyCode::Enter)]),
            Some(Answer::Decline)
        );
        assert_eq!(
            answered_by_keys([pressed(KeyCode::Down), pressed(KeyCode::Enter)]),
            Some(Answer::Leave)
        );
    }

    /// The list does not wrap. Holding an arrow down must not carry the cursor off the row a person
    /// stopped at and round onto the one that trusts.
    #[test]
    fn the_arrows_walk_the_rows_and_stop_at_their_ends() {
        let mut choosing = Choosing::new();
        for _ in 0..100 {
            handle_key(&mut choosing, pressed(KeyCode::Up));
        }
        assert_eq!(choosing.chosen(), Answer::Trust);

        for _ in 0..100 {
            handle_key(&mut choosing, pressed(KeyCode::Down));
        }
        assert_eq!(choosing.chosen(), Answer::Leave);
    }

    /// Leaving is not a quiet decline: a session that started anyway would be one the user
    /// never agreed to have.
    #[test]
    fn leaving_starts_no_session() {
        assert!(trust_for(Answer::Leave, here()).is_none());
        assert!(trust_for(Answer::Decline, here()).is_some());
    }

    /// No other control chord points at the way out, so a stray one leaves the cursor where the
    /// person left it rather than moving it onto the row that ends the session.
    #[test]
    fn no_other_control_chord_moves_the_cursor() {
        for chord in ['y', 'n', 'd', 'z', 'u'] {
            let mut choosing = Choosing::new();
            assert_eq!(
                handle_key(
                    &mut choosing,
                    KeyEvent::new(KeyCode::Char(chord), KeyModifiers::CONTROL)
                ),
                Outcome::Continue,
                "ctrl-{chord} answered the question"
            );
            assert_eq!(
                choosing.chosen(),
                Answer::Decline,
                "ctrl-{chord} moved the cursor"
            );
        }
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
